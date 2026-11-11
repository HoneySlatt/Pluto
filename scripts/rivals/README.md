# The rivals' bench

Measures Pluto and its rivals from outside, on one workload, each in its own sandbox
([ADR-0026](../../docs/adr/0026-the-rivals-measured-how.md)). Its figures go to
[`docs/measurements.md`](../../docs/measurements.md).

## What a run is

`run.sh <scenario> <runs> <subject>...` runs each subject in turn, `runs` times, interleaved. One
run:

1. waits until the machine is calm: the rest of it uses less than `BUSY_LIMIT` of a core (0.30)
   over 5 seconds, for up to `CALM_WAIT` minutes (5), or the run is marked noisy;
2. starts `sandbox.sh` in a systemd scope of its own, with a cgroup per part: `bench`,
   `compositor`, `fake`, `driver` and `subject`;
3. in the sandbox: a network namespace with the loopback and a pair of virtual interfaces without
   an address or a route, so that Chromium believes it is online while nothing leaves; bubblewrap
   back to the owner's uid, in a process namespace of its own, with a cleared environment, seeing
   the Nix store, the GPU, the run's folder and the bench's own files, but not `/home`; a
   throwaway home, unpacked from the subject's frozen profile; a session bus; a headless sway at 3840 × 2160 and 240 Hz on the GPU; the fake
   model (`crates/pluto-fake-model`) on the sandbox's own loopback; no browser (`xdg-open` only
   notes what was asked);
4. the subject starts, and its driver brings it to the scenario's state; a screenshot keeps what
   its window shows. The start to the first frame is the time from the subject's start to the
   first window the compositor maps, read from sway's window events;
5. `pluto-measure` (`crates/pluto-measure`) samples the subject's cgroup from outside, once a
   second, for `MEASURE_SECONDS`;
6. the subject is stopped, and once none of its processes is left, its home is weighed again: the
   profile's growth is what the run added to it, the project the agents work in left out;
7. the scope stops, with everything in it; an interrupted run stops it too.

Scenarios:

- `rest`: 15 threads each run a short turn of the script (a command, a file read, a Markdown
  answer), the subject settles 10 seconds, then rests while it is measured, 600 seconds.
- `stream`: 15 threads start the script's long turn (about three minutes); once the fifteenth has
  started, one of them is opened in the window where the subject can show one
  (`subject_show`), and the subject is measured for 45 seconds while all of them stream.
- `prepare`: the subject's first start, and what it needs then (`subject_prepare`: a welcome page
  dismissed, a project added); then it is stopped and its home is frozen into
  `target/rivals/profiles/<subject>/home.tar`, with its `SHA256` and the subject's version in
  `version.txt`. `rest` and `stream` start every run from it, and refuse a subject whose profile
  is of another version: each new version is prepared again
  ([ADR-0026](../../docs/adr/0026-the-rivals-measured-how.md)). Nothing is measured. The
  profiles are prepared offline; none of today's subjects needs the network for its first start.
- `probe`: for writing a subject's file: the subject starts, and screenshots of the headless screen
  show where it stands (`PROBE_WAIT`, `PROBE_KEYS` for wtype, `PROBE_CLICK` for a pointer).

With `FRAMES=1`, sway logs every client request and the subject's `wl_surface` commits are counted;
the log slows the compositor, so those runs' resources are not kept.

