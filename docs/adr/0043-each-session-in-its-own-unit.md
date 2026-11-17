# 0043. Each session in its own unit; an agent approves only what its brief grants

- Status: Accepted; supersedes in part [0033](0033-one-pluto-agent-process-per-session.md) (what
  supervises sessions) and [0042](0042-a-cli-first-the-tui-with-the-assistant.md) (what supervises
  sessions before the server; approvals answered by an agent)
- Date: 2026-11-17

## Context

The quality bar guarantees that "an update or a restart of the server never stops a running agent"
([ROADMAP](../ROADMAP.md#quality-bar)). Two decisions say who looks after the sessions:
[ADR-0033](0033-one-pluto-agent-process-per-session.md) has the server supervise them, and
[ADR-0042](0042-a-cli-first-the-tui-with-the-assistant.md) has the command line supervise the
sessions it starts until the server exists. Neither says what *supervise* means. If it means being
the sessions' parent process, the guarantee breaks:

- **systemd stops a unit's whole control group.** The default `KillMode=control-group` kills every
  process left in a unit's control group when the unit stops, and a restart is a stop and a start;
  `KillMode=process` is marked "not recommended"
  ([systemd.kill(5)](https://www.freedesktop.org/software/systemd/man/latest/systemd.kill.html),
  systemd 262). A child inherits its parent's control group.
- **The fork runs as such a unit**, `Restart=always`, with the default `KillMode`, in the owner's
  NixOS. Stage 3's delegated tasks are started by a lead agent, which runs under it: a
  `pluto-agent` started as its child would die at the fork's next restart or rebuild.
- **A command line is a poor parent.** Run from a script, by SSH or by an agent, it returns at
  once, and a terminal that closes sends a hangup.
- **Codex shows the cost.** Its managed daemon kills active turns at each update
  ([#40969](https://github.com/openai/codex/issues/40969), open on 2026-11-17), and turns cut that
  way wrote no event saying why ([#51201](https://github.com/openai/codex/issues/51201)).

ADR-0042 also lets the command line "answer approvals … from an agent".
[ADR-0016](0016-uwuis-decisions-of-2026-11-07-taken-by-pluto.md) holds that what only a person may
do in Pluto is never offered to agents. The owner's agents run as the owner, with the session bus
and the keyring's sockets reachable: nothing on the machine tells an agent's command from the
owner's.

The rivals' bench already runs each subject in its own transient unit
(`systemd-run --user --scope … -p Delegate=yes`, `scripts/rivals/run.sh`).

The architecture audit of 2026-11-17 traced these scenarios
([`2026-11-17-architecture-and-contribution.md`](../audits/2026-11-17-architecture-and-contribution.md));
the owner chose this option among three.

## Decision

**Supervising a session means starting, finding, watching and stopping it, never being its
parent.** The command line, the server and a lead agent are never a session's parent process.

**On Linux, each session runs in its own systemd user unit**, transient, started by whoever starts
the session. Its environment is built as an allowlist when it is started, never inherited: the user
manager's environment holds the owner's display, and a unit started through the session bus would
regain it. Each session gets its own runtime folder.

**The sessions' folder is their list.** A session's log is held by its one writer's lock, which the
kernel releases when the process dies, so a live session is one whose lock is held, and nothing
goes stale. The path of a session's control socket is recorded in the session's own folder.
Sessions are never looked for in `XDG_RUNTIME_DIR`, which differs between the owner's terminal and
an agent's service.

**A delegation tree is stopped whole.** Each child session records its parent session and its time
limit ([ADR-0034](0034-the-methods-mechanisms-native.md)). A tree's units share one slice, so one
command stops the tree and its memory can be capped.

**The guarantee obtained is shown.** Where a session cannot have its own unit, for lack of a user
manager, it runs detached with `setsid`, which survives a terminal's hangup but not the stop of
its caller's unit, and the command line says which of the two a session got. Without lingering,
the user manager stops every unit at the owner's last logout; the command line says so too.

**An agent approves only what its brief grants.** Approvals are typed by class. An agent's
approval counts only within what its brief delegates to it, bound to one call and to the hash of
the typed action, and used once. Every other approval comes from a person, through a channel
agents cannot reach.

## Alternatives considered

- **A supervisor apart, the parent of every session.** Simpler to port beyond Linux. But it dies
  at its own update, which is the failure this decision removes, and it runs at rest.
- **Sessions as children of the server or the command line, resumed from their log after a
  stop**, as Codex does. The least code. But the guarantee falls, an effect in flight is cut, and
  the model's call is billed again on resume.
- **`KillMode=process` on the server's unit.** One line. But systemd advises against it, the
  sessions stay in the server's control group, and the server no longer stops what it started.
- **Approvals open to any caller of the command line**, as ADR-0042 read. No mechanism to build.
  But a tool's output could lead a lead agent to approve a push, a payment or a network access in
  the owner's place, against ADR-0016.

## Consequences

- The session log of M2.1 holds each session's parent and time limit, and who gave each
  approval, under which grant.
- M2.3's runner is the mechanism's first user, with the opposite lifetime: a run lives in its own
  unit, so its cost is measured apart and it is stopped whole, and it dies with its runner.
- Starting a session costs a unit's start; it is measured with `pluto-agent`'s budgets at Stage 3.
- This is Linux's mechanism. macOS, Windows and Android are decided when Pluto goes public, as
  [ADR-0035](0035-the-sandbox-and-the-vault.md) sets for the sandbox.
- How a person is told apart from an agent running as the same user, so that a person-only
  approval cannot come from an agent, is designed with the vault at Stage 3. Until that channel
  exists, approvals that only a person may give are not offered: a task that needs one waits.

## Validation and reconsideration

Reviewed reasoning, not measured: no new-foundation code exists. The systemd facts are from its
manual; the Codex reports are users' and maintainers' issues.

The checks that close this decision, on test data and test profiles:
- the unit that started a session restarts while the session runs a tool: the session finishes and
  its log is whole, and the command line finds it again;
- the command line exits, or its terminal closes, while a session runs: the session goes on;
- a delegation tree is stopped while its children run: every unit of its slice stops, and each
  child's log ends with its stop;
- an agent's approval outside its brief is refused, and an approval is not accepted twice;
- without a user manager, the command line reports the weaker guarantee.

Whether GitHub's runners give a systemd user manager is not known; if not, the unit path is
checked by hand on the owner's machine at each milestone that touches it, and said so.

A new decision would be proposed to the owner if a supported platform has no equivalent of a
unit, or if a unit's start exceeds the start budget `pluto-agent` gets at Stage 3.
