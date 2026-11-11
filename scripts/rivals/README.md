# The rivals' bench

Measures Pluto and its rivals from outside, on one workload, each in its own sandbox
([ADR-0026](../../docs/adr/0026-the-rivals-measured-how.md)). Its figures go to
[`docs/measurements.md`](../../docs/measurements.md).

## What a run is

`run.sh <scenario> <runs> <subject>...` runs each subject in turn, `runs` times, interleaved. One
run:

1. waits until the machine is calm: the rest of it uses less than `BUSY_LIMIT` of a core (0.30)
   over 5 seconds, for up to five minutes, or the run is marked noisy;
2. starts `sandbox.sh` in a systemd scope of its own, with a cgroup per part: `bench`,
   `compositor`, `fake`, `driver` and `subject`;
3. in the sandbox: a network namespace with the loopback and a pair of virtual interfaces without
   an address or a route, so that Chromium believes it is online while nothing leaves; bubblewrap
   back to the owner's uid, seeing the Nix store, the GPU and the run's folder but not `/home`; a
   throwaway home; a session bus; a headless sway at 3840 × 2160 and 240 Hz on the GPU; the fake
   model (`crates/pluto-fake-model`) on the sandbox's own loopback;
4. the subject starts, and its driver brings it to the scenario's state;
5. `pluto-measure` (`crates/pluto-measure`) samples the subject's cgroup from outside, once a
   second, for `MEASURE_SECONDS`;
6. the scope stops, with everything in it.

Scenarios:

- `rest`: 15 threads each run a short turn of the script (a command, a file read, a Markdown
  answer), the subject settles 10 seconds, then rests while it is measured, 600 seconds.
- `stream`: 15 threads start the script's long turn at once; the subject is measured for 45
  seconds while they stream.
- `probe`: for writing a subject's file: the subject starts, and screenshots of the headless screen
  show where it stands (`PROBE_WAIT`, `PROBE_KEYS` for wtype, `PROBE_CLICK` for a pointer).

Each run's folder, `OUT/<date>/<subject>/<scenario>-<n>/` (`OUT` is `/tmp/pluto-rivals`), keeps:
`figures.json` (`pluto-measure`'s), `frames.txt` (the subject's `wl_surface` commits while
measured, from sway's protocol log), `fingerprint.txt` (versions, the machine, the script's hash,
the rest of the machine's load, `noisy`), the fake model's journal `fake.jsonl`, and every log.

`summary.sh <OUT/date> <scenario>` gives each subject's medians and ranges as a Markdown table, for
[`docs/measurements.md`](../../docs/measurements.md).

## Before running

In `nix develop .#measure`, which holds the sandbox's tools, sway, wtype, grim, appimage-run and
the agents' CLIs:

```sh
cargo build --release -j2                     # the client, target/release/pluto
cargo build --release -p pluto-fake-model -p pluto-measure
cargo build --release -p pluto-core --example drive
scripts/rivals/run.sh rest 5 pluto t3code zed
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
| `subject_open N` | launches N threads of the short turns and returns once they are done |
| `subject_stream N` | launches N threads of the long turn and returns once they stream |
| `subject_stop` | stops it |

A subject is driven by its own protocol where it has one (Pluto, T3 Code), by the keyboard
otherwise (Zed), never through the owner's session. Each subject's file says how it is configured,
and which of its defaults are changed: telemetry is turned off wherever a documented switch exists,
since offline it would only retry.

## Limits

- The headless compositor's frame clock is not a display's: frames are counted, never judged on
  time ([ADR-0026](../../docs/adr/0026-the-rivals-measured-how.md)).
- `pluto-measure` misses processes that start and end between two samples; their processor time
  is in `unattributed_cpu_seconds`.
- Energy is not measured yet: it waits for the root sampler of M1.6c.
