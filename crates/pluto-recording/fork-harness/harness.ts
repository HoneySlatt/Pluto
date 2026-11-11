/**
 * Drives the fork's real adapters through scripted scenarios
 * (docs/adr/0025-agents-sessions-recorded-and-replayed.md).
 *
 * The adapter is the fork's own: `ClaudeAdapterV2` with the real Claude Agent SDK, or the Grok
 * ACP adapter with its production runtime. Only the agent's executable changes. Recording puts
 * `pluto-record` in front of the real CLI; replaying puts `pluto-replay-agent` in its place. Both
 * are reached through a small shell script that the adapter starts as if it were the CLI.
 *
 * - `buildAdapter`: the adapter, with the executable it starts.
 * - `wrapperForRecording`, `wrapperForReplay`: the scripts.
 * - `runScenario`: opens the sessions, sends the prompts and acts at fixed logical points.
 * - `threadView`: the thread as a person sees it, compared between a live run and its replay.
 *
 * Clock: the scenario runs on the fork's deterministic runtime (its test clock and a seeded
 * random); the adapter's construction, every session call and the closing of each session run
 * on the wall clock, because a real process answers in real time. This is the recipe of the
 * fork's Grok recorder. Every wait is on an event, bounded by a wall-clock limit.
 *
 * The fork is never written to. Its modules are imported through the `@fork/` alias, which
 * `vitest.config.mjs` points at the fork named by `PLUTO_FORK`; its packages resolve from its own
 * `node_modules`. `scripts/fork-replay.sh` sets everything up and runs the test files:
 * `self-test.test.ts` (the proof on hand-written recordings), `replay.test.ts` (the committed
 * recordings) and, by hand only, `record.test.ts` (a live recording).
 *
 * A Grok process never ends by itself: when a session closes, the fork's ACP runtime kills the
 * agent's whole process tree at once. A Grok recording therefore ends without `stdin_closed` and
 * `exit`; the fake agent saves its status after every step, so the kill loses nothing.
 */
import * as NodeChildProcess from "node:child_process";
import * as NodeFS from "node:fs";
import * as NodeOS from "node:os";
import * as NodePath from "node:path";
import * as NodeURL from "node:url";

import * as NodeServices from "@effect/platform-node/NodeServices";
import {
  ClaudeSettings,
  MessageId,
  NodeId,
  type OrchestrationV2AppThread,
  type OrchestrationV2ProviderThread,
  ProjectId,
  ProviderSessionId,
  RunAttemptId,
  RunId,
  ThreadId,
} from "@t3tools/contracts";
import * as IdAllocator from "@t3tools/provider-core/server/IdAllocator";
import * as McpProviderSessions from "@t3tools/provider-core/server/McpProviderSessions";
import * as ProviderAdapter from "@t3tools/provider-core/server/ProviderAdapter";
import type { ProviderContinuationRequest } from "@t3tools/provider-core/server/ProviderContinuationRequests";
import * as ProviderEventLoggers from "@t3tools/provider-core/server/ProviderEventLoggers";
import { GrokSettings } from "@t3tools/provider-grok/settings";
import { GROK_DEFAULT_INSTANCE_ID, makeGrokAdapterV2 } from "@t3tools/provider-grok/testing";
import * as TestProviderHost from "@t3tools/provider-testing/TestProviderHost";
import * as HostProcess from "@t3tools/shared/HostProcess";
import { resolveSelfInvocation } from "@t3tools/shared/nodeRuntime";
import * as Cause from "effect/Cause";
import * as Clock from "effect/Clock";
import * as Crypto from "effect/Crypto";
import * as DateTime from "effect/DateTime";
import * as Effect from "effect/Effect";
import * as Exit from "effect/Exit";
import * as Fiber from "effect/Fiber";
import * as FileSystem from "effect/FileSystem";
import * as Layer from "effect/Layer";
import * as Path from "effect/Path";
import * as Queue from "effect/Queue";
import * as Schema from "effect/Schema";
import * as Scope from "effect/Scope";
import * as Stream from "effect/Stream";

import * as ClaudeAdapterV2 from "@fork/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts";
import { provideDeterministicTestRuntime } from "@fork/apps/server/src/orchestration-v2/testkit/DeterministicRuntime.ts";

import type { Scenario } from "./scenarios.ts";

export type Agent = "claude" | "grok";
export const AGENTS: ReadonlyArray<Agent> = ["claude", "grok"];

type AdapterEvent = ProviderAdapter.ProviderAdapterV2Event;
type SessionRuntime = ProviderAdapter.ProviderAdapterV2SessionRuntime;
type ProviderThread = OrchestrationV2ProviderThread;

/** A failure the harness reports: what it waited for, or what went wrong. */
export class HarnessError extends Error {
  override readonly name = "HarnessError";
}

// --- Places and programs -----------------------------------------------------------------------

const HARNESS_DIR = NodePath.dirname(NodeURL.fileURLToPath(import.meta.url));
/** Committed recordings, `<agent>/<scenario>.jsonl`, each beside its `<scenario>.thread.json`. */
export const RECORDINGS_DIR = NodePath.resolve(HARNESS_DIR, "../recordings");
/** Hand-written recordings, `<agent>/<scenario>.jsonl`, that stand for the real CLIs. */
export const SYNTHETIC_DIR = NodePath.join(HARNESS_DIR, "synthetic");

/** How long any one wait may last: a real agent's turn, with room to spare. */
const WAIT_LIMIT = "120 seconds";
/** How long the fake agent waits for the client before it gives up (its `--timeout-ms`). */
const REPLAY_SILENCE_MS = 20_000;

export function requiredEnv(name: string): string {
  const value = process.env[name];
  if (value === undefined || value.trim() === "") {
    throw new HarnessError(`${name} is not set: run the harness through scripts/fork-replay.sh`);
  }
  return value;
}

/** The fork's checkout; the config's `@fork/` alias points at the same folder. */
export const forkRoot = () => requiredEnv("PLUTO_FORK");

/** The SDK or protocol library the fork drives the agent with, as `name version`. */
export function sdkName(agent: Agent): string {
  const packageDir =
    agent === "claude" ? "@anthropic-ai/claude-agent-sdk" : "effect-acp";
  const manifest = JSON.parse(
    NodeFS.readFileSync(
      NodePath.join(forkRoot(), "apps/server/node_modules", packageDir, "package.json"),
      "utf8",
    ),
  ) as { name: string; version?: string; claudeCodeVersion?: string };
  // effect-acp is a package of the fork's own workspace and has no version of its own.
  const version = manifest.version ?? "workspace";
  return manifest.claudeCodeVersion === undefined
    ? `${manifest.name} ${version}`
    : `${manifest.name} ${version} (Claude Code ${manifest.claudeCodeVersion})`;
}

