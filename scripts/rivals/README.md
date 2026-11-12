# The rivals' bench

Measures Pluto and its rivals from outside, on one workload, each in its own sandbox
([ADR-0026](../../docs/adr/0026-the-rivals-measured-how.md),
[ADR-0028](../../docs/adr/0028-the-open-rivals-and-the-web-chats-measured.md),
[ADR-0029](../../docs/adr/0029-the-benchs-tools-and-what-a-rivals-figure-holds.md)). Its figures
go to [`docs/measurements.md`](../../docs/measurements.md).

## What a run is

`run.sh <scenario> <runs> <subject>...` runs each subject in turn, `runs` times, interleaved. One
run:

1. waits until the machine is calm: the rest of it uses less than `BUSY_LIMIT` of a core (0.30)
   over 5 seconds, for up to `CALM_WAIT` minutes (5), or the run is marked noisy;
2. starts `sandbox.sh` in a systemd scope of its own, with a cgroup per part: `bench`,
   `compositor`, `fake`, `driver` and `subject`, the subject's own parts below it;
3. in the sandbox:
   - a network namespace with the loopback and a pair of virtual interfaces without an address or
     a route, so that Chromium believes it is online while nothing leaves;
   - bubblewrap back to the owner's uid, in a process namespace of its own, with a cleared
     environment, seeing the Nix store, the GPU, the run's folder and the bench's own files, but
     not `/home`; its own `/etc/passwd`, where the subject's login shell is `/bin/sh` in its own
     home, never the owner's;
   - a throwaway home, unpacked from the subject's frozen profile;
   - a session bus, and a Secret Service on it (gnome-keyring, its login keyring unlocked);
   - a headless sway at 3840 × 2160 and 240 Hz on the GPU, with no input device of its own;
   - the input tool (`crates/pluto-input`): one virtual keyboard (a US layout) and one virtual
     pointer for the whole run, made before the subject starts; drivers send it commands through
     the run's `input` pipe;
   - the fake model (`crates/pluto-fake-model`) on the sandbox's own loopback, in its chat mode
     for a chat subject;
   - no browser for the subject to open (`xdg-open` only notes what was asked);
4. the subject starts, and its driver brings it to the scenario's state; a screenshot keeps what
   its window shows. The start to the first frame is the time from the subject's start to the
   first window the compositor maps, read from sway's window events;
5. `pluto-measure` (`crates/pluto-measure`) samples the subject's cgroup from outside, once a
   second, for `MEASURE_SECONDS`, and each of its parts apart;
6. the subject is stopped, and once none of its processes is left, its home is weighed again: the
   profile's growth is what the run added to it, the project the agents work in left out;
7. the scope stops, with everything in it; an interrupted run stops it too.

A run counts only if it is calm and valid. Valid: the subject rendered on the GPU, asked for no
browser, did not end early, showed exactly one window when it was ready and when the measure
ended, and, streaming, had its 15 answers in flight at both ends of the measure with none ended
between them, read from the fake model's journal. Its fingerprint says which check failed.

Scenarios:

- `rest`: 15 threads each run a short turn of the script (a command, a file read, a Markdown
  answer; for a chat, the same texts as one answer), the subject settles 10 seconds, then rests
  while it is measured, 600 seconds.
- `stream`: 15 threads start the script's long turn (about three minutes); once the fifteenth has
  started, one of them is opened in the window where the subject can show one
  (`subject_show`), and the subject is measured for 45 seconds while all of them stream.
- `prepare`: the subject's first start, with a route out (pasta, which forwards nothing in and
  hides the machine's own services), and what it needs then (`subject_prepare`: a welcome page
  dismissed, a project added, a provider set, a user signed in); then it is stopped, its pending
  updates and its programs' profile locks removed, and its home frozen into
  `target/rivals/profiles/<subject>/home.tar`, with its `SHA256`, the subject's version in
  `version.txt`, and in `prepared.txt` its date and when its sign-in expires. `rest` and `stream`
  start every run from it, and refuse a subject whose profile is of another version, or whose
  sign-in has expired: it is prepared again. Nothing is measured.
- `probe`: for writing a subject's file: the subject starts from its frozen profile, or from its
  setup, and screenshots show where it stands after `PROBE_WAIT` seconds, after a click
  (`PROBE_CLICK="x y"`) and after input commands (`PROBE_INPUT`, separated by `;`); its processes
  go to `processes.tsv`. With `PROBE_CONSOLE=1` the subject keeps running, and each line written
  into the run's `console` pipe runs in the driver (`shot NAME`, `click X Y`, `input ...`,
  `process_tree`, a call to its API), its output in `console.log`, until `quit`; `READY_WAIT=7200`
  gives it the time. With `ROUTE_OUT=1` it has the route out, as `prepare`.

With `FRAMES=1`, sway logs every client request and the subject's `wl_surface` commits are counted;
the log slows the compositor, so those runs' resources are not kept.

