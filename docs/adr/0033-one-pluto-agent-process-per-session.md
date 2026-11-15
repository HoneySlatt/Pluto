# 0033. One `pluto-agent` process per session, its log on disk

- Status: Accepted; amends [0005](0005-plutos-own-typed-protocol.md) (the agent's part of the
  protocol carries the session log's events) and [0008](0008-storage-and-the-history-moved.md) (a
  session is stored as its log)
- Date: 2026-11-15

## Context

Pluto's loop is a crate ([ADR-0032](0032-plutos-loop-and-its-provider-clients.md)). Where it runs,
and where a session lives, decide what survives a crash, an update, and ten times the load.

- **A guarantee of the bar**: an update or a restart of the server never stops a running agent
  ([ROADMAP](../ROADMAP.md#quality-bar)). A loop inside the server breaks it.
- **Anthropic** first ran session, harness and sandbox in one container, which became "a pet": a
  failed container lost its session. It split them; a harness restarts from the session's log. p50
  time to first token fell about 60 % (reported,
  [Managed Agents](https://www.anthropic.com/engineering/managed-agents)).
- **Codex** keeps threads, turns and items with started, delta and completed events, persisted so
  clients reconnect. **Pi** keeps sessions as append-only JSON Lines, with atomic publishing and a
  repair of a torn last line.
- **The phone** gets "Pluto's own agent (Stage 4) in a light form", offline
  ([ADR-0018](0018-the-markets-features-and-the-triage.md); Stage 4 of the ROADMAP of 2026-11-08):
  the loop must run without the server.
- **The inspector** shows what the model receives at every turn
  ([ADR-0036](0036-the-inspector-appends-by-default.md)). A snapshot of every request, for 15
  threads of 100 requests of 400 KB, would make about 600 MB a day, an estimate, against the
  database's budget of 25 MiB a day.
- At 150 threads in one process, one crash stops them all.

## Decision

**Each agent session runs in its own `pluto-agent` process.** The server supervises them once it
exists ([ADR-0039](0039-the-order-of-the-stages.md)); until then the TUI does.

**A session is an append-only, versioned event log on disk.** It is the session's truth. The
context sent to a model is a view derived from it. A change to the context is a new entry, never a
rewrite. Fetched pages are kept, so research sources can be replayed.

**Every engine writes the same log** ([ADR-0031](0031-three-kinds-of-engine.md)): Pluto's loop,
the vendors' engines and ACP agents. A thread looks the same whatever ran it.

**The same code is a library** for the TUI and the phone.

**Its format is decided in Stage 2**, in its own ADR: encoding, versioning, entries, provenance,
and how a torn write is repaired.

## Alternatives considered

- **The loop inside the server.** One process, simplest. But it breaks the guarantee above, and
  one crash stops every thread.
- **One snapshot per request.** The inspector is trivial. But it grows about 24 times past the
  storage budget.
- **Each vendor's own session files as the truth.** Nothing to write. But one format per vendor,
  each changing with its CLI, and no common thread model.
- **Tables in a database as the truth.** Easy queries. But a session's history becomes rows to
  migrate, and context edits become updates in place.

## Consequences

- [ADR-0005](0005-plutos-own-typed-protocol.md) holds; the agent's part of the protocol carries the
  log's events.
- [ADR-0008](0008-storage-and-the-history-moved.md) holds for what is not a session. The history
  imported from the fork becomes logs. The lead's choice, shown to the owner with this record: the
  server's SQLite indexes the logs.
- The storage budget of 25 MiB a day of heavy use now counts the sessions' logs and the server's
  database together, not the database alone.
- A process per session costs memory and a start. Both are measured in Stage 3, and its budgets are
  set from those figures. An idle process stops, and resumes from its log
  ([ADR-0006](0006-agents-drivers.md)).
- The ROADMAP's open decision on the storage's model is taken for sessions.
