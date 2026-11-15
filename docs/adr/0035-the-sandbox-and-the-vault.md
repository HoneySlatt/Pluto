# 0035. The sandbox after Codex's, with a vault

- Status: Accepted; amends [0009](0009-secrets-privacy-and-the-sandbox.md) (how the sandbox is
  built; a vault for secrets and hidden checks; `claude` and `codex` wrapped too)
- Date: 2026-11-15

## Context

[ADR-0009](0009-secrets-privacy-and-the-sandbox.md) decided a sandbox that can be adjusted, built
on bubblewrap on Linux, and locked for unattended tasks. Pluto now runs tools itself, in its own
loop and for Claude Code through a mod ([ADR-0031](0031-three-kinds-of-engine.md)).

What the field does, read on 2026-11-15:
- **Codex** on Linux: bubblewrap for files and namespaces, seccomp for system calls, Landlock kept
  as a legacy path, and a managed proxy for the network
  ([linux-sandbox](https://raw.githubusercontent.com/openai/codex/main/codex-rs/linux-sandbox/README.md),
  Apache-2.0). It is the best Rust reference, not published as a crate.
- **Claude Code**: bubblewrap and Seatbelt through Anthropic's `sandbox-runtime`, in TypeScript,
  with the network denied by default behind filtering proxies. It runs unsandboxed on native
  Windows.
- **Anthropic's Managed Agents**: tools behind one call; tokens never reachable from code in the
  sandbox.
- **OpenCode and Pi**: permission prompts, no system sandbox.
- **The personal agents' incidents** (exposed instances, malicious skills, one set of connections
  shared by every agent) come from code that runs with the user's rights
  ([ADR-0030](0030-pluto-becomes-a-harness.md)).
- **The owner's method today**: every child runs as the owner and could read the hidden checks.

## Decision

**On Linux, the sandbox is shaped after Codex's:**
- bubblewrap for files and namespaces;
- seccomp for system calls;
- Landlock where it adds;
- a filtering proxy for the network, which is denied by default.

Codex's code is read and copied only where it fits, with its notice in `NOTICE.md`.

**A vault outside the sandbox** holds the owner's keys and the delegation's hidden checks. A tool
that needs a secret runs outside; the model and the sandboxed code never see it.

**`claude` and `codex` run inside the same sandbox.** Wrapping a binary does not modify it. Each
agent, and later the assistant, has its own connections, never one set shared by all.

**macOS and Windows** are decided when Pluto goes public, as ADR-0009 set.

## Alternatives considered

- **Each vendor's own sandbox.** Nothing to build. But unequal, absent for Pluto's loop and ACP
  agents, and Claude Code has none on Windows.
- **Permission prompts only**, as OpenCode and Pi. Little friction. But a prompt is not a
  boundary, and unattended tasks cannot be asked.
- **MicroVMs** (microsandbox, libkrun). The strongest isolation. But heavier, and microsandbox runs
  on Apple Silicon only, not the owner's Intel Mac. An optional tier later, if a need shows.
- **Containers** (Docker, Podman). Familiar. But a daemon to run, and heavier than namespaces on
  the HP.

## Consequences

- New dependencies (`seccompiler`, `landlock`) are asked of the owner when the sandbox's
  milestone comes.
- bubblewrap needs unprivileged user namespaces; the owner's NixOS allows them.
- How the vault keeps secrets without a graphical session stays open
  ([ROADMAP](../ROADMAP.md#open)).
