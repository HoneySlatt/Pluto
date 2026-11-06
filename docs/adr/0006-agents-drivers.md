# 0006. Agents' drivers

- Status: Accepted; amended by [0011](0011-plutos-own-harness-minimal-and-measured.md) (Pluto's own agent)
- Date: 2026-11-06

## Context

Every agent Pluto drives is first-rank ([ADR-0003](0003-what-pluto-is-to-become.md)):
- Claude Code and Codex;
- OpenCode, Kimi Code, Mistral Vibe and Z Code;
- Grok, Antigravity, Pi and Muse;
- DeepSeek Harness;
- local models through Ollama.

Upstream changed its Claude driver 288 times in six months, Codex 244 times and OpenCode 164 times.
The Agent Client Protocol (ACP) is a common protocol for agents, with an official Rust crate,
`agent-client-protocol` 5.0.0. In that crate, forks, subagents, session status and token usage are
still behind `unstable_*` features. The fork chose native drivers for Kimi Code and Mistral Vibe,
for steering, compaction, plan review, rollback and usage.

## Decision

**A driver is native where it brings what ACP lacks, and ACP otherwise.** Each native driver keeps
a written list of what it gives beyond the agent's ACP mode, when the agent has one. A driver
whose list empties moves to ACP.

**Cursor is dropped.** The owner does not use it, and its agent exists only as a TypeScript library.

**DeepSeek Harness is decided when its interface is known.** On 2026-11-06 it is a developer
preview in TypeScript, driven only through its web interface. Meanwhile DeepSeek's models are
reached through OpenCode.

**Every driver is tested without a real turn.** Agent CLIs are pinned in the Nix shell. Each
driver's tests replay recorded sessions, or run against a fake agent. CI never starts a real
provider session.

**Agents are frugal with the machine.**
- An idle session's process stops after a few idle minutes, and resumes on the next turn.
- A new session waits while the machine lacks memory.
- Each agent may have a memory ceiling, set with its sandbox
  ([ADR-0009](0009-secrets-privacy-and-the-sandbox.md)).

## Alternatives considered

- **Native everywhere**: the most complete, and the heaviest to maintain.
- **ACP wherever it exists**: the least maintenance, but steering, plan review, rollback and usage
  are lost where the fork has them today.

## Consequences

- The cost of following each agent's changes is paid once per agent per release of its CLI. It is
  seen in the replay tests, not by the owner.
- An agent that stops on idle must resume cheaply. What a resume costs is measured for each agent
  before the stop is turned on.