/** Variables the SDK or the fork sets for the CLI; `pluto-record` keeps them, and nothing else. */
const KEPT_ENVIRONMENT: Record<Agent, ReadonlyArray<string>> = {
  claude: ["CLAUDE_CODE_ENTRYPOINT", "CLAUDE_AGENT_SDK_VERSION"],
  grok: ["GROK_OAUTH2_REFERRER"],
};

function shellQuote(text: string): string {
  return `'${text.replaceAll("'", `'\\''`)}'`;
}

/** Writes an executable script named like the agent's CLI, which `exec`s `command`. */
function writeWrapper(dir: string, agent: Agent, command: ReadonlyArray<string>): string {
  NodeFS.mkdirSync(dir, { recursive: true });
  const path = NodePath.join(dir, agent);
  const line = [...command.map(shellQuote), '"$@"'].join(" ");
  NodeFS.writeFileSync(path, `#!/bin/sh\nexec ${line}\n`, { mode: 0o755 });
  return path;
}

export interface RecordingWrapperInput {
  readonly dir: string;
  readonly agent: Agent;
  readonly scenario: string;
  readonly out: string;
  readonly workspace: string;
  readonly home: string;
  /** The real CLI, or what stands for it. */
  readonly program: string;
}

/** A script that runs the agent's CLI behind `pluto-record`, which writes `out`. */
export function wrapperForRecording(input: RecordingWrapperInput): string {
  return writeWrapper(input.dir, input.agent, [
    requiredEnv("PLUTO_RECORD_BIN"),
    "record",
    "--out",
    input.out,
    "--agent",
    input.agent,
    "--scenario",
    input.scenario,
    "--workspace",
    input.workspace,
    "--home",
    input.home,
    "--meta",
    `sdk=${sdkName(input.agent)}`,
    ...KEPT_ENVIRONMENT[input.agent].flatMap((name) => ["--keep-env", name]),
    "--",
    input.program,
  ]);
}

export interface ReplayWrapperInput {
  readonly dir: string;
  readonly agent: Agent;
  readonly recording: string;
  /** The fake agent's state folder: its process counter and one status file per process. */
  readonly state: string;
  readonly workspace: string;
  readonly home: string;
}

/** A script that plays `recording` in the CLI's place. */
export function wrapperForReplay(input: ReplayWrapperInput): string {
  return writeWrapper(input.dir, input.agent, [
    requiredEnv("PLUTO_REPLAY_BIN"),
    "--recording",
    input.recording,
    "--state",
    input.state,
    "--workspace",
    input.workspace,
    "--home",
    input.home,
    "--timeout-ms",
    String(REPLAY_SILENCE_MS),
    "--",
  ]);
}

/** Folders for one run: a synthetic project, a disposable home, and room for scripts. */
export interface Scratch {
  readonly root: string;
  readonly workspace: string;
  readonly home: string;
  readonly bin: string;
  readonly state: string;
}

const PROJECT_README =
  "# Synthetic project\n\nA disposable project for Pluto's recorded agent sessions. It holds no secret.\n";

/** Makes the folders of one run under the system's temporary folder, outside the fork. */
function makeScratch(name: string, home?: string, workspace?: string): Scratch {
  const root = NodeFS.realpathSync(
    NodeFS.mkdtempSync(NodePath.join(NodeOS.tmpdir(), `pluto-fork-harness-${name}-`)),
  );
  if (workspace !== undefined) {
    NodeFS.rmSync(workspace, { recursive: true, force: true });
  }
  const scratch = {
    root,
    workspace: workspace ?? NodePath.join(root, "project"),
    home: home ?? NodePath.join(root, "home"),
    bin: NodePath.join(root, "bin"),
    state: NodePath.join(root, "state"),
  };
  for (const dir of [scratch.workspace, scratch.home, scratch.bin]) {
    NodeFS.mkdirSync(dir, { recursive: true });
  }
  NodeFS.writeFileSync(NodePath.join(scratch.workspace, "README.md"), PROJECT_README);
  return scratch;
}

/**
 * Runs `use` with fresh folders, removed after a success. After a failure they stay, to be read,
 * and so they do after a success when `PLUTO_HARNESS_KEEP=1`; the harness says where. A
 * `workspace` given is used, emptied first, instead of one inside the scratch folder.
 */
export function withScratch<A, E, R>(
  name: string,
  use: (scratch: Scratch) => Effect.Effect<A, E, R>,
  home?: string,
  workspace?: string,
): Effect.Effect<A, E, R> {
  return Effect.suspend(() => {
    const scratch = makeScratch(name, home, workspace);
    return use(scratch).pipe(
      Effect.onExit((exit) =>
        Effect.sync(() => {
          if (Exit.isSuccess(exit) && process.env.PLUTO_HARNESS_KEEP !== "1") {
            NodeFS.rmSync(scratch.root, { recursive: true, force: true });
            if (workspace !== undefined) {
              NodeFS.rmSync(workspace, { recursive: true, force: true });
            }
          } else {
            console.log(`fork-harness: kept ${scratch.root}`);
          }
        }),
      ),
    );
  });
}

// --- The adapters ------------------------------------------------------------------------------

const wallClock = Clock.Clock.defaultValue();

/** Runs `effect` on the wall clock: a real process answers in real time. */
function onWallClock<A, E, R>(effect: Effect.Effect<A, E, R>): Effect.Effect<A, E, R> {
  return Effect.provideService(effect, Clock.Clock, wallClock);
}

/** Runs a scenario's effect on the fork's deterministic runtime, with `home` as the home. */
export function inHarnessRuntime<A, E>(
  effect: Effect.Effect<A, E, Scope.Scope>,
  home: string,
): Effect.Effect<A, E> {
  return effect.pipe(
    Effect.provideService(HostProcess.HomeDirectory, home),
    Effect.scoped,
    provideDeterministicTestRuntime,
  );
}

export interface HarnessAdapter {
  readonly adapter: ProviderAdapter.ProviderAdapterV2["Service"];
  readonly workspace: string;
  /** What the adapter offered to start on its own: the agent's wake turns. */
  readonly continuations: Queue.Queue<ProviderContinuationRequest>;
}

/** The environment the adapter hands the CLI: the disposable home, and nothing personal. */
function agentEnvironment(agent: Agent, home: string): NodeJS.ProcessEnv {
  return {
    PATH: process.env.PATH ?? "/usr/bin:/bin",
    HOME: home,
    LANG: "C.UTF-8",
    // The fork's skill discovery reads Claude's config folder; this keeps it in the home.
    ...(agent === "claude" ? { CLAUDE_CONFIG_DIR: NodePath.join(home, ".claude") } : {}),
  };
}

/**
 * The fork's real adapter for `agent`, starting `executable` as its CLI in `workspace`. Claude's
 * adapter gets the real SDK query runner, with `executable` as its `binaryPath`; Grok's gets its
 * production ACP runtime, which starts `executable` with Grok's production arguments.
 */
