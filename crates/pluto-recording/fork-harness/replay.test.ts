/**
 * Replays every committed recording, `recordings/<agent>/<scenario>.jsonl`, through the fork's
 * adapter, with `pluto-replay-agent` in the CLI's place. Each replay must pass its scenario's
 * assertions, show exactly the thread of its reference, `<scenario>.thread.json`, which the live
 * run wrote, and consume every recorded record of every process.
 */
import * as NodeFS from "node:fs";

import { assert, describe, it } from "@effect/vitest";
import * as Effect from "effect/Effect";

import {
  driveScenario,
  readReference,
  RECORDINGS_DIR,
  recordingsIn,
  referenceOf,
  replayProblems,
  type Agent,
  withScratch,
  wrapperForReplay,
} from "./harness.ts";
import { scenarioNamed } from "./scenarios.ts";

const replay = (agent: Agent, scenarioName: string, recording: string) =>
  withScratch(`${agent}-${scenarioName}-replay`, (scratch) =>
    Effect.gen(function* () {
      const scenario = scenarioNamed(agent, scenarioName);
      const reference = readReference(referenceOf(recording));
      const replayAgent = wrapperForReplay({
        dir: scratch.bin,
        agent,
        recording,
        state: scratch.state,
        workspace: scratch.workspace,
        home: scratch.home,
      });
      const view = yield* driveScenario(agent, scenario, replayAgent, scratch);
      scenario.assert(view);
      assert.deepEqual(view, reference, "the replay shows the thread the live run showed");
      assert.deepEqual(replayProblems(recording, scratch.state), [], "the replay consumed every record");
    }),
  );

const recordings = recordingsIn(RECORDINGS_DIR);

describe("the committed recordings, replayed through the fork's adapters", () => {
  // Named with the count, so that the run says how many it replays, none included.
  it(`replays ${recordings.length} committed recordings, each beside its reference`, () => {
    const alone = recordings.filter(({ path }) => !NodeFS.existsSync(referenceOf(path)));
    assert.deepEqual(alone.map(({ path }) => path), [], "recordings without a reference");
  });

  for (const { agent, scenario, path } of recordings) {
    it.effect(`${agent}/${scenario}`, () => replay(agent, scenario, path));
  }
});
