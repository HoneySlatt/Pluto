# 0008. The server's storage, and the history moved from the fork

- Status: Accepted
- Date: 2026-11-06

## Context

The fork stores an event store and its projections in SQLite. The schema has about 62 migrations
and 56 kinds of thread events. Its database in `~/.pluto/userdata` was 2.7 GiB on 2026-11-06
after weeks of use. What fills it is not yet known. The owner's history must not be lost.

## Decision

**The server stores in SQLite.** Its schema is versioned and migrated. The database is backed up
before each migration, and writes are atomic.

**Streaming is not stored delta by delta.** A message is stored once it is complete. While it
streams, only what lets an interrupted turn be resumed is kept. Old threads may be archived by rules
the owner sets.

**Any thread exports to readable Markdown and JSON.**

**The fork's history is imported as final states, not replayed as events.** The importer reads the
fork's projections on a copy of the database:
- threads, messages, turns and activities;
- diffs;
- each agent's resume cursor, so that a thread can go on where the agent allows it.

The import is checked against the copy: counts, and every thread's content compared. The importer
is written early, beside the new storage. Its comparisons serve as the oracle for the new data
model.

## Alternatives considered

- **Readable files, a folder per thread**: easy to back up, but search and sync are slower and more
  fragile.
- **Replaying the fork's events into the new model**: faithful, but it binds the new model to 56
  event kinds of another team's design.
- **An archive only, or starting afresh**: simpler, but old threads could not be continued, or would
  be lost to Pluto.

## Consequences

- What fills the fork's 2.7 GiB is examined, on a copy, before the storage is designed.
- The storage's own model is decided with its milestone: an event log or plain tables.