Each campaign's folder, `OUT/<date>-<time>/` (`OUT` is `/tmp/pluto-rivals`), holds a folder per run,
`<subject>/<scenario>-<n>/`, which keeps: `figures.json` (`pluto-measure`'s, with the busiest
threads named), `figures-<part>.json` for a subject split in parts, `frames.txt`, `start.txt`
(`first_window_ms`), `profile.txt` (the profile's bytes before and after, and its growth) with
`profile-after.txt` (where its bytes are), `windows.jsonl` (the compositor's window events),
`measured.png`, `fingerprint.txt` (versions and hashes, the frozen profile's hash, the input
tool's and Chromium's, the machine, the script's hash, the noise limit and the rest of the
machine's load, the validity checks, Chromium's command line for a web subject), the fake model's
journal `fake.jsonl`, and every log.

`summary.sh <campaign-folder> <scenario>` gives each subject's medians and ranges as Markdown
tables, its resources, then its start and its profile's growth, for
[`docs/measurements.md`](../../docs/measurements.md). It keeps only calm and valid runs.

## Before running

In `nix develop .#measure`, which holds the sandbox's tools (bubblewrap, pasta, sway, grim,
gnome-keyring, appimage-run, skopeo, umoci, Chromium) and the agents' CLIs, with `pluto-server`
on the PATH for Pluto's subject:

```sh
cargo build --release -j2                     # the client, target/release/pluto
cargo build --release -p pluto-fake-model -p pluto-measure -p pluto-input
cargo build --release -p pluto-core --example drive
scripts/rivals/fetch.sh jan                   # once per version of a vendor's package or image
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

The vendors' packages and images are listed in [`manifest.txt`](manifest.txt), each with its
version, its source and its digest. `fetch.sh <name>...` downloads them once, by hand, after the
owner's go, and checks each against its digest:

- an AppImage (T3 Code, OpenCode Desktop, Emdash, Nimbalyst, Jan) into an empty folder of its own,
  `target/rivals/packages/<name>-<version>/`, with its `SHA256`, unpacked into `unpacked/`;
- an archive (Zed) the same way, unpacked into `unpacked/`; a file (LibreChat's `.env.example`)
  kept as is;
- an image (Open WebUI's, LibreChat's, MongoDB's, and the services of LibreChat's default
  deployment) copied by skopeo for linux/amd64, unpacked by umoci, without root, into
  `target/rivals/images/sha256-<digest>/`. A subject runs its programs from it in bubblewrap, as a
  container would, without a container runtime (`vendors.sh`'s `run_image`): its files seen through
  a throwaway layer, its data bound from the subject's home, in the run's network and cgroups.

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
| `subject_session_days` | optional: how long the sign-in its profile holds lasts |
| `subject_threads` | optional: the answers it streams, if not 15 (the control: none) |

And it may set: `FAKE_CHAT=1` for a chat (the fake model's chat mode), `SEND_KEY` for the key that
sends a typed prompt (Return otherwise). `chromium.sh` holds the web subjects' browser, its
switches each with its reason; `vendors.sh` finds the packages and images and runs an image.

The subjects today:

| Subject | What runs | Its agent | Driven by |
|---|---|---|---|
| `pluto`, `pluto-codex`, `pluto-opencode` | the fork and the egui client | Claude Code, Codex or OpenCode of the measure shell | the fork's protocol |
| `t3code` | its AppImage | Claude Code of the measure shell | its server's protocol |
| `zed` | its archive | its own | the keyboard |
| `opencode-desktop` | its AppImage, its bundled server | the server (`opencode-cli`) | its server's API |
| `emdash` | its AppImage | Claude Code of the measure shell, in a terminal | the keyboard and the pointer |
| `nimbalyst` | its AppImage | its bundled Claude Code, through the Agent SDK | the keyboard |
| `jan` | its AppImage, through Xwayland | none: a chat | the keyboard |
| `openwebui`, `openwebui-default` | its slim, or standard, image, and Chromium | none: a chat | the keyboard and the pointer |
| `librechat`, `librechat-default` | its API and MongoDB, or its six services, and Chromium | none: a chat | the keyboard and the pointer |
| `chromium-blank` | Chromium on an empty page | none: the web rivals' control | nothing |

The `-default` subjects are measured at rest only. Each subject's file says how it is configured,
which of its defaults are changed and why, what its first start does, and what its probe found:
telemetry is turned off wherever a documented switch exists, since offline it would only retry.

A subject is driven by its own protocol where it has one, by its keyboard and pointer otherwise,
never through the owner's session. A driver that types checks that each prompt reached the fake
model (its first request's begin line in the journal), and types it once more if it did not
(`lib.sh`'s `typed_turn`). A driver that clicks names places of the 3840 × 2160 output, found at
its probe: a new version that moves them is probed again.

## Limits

- The headless compositor's frame clock is not a display's: frames are counted, never judged on
  time ([ADR-0026](../../docs/adr/0026-the-rivals-measured-how.md)).
- Under a heavy load, T3 Code's server has sent nothing for a minute to `drive` just connected,
  cause not known; its driver then tries once more, if no thread reached the fake model.
- `pluto-measure` misses processes that start and end between two samples; their processor time
  is in `unattributed_cpu_seconds`.
- Energy is not measured yet: it waits for the root sampler of M1.6c.
- The pointer's places are the windows' layout of each version; a moved button makes a driver's
  prompt check fail, which stops the run rather than measure it wrong.
- Preparing a profile online sends the owner's address to the vendors whose applications call home
  at their first start ([ADR-0028](../../docs/adr/0028-the-open-rivals-and-the-web-chats-measured.md)).

## Its tools and their licences

None is linked into Pluto; each runs beside it, from the measure shell: bubblewrap (LGPL-2.0 or
later), passt's pasta (BSD-3-Clause, GPL-2.0 or later), sway (MIT), iproute2 (GPL-2.0), grim
(MIT), gnome-keyring (LGPL-2.1 or later, GPL-2.0 or later), appimage-run (nixpkgs, MIT) with
libgpg-error (LGPL-2.1 or later), skopeo and umoci (Apache-2.0), Chromium (BSD-3-Clause), and the
agents' CLIs under their own terms (Claude Code: Anthropic's; Codex: Apache-2.0; OpenCode: MIT).
The input tool is the bench's own (`crates/pluto-input`, MIT or Apache-2.0).
