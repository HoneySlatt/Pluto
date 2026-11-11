/**
 * The harness's proof without a real agent. Each hand-written recording in `synthetic/` stands
 * for a real CLI: the fake agent plays it behind `pluto-record`, which records it as if it were
 * live. The scenario's assertions must hold on that "live" run; then the new recording is
 * replayed through the same adapter, and the replay must give the same thread and consume every
 * recorded frame.
 *
 * What this cannot prove: that the real CLIs send what the synthetic files hold. Those are
 * written after the SDK's frames and T3's recordings, not captured; `record.test.ts` is the live
 * check, run by the owner.
 */
import * as NodePath from "node:path";

import { assert, describe, it } from "@effect/vitest";
import * as Effect from "effect/Effect";

import {
  driveScenario,
  readReference,
  recordingsIn,
  referenceOf,
  replayProblems,
  SYNTHETIC_DIR,
  type Agent,
  withScratch,
  wrapperForRecording,
  wrapperForReplay,
  writeReference,
} from "./harness.ts";
import { type Scenario, SCENARIOS, scenarioNamed } from "./scenarios.ts";

const selfTest = (agent: Agent, scenario: Scenario, synthetic: string) =>
  withScratch(`${agent}-${scenario.name}-live`, (live) =>
    withScratch(`${agent}-${scenario.name}-replay`, (replay) =>
      Effect.gen(function* () {
        const recording = NodePath.join(live.root, `${scenario.name}.jsonl`);
        const syntheticState = NodePath.join(live.root, "synthetic-state");

        // The "real" CLI: the fake agent playing the synthetic file, recorded by pluto-record.
        const syntheticAgent = wrapperForReplay({
          dir: NodePath.join(live.bin, "synthetic"),
          agent,
          recording: synthetic,
          state: syntheticState,
          workspace: live.workspace,
          home: live.home,
        });
        const recorder = wrapperForRecording({
          dir: live.bin,
          agent,
          scenario: scenario.name,
          out: recording,
          workspace: live.workspace,
          home: live.home,
          program: syntheticAgent,
        });
        const liveView = yield* driveScenario(agent, scenario, recorder, live, recording);
        scenario.assert(liveView);
        writeReference(referenceOf(recording), liveView);

        const replayAgent = wrapperForReplay({
          dir: replay.bin,
          agent,
          recording,
          state: replay.state,
          workspace: replay.workspace,
          home: replay.home,
        });
        const replayView = yield* driveScenario(agent, scenario, replayAgent, replay);
        scenario.assert(replayView);
        assert.deepEqual(
          replayView,
          readReference(referenceOf(recording)),
          "the replay shows the live thread",
        );
        // Last, so that a failure here comes after every other check has passed.
        assert.deepEqual(
          replayProblems(synthetic, syntheticState),
          [],
          "the synthetic file was played whole",
        );
        assert.deepEqual(replayProblems(recording, replay.state), [], "the replay consumed every record");
      }),
    ),
  );

const synthetic = recordingsIn(SYNTHETIC_DIR);

describe("the harness on synthetic recordings", () => {
  it(`has ${synthetic.length} synthetic recordings, one per scenario of each agent`, () => {
    const expected = SCENARIOS.flatMap((scenario) =>
      scenario.agents.map((agent) => `${agent}/${scenario.name}`),
    ).sort();
    const found = synthetic.map(({ agent, scenario }) => `${agent}/${scenario}`).sort();
    assert.deepEqual(found, expected);
  });

  for (const { agent, scenario, path } of synthetic) {
    it.effect(`${agent}/${scenario}`, () => selfTest(agent, scenarioNamed(agent, scenario), path));
  }
});
