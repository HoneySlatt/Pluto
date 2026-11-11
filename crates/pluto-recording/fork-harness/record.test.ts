/**
 * Records one scenario with the real agent's CLI. The owner runs it by hand; it spends the
 * owner's quota, so it is never part of the checks, and it is skipped unless asked for:
 *
 * ```sh
 * PLUTO_RECORD_AGENT=claude PLUTO_RECORD_SCENARIO=turn \
 * PLUTO_AGENT_BIN=/path/to/claude PLUTO_RECORD_HOME=/path/to/disposable-home \
 * PLUTO_RECORD_OUT=/path/to/staging nix develop --command scripts/fork-replay.sh --record
 * ```
 *
 * `PLUTO_RECORD_HOME` is the disposable home where the owner signed in once to the agent, never
 * the owner's own home; the CLI runs with it as its home, in a fresh synthetic project. The
 * recording goes to `PLUTO_RECORD_OUT/<agent>/<scenario>.jsonl`: the live run must pass the
 * scenario's assertions, then the staged recording is replayed at once and must give the same
 * thread. Only then is the reference, `<scenario>.thread.json`, written beside it; after a failure
 * both files are removed. The owner reads `pluto-record review` on the recording before moving the
 * pair to `recordings/`.
 */
import * as NodeFS from "node:fs";
import * as NodeOS from "node:os";
import * as NodePath from "node:path";

import { assert, describe, it } from "@effect/vitest";
import * as Effect from "effect/Effect";
import * as Cause from "effect/Cause";
import * as Exit from "effect/Exit";

import {
  AGENTS,
  driveScenario,
  referenceOf,
  replayProblems,
  requiredEnv,
  type Agent,
  withScratch,
  wrapperForRecording,
  wrapperForReplay,
  writeReference,
} from "./harness.ts";
import { scenarioNamed } from "./scenarios.ts";

const agentName = process.env.PLUTO_RECORD_AGENT;
const scenarioName = process.env.PLUTO_RECORD_SCENARIO;
const asked = agentName !== undefined && scenarioName !== undefined;

/** A folder the owner names. `fork-replay.sh --record` makes it absolute from where it ran. */
function absoluteFolder(name: string): string {
  const value = requiredEnv(name);
  if (!NodePath.isAbsolute(value)) {
    throw new Error(
      `${name} is ${JSON.stringify(value)}, not an absolute path: run scripts/fork-replay.sh --record, which makes it absolute`,
    );
  }
  return NodePath.normalize(value);
}

/** Whether `outer` is `inner` or holds it. */
function holds(outer: string, inner: string): boolean {
  const withSlash = (path: string) => `${NodeFS.realpathSync(path).replace(/\/+$/, "")}/`;
  return withSlash(inner).startsWith(withSlash(outer));
}

/**
 * Refuses a recording home that is the owner's real home or holds it: the CLI would then run
 * among the owner's own sign-ins, settings and files. A new folder inside the real home, such as
 * the default `~/.local/state/pluto-recording/home`, is a home of its own and is fine.
 */
function refuseRealHome(home: string): void {
  const realHomes = [
    NodeOS.homedir(),
    NodeOS.userInfo().homedir,
    process.env.HOME,
    // $HOME as fork-replay.sh found it, before anything could change it.
    process.env.PLUTO_REAL_HOME,
  ].filter((path): path is string => path !== undefined && path !== "" && NodeFS.existsSync(path));
  for (const real of realHomes) {
    if (holds(home, real)) {
      throw new Error(
        `PLUTO_RECORD_HOME, ${home}, is or holds your own home, ${real}: give a disposable home of its own, where you signed in once to the agent`,
      );
    }
  }
}

const record = (agent: Agent, name: string) => {
  const scenario = scenarioNamed(agent, name);
  const staging = NodePath.join(absoluteFolder("PLUTO_RECORD_OUT"), agent);
  const recording = NodePath.join(staging, `${name}.jsonl`);
  const reference = referenceOf(recording);
  const home = absoluteFolder("PLUTO_RECORD_HOME");
  if (!NodeFS.existsSync(home)) {
    throw new Error(`PLUTO_RECORD_HOME, ${home}, does not exist: sign in to ${agent} there first`);
  }
  refuseRealHome(home);
  const realAgent = requiredEnv("PLUTO_AGENT_BIN");
  /**
   * A failed recording leaves the staging folder, so that it is never accepted, for the run's kept
   * folder, beside the adapter's events and the error, to find why it failed.
   */
  const unstage = (kept: string, error: string) =>
    Effect.sync(() => {
      for (const file of [recording, reference]) {
        if (NodeFS.existsSync(file)) {
          NodeFS.renameSync(file, NodePath.join(kept, NodePath.basename(file)));
        }
      }
      NodeFS.writeFileSync(NodePath.join(kept, "error.txt"), `${error}\n`);
      console.log(`fork-harness record: the recording failed; it, its events and the error are in ${kept}`);
    });
  return withScratch(
    `${agent}-${name}-record`,
    (live) =>
      Effect.gen(function* () {
        NodeFS.mkdirSync(staging, { recursive: true });
        // pluto-record appends to a file that exists: a new recording starts from nothing.
        NodeFS.rmSync(recording, { force: true });
        NodeFS.rmSync(reference, { force: true });
        const recorder = wrapperForRecording({
          dir: live.bin,
          agent,
          scenario: name,
          out: recording,
          workspace: live.workspace,
          home: live.home,
          program: realAgent,
        });
        const liveView = yield* driveScenario(agent, scenario, recorder, live, recording);
        scenario.assert(liveView);

        yield* withScratch(`${agent}-${name}-check`, (replay) =>
          Effect.gen(function* () {
            const replayAgent = wrapperForReplay({
              dir: replay.bin,
              agent,
              recording,
              state: replay.state,
              workspace: replay.workspace,
              home: replay.home,
            });
            const replayView = yield* driveScenario(agent, scenario, replayAgent, replay);
            assert.deepEqual(replayView, liveView, "the replay shows the live thread");
            assert.deepEqual(replayProblems(recording, replay.state), [], "the replay consumed every record");
          }),
        );
        // Only a recording that replays to its own live thread gets a reference.
        writeReference(reference, liveView);
        console.log(`fork-harness record: wrote ${recording} and its reference`);
      }).pipe(
        Effect.onExit((exit) =>
          Exit.isSuccess(exit) ? Effect.void : unstage(live.root, Cause.pretty(exit.cause)),
        ),
      ),
    home,
    // A fixed project folder: its path, streamed in pieces, cannot become `<workspace>`, and a
    // random name in it would make the scan refuse the recording.
    NodePath.join("/tmp/pluto-recording", `${agent}-${name}`),
  );
};

describe.skipIf(!asked)("a scenario recorded with the real agent", () => {
  it.effect(`${agentName}/${scenarioName}`, () => {
    if (!AGENTS.includes(agentName as Agent)) {
      throw new Error(`PLUTO_RECORD_AGENT must be one of ${AGENTS.join(", ")}`);
    }
    return record(agentName as Agent, scenarioName!);
  });
});