Each campaign's folder, `OUT/<date>-<time>/` (`OUT` is `/tmp/pluto-rivals`), holds a folder per run,
`<subject>/<scenario>-<n>/`, which keeps: `figures.json` (`pluto-measure`'s, with the busiest
threads named), `figures-<part>.json` for a subject split in parts, `frames.txt`, `start.txt`
(`first_window_ms`), `profile.txt` (the profile's bytes before and after, and its growth) with
`profile-after.txt` (where its bytes are), `windows.jsonl` (the compositor's window events),
`measured.png`, `fingerprint.txt` (versions and hashes, the frozen profile's hash, the machine,
the script's hash, the noise limit and the rest of the machine's load, whether the GPU rendered,
whether a browser was asked for, an early end), the fake model's journal `fake.jsonl`, and every
log.

`summary.sh <campaign-folder> <scenario>` gives each subject's medians and ranges as Markdown
tables, its resources, then its start and its profile's growth, for [`docs/measurements.md`](../../docs/measurements.md). It keeps only calm runs: not
noisy, rendered on the GPU, no browser, no early end.

## Before running

In `nix develop .#measure`, which holds the sandbox's tools, sway, wtype, grim, wlrctl,
appimage-run and the agents' CLIs, with `pluto-server` on the PATH for Pluto's subject:

```sh
cargo build --release -j2                     # the client, target/release/pluto
cargo build --release -p pluto-fake-model -p pluto-measure
cargo build --release -p pluto-core --example drive
scripts/rivals/run.sh prepare 1 pluto t3code zed  # once per version of a subject
BUSY_LIMIT=0.6 scripts/rivals/run.sh rest 3 pluto t3code zed
```

A campaign runs as a systemd user service of its own, never inside an agent's session, whose
restart would stop it:

```sh
systemd-run --user --unit=pluto-rivals-campaign --working-directory="$PWD" --collect \
    -p StandardOutput=file:/tmp/pluto-rivals/campaign.log \
    -p StandardError=file:/tmp/pluto-rivals/campaign.log \
    nix develop .#measure -c env BUSY_LIMIT=0.6 scripts/rivals/run.sh rest 3 pluto t3code zed
```

The vendors' packages are downloaded once, by hand, after the owner's go, each into an empty
folder of its own under `target/rivals/packages/<name>-<version>/`, with its `SHA256`:

- T3 Code: the AppImage of its GitHub release, unpacked there with
  `appimage-run -x unpacked <AppImage>`;
- Zed: `zed-linux-x86_64.tar.gz` of its GitHub release, unpacked into `unpacked/`.

## A subject's file

`subjects/<name>.sh` defines, for `sandbox.sh` and `lib.sh`:

| Function | Does |
|---|---|
| `subject_version` | prints its version and its package's hash, for the fingerprint |
| `subject_binds` | prints the paths outside the store and the repository it needs, one a line |
| `subject_agents` | prints the names of the agents' executables, which `pluto-measure` counts apart |
| `subject_parts` | prints the sub-cgroups it splits into, each measured apart too (Pluto: `client server`) |
| `subject_setup` | writes its configuration in the throwaway home, pointed at `$FAKE_URL` |
| `subject_start` | starts it (in the `subject` cgroup); it must `exec` its main program |
| `subject_prepare` | optional: what its first start needs before its home is frozen (`prepare`); 20 s otherwise |
| `subject_open N` | launches N threads of the short turns and returns once they are done |
| `subject_stream N` | launches N threads of the long turn and returns once they stream |
| `subject_show` | optional: opens one streaming thread in its window, when its threads were launched outside it |
| `subject_stop` | stops it |

The subjects today: `pluto` (the fork and the egui client, Claude Code as the agent),
`pluto-codex` and `pluto-opencode` (the same, Codex or OpenCode as the agent; OpenCode reaches the
fake model over Chat Completions), `t3code` and `zed`.

A subject is driven by its own protocol where it has one (Pluto, T3 Code), by the keyboard
otherwise (Zed), never through the owner's session. A driver that types checks that each prompt
reached the fake model, and types it once more if it did not. The pointer is not used: each
`wlrctl` call is a new virtual pointer, whose click sway may handle before the window has the
pointer's focus. Each subject's file says how it is configured, and which of its defaults are
changed: telemetry is turned off wherever a documented switch exists, since offline it would only
retry.

## Limits

- The headless compositor's frame clock is not a display's: frames are counted, never judged on
  time ([ADR-0026](../../docs/adr/0026-the-rivals-measured-how.md)).
- `pluto-measure` misses processes that start and end between two samples; their processor time
  is in `unattributed_cpu_seconds`.
- Energy is not measured yet: it waits for the root sampler of M1.6c.

## Its tools and their licences

None is linked into Pluto; each runs beside it, from the measure shell: bubblewrap (LGPL-2.0 or
later), sway (MIT), iproute2 (GPL-2.0), wtype (MIT), grim (MIT), wlrctl (MIT), appimage-run
(nixpkgs, MIT), and the agents' CLIs under their own terms (Claude Code: Anthropic's; Codex:
Apache-2.0; OpenCode: MIT).
