# Glossary

Terms this project uses in a sense of its own.

- **Agent**: a coding agent's program that Pluto drives, such as Claude Code, Codex or Kimi Code,
  usually its official CLI.
- **Pluto's agent**: Pluto's own harness, a driver of the server for local models and API keys
  ([ADR-0011](adr/0011-plutos-own-harness-minimal-and-measured.md)).
- **Harness**: the loop, tools, prompts and context management around a model that make it a coding
  agent.
- **The bench**: Pluto's small fixed set of coding tasks, run on the owner's local models to measure
  the harness and each of its components.
- **A rival**: a program of the market that Pluto is measured against, named only to be measured,
  with its version and date ([ADR-0017](adr/0017-pluto-measured-against-the-market.md)).
- **The fake model service**: a scripted service that streams the same workload to Pluto and its
  rivals as a model would, so that they compare with no quota spent; it counts the bytes it
  receives, by part, which stand for tokens until Stage 2 brings a tokenizer;
  `crates/pluto-fake-model` ([ADR-0017](adr/0017-pluto-measured-against-the-market.md),
  [ADR-0026](adr/0026-the-rivals-measured-how.md)).
- **The margin**: how much better than the best rival Pluto must be on a family of criteria, such
  as twice on resources ([ADR-0017](adr/0017-pluto-measured-against-the-market.md)).
- **A recording**: an agent's raw stdio traffic during one scripted scenario, masked, in
  `crates/pluto-recording/recordings/` ([ADR-0025](adr/0025-agents-sessions-recorded-and-replayed.md)).
- **A scenario's reference**: the thread a person would see, taken from a recording's live run;
  every replay must give it exactly.
- **The fake agent**: `pluto-replay-agent`, which plays a recording in an agent CLI's place.
- **The stamp**: the hashes of what a replay reads, written in `recordings/` when the owner's
  machine replays the recordings into the fork; CI compares them to show nothing it covers
  changed without that replay.
- **The rivals' bench**: `scripts/rivals/`, which runs Pluto or a rival in a sandbox of its own on
  the fake model's workload and measures it from outside with `pluto-measure`
  ([ADR-0026](adr/0026-the-rivals-measured-how.md)).
- **A subject**: what one run of the rivals' bench measures: Pluto, or a rival.
- **A control subject**: a subject that measures no application, only what a rival runs on: the
  same Chromium on an empty page, beside the web rivals
  ([ADR-0028](adr/0028-the-open-rivals-and-the-web-chats-measured.md)).
- **Chat mode**: the fake model's mode for chat applications, which send no tool: a prompt that
  begins with `[turn N]` gets that turn's texts, without its tool calls
  ([ADR-0028](adr/0028-the-open-rivals-and-the-web-chats-measured.md)).
- **The headline memory**: a program's private memory, swap and GPU memory, summed over all its
  processes; the quality bar's memory figure since [ADR-0026](adr/0026-the-rivals-measured-how.md).
- **The frame bench**: `examples/bench.rs`, which measures each frame's CPU cost of the client
  against a server; not to be confused with the bench above.
- **uwui's agent bench**: the fixed tasks that uwui gives the same agents with uwui and with its
  rivals, to measure how well agents build applications on it (SlimeEngine's ADR-0213); not to be
  confused with Pluto's bench.
- **Declared action**: an action that an application on uwui declares once for agents, with its
  name, typed parameters, description, permission and family (SlimeEngine's ADR-0218).
- **Kept for a person**: an action that no agent may run, whatever consent it holds, such as
  approving an agent's request ([ADR-0016](adr/0016-uwuis-decisions-of-2026-11-07-taken-by-pluto.md)).
- **uwui's core look and official themes**: the sober look that uwui's core ships, light, dark and
  of high contrast; and the looks in crates of their own beside it, Glass, Material 3 Expressive
  and Pro, which an application adds if it wants them.
- **Model profile**: the harness's settings for one model: its edit format, tool names, message
  format, and which components are on.
- **Driver**: the server's code that speaks one agent's protocol and turns it into Pluto's threads.
  *Native* when written for that agent, *ACP* when it goes through the Agent Client Protocol.
- **At rest**: a client or a server with nothing to do: no input, no turn running. The quality bar
  asks it to draw nothing and wake for nothing.
- **First useful frame**: the first frame the client draws with the server's projects and threads;
  the start's budget ends when it is painted ([ADR-0014](adr/0014-how-today-is-measured.md)).
- **The fixture**: the disposable server of synthetic threads that `scripts/fixture-server.sh`
  builds, on which every automated measurement runs, never on the owner's data.
- **The fork**: `pluto-server`, the TypeScript fork of T3 Code's server, frozen since 2026-11-06.
- **The gateway**: the part of Pluto's Rust server that forwards to the fork what is not yet
  ported, and translates its protocol into Pluto's. It disappears when the fork stops.
- **The relay**: the owner's own server through which devices reach each other away from home,
  built on iroh; it sees only encrypted traffic.
- **The triage**: the owner's marking of every feature as keep, cut or rethink; its result is the
  specification.
- **A row**: one feature of [`features.md`](features.md) that the owner marked on its own, numbered
  by its family (`7.3`).
- **The dashboard**: the client's home screen, a grid of thread cards with a strip of quotas,
  machines and scheduled tasks ([ADR-0022](adr/0022-the-triage-row-by-row.md)).
- **The live line**: the one line on which a running turn shows what its agent does, and for how
  long.
- **The turn's summary**: the head of a finished turn: its duration, files changed, commands run
  and result, its details folded below.
- **The artifacts panel**: the surface where agents' pages, MCP apps and designs open whole.
- **The one configuration**: the MCP servers, skills and instructions declared once in Pluto and
  given to each agent in its own format
  ([ADR-0018](adr/0018-the-markets-features-and-the-triage.md)).
- **A zone**: a part of the window (the thread, the composer, the review, the terminal…) whose keys
  are bound together.
- **First-rank agent**: an agent Pluto drives with every ability, native or supplied by Pluto
  ([ADR-0003](adr/0003-what-pluto-is-to-become.md)).
- **Settle**: to put a finished thread aside, out of the live ones; *auto-settle* does it after its
  pull request merges or after some days. To *snooze* hides a thread until a time.
- **A sweep**: pressing a thread's action and dragging along the list to apply it to every thread
  passed over.
- **A surface**: one kind of tab of the panel beside a thread: review, files, terminal, browser,
  artifacts.
- **The layers icon**: the mark beside a setting that says where its value comes from (default,
  machine, project, thread) and resets it to inherit.
- **The lineage card**: a thread's parent, forks, subagents, context transfers and a workflow's
  phases, in its details.
- **A side question**: a question asked beside a thread that neither turns its agent from its work
  nor fills its context ([ADR-0018](adr/0018-the-markets-features-and-the-triage.md)).
- **A living artifact**: a page, diagram, chart or small application an agent makes, run in the
  conversation, isolated, with its versions ([ADR-0020](adr/0020-the-chat.md)).
- **Thread**: one conversation with one agent, in a project, on one server.
- **Chat space**: the project where chats live, apart from code projects, with its memory.
- **Tier**: a class of machine the quality bar names: the high tier (the owner's desktop) and the
  lowest (the owner's HP laptop).
