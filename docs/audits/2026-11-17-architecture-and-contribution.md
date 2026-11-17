# Audit of 2026-11-17: architecture, contribution, and what to decide early

An audit on the owner's demand, as the owner's method describes audits, before
Stage 1's audit and M2.0. Its question: is Pluto built and planned clearly and solidly enough for
contributors to work well, and for the project to evolve for years without debt that is hard to
correct?

## Scope

- **Dimensions**: architecture and evolvability, stability and recovery, data safety, security
  and privacy, contribution, documentation, and the cross-cutting needs that cost most when added
  late.
- **Code**: the repository at `cf06d24`, read only; the owner's agents' module and the method's
  files, read only. The fork's code, the Android app's Kotlin, uwui and `pluto-core`'s internals
  were not examined.
- **How**: static reading; no build, test or program was run during the audit. Two reviewers, read
  only: a web researcher for primary sources, and a challenger on the strongest model, who attacked
  the draft recommendations. Every finding below was checked against the line it cites.
- **What exists**: the frozen egui client, `pluto-core` and `pluto-ffi`, and Stage 1's tools. The
  new foundation (`pluto-log`, `pluto-agent`, Pluto's server, the protocol's crate) is planned
  only, so what is said of it is about its contracts.

## Verdict

- **Today's code** is mostly frozen and leaves at Stage 5. Stage 1's tools are good: the
  recordings' format is versioned from its first line (`crates/pluto-recording/src/format.rs`),
  tests replay recorded sessions, and the rivals' bench runs each subject in its own unit.
- **The direction** is sound on its large choices: a process per session, an append-only log as
  the truth, the method's mechanisms native and the method in files, the vault apart from the
  sandbox, the bench as each component's gate. M2.0's list already names most traps other
  harnesses paid for late. It is a list of principles, not yet contracts: one line is wrong as
  written, two decisions contradicted each other, and three gaps are uncovered.
- **Contribution** is weak in practice: one author, CI that does not run on pull requests, a
  license that does not match the crates, no `CONTRIBUTING.md` or `SECURITY.md`, a check that builds
  the egui client, a flake for `x86_64-linux` alone, and large documents. Nothing needs a paid
  account or a secret, except the replay into the fork.
- **The largest risks**: who is the sessions' parent and who may approve (decided since, ADR-0043);
  a log format that would freeze without effects' intents and outcomes, critical entries, or
  idempotency keys; the fork pushed nowhere and the owner's history without a backup.
- **Not concluded by reading**: the new foundation's performance; an `fsync`'s cost on the HP's
  disk; systemd without a user manager, in CI or by SSH; what Codex's own tools can reach.

## Strengths to keep

- One process per session and a log on disk as its truth
  ([ADR-0033](../adr/0033-one-pluto-agent-process-per-session.md)): the split Anthropic made late,
  and the one Codex lacks today.
- Formats versioned from their first line, already practised in the recordings.
- Checks without a real provider: the fake model on four wire formats, masked recordings, a stamp
  CI compares.
- Mechanisms native, the method in files ([ADR-0034](../adr/0034-the-methods-mechanisms-native.md)).
- The bench as the gate of every component, and no component kept without its gain.
- CI with actions pinned by digest, `cargo deny`, links checked offline, a weekly run.
- The providers' terms written into the decisions ([ADR-0031](../adr/0031-three-kinds-of-engine.md)).

## Findings

Ranked by severity. *Status* says whether it is a present defect, known debt, a planned decision,
a missing specification or a hypothesis.

1. **The sessions' supervision broke the restart guarantee.** ADR-0033 had the server supervise
   sessions and ADR-0042 the command line; the fork's unit has `Restart=always` and systemd's
   default `KillMode=control-group`, which kills a unit's whole control group. A `pluto-agent`
   started by a lead agent under the fork would die at the fork's restart. Status: a planned
   decision with a contradiction. **Decided**: [ADR-0043](../adr/0043-each-session-in-its-own-unit.md).
2. **Agents could approve what only a person may.** ADR-0042 let the command line answer approvals
   "from an agent"; [ADR-0016](../adr/0016-uwuis-decisions-of-2026-11-07-taken-by-pluto.md) forbids
   offering agents what only a person may do; agents run as the owner. Status: contradiction.
   **Decided**: ADR-0043; the channel that tells a person from an agent is designed at Stage 3.