export const buildAdapter = (
  agent: Agent,
  executable: string,
  workspace: string,
  home: string,
): Effect.Effect<HarnessAdapter, never, Scope.Scope> =>
  onWallClock(
    Effect.gen(function* () {
      const continuations = yield* Queue.unbounded<ProviderContinuationRequest>();
      const continuationRequests = {
        offer: (request: ProviderContinuationRequest) =>
          Queue.offer(continuations, request).pipe(Effect.asVoid),
      };
      const adapter =
        agent === "claude"
          ? yield* claudeAdapter(executable, home, continuationRequests)
          : yield* grokAdapter(executable, workspace, home, continuationRequests);
      return { adapter, workspace, continuations };
    }),
  );

type ContinuationSink = { readonly offer: (request: ProviderContinuationRequest) => Effect.Effect<void> };

const claudeAdapter = (executable: string, home: string, continuationRequests: ContinuationSink) =>
  Effect.gen(function* () {
    const context = yield* Layer.build(
      ClaudeAdapterV2.layerQueryRunner.pipe(
        Layer.provideMerge(
          Layer.mergeAll(
            IdAllocator.layer,
            McpProviderSessions.layer,
            Layer.succeed(
              ProviderEventLoggers.ProviderEventLoggers,
              ProviderEventLoggers.NoOpProviderEventLoggers,
            ),
            NodeServices.layer,
          ),
        ),
      ),
    );
    return yield* Effect.gen(function* () {
      const fileSystem = yield* FileSystem.FileSystem;
      return yield* ClaudeAdapterV2.makeClaudeAdapterV2({
        instanceId: ClaudeAdapterV2.CLAUDE_DEFAULT_INSTANCE_ID,
        settings: Schema.decodeSync(ClaudeSettings)({ binaryPath: executable }),
        environment: agentEnvironment("claude", home),
        // In the home, so that the recorded `--add-dir` is a placeholder, not a random path.
        attachmentsDir: NodePath.join(home, "attachments"),
        fileSystem,
        path: yield* Path.Path,
        crypto: yield* Crypto.Crypto,
        idAllocator: yield* IdAllocator.IdAllocatorV2,
        queryRunner: yield* ClaudeAdapterV2.ClaudeAgentSdkQueryRunner,
        continuationRequests,
      });
    }).pipe(Effect.provide(context), Effect.orDie);
  });

const grokAdapter = (
  executable: string,
  workspace: string,
  home: string,
  continuationRequests: ContinuationSink,
) =>
  Effect.gen(function* () {
    const context = yield* Layer.build(
      Layer.mergeAll(
        TestProviderHost.layer({ cwd: workspace }),
        IdAllocator.layer,
        McpProviderSessions.layer,
      ).pipe(Layer.provideMerge(NodeServices.layer)),
    );
    return yield* Effect.gen(function* () {
      return yield* makeGrokAdapterV2({
        instanceId: GROK_DEFAULT_INSTANCE_ID,
        settings: Schema.decodeSync(GrokSettings)({ binaryPath: executable }),
        environment: agentEnvironment("grok", home),
        hostPlatform: process.platform,
        selfInvocation: yield* resolveSelfInvocation(),
        continuationRequests,
      });
    }).pipe(Effect.provide(context), Effect.orDie);
  });

// --- Running a scenario ------------------------------------------------------------------------

const MODEL_SELECTIONS = {
  claude: { instanceId: ClaudeAdapterV2.CLAUDE_DEFAULT_INSTANCE_ID, model: "claude-haiku-5-5" },
  grok: { instanceId: GROK_DEFAULT_INSTANCE_ID, model: "grok-build" },
} as const;

const THREAD_ID = ThreadId.make("thread-harness");

/** One event the adapter emitted, with the session (from 1) it came from. */
export interface LoggedEvent {
  readonly session: number;
  readonly event: AdapterEvent;
}

/** One turn the harness started. */
export interface StartedTurn {
  readonly session: number;
  /** The run's ordinal in the thread, from 1; `turn.terminal` carries it. */
  readonly ordinal: number;
  readonly runId: string;
  readonly attemptId: string;
  /** Who sent the turn's message: the person, or the agent for its own wake turn. */
  readonly from: "user" | "agent";
  readonly prompt: string | null;
}

export interface OpenedSession {
  readonly session: number;
  readonly resumed: boolean;
  /** The native thread the session ended on, as the adapter last reported it. */
  nativeThread: string | null;
}

/** Everything a scenario's run left: what `threadView` reads. */
export interface ScenarioRun {
  readonly agent: Agent;
  readonly scenario: string;
  readonly sessions: Array<OpenedSession>;
  readonly turns: Array<StartedTurn>;
  readonly events: Array<LoggedEvent>;
}

/** The adapters' events as they come, with waits on what they show. */
class EventLog {
  private readonly watchers = new Set<() => boolean>();
  private failure: string | undefined;

  /** `file`, when given, receives each event as it comes: a failed run is read there. */
  constructor(
    readonly entries: Array<LoggedEvent>,
    private readonly file?: string,
  ) {}

  push(entry: LoggedEvent): void {
    this.entries.push(entry);
    if (this.file !== undefined) {
      NodeFS.appendFileSync(this.file, `${JSON.stringify(entry)}\n`);
    }
    this.notify();
  }

  /** The event stream of a session failed: every wait fails with it. */
  fail(message: string): void {
    this.failure ??= message;
    this.notify();
  }

  /** Something a wait reads besides the events changed: every wait checks again. */
  notify(): void {
    for (const watcher of [...this.watchers]) {
      if (watcher()) this.watchers.delete(watcher);
    }
  }

  /** Waits until `check` returns a value, re-checking at each event, for `WAIT_LIMIT` at most. */
  until<A>(description: string, check: () => A | undefined): Effect.Effect<A, HarnessError> {
    return onWallClock(
      this.whenever(description, check).pipe(
        Effect.timeoutOrElse({
          duration: WAIT_LIMIT,
          orElse: () =>
            Effect.fail(new HarnessError(`waited ${WAIT_LIMIT} for ${description}: ${this.pending()}`)),
        }),
      ),
    );
  }

  /** Waits until `check` returns a value, however long that takes. */
  whenever<A>(description: string, check: () => A | undefined): Effect.Effect<A, HarnessError> {
    return Effect.callback<A, HarnessError>((resume) => {
      const attempt = () => {
        const found = check();
        if (found !== undefined) {
          resume(Effect.succeed(found));
          return true;
        }
        if (this.failure !== undefined) {
          resume(Effect.fail(new HarnessError(`${this.failure}, while waiting for ${description}`)));
          return true;
        }
        return false;
      };
      if (attempt()) return;
      this.watchers.add(attempt);
      return Effect.sync(() => {
        this.watchers.delete(attempt);
      });
    });
  }

