# 0047. Stage numbers in ADRs written before the stages were reordered

- Status: Accepted; supersedes in part [0015](0015-the-budgets-set-from-todays-figures.md),
  [0018](0018-the-markets-features-and-the-triage.md), [0020](0020-the-chat.md),
  [0025](0025-agents-sessions-recorded-and-replayed.md) and
  [0033](0033-one-pluto-agent-process-per-session.md) (the stage each names for a piece of work)
- Date: 2026-11-18

## Context

[ADR-0039](0039-the-order-of-the-stages.md) reordered the stages on 2026-11-15. ADRs written
before it name stages of the ROADMAP of 2026-11-08, and none of them was changed since, so they
and the ROADMAP disagree on when the same work comes. Stage 1's audit
([`docs/audits/2026-11-18-stage-1.md`](../audits/2026-11-18-stage-1.md)) found these:

- ADR-0015: frames "measured from Stage 6" and two budgets "from Stage 2"; the ROADMAP's quality
  bar measures them from Stage 5 on uwui, and from Stage 5 and Stage 3.
- ADR-0018: the stages of its families' work, of 2026-11-08; the ROADMAP puts the scheduler,
  triggers, the inbox, the browser and computer use in Stage 5.
- ADR-0020: agents' sessions kept ready "decided at Stage 2 and measured", twice; the ROADMAP and
  `docs/specification.md` put it at Stage 6.
- ADR-0025: "Stage 2's drivers" and "the fork is frozen and leaves at Stage 3"; the engines are
  Stage 4's, and the fork leaves at Stage 5 ([ADR-0038](0038-a-new-foundation-beside-the-fork.md)).
- ADR-0033: "Pluto's own agent (Stage 4)", which it marks as the ROADMAP of 2026-11-08's.

The project's rules forbid writing a precision into an older ADR's status, and consolidating each
chain of amendments is planned with the milestone that next touches its topic, so the stage
numbers are settled here, once, for all of them.

## Decision

**Where an ADR names a stage of the ROADMAP of 2026-11-08 for a piece of work, that work comes
at the stage the ROADMAP gives it today.** That is every ADR written before ADR-0039, and ADR-0033
where it says so. The ROADMAP's stages and open decisions are the one home of when work comes
([ADR-0039](0039-the-order-of-the-stages.md)); a later ADR that moves work moves it there.

**Nothing else of those ADRs changes.** Each rule they state still holds; only the stage it is
first built, decided or measured at follows the ROADMAP.

## Alternatives considered

- **Leave the numbers until each topic's consolidation**: the chat's consolidation comes at
  Stage 6, after the stage ADR-0020 names has passed; a reader meanwhile cannot tell which number
  holds.
- **One ADR for each of the five**: five ADRs restating one rule.

## Consequences

- The five ADRs' statuses say they are superseded in part by this one.
- A stage named in a consolidated ADR later is the ROADMAP's of its date, and states so.

## Validation and reconsideration

The audit lists the lines that found the five. A stage number found in
another ADR written before ADR-0039 falls under this decision; one in a later ADR that disagrees
with the ROADMAP is a contradiction to put to the owner.
