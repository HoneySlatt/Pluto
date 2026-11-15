# 0040. Out of the plan: the editor, office editors, calls and the smart home

- Status: Accepted; amends [0019](0019-a-code-editor-in-pluto.md) (the editor becomes an idea, out
  of the stages), [0020](0020-the-chat.md) (a document written together no longer rests on the
  editor's live merge) and [0003](0003-what-pluto-is-to-become.md) (files viewed and lightly
  edited, real editing in the owner's editor, as first decided)
- Date: 2026-11-15

## Context

The design session of 2026-11-15 widened what Pluto is for
([ADR-0030](0030-pluto-becomes-a-harness.md)): a harness, design as a first-class space, a
personal assistant. Its challenger found the whole scope beyond one programme:
- the owner is the bottleneck, with checks and measures waiting by hand in several projects;
- each new space adds tests only the owner can run;
- the fastest teams delete: Cursor removed three roles from its harness, Amp three features in
  five weeks.

It recommended cutting what a lean kernel can leave for later, the door kept open.

[ADR-0019](0019-a-code-editor-in-pluto.md) had planned a code editor as Stage 7, the largest stage
of all, an estimate.

## Decision

**Moved to the PLAN's Ideas**, out of the stages:
- **the code editor** of [ADR-0019](0019-a-code-editor-in-pluto.md). Its design stays the
  starting point if it returns. The architecture must not rule one out: files, search and git stay
  in the server, on any machine;
- **office editors**: agents write documents as files, and Pluto previews them;
- **joining calls**;
- **actions on a smart home**.

**Kept:**
- computer use, in an isolated session ([ADR-0018](0018-the-markets-features-and-the-triage.md));
- tools' own interfaces and agents' pages, in uwui's isolated web view, as the
  [specification](../specification.md) and [ADR-0022](0022-the-triage-row-by-row.md) decide.

## Alternatives considered

- **The editor kept as Stage 7.** The owner's choice of 2026-11-08. But it is larger than every
  other stage together, and the harness now comes first.
- **The editor cut for good.** Simplest. But an idea costs nothing, and the owner's need for one
  may return.
- **Office editors in Pluto.** Documents without another program. But a suite of its own, for what
  agents already do well in files.

## Consequences

- The ROADMAP loses the editor's stage. Going public stays Stage 8.
- The editor's rivals and its typing budget leave the quality bar until it returns.
- Pluto keeps viewing files, editing them lightly, and opening the owner's editor at the line.
- [ADR-0020](0020-the-chat.md)'s document written beside the chat is designed with the chat at
  Stage 6, without the editor's live merge.
- The sandbox on Windows, which ADR-0019 moved to Stage 7, is decided when Pluto goes public again,
  as [ADR-0009](0009-secrets-privacy-and-the-sandbox.md) first set.
- The modal navigation outside the editor
  ([ADR-0022](0022-the-triage-row-by-row.md)) waits for the editor's modal core, or a decision of
  its own.
