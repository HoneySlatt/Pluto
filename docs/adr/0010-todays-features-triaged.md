# 0010. Today's features triaged, and the egui client frozen

- Status: Accepted
- Date: 2026-11-06

## Context

Since 2026-10-20 the client has ported most of T3 Code v0.1.0's web client. `docs/parity/` lists
about 330 gaps that remained, and `docs/release-backlog.md` ordered them toward a private 1.0 of full
parity. The owner names "too many features" as what weighs most today. The server and the uwui
client will be written anew ([ADR-0004](0004-plutos-own-server-in-rust.md),
[ADR-0001](0001-a-native-client-on-egui-until-uwui.md)), so they need a specification.

## Decision

**The new Pluto starts from today's features, triaged.** Every feature of the client, the server
fork and the phone is listed, and the owner marks each one keep, cut or rethink. What is kept, plus
the differences of [ADR-0003](0003-what-pluto-is-to-become.md), is the specification of the Rust
server and of the uwui client. The triage comes first: it decides the server's scope.

**The egui client is frozen.** Porting T3 stops, and `docs/release-backlog.md` is abandoned. The
egui client receives only:
- fixes;
- the triage's cuts;
- the move to Pluto's protocol.

The differences reach the screen with the uwui client.

## Alternatives considered

- **A blank page**: the freest design, but features that serve the owner unseen could be forgotten.
- **Finishing the backlog**: the planned milestones 4 and 5, but work on a client that will be
  replaced.
- **Adding the differences to egui now**: sooner, but written twice.

## Consequences

- `docs/parity/` stays as the inventory the triage starts from.
- The cuts may lower the egui client's memory and wake-ups before uwui. They are measured.
- The Android app follows the same triage.
