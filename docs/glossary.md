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
- **Thread**: one conversation with one agent, in a project, on one server.
- **Chat space**: the project where chats live, apart from code projects, with its memory.
- **Tier**: a class of machine the quality bar names: the high tier (the owner's desktop) and the
  lowest (the owner's HP laptop).