3. **Effects in flight are not in the log's plan.** M2.1 lists torn writes and one writer, not the
   order between an effect and its record. A log already written cannot gain it. Before M2.1's
   format: an intent entry before each effect Pluto runs, an outcome entry after, paired by the
   call's id; on resume, an outcome "interrupted, unknown" is appended and nothing is run again
   automatically; an approval is bound to the call and the hash of its typed action, used once.
   Effects an engine runs are recorded as reported, the guarantee shown as absent. Its cost, an
   `fsync` before each tool call, is measured in M2.1's append time on the HP. Status: missing
   specification.
4. **"Unknown variants ignored" would ignore a revocation.** PLAN's M2.0 line would let a reader
   skip a cancel, a permission or a budget it does not know. RFC 5280 rejects an unknown *critical*
   extension and may ignore others ([§4.2](https://www.rfc-editor.org/rfc/rfc5280#section-4.2)).
   The writer marks each entry's class in its envelope; a viewer shows an unknown entry as unknown
   and keeps it verbatim; an enforcer refuses an unknown critical one; an agent that resumes must
   know every entry. serde's `#[serde(other)]` drops the content, so a raw variant is needed. The
   corpus gains entries from the future. PLAN puts the version on every message, ADR-0005 at
   connection: to settle. Status: planned, wrongly worded.
5. **No idempotency key, no sequence, and Stage 3's channel unplaced.** `pluto-core` draws a new
   `commandId` for each command (`crates/pluto-core/src/commands.rs`) and only reports a slow one.
   M2.1 adds a client-chosen id kept with each entry and a gapless sequence per session. At Stage 3
   the command line follows a session by reading its log; approvals, steering and interrupts go
   through the protocol crate's agent part, versioned, and Stage 5 keeps only the network's
   transport and encoding. Status: missing specification; a contradiction of dates with the
   ROADMAP's open decision on the protocol.
6. **License and the way in.** `LICENSE` holds only T3's MIT text; `NOTICE.md` calls Pluto MIT;
   the root crate, `pluto-core`, `pluto-ffi` and `uniffi-bindgen` declare MIT, the four tool crates
   MIT OR Apache-2.0. [ADR-0035](../adr/0035-the-sandbox-and-the-vault.md) plans to copy Codex's
   sandbox code, under Apache-2.0, which cannot be offered under MIT. CI runs on `push` and a
   schedule, not on `pull_request`. Before the first outside contribution, or the first line copied
   from Codex: both license texts with Pluto's own copyright line; the frozen crates stay MIT; the
   sandbox written after reading Codex's, or in a crate under Apache-2.0 alone; a DCO; CI on pull
   requests, never `pull_request_target` with secrets. Status: known debt, misstated; two gaps.
7. **The abstraction rule blocks M2.0.** `AGENTS.md` says "no abstraction without a second concrete
   use"; the owner's rules now accept reuse, a boundary around an unstable dependency, a protected
   invariant or a test. M2.0 plans an injected clock and a platform boundary with one platform;
   `rmcp` went from 1.0 to 3.0 in five months and `agent-client-protocol` from 1.0 to 5.0 in four
   and a half ([crates.io](https://crates.io/crates/agent-client-protocol/versions)). To align before
   M2.0, on the owner's go. Status: planned, too late.
8. **No one way to write a file durably.** Seven write-then-rename sites, none with an `fsync`,
   several ignoring the error: `src/ui/drafts.rs`, `src/app.rs` (three), `src/ui/stash.rs`,
   `src/ui/settings.rs`, `crates/pluto-ffi/src/prefs.rs`. The client's settings carry no version.
   For the new foundation: one function (temporary file, `fsync`, rename, `fsync` of the folder,
   typed errors), the others refused by clippy's `disallowed_methods`; a failed `fsync` never
   retried into a success. Its first user is M2.3's results files. Status: a present defect in
   frozen code, low impact ([`debt.md`](../debt.md)).
9. **The project's memory reads well to its owner, less to a newcomer.** PLAN cited a local folder
   git does not hold; [ADR-0009](../adr/0009-secrets-privacy-and-the-sandbox.md) is amended by six
   ADRs; `features.md`, `measurements.md`, `specification.md`, the ROADMAP and the README are each
   over 4,000 words; the quality bar lacks the change and failure scenarios the method now asks
   for. Status: partly known; the consolidation is planned for Stage 1's audit.
10. **The bench's network for real models is undefined.** "No network but the model's endpoint"
    needs more than bubblewrap. On the fake model, the rivals' empty network namespace is enough; a
    local Ollama needs one forwarded port; M2.5's frontier runs need an egress allowlist that never
    decrypts TLS. M2.3 says which kind of run gets which network, without building Stage 3's
    sandbox. Status: missing specification.
11. **The fork exists on one disk, and the history has no backup.** The owner's NixOS builds the
    fork from a local path, and the owner's delegation runs through it. Status: urgent, already in
    the PLAN; the owner's to do.

## Decisions to take early

- **Before the first crate**: the workspace split (mechanical); the execution model per binary,
  the core doing no I/O; no third-party type in a persisted format or the protocol, checked with
  `cargo deny`'s `wrappers` (as Codex does for `reqwest`) and a direction check on
  `cargo metadata`; typed errors with stable codes in reserved ranges, which the interface later
  translates; the durable-write function; the workspace's lints and a toolchain file.
- **Before M2.1, the first persistent format**: the data folders and the public name (the logs in
  XDG's data, the index in its state, declared derived and rebuilt from the logs, the cache apart,
  one root that a test profile can move); the log's fields (intents and outcomes, client ids, a
  sequence, the parent session, an entry's class, provenance, the digest of each method file read,
  a raw entry for an engine's unknown event, no automatic deletion); the index's rebuild time,
  measured on a copy of the history.
- **Before Stage 3**: the trust map, on one page (a fresh tool process per call; the sandbox
  without the session bus, keyring or systemd sockets; "tool code never sees an engine's
  credentials" holds only where Pluto runs the engine's tools); the command line's channel; a
  structured table of roles proposed to the owner, whose table is prose today.
- **Before the first outside contribution**: finding 6, and a private way to report a
  vulnerability.
- **Deferred, with their trigger**: a client's lag signal (the server, Stage 5); WebAssembly
  extensions (a second concrete use); the sandbox off Linux (going public); a thread continued in
  the fork after its import (the import's milestone, Stage 5).
- **Cross-cutting needs**: accessibility and IME are in [`uwui-needs.md`](../uwui-needs.md);
  bidirectional text is not, and costs most to add late in a text engine: to ask of uwui now.
  Translation: typed error codes now, a catalogue of texts with uwui's first screen. Offline,
  remote clients and versions: findings 4 and 5. Observability: planned. An update that fails: an
  older Pluto refuses to write newer data, and the backup is restored.

## Change and failure scenarios

Traced statically; none was run.

| Scenario | What carries it | What is still unknown |
|---|---|---|
| A provider with other capabilities | one of four provider clients, the table of quirks as data, the profile; a missing capability shown as absent | the quirks table's schema |
| A vendor CLI breaks | the engine's adapter; a supported range of versions; unknown events kept raw | the CLIs are not pinned in the Nix shell (debt) |
| A tool that needs approval | a typed action and its class; intent and outcome; a process per call | the channel a person approves through (ADR-0043) |
| The method or the routing changes | files read at a task's start, their digests logged; the strictest restriction wins | Pluto would read a Markdown table of prose |
| A structuring dependency replaced | no third-party type in formats; `wrappers` | none, once the rule holds |
| The log evolves, old sessions, other clients | never migrated in place; readers of every version; an older writer refuses a newer log | — |
| A migration interrupted | logs not migrated; the index rebuilt; settings backed up and written atomically; the import resumable by the fork's thread id | the rebuild's time |
| The server restarts during a turn | each session in its own unit (ADR-0043); the server finds it by its folder | lingering; CI's user manager |
| The connection drops after an accepted action | the client's id, deduplication, the sequence | — |
| A delegation cancelled with children | one slice per tree; a stop entry in each log; time limits; orphans found by their parent and lock | an engine's own subagents |
| Ten times the events a client can take | the log as buffer, read from a position; a bounded buffer and a lag signal at Stage 5 | the storage budget against streamed deltas (M2.1) |
| Two contributors at once | changelog fragments and a generated ADR index; a crate per seam | the PLAN and the ADR numbers are shared; the fork's stamp is the owner's alone |

## The owner's agents' module and the method

- **The method's changes of 2026-11-16** put architecture into the bar: abstraction criteria,
  lifetimes and limits, contracts that evolve deliberately, an ADR's validation and
  reconsideration, an ARCHITECTURE with invariants and ownership, limits and recovery, and
  evolution constraints, and one ADR per topic rather than a chain of amendments. The copies the
  agents read were compared with the method's sources and matched.
- **For Pluto**: the quality bar names a few change and failure scenarios (the table above is
  their pool); the target in ARCHITECTURE gets the three new sections; only the consolidated ADRs
  get a validation section.
- **Rules as inputs**: the method reaches each agent by a different route, with no version, so
  Pluto records each file's digest and reads the files once per task. "Stricter, never looser"
  becomes a mechanism where the sandbox can hold it.
- **The agents' isolation guards against accidents, not against a program set on escaping**, as
  it says itself: it unsets the display's variables and gives a runtime folder of its own, but
  links the session bus, systemd, the keyring and gcr, and leaves the home and the network open.
  Pluto may reuse its idea (an allowlist of the environment, a runtime folder per session) and
  its headless runner for tests; it guarantees the rest itself.
- **Nix stays optional**: XDG defaults, a Nix module that only sets them, the engines' CLIs pinned
  in the flake for tests and checked by version on the PATH without it.

## Contradictions found

- `AGENTS.md`'s abstraction rule against the owner's rules (finding 7).
- ADR-0042 against ADR-0016 on approvals; ADR-0033 and ADR-0042 against the restart guarantee
  (both closed by ADR-0043).
- PLAN's version on every message against ADR-0005's at connection (finding 4).
- PLAN's statement of the crates' licenses (finding 6).
- The ROADMAP's protocol at Stage 5 against Stage 3's channel (finding 5).
- PLAN's reference to a local folder against `AGENTS.md`'s documentation rule (finding 9).
- ADR-0020's Umbriel, already known.

## What was decided and done

- The owner chose each session in its own unit, and agents approving only what their brief grants:
  [ADR-0043](../adr/0043-each-session-in-its-own-unit.md).
- The method's tooling for documents, waiting uncommitted since 2026-11-16, was brought to the
  method's latest templates and committed: the ADRs' index written and checked, changelog
  fragments, the ADR template, and the scripts' tests. Its changelog test assumed a project without
  a *Fixed* section and was corrected here.
- The rest is planned in [`PLAN.md`](../PLAN.md), each item with its milestone.

## Checks left to run

On test data and test profiles: a session's parent unit restarted during a tool call; an agent
killed between an intent and its outcome; a disk filled during an append; an append's time with
`fsync` on the HP; a user manager on GitHub's runner; lingering on and off; following a session by
inotify; the corpus of entries from the future; `Origin` and DNS rebinding against the local
server; an agent's approval outside its brief refused; the bus, keyring and systemd unreachable
from the sandbox; the index rebuilt and the import replayed on a copy of the history; the
platform-free crates on Windows and macOS; the newcomer test.

## Sources

- [systemd.kill(5)](https://www.freedesktop.org/software/systemd/man/latest/systemd.kill.html),
  systemd 262: `KillMode`'s default and advice.
- Codex: [#40969](https://github.com/openai/codex/issues/40969) (turns killed at the daemon's
  update, open), [#51201](https://github.com/openai/codex/issues/51201) (no event for a cut turn),
  its [app-server's README](https://github.com/openai/codex/blob/main/codex-rs/app-server/README.md)
  (`thread/rollback` removed, old events still replayed), its
  [`deny.toml`](https://github.com/openai/codex/blob/main/codex-rs/deny.toml) (`wrappers`), its
  [contributing guide](https://github.com/openai/codex/blob/main/docs/contributing.md) (no outside
  code).
- OpenCode: [PR #52573](https://github.com/anomalyco/opencode/pull/52573) (a daemon's registration
  lost in `$TMPDIR`), [GHSA-vxw4-wv6m-9hhh](https://github.com/advisories/GHSA-vxw4-wv6m-9hhh)
  (an unauthenticated local server, fixed in 1.0.216).
- MCP: the [2026-07-28 changelog](https://modelcontextprotocol.io/specification/2026-07-28/changelog)
  (no handshake, version and capabilities on each request), the
  [Streamable HTTP transport](https://modelcontextprotocol.io/specification/draft/basic/transports/streamable-http)
  (`Origin` checked, loopback, DNS rebinding).
- [ACP's initialization](https://agentclientprotocol.com/protocol/initialization) (version
  negotiation).
- [RFC 5280 §4.2](https://www.rfc-editor.org/rfc/rfc5280#section-4.2) (critical extensions);
  [protobuf's proto3 guide](https://protobuf.dev/programming-guides/proto3/) (unknown fields kept).
- [Temporal's activities](https://docs.temporal.io/activity-definition) (idempotency, retries).
- Pi's [session format](https://docs.pi.dev/latest/session-format) and its agent package's
  [changelog](https://github.com/earendil-works/pi/blob/main/packages/agent/CHANGELOG.md) (v4
  `SessionRepo`, torn tails repaired atomically).
- Zed: [#54618](https://github.com/zed-industries/zed/issues/54618) (threads hidden after an
  upgrade), [#56576](https://github.com/zed-industries/zed/issues/56576) (threads keyed on a
  container's id), its [contributing guide](https://github.com/zed-industries/zed/blob/main/CONTRIBUTING.md)
  (a CLA).
