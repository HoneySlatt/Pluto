# Architecture decision records

An ADR records one decision: its context, the decision, the alternatives rejected and the
consequences. Once accepted, only its status changes, and its links are repointed when their target
moves. A later decision that replaces it whole makes it `Superseded by NNNN`, and the newer one
`Accepted; supersedes NNNN`; one that changes part of it makes it `Accepted; amended by NNNN`, and
the newer one `Accepted; amends NNNN (what)`.

New ADRs copy the template below and take the next free number.

| ADR | Title | Status |
|---|---|---|
| [0001](0001-a-native-client-on-egui-until-uwui.md) | A native client, on egui until uwui | Accepted; amended by 0016 and 0038 |
| [0002](0002-the-android-app-until-uwui.md) | The Android app, in Kotlin until uwui | Accepted; amended by 0018 and 0038 |
| [0003](0003-what-pluto-is-to-become.md) | What Pluto is to become | Accepted; amended by 0011, 0016, 0018, 0019, 0020, 0022, 0030 and 0040 |
| [0004](0004-plutos-own-server-in-rust.md) | Pluto's own server, in Rust, in front of the frozen fork | Superseded by 0038 |
| [0005](0005-plutos-own-typed-protocol.md) | Pluto's own typed protocol | Accepted; amended by 0033 |
| [0006](0006-agents-drivers.md) | Agents' drivers | Accepted; amended by 0011, 0021 and 0031 |
| [0007](0007-several-servers-and-a-relay.md) | Several servers in one view, and a relay of Pluto's own | Accepted; amended by 0018 |
| [0008](0008-storage-and-the-history-moved.md) | The server's storage, and the history moved from the fork | Accepted; amended by 0033 |
| [0009](0009-secrets-privacy-and-the-sandbox.md) | Secrets, privacy and the agents' sandbox | Accepted; amended by 0016, 0018, 0019, 0022, 0035 and 0040 |
| [0010](0010-todays-features-triaged.md) | Today's features triaged, and the egui client frozen | Accepted; amended by 0021 and 0038 |
| [0011](0011-plutos-own-harness-minimal-and-measured.md) | Pluto's own harness, minimal and measured | Superseded by 0032 |
| [0012](0012-the-checks-and-ci.md) | The checks in one script, run by CI in the Nix shell | Accepted; amended by 0025 |
| [0013](0013-plutos-logo.md) | Pluto's logo | Accepted |
| [0014](0014-how-today-is-measured.md) | How today is measured | Accepted; amended by 0026 |
| [0015](0015-the-budgets-set-from-todays-figures.md) | The budgets set from today's figures | Accepted; amended by 0017 and 0026 |
| [0016](0016-uwuis-decisions-of-2026-11-07-taken-by-pluto.md) | uwui's decisions of 2026-11-07, taken by Pluto | Accepted; amends 0001, 0003 and 0009; amended by 0017 |
| [0017](0017-pluto-measured-against-the-market.md) | Pluto measured against the market | Accepted; amends 0015 and 0016; amended by 0022, 0026, 0027, 0037 and 0039 |
| [0018](0018-the-markets-features-and-the-triage.md) | The market's features, and the triage by family | Accepted; amends 0002, 0003, 0007 and 0009; amended by 0022 |
| [0019](0019-a-code-editor-in-pluto.md) | A code editor in Pluto | Accepted; amends 0003 and 0009; amended by 0040 |
| [0020](0020-the-chat.md) | The chat | Accepted; amends 0003; amended by 0040 |
| [0021](0021-deepseek-harness-in-todays-pluto.md) | DeepSeek Harness in today's Pluto | Accepted; amends 0006 and 0010 |
| [0022](0022-the-triage-row-by-row.md) | The triage, row by row | Accepted; amends 0003, 0009, 0017 and 0018; amended by 0024 and 0040 |
| [0023](0023-the-specification-accepted.md) | The specification accepted | Accepted |
| [0024](0024-cuts-kept-in-the-egui-client-until-uwui.md) | Cuts kept in the egui client until uwui | Accepted; amends 0022 |
| [0025](0025-agents-sessions-recorded-and-replayed.md) | Agents' sessions recorded and replayed | Accepted; amends 0012 |
| [0026](0026-the-rivals-measured-how.md) | How the rivals are measured | Accepted; amends 0014, 0015 and 0017; amended by 0028 |
| [0027](0027-the-rivals-measures-set-aside.md) | The rivals' measures set aside until the owner's go | Accepted; amends 0017; amended by 0039 |
| [0028](0028-the-open-rivals-and-the-web-chats-measured.md) | How the open rivals and the web chats are measured | Accepted; amends 0026; amended by 0029 |
| [0029](0029-the-benchs-tools-and-what-a-rivals-figure-holds.md) | The bench's tools for the six, and what a rival's figure holds | Accepted; amends 0028 |
| [0030](0030-pluto-becomes-a-harness.md) | Pluto becomes a harness: one kernel, two products | Accepted; amends 0003 |
| [0031](0031-three-kinds-of-engine.md) | Three kinds of engine under one thread model | Accepted; amends 0006 |
| [0032](0032-plutos-loop-and-its-provider-clients.md) | Pluto's loop and its own provider clients | Accepted; supersedes 0011 |
| [0033](0033-one-pluto-agent-process-per-session.md) | One `pluto-agent` process per session, its log on disk | Accepted; amends 0005 and 0008 |
| [0034](0034-the-methods-mechanisms-native.md) | The method's mechanisms native, the method itself in text | Accepted |
| [0035](0035-the-sandbox-and-the-vault.md) | The sandbox after Codex's, with a vault | Accepted; amends 0009 |
| [0036](0036-the-inspector-appends-by-default.md) | The inspector appends by default | Accepted |
| [0037](0037-how-the-harness-is-judged.md) | How the harness is judged | Accepted; amends 0017 |
| [0038](0038-a-new-foundation-beside-the-fork.md) | A new foundation beside the frozen fork, in the same repository | Accepted; supersedes 0004; amends 0001, 0002 and 0010 |
| [0039](0039-the-order-of-the-stages.md) | The order of the stages, and Stage 1 closed | Accepted; amends 0017 and 0027 |
| [0040](0040-out-of-the-plan.md) | Out of the plan: the editor, office editors, calls and the smart home | Accepted; amends 0003, 0009, 0019, 0020 and 0022 |

## Template

```markdown
# NNNN. The decision, as a short title

- Status: Accepted
- Date: YYYY-MM-DD

## Context

What forces the decision: the need, the constraints, what the code holds today, what was measured.

## Decision

What is decided, in bold lead-ins, one paragraph each.

## Alternatives considered

- **Alternative**: what it would give, and why it was rejected.

## Consequences

- What follows, good and bad, and what is left to decide later.
```
