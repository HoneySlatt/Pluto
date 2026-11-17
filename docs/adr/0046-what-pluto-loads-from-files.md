# 0046. What Pluto loads from files: pinned, gated, layered, inventoried

- Status: Accepted
- Date: 2026-11-17

## Context

Pluto loads behaviour from files: the owner's rules and method, skills, hooks, named agents, MCP
servers' configuration, and the same files inside the repositories it works in
([ADR-0034](0034-the-methods-mechanisms-native.md)). It opens no extensions to others, but keeps
the way open ([ADR-0045](0045-features-from-other-harnesses-add-ons.md)). M2.0 already lists a
trust gate before a project's files are read or run, and extensions pinned by digest.

What other harnesses had to fix late, read on 2026-11-17 in their advisories and documentation:

- **Files that changed under a review.** Claude Code's advisory GHSA-pq7j-f95f-qfcv: a branch named
  like a pinned commit installed other code; auto-update can change reviewed files. Codex records
  trust against a hook's current hash and skips a changed one until it is trusted again.
- **A repository's files run before trust.** Six of Claude Code's advisories were trust-dialog
  bypasses, and Codex's CVE-2025-61260 ran a project's MCP commands unprompted. Pi and Kimi now
  keep a project's resources inactive until trust, and Kimi reads no hook from a project.
- **Layers that could grant.** Gemini CLI forbids an extension's policy to allow anything;
  OpenCode keeps two policies a repository cannot remove; Cursor's Bugbot lets a file lower an
  autofix mode, never raise it.
- **Hooks that let everything through on failure.** Kimi's and goose's hooks fail open; Amp defines
  four outcomes for a tool call: allow, reject, modify, synthesize.
- **Plugin interfaces left behind.** OpenCode rebuilt more than 80 of its features as internal
  plugins for its 2.0, its first plugin interface having never had the attention it needed; its
  plugins then broke twice in three days.
- **Skills' format.** The agentskills.io specification (Apache-2.0) is read by Claude Code, Codex,
  Copilot, OpenCode and others: a folder with `SKILL.md`, a name and a description, loaded in three
  steps.
- **What a plugin costs.** Claude Code prints a plugin's always-on tokens before it is enabled, for
  its official plugins only; ClawHub shows a release's status apart from its authority.

The owner adopted each of the seven rules below, one at a time.

## Decision

**1. Pinned by digest.** Everything Pluto loads from a file (a rule, the method, a skill, a hook, a
named agent, an MCP server's configuration) is recorded with its digest. A file that changed is
shown to the owner and reviewed again before it is used; nothing changes silently. A digest is
checked on the file as fetched, never on a name or a reference.

**2. A repository's files inactive until trust.** In a project not trusted yet, its `AGENTS.md` is
read as untrusted content, and its skills, hooks, named agents and MCP servers stay inactive. A run
with no one to ask skips them. Names and descriptions are cleaned of invisible characters and
markup before any model or screen shows them.

**3. A lower layer restricts, never grants.** Settings come in layers (Pluto's, the owner's, a
project's, a skill's, and later an extension's). A lower layer may forbid a tool, cut the network or
lower a mode; it never opens what a higher layer closes. A policy the owner sets cannot be turned
off by a repository.

**4. Hooks.** Hooks come only from the owner's own configuration, never from a repository. Their
schema is strict: an unknown field fails the load. A policy hook that fails blocks; a notification
hook that fails does not. A hook's outcome is part of the tool's contract: allow, refuse, change
the input, or supply the result.

**5. Pluto's own features use its contracts.** Pluto's built-in tools, engines and provider
clients go through the same contracts M2.0 defines (the tool, the engine, the provider client),
with no shortcut around them. Opening extensions later would expose contracts Pluto already uses.
No plugin system is built ahead of need.

**6. Skills in the open format.** A skill is a folder with `SKILL.md`, per agentskills.io. What is
Pluto's own goes under `metadata`. A renamed identifier keeps a map from the old name. A skill loads
in three steps: its name and description at the start, its body when it is used, its other files
on demand.

**7. An inventory before anything is enabled.** Before a skill, an MCP server or a hook is enabled,
Pluto shows what it can reach (tools, commands, network, environment variables, files) and what it
costs in tokens at every turn, computed by code with no model, and flags what it uses without
declaring it.

## Alternatives considered

- **Pinning extensions only**, as M2.0 first wrote it. Less to check. But the owner's own files and
  a repository's are where a silent change reaches the model today.
- **A repository's `AGENTS.md` read as trusted**, only its hooks and MCP servers blocked. Less
  friction on a first run. But instructions in a cloned repository steer an agent as surely as a
  hook does.
- **A plugin interface designed now**, ahead of extensions. Ready on the day. But an interface with
  no user is the one OpenCode had to rebuild; contracts proven by Pluto's own features are the
  sound base.
- **A skills format of Pluto's own.** Free to fit Pluto. But the owner's skills would no longer be
  read by the other agents, and theirs could not be adapted.

## Consequences

- M2.0's contracts (the tool, the engine, the provider client) carry the hook's outcomes, the
  capability a part declares, and its cost; the session log records each loaded file's digest
  (M2.1).
- Stage 3 builds the loader, the trust gate, the layers, the review of a changed file and the
  inventory.
- The method's files, distributed by the owner's NixOS to several agents, are pinned like any
  other: a rebuild that changes them asks for a review in Pluto.

## Validation and reconsideration

Reviewed reasoning; nothing is built yet. Checks for Stage 3: a changed file is refused until
reviewed; a repository's hook is never loaded; a project's setting that tries to allow what the
owner forbids is refused; a policy hook that crashes blocks its call; a skill written for Claude
Code loads unchanged.

A new decision would be proposed to the owner if the skills specification breaks compatibility, or
if the review of changed files proves too heavy in daily use.