  /** What is still open, for a timeout's message. */
  private pending(): string {
    const requests = latestBy(
      this.entries.flatMap(({ event }) =>
        event.type === "runtime_request.updated" ? [event.runtimeRequest] : [],
      ),
      (request) => request.id,
    ).filter((request) => request.status === "pending");
    return requests.length === 0
      ? "no runtime request pending"
      : `pending runtime requests: ${requests.map((request) => request.kind).join(", ")}`;
  }
}

/** The last value per key, in order of each key's first appearance. */
function latestBy<A>(values: ReadonlyArray<A>, key: (value: A) => string): Array<A> {
  const latest = new Map<string, A>();
  for (const value of values) latest.set(key(value), value);
  return [...latest.values()];
}

/** Appends one scripted action to the recording, in one write, after the frames it answers. */
function appendAction(recordingPath: string | undefined, name: string, detail: object): void {
  if (recordingPath === undefined) return;
  if (!NodeFS.existsSync(recordingPath)) {
    throw new HarnessError(`the action ${name} came before the agent's CLI started`);
  }
  NodeFS.appendFileSync(recordingPath, `${JSON.stringify({ kind: "action", name, detail })}\n`);
}

/** A session the harness drives: its runtime, its scope and its provider thread. */
interface LiveSession {
  readonly record: OpenedSession;
  readonly runtime: SessionRuntime;
  readonly scope: Scope.Closeable;
  providerThread: ProviderThread;
  readonly starts: Array<Fiber.Fiber<void, unknown>>;
}

/**
 * Runs `scenario` on `harness`'s adapter. When `recordingPath` is given, each scripted action is
 * appended to it as an `action` record.
 */
export const runScenario = (
  agent: Agent,
  scenario: Scenario,
  harness: HarnessAdapter,
  recordingPath?: string,
  eventsFile?: string,
): Effect.Effect<ScenarioRun, HarnessError> =>
  Effect.gen(function* () {
    const run: ScenarioRun = {
      agent,
      scenario: scenario.name,
      sessions: [],
      turns: [],
      events: [],
    };
    /** Requests the harness is answering or has answered: none is answered twice. */
    const claimed = new Set<string>();
    /** The answers the adapter took and, when recording, that reached the CLI, in order. */
    const sent: Array<AnswerableRequest> = [];
    const log = new EventLog(run.events, eventsFile);
    const modelSelection = MODEL_SELECTIONS[agent];
    const runtimePolicy = ProviderAdapter.ProviderAdapterV2RuntimePolicy.make({
      runtimeMode: "approval-required",
      interactionMode: "default",
      cwd: harness.workspace,
    });
    let current: LiveSession | undefined;

    const openSession = (resumeFrom?: LiveSession) =>
      Effect.gen(function* () {
        const number = run.sessions.length + 1;
        const scope = yield* Scope.make();
        // Closed by the scenario once it is done with the session, or here when it fails; on
        // the wall clock, since closing stops a real process.
        yield* Effect.addFinalizer(() => onWallClock(Scope.close(scope, Exit.void)));
        const previousNative = resumeFrom?.providerThread.nativeThreadRef?.nativeId ?? undefined;
        const runtime = yield* onWallClock(
          harness.adapter
            .openSession({
              threadId: THREAD_ID,
              providerSessionId: ProviderSessionId.make(`session-${number}`),
              modelSelection,
              runtimePolicy,
              ...(resumeFrom === undefined
                ? {}
                : { resumeFromSession: resumeFrom.runtime.providerSession }),
              ...(previousNative === undefined ? {} : { initialNativeThreadId: previousNative }),
            })
            .pipe(Scope.provide(scope)),
        );
        yield* runtime.events.pipe(
          Stream.runForEach((event) => Effect.sync(() => log.push({ session: number, event }))),
          Effect.catchCause((cause) =>
            Effect.sync(() => log.fail(`session ${number}'s events failed: ${Cause.pretty(cause)}`)),
          ),
          Effect.forkIn(scope),
        );
        const providerThread = yield* onWallClock(
          resumeFrom === undefined
            ? runtime.ensureThread({ threadId: THREAD_ID, modelSelection, runtimePolicy })
            : runtime.resumeThread({
                providerThread: resumeFrom.providerThread,
                threadId: THREAD_ID,
                modelSelection,
                runtimePolicy,
              }),
        );
        const record: OpenedSession = {
          session: number,
          resumed: resumeFrom !== undefined,
          nativeThread: providerThread.nativeThreadRef?.nativeId ?? null,
        };
        run.sessions.push(record);
        const session: LiveSession = { record, runtime, scope, providerThread, starts: [] };
        yield* answerRequests(session).pipe(Effect.forkIn(scope));
        return session;
      }).pipe(Effect.mapError(asHarnessError("opening a session")));

    /**
     * Answers every approval with accept and every question with its first option. An answer
     * counts as sent once the adapter took it and, when recording, once it reached the CLI:
     * `pluto-record` writes each line before it forwards it, so whatever the harness sends after
     * comes after it on the wire. Claude's adapter takes an answer before the SDK writes it.
     */
    const answerRequests = (session: LiveSession): Effect.Effect<never, HarnessError> =>
      Effect.forever(
        Effect.gen(function* () {
          // A session may wait long for its next request; closing the session ends this wait.
          const next = yield* log.whenever("the next runtime request", () =>
            nextAnswerableRequest(log.entries, session.record.session, claimed),
          );
          claimed.add(next.requestId);
          appendAction(
            recordingPath,
            next.kind === "approval" ? "approve" : "answer",
            next.kind === "approval"
              ? { decision: "accept" }
              : { questions: next.questions.length, option: "first" },
          );
          const answersOnTheWire =
            recordingPath === undefined ? 0 : clientAnswersIn(recordingPath);
          yield* onWallClock(
            session.runtime.respondToRuntimeRequest(
              next.kind === "approval"
                ? { requestId: next.requestId, decision: "accept" }
                : { requestId: next.requestId, answers: firstOptions(next.questions) },
            ),
          ).pipe(Effect.mapError(asHarnessError("answering a runtime request")));
          if (recordingPath !== undefined) {
            yield* untilRecordingHolds(recordingPath, answersOnTheWire + 1);
          }
          sent.push(next);
          log.notify();
        }),
      );

    const startTurn = (session: LiveSession, from: "user" | "agent", prompt: string | null) =>
      Effect.gen(function* () {
        const ordinal = run.turns.length + 1;
        const turn: StartedTurn = {
          session: session.record.session,
          ordinal,
          runId: `run-${ordinal}`,
          attemptId: `attempt-${ordinal}`,
          from,
          prompt,
        };
        run.turns.push(turn);
        const now = yield* DateTime.now;
        const input = turnInput({
          turn,
          providerThread: session.providerThread,
          now,
          modelSelection,
          runtimePolicy,
        });
        // An ACP prompt returns when its turn ends; the scenario goes on meanwhile.
        const fiber = yield* onWallClock(session.runtime.startTurn(input)).pipe(
          Effect.forkIn(session.scope),
        );
        session.starts.push(fiber as Fiber.Fiber<void, unknown>);
        return turn;
      });

    const closeSession = (session: LiveSession) =>
      Effect.gen(function* () {
        session.record.nativeThread =
          lastNativeThread(log.entries, session.record.session) ?? session.record.nativeThread;
        // A turn that failed to start says why here, rather than as a missing event.
        for (const start of session.starts) {
          const exit = yield* onWallClock(
            Fiber.await(start).pipe(
              Effect.timeoutOrElse({
                duration: WAIT_LIMIT,
                orElse: () =>
                  Effect.fail(new HarnessError(`a turn's start had not returned after ${WAIT_LIMIT}`)),
              }),
            ),
          );
          if (Exit.isFailure(exit) && !Cause.hasInterruptsOnly(exit.cause)) {
            return yield* Effect.fail(
              new HarnessError(`a turn failed to start: ${Cause.pretty(exit.cause)}`),
            );
          }
        }
        yield* onWallClock(Scope.close(session.scope, Exit.void));
      });

    const currentTurn = () => {
      const turn = run.turns.at(-1);
      if (turn === undefined) throw new HarnessError("the scenario waits on a turn it never started");
      return turn;
    };

    for (const step of scenario.steps) {
      switch (step.kind) {
        case "prompt": {
          current ??= yield* openSession();
          yield* startTurn(current, "user", step.text);
          break;
        }
        case "await_turn_end": {
          const turn = currentTurn();
          yield* log.until(`the end of turn ${turn.ordinal}`, () =>
            terminalOf(log.entries, turn) === undefined ? undefined : true,
          );
          break;
        }
        case "interrupt_after_first_approval": {
          const session = current;
          const turn = currentTurn();
          if (session === undefined) throw new HarnessError("interrupt before any session");
          // The approval's answer was sent (see `answerRequests`) and its tool runs. Neither
          // adapter reports the request leaving `pending`. Grok's tool turns `running` once
          // the CLI starts it; Claude's shows `running` from the start, and the answer having
          // reached the CLI is what says the tool started.
          yield* log.until(`turn ${turn.ordinal}'s first approved tool call, running`, () => {
            const approval = sent.find((request) => request.kind === "approval");
            if (approval === undefined) return undefined;
            const tool = toolOfRequest(log.entries, session.record.session, approval.requestId);
            return tool?.status === "running" ? true : undefined;
          });
          const providerTurnId = yield* log.until(`turn ${turn.ordinal}'s provider turn`, () =>
            providerTurnOf(log.entries, turn),
          );
          appendAction(recordingPath, "interrupt", { after: "first approved tool started" });
          yield* onWallClock(
            session.runtime.interruptTurn({
              providerThread: session.providerThread,
              providerTurnId,
            }),
          ).pipe(Effect.mapError(asHarnessError("interrupting the turn")));
          break;
        }
        case "resume": {
          const first = current;
          if (first === undefined) throw new HarnessError("resume before any session");
          yield* closeSession(first);
          current = undefined;
          appendAction(recordingPath, "resume", { session: run.sessions.length + 1 });
          current = yield* openSession(first);
          break;
        }
        case "await_wake": {
          const session = current;
          if (session === undefined) throw new HarnessError("wake before any session");
          const request = yield* onWallClock(
            Queue.take(harness.continuations).pipe(
              Effect.timeoutOrElse({
                duration: WAIT_LIMIT,
                orElse: () =>
                  Effect.fail(new HarnessError(`waited ${WAIT_LIMIT} for the agent's wake turn`)),
              }),
            ),
          );
          appendAction(recordingPath, "wake", { continuation: run.turns.length + 1 });
          yield* startTurn(session, "agent", request.detail);
          break;
        }
      }
    }
    if (current !== undefined) yield* closeSession(current);
    return run;
  }).pipe(Effect.scoped);

