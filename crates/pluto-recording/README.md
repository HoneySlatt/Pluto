# pluto-recording

Agents' sessions recorded and replayed, so that Pluto's drivers are tested without a real turn
([ADR-0025](../../docs/adr/0025-agents-sessions-recorded-and-replayed.md)).

- `pluto-record` sits between a server and an agent's CLI. It masks each line of the traffic,
  writes it to a recording, then forwards it.
- `pluto-replay-agent` plays a recording in the CLI's place.
- `fork-harness/` drives the fork's real adapters through scripted scenarios, recording or
  replaying. The fork's own test runner runs it, from outside the fork.
- `recordings/<agent>/<scenario>.jsonl` are the owner's recordings. Beside each,
  `<scenario>.thread.json` is the thread its live run showed: every replay must give it exactly.

The format, the masking, the scan and the replay's rules are in the module documentation of
`src/`. How the parts fit is in [ARCHITECTURE.md](../../docs/ARCHITECTURE.md#agents-sessions-recorded).

## The checks

- `scripts/check.sh`, in CI: every recording, and every synthetic one, reads at its version, the
  scan finds nothing in it or its reference, and it replays with a client that chooses other ids.
  The stamp (`recordings/fork-replay.sha256`) still matches what a replay reads, as
  `stamped_files` and `dependencies_line` in `scripts/fork-replay.sh` list it.
- `scripts/check.sh --fork`, on the owner's machine only: the harness's self-test on its
  synthetic recordings (`fork-harness/synthetic/`), then every recording replayed through the
  fork's adapters. It writes the stamp. Any change to what the stamp covers needs this run, or
  CI fails.

## Recording real sessions

Only the owner records: each scenario starts a real agent and spends a little of its quota.
Agents never run these commands for real; the dry run below is anyone's.

The scenarios, for Claude: `turn`, `approval`, `question`, `interrupt`, `resume`. For Grok, the
same and `wake`. Each sends short prompts in a fresh synthetic project. It approves, answers the
first option, or interrupts by itself, at a fixed point.

1. Once, sign in to the agents in the disposable home (`~/.local/state/pluto-recording/home`,
   or `$PLUTO_RECORDING_HOME`). Your real home, settings and memory are never used.

   ```sh
   nix develop --command scripts/record-session.sh login
   ```

   Claude starts, in an empty folder: answer its first questions, sign in (type `/login` if it
   does not ask; open the link it prints), then leave with `/exit`. Grok starts next: sign in as
   it asks, then quit. No message is needed. The sign-ins stay in the disposable home.
2. Record. Each scenario takes a minute or less. A review of each recording follows. Read its
   *values to read*: every string of 8 characters or more outside the conversation's text, once
   each; the protocols' own per-frame ids and times are only counted. An account id, an
   e-mail or a name of yours there means the recording must not be accepted: tell an agent, who
   extends the masking.

   ```sh
   nix develop --command scripts/record-session.sh claude all
   nix develop --command scripts/record-session.sh grok all
   ```

   One scenario alone: `scripts/record-session.sh claude interrupt`. A scenario that fails says
   why; run it again, or tell an agent what it printed.
3. Accept. This checks every staged recording, moves them into `recordings/`, replays
   everything through the fork, and writes the stamp. If that replay fails, the recordings stay in
   `recordings/` with an old stamp, and CI fails until it is fixed: tell an agent what it printed.

   ```sh
   nix develop --command scripts/record-session.sh accept
   ```
4. Commit `crates/pluto-recording/recordings/`, or ask an agent to.

When a recording's scan finds something, the script refuses it. Record it again; if it persists,
an agent extends the masking (`src/mask.rs`), with a test. A rule added later reaches the
recordings already made with `pluto-record remask FILE...`, which changes only what the new rule
covers; their replay through the fork then writes the stamp again.

A scenario that fails says so and stops the series. Its recording, the adapter's events and the
error stay in the run's folder, which the harness names (under the system's temporary folder), and
never in the staging folder. Each scenario works in a fixed project folder,
`/tmp/pluto-recording/<agent>-<scenario>`: a path the agent streams in pieces cannot become
`<workspace>`, and a random folder name in it would make the scan refuse the recording.

Anyone can check the procedure without a real agent: with `PLUTO_RECORDING_DRY_RUN=1`, `claude`
and `grok` record stand-ins that play the synthetic recordings, in a throwaway home, and
`accept` only checks what is staged.

## Limits

- The synthetic recordings are written by hand. They prove the chain, not what the real CLIs
  send: only the owner's recordings do. Whether the real agents follow the scenarios' order
  (approve, then interrupt once the tool runs) is seen only then.
- A Grok recording's prompt frame holds the fork's own instructions, compared whole on replay: a
  client other than the fork must send the same text. Which part a Rust driver must send is
  decided with Stage 2's ACP driver.
- A reference is versioned (`pluto-thread-reference`, version 1). A change to what the harness's
  thread view keeps raises its version and needs a migration in the harness, or new recordings:
  a reference comes from a live run and is never rebuilt from its recording.
- The recordings hold the text the fork sends to the agents, T3 Code's instructions (MIT; its
  notice is in [`NOTICE.md`](../../NOTICE.md)).
- A Grok session ends killed: the fork's ACP runtime kills the agent's processes when it closes
  a session. Such a recording has no exit record; its replay ends once every record is consumed.
- Claude's folder-name form of a path and the path itself both become `<workspace>`; a replay
  gives back the plain path in both places.
- Masking may miss a shape it does not know; the scan, with detectors of its own, and the owner's
  reading of each summary are the last defences.
