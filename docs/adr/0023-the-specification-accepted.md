# 0023. The specification accepted

- Status: Accepted
- Date: 2026-11-10

## Context

[ADR-0010](0010-todays-features-triaged.md) decided that the new Pluto starts from today's
features, triaged by the owner, and that what is kept, with the differences of
[ADR-0003](0003-what-pluto-is-to-become.md), becomes the specification of the Rust server and of
the uwui client. The families were marked on 2026-11-08
([ADR-0018](0018-the-markets-features-and-the-triage.md)) and their rows on 2026-11-10
([ADR-0022](0022-the-triage-row-by-row.md)). The ROADMAP left "the features kept" open until
[M1.3](../PLAN.md#m13--the-triage).

[`specification.md`](../specification.md) was written from the marks and from ADR-0003 and
ADR-0017 to ADR-0022. A closing review checked it against those ADRs and the owner's answers;
its findings were fixed or put to the owner before acceptance.

## Decision

The owner accepted [`specification.md`](../specification.md) on 2026-11-10. It is what Pluto's
Rust server, its client on uwui and its phone must do. Each stage's milestones build from it. A
change to it is decided by an ADR, and the specification is updated with it.

## Alternatives considered

- **The rows of `features.md` as the specification**: one document less, but a list of today's
  features is not a description of the server and the client to build.
- **A specification per stage, written when the stage starts**: less written ahead, but the
  server of Stage 2 would be designed without knowing what Stages 3 to 7 ask of it.

## Consequences

- The ROADMAP's open decision on the features kept is taken.
- [M1.3](../PLAN.md#m13--the-triage) is done. [M1.4](../PLAN.md#m14--the-cuts) cuts what the
  specification's section 6 lists, from the egui client and the Android app.
- Stage 2's milestones are planned from the specification's server sections.