function asHarnessError(doing: string) {
  return (cause: unknown) =>
    cause instanceof HarnessError
      ? cause
      : new HarnessError(`${doing}: ${cause instanceof Error ? cause.message : String(cause)}`);
}

/**
 * What `startTurn` takes. The app thread holds what the adapters read, in the shape of the
 * fork's own adapter tests; the turn's ordinal is the thread's, so that a second session's first
 * turn is turn 2 of a native thread that already has one, which is what makes it resume.
 */
function turnInput(input: {
  readonly turn: StartedTurn;
  readonly providerThread: ProviderThread;
  readonly now: DateTime.Utc;
  readonly modelSelection: (typeof MODEL_SELECTIONS)[Agent];
  readonly runtimePolicy: ProviderAdapter.ProviderAdapterV2RuntimePolicy;
}): ProviderAdapter.ProviderAdapterV2TurnInput {
  const { turn, now } = input;
  return {
    appThread: {
      createdBy: "user",
      creationSource: "web",
      id: THREAD_ID,
      projectId: ProjectId.make("project-harness"),
      title: "Pluto fork harness",
      providerInstanceId: input.modelSelection.instanceId,
      modelSelection: input.modelSelection,
      runtimeMode: "approval-required",
      interactionMode: "default",
      branch: null,
      worktreePath: null,
      activeProviderThreadId: input.providerThread.id,
      lineage: { parentThreadId: null, relationshipToParent: null, rootThreadId: THREAD_ID },
      forkedFrom: null,
      createdAt: now,
      updatedAt: now,
      archivedAt: null,
      settledOverride: null,
      settledAt: null,
      lastVisitedAt: null,
      deletedAt: null,
    } satisfies OrchestrationV2AppThread,
    threadId: THREAD_ID,
    runId: RunId.make(turn.runId),
    runOrdinal: turn.ordinal,
    providerTurnOrdinal: turn.ordinal,
    attemptId: RunAttemptId.make(turn.attemptId),
    rootNodeId: NodeId.make(`node-${turn.ordinal}`),
    providerThread: input.providerThread,
    message: {
      createdBy: turn.from,
      creationSource: turn.from === "user" ? "web" : "provider",
      messageId: MessageId.make(`message-${turn.ordinal}`),
      text: turn.prompt ?? "",
      attachments: [],
    },
    modelSelection: input.modelSelection,
    runtimePolicy: input.runtimePolicy,
  };
}

type TurnItem = Extract<AdapterEvent, { type: "turn_item.updated" }>["turnItem"];
type RuntimeRequest = Extract<AdapterEvent, { type: "runtime_request.updated" }>["runtimeRequest"];
type ProviderTurn = Extract<AdapterEvent, { type: "provider_turn.updated" }>["providerTurn"];
type Question = Extract<TurnItem, { type: "user_input_request" }>["questions"][number];

