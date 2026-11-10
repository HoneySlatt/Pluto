# 0024. Cuts kept in the egui client until uwui

- Status: Accepted; amends [0022](0022-the-triage-row-by-row.md) (what leaves the egui client in
  M1.4)
- Date: 2026-11-10

## Context

[ADR-0022](0022-the-triage-row-by-row.md) lists what leaves Pluto, and
[M1.4](../PLAN.md#m14--the-cuts) removes it from the egui client and the Android app. The PLAN
says that a cut whose loss would break daily work before its replacement exists is put to the
owner first. Mapping the code showed three such cuts:

- **The action editor (28.6)** is the only way, in the egui client, to add or change a project's
  actions. They are kept in the server's settings. Its replacement, actions in the project's own
  file with a small form, comes with the uwui client.
- **When-clauses (14.2)** let two shortcuts share a key. Without them, six pairs collide (Mod+D
  splits the terminal or shows the diff; Mod+W, Mod+N, Mod+Enter, Mod+Alt+Enter, Mod+1 to 9), and
  shortcuts fire inside the terminal and text fields. Their replacement, keys bound per zone, comes
  with the uwui client.
- **The custom model editor's option descriptors** are listed among what leaves, but row 16.8 is
  marked Rethink and the specification's section 6 does not cut them. Without them, a custom model
  would offer its provider's default options.

## Decision

The owner's decisions of 2026-11-10:

- The action editor stays in the egui client until the uwui client replaces it. Importing actions
  from `t3.json` and reading `t3.json` are cut.
- The when-clause builder and the Keybindings page's clause column are cut. The egui client keeps
  evaluating the when-clauses the server's bindings carry, until zones replace them.
- The custom model editor keeps its option descriptors in the egui client. A custom model becomes
  an id and a name with the uwui client, its options coming from the agent.

None of the three is built again: the uwui client and the specification are unchanged.

## Alternatives considered

- **Cutting the action editor now**: about 1,150 lines less, but the owner could no longer add or
  change an action from Pluto until the uwui client.
- **Cutting when-clauses' evaluation too**: about 100 lines less, but the six pairs above would
  collide in daily use.
- **Zones written into the egui client**: no loss, but new code in a frozen client, written twice.
- **Cutting the option descriptors now**: about 930 lines less, but custom models would lose their
  options, against row 16.8's mark.

## Consequences

- The three stay as debt of the egui client in the [PLAN](../PLAN.md#known-debt), and leave with
  it.
- The specification's section 6 still lists the action editor and when-clauses: the uwui client
  never builds them.
