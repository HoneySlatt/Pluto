# 0025. Agents' sessions recorded and replayed

- Status: Accepted; amends [0012](0012-the-checks-and-ci.md) (a check that runs only on the owner's machine)
- Date: 2026-11-11

## Context

Stage 2's drivers must be tested without a real turn ([ADR-0006](0006-agents-drivers.md)): each
driver replays recorded sessions, and CI never starts a provider session. M1.5 delivers the
recordings: Claude's `stream-json` and one ACP agent's JSON-RPC, each replaying into today's event
mapping, that of the TypeScript fork `pluto-server`, without a difference.

What the fork holds, read on 2026-11-10 at its commit `131cb1fb4a`:

- It drives Claude through `@anthropic-ai/claude-agent-sdk` 0.3.276 (Claude Code 2.1.276). The SDK
  spawns the CLI at the path of the `binaryPath` setting and speaks `stream-json` on its stdio.
  Approvals and questions are the CLI's `can_use_tool` control requests; an interrupt is the SDK's.
  The mapping is `ClaudeAdapterV2.ts`; its output is `ProviderAdapterV2Event`.
- It drives Grok, the ACP registry's agents and Antigravity over ACP on stdio; Kimi Code and
  Mistral Vibe are native.
- From T3 it inherits scripted recorders: Claude's at the SDK's level, Grok's at the wire. It also
  has a fake ACP agent that replays the wire, and a deterministic test runtime. Its rule: the live
  run must pass a scenario's assertions, then the replay must reproduce them.
- It is pushed nowhere: its only remote is T3's upstream. Its `node_modules` weighs 3.9 GB. Pluto's
  CI cannot run it.
- Without care, Claude's traffic carries the owner's personal data. The `initialize` answer holds
  the account's e-mail and organisation. The `system/init` frame lists the owner's agents, skills,
  plugins and their paths. Grok's `initialize` holds a host name and user commands. No masking by
  shape catches them.

A first design was attacked by a challenger. It found that a reference written by the replay only
proves the replay repeats itself. It also found that recording by clicking forces the replay to
guess the owner's actions, and that line-by-line masking misses a secret split across streamed
deltas. The decisions below take its findings.

## Decision

**The sessions are scripted, and the owner runs them.** One command per scenario drives the
fork's real adapter, with the real CLI behind the recorder. The scripted actions are approving,
answering a question and interrupting at a fixed logical point, such as just after the first tool
call. The scenarios are a turn, an approval, a question, an interruption and a resume, plus
Grok's background wake. Agents never run them: they spend the owner's quota. They can be recorded
again at each release of a CLI.

**The recorder and the fake agent are a Rust crate of their own, `crates/pluto-recording`.**

- `pluto-record` sits between the server and the agent's CLI. It masks and writes each line,
  then forwards it, so that a frame never follows its own answer in the file and a killed process
  loses nothing written. The delay is that of masking one line.
- `pluto-replay-agent` plays a recording in the CLI's place. It matches each client frame by its
  kind and a few key fields, ignoring content the client owns. It learns the live values of the
  ids the client chose. It releases each agent frame once the client frames recorded before it
  have arrived. It fails fast on a mismatch or a silence.
- Masking is a module of its own, reused by Stage 3.
- The crate depends on the standard library, serde and libc, never on a UI crate.

**The file format is JSON Lines, versioned from its first line.**

- The header gives the format, its version, the agent, the protocol, the date, and the versions
  the harness knows.
- Then one record per line: a process spawned, a frame with its side and its time in
  milliseconds, a line that is not JSON, stdin closed, the process's exit, and the scripted
  actions.
- Ids the client chose, the project's folder and the home folder become placeholders when the
  file is written, so that two recordings of one scenario differ only where the agent did.
- A reader refuses a format or a version it does not know, and says so plainly.
- Beside each recording, its reference: the thread as a person would see it, taken from the live
  run.

**The ACP agent is Grok** (`grok agent stdio`): the owner's subscription, driven by the fork over
ACP, first-rank in ADR-0006. It is likely Stage 2's first ACP agent, without binding it.

**The replay reaches the fork through a harness kept in Pluto, guarded in CI by a stamp.**

- A small TypeScript harness is Pluto's own code. It imports the fork's adapter by its path and
  copies none of T3's sources. The fork's own test runner runs it from outside the fork, with its
  caches kept outside the fork.
- The same harness records, with the real CLI behind `pluto-record`, and replays, with
  `pluto-replay-agent` in the CLI's place. Both run on the fork's deterministic runtime.
- A scenario's hand-written assertions pass on the live run, and its replay must give the live
  run's reference exactly. Every recorded frame must be consumed.
- `scripts/check.sh --fork` runs these replays on the owner's machine, like `--apk`, and writes a
  stamp: the hashes of what a replay reads (the recordings, their references, the crate's code,
  the harness and its script, and the crate's resolved dependencies, as
  `scripts/fork-replay.sh` lists them), with the fork's commit and the SDK's version.
- CI computes the same hashes and fails on any difference. A change to any of these files
  therefore needs a new local run.
- CI also scans every recording for secrets and personal data, and replays each one through the
  fake agent with a client of its own, which chooses other ids. It shows the recordings do not
  depend on the fork's ids. It does not show that any client's prompt matches: Grok's prompt
  frame holds the fork's own instructions, compared whole. Which part of it a Rust driver must
  send is decided with Stage 2's ACP driver.

**Personal data is kept out in three layers.**

1. Prevention: the agent runs with a disposable home, where the owner signs in once to Claude and
   to Grok. Its environment is empty but for a short allowed list, and it works on a synthetic
   project that holds no secret.
2. Stripping, frame type by frame type: account fields, the lists of the owner's agents, skills,
   plugins and commands, memory files, the host's name.
3. Masking by shape, on decoded string values: tokens and keys of known forms, the values of
   secret-named keys, paths of the home and the project, user and host names inside paths. A
   value masked once is masked everywhere after.

The check scans every file again, with detectors of its own: e-mail addresses, home paths in
plain, encoded and Claude's folder-name forms, and long random strings. It also scans each
content block's streamed deltas joined together. The owner reads a short summary of each file
before committing it.

## Alternatives considered

- **Sessions by hand in the client**: real use and less code. But the replay must guess the
  owner's actions from the frames, and an interruption races the stream. Nothing can be recorded
  again at a CLI's release.
- **The recorder in `pluto-core`**: no new crate, but it would ship in the phone's app.
- **Extending T3's recorders in the fork**: the least code, but the fork is frozen and leaves at
  Stage 3. Claude's recorder works at the SDK's level, not on the wire a Rust driver speaks.
- **Arrays in the style of asciicast**: compact, but every frame becomes an escaped string,
  unreadable when the masking is reviewed.
- **The fork's own transcripts**: they exist, but the fork defines them, and Claude's are at the
  SDK's level.
- **A binary format**: small, but unreadable, and a new dependency.
- **Gemini CLI as the ACP agent**: ACP's reference, but Google has not said whether a third party
  may drive its CLI.
- **OpenCode's ACP mode on Ollama**: costs no quota, but the fork drives OpenCode natively, so it
  is not today's path.
- **Pushing the fork to a private repository for CI**: the full check in CI and a copy of the fork
  off the disk. But a heavy install at every run, likely beyond the 20-minute budget, and a token
  for access across repositories.
- **The whole fork as a black box, driven over T3's protocol**: no TypeScript in Pluto. But each
  replay costs seconds and 400 MB or more, and the server's timers make it unstable.
- **A reference written by the replay itself**: simple, but circular.
- **The owner's real home, with masking only**: no extra sign-in. But any field the stripping list
  does not know, such as one a CLI's update adds, would reach a committed file.

## Consequences

- The check of the fork's mapping runs only on the owner's machine. CI holds it through the stamp,
  which fails when a recording, a reference, the harness or the fake agent changes without a
  local run.
- The recordings outlive the fork. Their references describe the thread a person sees, which
  Pluto's own server must reproduce in Stage 2 whatever its protocol.
- The owner signs in once to Claude and to Grok in the disposable home. Those sign-ins stay
  there, outside every repository.
- Recordings hold the text the fork sends to the agents, T3 Code's instructions (MIT, whose
  notice `NOTICE.md` keeps), as the synthetic recordings do.
- The fork stays untouched. Its test runner may write caches inside its own `node_modules` unless
  told otherwise; the harness keeps them in Pluto's `target/`.
- Masking has false positives and may miss a new shape: the scan and the owner's reading are the
  last defences.