function eventsOf(entries: ReadonlyArray<LoggedEvent>, session?: number): Array<AdapterEvent> {
  return entries.flatMap((entry) =>
    session === undefined || entry.session === session ? [entry.event] : [],
  );
}

function terminalOf(entries: ReadonlyArray<LoggedEvent>, turn: StartedTurn) {
  return eventsOf(entries, turn.session).find(
    (event): event is Extract<AdapterEvent, { type: "turn.terminal" }> =>
      event.type === "turn.terminal" && event.runOrdinal === turn.ordinal,
  );
}

function providerTurnOf(
  entries: ReadonlyArray<LoggedEvent>,
  turn: StartedTurn,
): ProviderTurn["id"] | undefined {
  for (const event of eventsOf(entries, turn.session)) {
    if (event.type === "provider_turn.updated" && event.providerTurn.runAttemptId === turn.attemptId) {
      return event.providerTurn.id;
    }
  }
  return undefined;
}

function lastNativeThread(entries: ReadonlyArray<LoggedEvent>, session: number): string | undefined {
  let native: string | undefined;
  for (const event of eventsOf(entries, session)) {
    if (event.type === "provider_thread.updated") {
      native = event.providerThread.nativeThreadRef?.nativeId ?? native;
    }
  }
  return native;
}

type AnswerableRequest =
  | { readonly kind: "approval"; readonly requestId: RuntimeRequest["id"] }
  | {
      readonly kind: "question";
      readonly requestId: RuntimeRequest["id"];
      readonly questions: ReadonlyArray<Question>;
    };

/** The first pending request of `session` the harness has not claimed, once it can be answered. */
function nextAnswerableRequest(
  entries: ReadonlyArray<LoggedEvent>,
  session: number,
  claimed: ReadonlySet<string>,
): AnswerableRequest | undefined {
  const events = eventsOf(entries, session);
  const requests = latestBy(
    events.flatMap((event) => (event.type === "runtime_request.updated" ? [event.runtimeRequest] : [])),
    (request) => request.id,
  );
  for (const request of requests) {
    if (request.status !== "pending" || claimed.has(request.id)) continue;
    if (request.kind !== "user_input") return { kind: "approval", requestId: request.id };
    // A question is answered once its item, which holds the questions, has come too.
    const item = latestBy(
      events.flatMap((event) => (event.type === "turn_item.updated" ? [event.turnItem] : [])),
      (turnItem) => turnItem.id,
    ).find(
      (turnItem): turnItem is Extract<TurnItem, { type: "user_input_request" }> =>
        turnItem.type === "user_input_request" && turnItem.requestId === request.id,
    );
    if (item !== undefined) {
      return { kind: "question", requestId: request.id, questions: item.questions };
    }
  }
  return undefined;
}

function firstOptions(questions: ReadonlyArray<Question>): Record<string, string> {
  return Object.fromEntries(
    questions.map((question) => [question.id, question.options[0]?.label ?? ""]),
  );
}

/** Turn item types that show a tool call. */
const TOOL_ITEM_TYPES = new Set([
  "command_execution",
  "file_change",
  "file_search",
  "web_search",
  "dynamic_tool",
]);

/** The tool call a runtime request asks about: the request names it by its native id. */
function toolOfRequest(
  entries: ReadonlyArray<LoggedEvent>,
  session: number,
  requestId: string,
): TurnItem | undefined {
  const events = eventsOf(entries, session);
  const request = latestBy(
    events.flatMap((event) => (event.type === "runtime_request.updated" ? [event.runtimeRequest] : [])),
    (candidate) => candidate.id,
  ).find((candidate) => candidate.id === requestId);
  const nativeId = request?.nativeRequestRef?.nativeId;
  if (nativeId == null) return undefined;
  return latestBy(
    events.flatMap((event) => (event.type === "turn_item.updated" ? [event.turnItem] : [])),
    (item) => item.id,
  ).find((item) => TOOL_ITEM_TYPES.has(item.type) && item.nativeItemRef?.nativeId === nativeId);
}

/**
 * How many answers the client sent in a recording: Claude's permission answers (a
 * `control_response` with a `behavior`), ACP's answers to the agent's requests (a response with
 * an `outcome`). A line still being written is skipped; the next change reads it whole.
 */
function clientAnswersIn(recording: string): number {
  let count = 0;
  for (const line of NodeFS.readFileSync(recording, "utf8").split("\n")) {
    if (line.trim() === "") continue;
    let record: { kind?: string; from?: string; json?: Record<string, any> };
    try {
      record = JSON.parse(line);
    } catch {
      continue;
    }
    const frame = record.json;
    if (record.kind !== "frame" || record.from !== "client" || frame === undefined) continue;
    const claudeAnswer = frame.type === "control_response" && frame.response?.response?.behavior !== undefined;
    const acpAnswer = frame.method === undefined && frame.id !== undefined && frame.result?.outcome !== undefined;
    if (claudeAnswer || acpAnswer) count += 1;
  }
  return count;
}

/** Waits until the recording holds `count` client answers, watching the file for changes. */
const untilRecordingHolds = (recording: string, count: number): Effect.Effect<void, HarnessError> =>
  onWallClock(
    Effect.callback<void, HarnessError>((resume) => {
      const holds = () => clientAnswersIn(recording) >= count;
      if (holds()) {
        resume(Effect.void);
        return;
      }
      const watcher = NodeFS.watch(recording);
      const check = () => {
        if (!holds()) return;
        watcher.close();
        resume(Effect.void);
      };
      watcher.on("change", check);
      // Again once watched: a line written between the first look and the watch is seen here.
      check();
      return Effect.sync(() => watcher.close());
    }).pipe(
      Effect.timeoutOrElse({
        duration: WAIT_LIMIT,
        orElse: () =>
          Effect.fail(
            new HarnessError(`waited ${WAIT_LIMIT} for answer ${count} to reach the recording`),
          ),
      }),
    ),
  );

/**
 * Drives `scenario` on the adapter that starts `executable`, in `scratch`'s project and home,
 * on the harness's runtime; returns the thread a person sees.
 */
export const driveScenario = (
  agent: Agent,
  scenario: Scenario,
  executable: string,
  scratch: Scratch,
  recordingPath?: string,
): Effect.Effect<ThreadView, HarnessError> =>
  inHarnessRuntime(
    Effect.gen(function* () {
      const harness = yield* buildAdapter(agent, executable, scratch.workspace, scratch.home);
      // The adapter's own events, kept with the scratch folder after a failure or with
      // PLUTO_HARNESS_KEEP=1, to read when a run or a view is not what was expected.
      const events = NodePath.join(scratch.root, "events.jsonl");
      const run = yield* runScenario(agent, scenario, harness, recordingPath, events);
      return threadView(run, scratch);
    }),
    scratch.home,
  );

// --- The thread a person sees ------------------------------------------------------------------

