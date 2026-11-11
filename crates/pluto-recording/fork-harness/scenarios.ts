/**
 * The scripted scenarios ([ADR-0025](../../../docs/adr/0025-agents-sessions-recorded-and-replayed.md)):
 * their prompts, the logical points where the harness acts, and hand-written assertions over the
 * thread a person sees, in the spirit of T3's own fixture assertions.
 *
 * The prompts are short and cheap, and work on the synthetic project `makeScratch` creates. Every
 * scenario runs under the approval-required policy: the harness approves every permission request
 * and answers every question with its first option, each as it comes (see `runScenario`). The
 * steps below add what only some scenarios do.
 *
 * The assertions read only what the adapter reported. Neither adapter reports an approval's
 * decision or a question's answers (the request stays `pending`): they are checked when reported,
 * and otherwise through what followed, the approved tool call's status or the reply.
 */
import { assert } from "@effect/vitest";

import type { Agent, ThreadView, ViewItem, ViewTool, ViewTurn } from "./harness.ts";

export type Step =
  /** Starts a turn with this message, opening the first session when none is open. */
  | { readonly kind: "prompt"; readonly text: string }
  /** Waits for the current turn's terminal event. */
  | { readonly kind: "await_turn_end" }
  /**
   * Interrupts the current turn once its first approval's answer has reached the CLI and the
   * approved tool call runs: the tool has started.
   */
  | { readonly kind: "interrupt_after_first_approval" }
  /** Closes the session, then opens a second one that resumes the first's native thread. */
  | { readonly kind: "resume" }
  /** Waits for the agent to offer a turn of its own, then starts it as a continuation. */
  | { readonly kind: "await_wake" };

export interface Scenario {
  readonly name: string;
  readonly agents: ReadonlyArray<Agent>;
  readonly steps: ReadonlyArray<Step>;
  /** Fails, with a message, when the thread is not what the scenario asked for. */
  readonly assert: (view: ThreadView) => void;
}

export const PROMPTS = {
  turn: "Reply with exactly: pluto turn ok",
  approval: "Create a file named hello.txt containing exactly: hi",
  question:
    "Ask me which colour I prefer with your tool for asking the user a question, offering exactly two options, red and blue. Then reply with exactly the colour I chose.",
  interrupt: "Run the shell command `sleep 30`, then reply with exactly: slept",
  resumeFirst: "Remember the word plum. Reply with exactly: noted",
  resumeSecond: "Which word did I ask you to remember? Reply with that word only.",
  wake: "Start the shell command `sleep 3; echo tock` as a background task and do not wait for it. As soon as it has started, reply with exactly: ROOT_DONE. When it finishes, reply with exactly its output.",
} as const;

function onlySession(view: ThreadView) {
  assert.lengthOf(view.sessions, 1, "one session");
  const [session] = view.sessions;
  assert.isFalse(session!.resumed, "the session is a new one");
  return session!;
}

function onlyTurn(view: ThreadView): ViewTurn {
  const session = onlySession(view);
  assert.lengthOf(session.turns, 1, "one turn");
  return session.turns[0]!;
}

function itemsOf<K extends ViewItem["kind"]>(turn: ViewTurn, kind: K) {
  return turn.items.filter((item): item is Extract<ViewItem, { kind: K }> => item.kind === kind);
}

function assistantText(turn: ViewTurn): string {
  return itemsOf(turn, "assistant")
    .map((item) => item.text)
    .join("\n");
}

function assertCompletedWithText(turn: ViewTurn, text: string) {
  assert.equal(turn.outcome, "completed", `turn ${turn.turn} completed`);
  assert.include(assistantText(turn).toLowerCase(), text.toLowerCase(), `turn ${turn.turn}'s reply`);
}

/** The turn's only approval's tool call; the approval's decision is checked when reported. */
function approvedTool(turn: ViewTurn): ViewTool {
  const approvals = itemsOf(turn, "approval");
  assert.lengthOf(approvals, 1, "one approval");
  const [approval] = approvals;
  if (approval!.answer !== null) {
    assert.equal(approval!.answer, "accept", "the approval was answered allow");
  }
  const tool = itemsOf(turn, "tool").find((candidate) => candidate.id === approval!.toolCall);
  assert.isDefined(tool, "the approval names the tool call it asks about");
  return tool!;
}

const turn: Scenario = {
  name: "turn",
  agents: ["claude", "grok"],
  steps: [{ kind: "prompt", text: PROMPTS.turn }, { kind: "await_turn_end" }],
  assert: (view) => {
    const only = onlyTurn(view);
    assertCompletedWithText(only, "pluto turn ok");
    assert.lengthOf(itemsOf(only, "tool"), 0, "no tool call");
    assert.lengthOf(itemsOf(only, "approval"), 0, "no approval");
    assert.lengthOf(itemsOf(only, "question"), 0, "no question");
  },
};

