# Architecture decision records

An ADR records one decision: its context, the decision, the alternatives rejected and the
consequences. Once accepted, only its status changes. A later decision that replaces it whole makes
it `Superseded by NNNN`; one that changes part of it makes it `Accepted; amended by NNNN`, and the
newer one `Accepted; amends NNNN (what)`.

New ADRs copy the template below and take the next free number.

| ADR | Title | Status |
|---|---|---|
| [0001](0001-a-native-client-on-egui-until-uwui.md) | A native client, on egui until uwui | Accepted; amended by 0016 |
| [0002](0002-the-android-app-until-uwui.md) | The Android app, in Kotlin until uwui | Accepted |
| [0003](0003-what-pluto-is-to-become.md) | What Pluto is to become | Accepted; amended by 0011 and 0016 |
| [0004](0004-plutos-own-server-in-rust.md) | Pluto's own server, in Rust, in front of the frozen fork | Accepted |
| [0005](0005-plutos-own-typed-protocol.md) | Pluto's own typed protocol | Accepted |
| [0006](0006-agents-drivers.md) | Agents' drivers | Accepted; amended by 0011 |
| [0007](0007-several-servers-and-a-relay.md) | Several servers in one view, and a relay of Pluto's own | Accepted |
| [0008](0008-storage-and-the-history-moved.md) | The server's storage, and the history moved from the fork | Accepted |
| [0009](0009-secrets-privacy-and-the-sandbox.md) | Secrets, privacy and the agents' sandbox | Accepted; amended by 0016 |
| [0010](0010-todays-features-triaged.md) | Today's features triaged, and the egui client frozen | Accepted |
| [0011](0011-plutos-own-harness-minimal-and-measured.md) | Pluto's own harness, minimal and measured | Accepted |
| [0012](0012-the-checks-and-ci.md) | The checks in one script, run by CI in the Nix shell | Accepted |
| [0013](0013-plutos-logo.md) | Pluto's logo | Accepted |
| [0014](0014-how-today-is-measured.md) | How today is measured | Accepted |
| [0015](0015-the-budgets-set-from-todays-figures.md) | The budgets set from today's figures | Accepted |
| [0016](0016-uwuis-decisions-of-2026-11-07-taken-by-pluto.md) | uwui's decisions of 2026-11-07, taken by Pluto | Accepted; amends 0001, 0003 and 0009 |

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