/** One item of a turn, as a person sees it. */
export type ViewItem =
  | { readonly kind: "assistant"; readonly text: string }
  | ViewTool
  | {
      readonly kind: "approval";
      /** What it asks for: `command`, `file-change`, ... */
      readonly request: string;
      /** The tool call it asks about, by the `id` its `tool` item has. */
      readonly toolCall: string | null;
      readonly status: string;
      /** The decision as the adapter reported it; `null` when it reported none. */
      readonly answer: string | null;
    }
  | {
      readonly kind: "question";
      readonly questions: ReadonlyArray<{
        readonly id: string;
        readonly question: string;
        readonly options: ReadonlyArray<string>;
      }>;
      readonly status: string;
      /** The answers as the adapter reported them; `null` when it reported none. */
      readonly answer: Record<string, string> | null;
    }
  | { readonly kind: "notice"; readonly type: string; readonly text: string | null };

/** A tool call, with what a wrong mapping of the CLI's frames would change. */
export interface ViewTool {
  readonly kind: "tool";
  /** The call, named by its order of first appearance: `tool-1`. */
  readonly id: string;
  readonly name: string;
  readonly input: unknown;
  readonly status: string;
  readonly output: string | null;
  /**
   * By type: a command's exit code and failure flag, a file change's diff and counts, a search's
   * results; `null` for other tools.
   */
  readonly details: Record<string, unknown> | null;
}

export interface ViewTurn {
  readonly turn: number;
  readonly from: "user" | "agent";
  readonly prompt: string | null;
  readonly outcome: string;
  readonly items: ReadonlyArray<ViewItem>;
}

export interface ViewSession {
  readonly session: number;
  readonly resumed: boolean;
  readonly nativeThread: string | null;
  readonly turns: ReadonlyArray<ViewTurn>;
}

export interface ThreadView {
  readonly agent: Agent;
  readonly scenario: string;
  readonly sessions: ReadonlyArray<ViewSession>;
}

/** Item types a person does not see as such: reasoning, and what the turn header already says. */
const UNSEEN_ITEM_TYPES = new Set(["reasoning", "user_message", "checkpoint"]);

/**
 * The thread as a person would see it, from the final state of each item: per session, per
 * turn, its outcome and its items in order. No reasoning, no deltas, no timing. Ids become their
 * order of first appearance. Then `pluto-record mask-document` replaces the run's folders and any
 * secret, so that a live run and its replay compare equal.
 */
export function threadView(run: ScenarioRun, places: { workspace: string; home: string }): ThreadView {
  const ids = new IdNames();
  const view: ThreadView = {
    agent: run.agent,
    scenario: run.scenario,
    sessions: run.sessions.map((session) => ({
      session: session.session,
      resumed: session.resumed,
      nativeThread: session.nativeThread === null ? null : ids.name("thread", session.nativeThread),
      turns: run.turns
        .filter((turn) => turn.session === session.session)
        .map((turn) => viewTurn(run, turn, ids)),
    })),
  };
  return maskDocument(view, places) as ThreadView;
}

/** Names ids by their order of first appearance, per kind: `thread-1`, `tool-2`, `question-1`. */
class IdNames {
  private readonly names = new Map<string, string>();
  private readonly counts = new Map<string, number>();

  name(kind: string, id: string): string {
    const key = `${kind}\u0000${id}`;
    const known = this.names.get(key);
    if (known !== undefined) return known;
    const count = (this.counts.get(kind) ?? 0) + 1;
    this.counts.set(kind, count);
    const name = `${kind}-${count}`;
    this.names.set(key, name);
    return name;
  }
}

function viewTurn(run: ScenarioRun, turn: StartedTurn, ids: IdNames): ViewTurn {
  const events = eventsOf(run.events, turn.session);
  const requests = new Map(
    latestBy(
      events.flatMap((event) => (event.type === "runtime_request.updated" ? [event.runtimeRequest] : [])),
      (request) => request.id,
    ).map((request) => [request.id, request] as const),
  );
  const turnItems = latestBy(
    events.flatMap((event) =>
      event.type === "turn_item.updated" && event.turnItem.runId === turn.runId ? [event.turnItem] : [],
    ),
    (item) => item.id,
  );
  const toolNativeIds = new Set(
    turnItems.flatMap((item) =>
      TOOL_ITEM_TYPES.has(item.type) && item.nativeItemRef?.nativeId != null
        ? [item.nativeItemRef.nativeId]
        : [],
    ),
  );
  const items = turnItems
    .map((item, appearance) => ({ item, appearance }))
    .sort((left, right) => left.item.ordinal - right.item.ordinal || left.appearance - right.appearance)
    .flatMap(({ item }) => viewItem(item, requests, toolNativeIds, ids));
  const outcome = terminalOf(run.events, turn)?.status ?? "unfinished";
  return {
    turn: turn.ordinal,
    from: turn.from,
    prompt: turn.prompt,
    outcome,
    items: STOPPED_OUTCOMES.has(outcome) ? items.map(stoppedTool) : items,
  };
}

/** Turn outcomes after which a tool that did not complete was stopped by the person. */
const STOPPED_OUTCOMES = new Set(["interrupted", "cancelled"]);
/** What a tool that did not complete may end as, once its turn is stopped. */
const STOPPED_TOOL_STATUSES = new Set(["failed", "cancelled", "interrupted"]);

/**
 * A tool its stopped turn cut short: `stopped`, without output or details. Whether the adapter
 * read the CLI's own report of the cut (`failed`, with the CLI's notice as output) before the
 * interrupt closed the CLI, or ended the tool itself (`interrupted`), is a race inside the
 * adapter, which a replay does not reproduce; the person sees a stopped tool either way.
 */
function stoppedTool(item: ViewItem): ViewItem {
  return item.kind === "tool" && STOPPED_TOOL_STATUSES.has(item.status)
    ? { ...item, status: "stopped", output: null, details: null }
    : item;
}