const approval: Scenario = {
  name: "approval",
  agents: ["claude", "grok"],
  steps: [{ kind: "prompt", text: PROMPTS.approval }, { kind: "await_turn_end" }],
  assert: (view) => {
    const only = onlyTurn(view);
    assert.equal(only.outcome, "completed", "the turn completed");
    const approved = approvedTool(only);
    assert.equal(approved.status, "completed", "the approved tool call completed");
    assert.include(JSON.stringify(approved.input), "hello.txt", "the approved tool call writes hello.txt");
    for (const tool of itemsOf(only, "tool")) {
      assert.equal(tool.status, "completed", `${tool.name} completed`);
    }
  },
};

const question: Scenario = {
  name: "question",
  agents: ["claude", "grok"],
  steps: [{ kind: "prompt", text: PROMPTS.question }, { kind: "await_turn_end" }],
  assert: (view) => {
    const only = onlyTurn(view);
    const questions = itemsOf(only, "question");
    assert.lengthOf(questions, 1, "one question");
    const [asked] = questions;
    assert.lengthOf(asked!.questions, 1, "one question asked");
    const [first] = asked!.questions;
    assert.isAtLeast(first!.options.length, 2, "the question offers its options");
    if (asked!.answer !== null) {
      assert.deepEqual(asked!.answer, { [first!.id]: first!.options[0]! }, "answered with the first option");
    }
    // The reply names the option the harness chose.
    assertCompletedWithText(only, first!.options[0]!);
  },
};

const interrupt: Scenario = {
  name: "interrupt",
  agents: ["claude", "grok"],
  steps: [
    { kind: "prompt", text: PROMPTS.interrupt },
    { kind: "interrupt_after_first_approval" },
    { kind: "await_turn_end" },
  ],
  assert: (view) => {
    const only = onlyTurn(view);
    // T3's own Grok assertion accepts both: Grok reports a cancelled prompt as cancelled.
    assert.include(["interrupted", "cancelled"], only.outcome, "the turn was interrupted");
    const approved = approvedTool(only);
    assert.include(["stopped", "interrupted"], approved.status, "the approved tool call was stopped");
    assert.notInclude(assistantText(only).toLowerCase(), "slept", "the turn did not finish");
  },
};

const resume: Scenario = {
  name: "resume",
  agents: ["claude", "grok"],
  steps: [
    { kind: "prompt", text: PROMPTS.resumeFirst },
    { kind: "await_turn_end" },
    { kind: "resume" },
    { kind: "prompt", text: PROMPTS.resumeSecond },
    { kind: "await_turn_end" },
  ],
  assert: (view) => {
    assert.lengthOf(view.sessions, 2, "two sessions");
    const [first, second] = view.sessions;
    assert.isFalse(first!.resumed, "the first session is new");
    assert.isTrue(second!.resumed, "the second session resumes");
    assert.isNotNull(first!.nativeThread, "the first session has a native thread");
    assert.equal(second!.nativeThread, first!.nativeThread, "on the first session's native thread");
    assert.lengthOf(first!.turns, 1, "one turn in the first session");
    assert.lengthOf(second!.turns, 1, "one turn in the second session");
    assertCompletedWithText(first!.turns[0]!, "noted");
    assertCompletedWithText(second!.turns[0]!, "plum");
  },
};

const wake: Scenario = {
  name: "wake",
  agents: ["grok"],
  steps: [
    { kind: "prompt", text: PROMPTS.wake },
    { kind: "await_turn_end" },
    { kind: "await_wake" },
    { kind: "await_turn_end" },
  ],
  assert: (view) => {
    const session = onlySession(view);
    assert.lengthOf(session.turns, 2, "the person's turn, then the agent's own");
    const [root, woken] = session.turns;
    assertCompletedWithText(root!, "ROOT_DONE");
    assert.equal(approvedTool(root!).status, "completed", "the background command completed");
    assert.equal(woken!.from, "agent", "the second turn is the agent's wake turn");
    assertCompletedWithText(woken!, "tock");
  },
};

export const SCENARIOS: ReadonlyArray<Scenario> = [turn, approval, question, interrupt, resume, wake];

export function scenarioNamed(agent: Agent, name: string): Scenario {
  const scenario = SCENARIOS.find((candidate) => candidate.name === name);
  if (scenario === undefined || !scenario.agents.includes(agent)) {
    throw new Error(`no scenario ${JSON.stringify(name)} for ${agent}`);
  }
  return scenario;
}