/** The view of one turn item, from what the adapter reported only. */
function viewItem(
  item: TurnItem,
  requests: ReadonlyMap<string, RuntimeRequest>,
  toolNativeIds: ReadonlySet<string>,
  ids: IdNames,
): Array<ViewItem> {
  if (UNSEEN_ITEM_TYPES.has(item.type)) return [];
  const tool = (name: string, input: unknown, output: string | null, details: ViewTool["details"]) => [
    {
      kind: "tool" as const,
      id: ids.name("tool", item.nativeItemRef?.nativeId ?? item.id),
      name,
      input: input ?? null,
      status: item.status,
      output,
      details,
    },
  ];
  switch (item.type) {
    case "assistant_message":
      return item.text.trim() === "" ? [] : [{ kind: "assistant", text: item.text }];
    case "command_execution":
      return tool(item.title ?? "command", item.input, item.output ?? null, {
        exitCode: item.exitCode ?? null,
        outputIndicatesFailure: item.outputIndicatesFailure ?? null,
      });
    case "file_change":
      return tool(item.title ?? "file change", item.fileName, item.newStr ?? null, {
        diff: item.diffStr ?? null,
        changes: item.changes ?? null,
        additions: item.additions ?? null,
        deletions: item.deletions ?? null,
      });
    case "file_search":
      return tool(item.title ?? "file search", item.pattern ?? null, null, {
        results: item.results ?? null,
      });
    case "web_search":
      return tool(item.title ?? "web search", item.patterns ?? null, null, {
        results: item.results ?? null,
      });
    case "dynamic_tool":
      return tool(
        item.toolName ?? item.title ?? "tool",
        item.input,
        item.output === undefined ? null : outputText(item.output),
        null,
      );
    case "approval_request": {
      const request = requests.get(item.requestId);
      const toolNativeId = request?.nativeRequestRef?.nativeId;
      return [
        {
          kind: "approval",
          request: item.requestKind,
          toolCall:
            toolNativeId != null && toolNativeIds.has(toolNativeId)
              ? ids.name("tool", toolNativeId)
              : null,
          status: item.status,
          answer: request?.decision ?? null,
        },
      ];
    }
    case "user_input_request": {
      const answers = requests.get(item.requestId)?.answers as Record<string, unknown> | undefined;
      return [
        {
          kind: "question",
          questions: item.questions.map((question) => ({
            id: ids.name("question", question.id),
            question: question.question,
            options: question.options.map((option) => option.label),
          })),
          status: item.status,
          answer:
            answers === undefined
              ? null
              : Object.fromEntries(
                  Object.entries(answers).map(([id, value]) => [
                    ids.name("question", id),
                    outputText(value),
                  ]),
                ),
        },
      ];
    }
    default: {
      const text =
        "message" in item && typeof item.message === "string"
          ? item.message
          : "failure" in item && item.failure !== null && typeof item.failure === "object"
            ? String((item.failure as { message?: unknown }).message ?? "")
            : null;
      return [{ kind: "notice", type: item.type, text }];
    }
  }
}

function outputText(value: unknown): string {
  if (typeof value === "string") return value;
  if (Array.isArray(value)) return value.map(outputText).join(", ");
  return JSON.stringify(value);
}

/** Passes a document through `pluto-record mask-document`: folders and secrets, as recorded. */
export function maskDocument(document: unknown, places: { workspace: string; home: string }): unknown {
  const result = NodeChildProcess.spawnSync(
    requiredEnv("PLUTO_RECORD_BIN"),
    ["mask-document", "--workspace", places.workspace, "--home", places.home],
    // A view of 64 MiB comes back larger, indented: the default 1 MiB would cut it.
    { input: JSON.stringify(document), encoding: "utf8", maxBuffer: 256 * 1024 * 1024 },
  );
  if (result.status !== 0) {
    throw new HarnessError(`pluto-record mask-document failed: ${result.stderr || result.error}`);
  }
  return JSON.parse(result.stdout);
}

// --- Recordings and the fake agent's state -----------------------------------------------------

/** `<dir>/<agent>/<name>.jsonl` for every agent folder that holds some. */
export function recordingsIn(dir: string): Array<{ agent: Agent; scenario: string; path: string }> {
  const found: Array<{ agent: Agent; scenario: string; path: string }> = [];
  for (const agent of AGENTS) {
    const agentDir = NodePath.join(dir, agent);
    if (!NodeFS.existsSync(agentDir)) continue;
    for (const file of NodeFS.readdirSync(agentDir).sort()) {
      if (!file.endsWith(".jsonl")) continue;
      found.push({ agent, scenario: file.slice(0, -".jsonl".length), path: NodePath.join(agentDir, file) });
    }
  }
  return found;
}

/** `x.jsonl`'s reference, `x.thread.json`. */
export function referenceOf(recording: string): string {
  return recording.replace(/\.jsonl$/, ".thread.json");
}

/**
 * A reference's file: the thread view under a format and a version. A change to what
 * `threadView` keeps raises the version; the references of an older version then need a
 * migration here, or new recordings by the owner, since a reference comes from a live run and is
 * never rebuilt from its recording.
 */
const REFERENCE_FORMAT = "pluto-thread-reference";
const REFERENCE_VERSION = 1;

export function writeReference(path: string, view: ThreadView): void {
  const file = { format: REFERENCE_FORMAT, version: REFERENCE_VERSION, thread: view };
  NodeFS.writeFileSync(path, `${JSON.stringify(file, null, 2)}\n`);
}

export function readReference(path: string): ThreadView {
  const file = JSON.parse(NodeFS.readFileSync(path, "utf8")) as {
    format?: unknown;
    version?: unknown;
    thread?: ThreadView;
  };
  if (file.format !== REFERENCE_FORMAT) {
    throw new HarnessError(`${path} is not a thread reference: its format is ${String(file.format)}`);
  }
  if (file.version !== REFERENCE_VERSION) {
    throw new HarnessError(
      `${path} is a thread reference of version ${String(file.version)}; this harness reads version ${REFERENCE_VERSION}`,
    );
  }
  if (file.thread === undefined) {
    throw new HarnessError(`${path} is a thread reference without its thread`);
  }
  return file.thread;
}

/** How many processes the recording spawned. */
export function spawnCount(recording: string): number {
  return NodeFS.readFileSync(recording, "utf8")
    .split("\n")
    .filter((line) => line.trim() !== "")
    .slice(1)
    .filter((line) => (JSON.parse(line) as { kind?: string }).kind === "spawn").length;
}

/**
 * Checks the fake agent's state folder: one `process-<n>.json` per recorded spawn, each
 * complete. Returns what is wrong, or nothing.
 */
export function replayProblems(recording: string, state: string): Array<string> {
  const expected = spawnCount(recording);
  const files = NodeFS.existsSync(state)
    ? NodeFS.readdirSync(state).filter((file) => /^process-\d+\.json$/.test(file))
    : [];
  const problems: Array<string> = [];
  if (files.length !== expected) {
    problems.push(`${files.length} status files for ${expected} recorded processes`);
  }
  for (let process = 0; process < expected; process += 1) {
    const path = NodePath.join(state, `process-${process}.json`);
    if (!NodeFS.existsSync(path)) {
      problems.push(`process ${process} left no status file`);
      continue;
    }
    const status = JSON.parse(NodeFS.readFileSync(path, "utf8")) as {
      complete: boolean;
      consumed: number;
      total: number;
      waiting_for: string | null;
      error: string | null;
    };
    if (!status.complete) {
      problems.push(
        `process ${process}: ${status.consumed} of ${status.total} records consumed, waiting for ${status.waiting_for ?? "nothing"}${status.error === null ? "" : `: ${status.error}`}`,
      );
    }
  }
  return problems;
}
