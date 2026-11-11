# Measurements

The figures measured against the budgets, with how each was taken. The product's budgets are in
the [ROADMAP](ROADMAP.md)'s quality bar, a milestone's own in its [PLAN](PLAN.md) entry; this file
holds what was measured, and when.

## The check

Budget: `scripts/check.sh` takes at most 20 minutes, locally and in CI with its cache
([M1.1](PLAN.md#m11--under-the-method)). A CI run without the cache, the first of a branch or after
GitHub drops a cache unused for a while, may take longer: the owner accepted about 30 minutes on
2026-11-07.

| Where | Build | Duration | Date |
|---|---|---|---|
| The owner's desktop, 16 threads | cold: an empty `target/`, crates already downloaded | 5 min 50 s | 2026-11-07 |
| The owner's desktop, 16 threads | warm: nothing changed since the last run | 7.8 s | 2026-11-07 |
| The owner's desktop, 16 threads | `--apk` in `nix develop .#mobile`: the Kotlin tests and the APK added, the desktop's build warm | 3 min 56 s | 2026-11-07 |
| CI, `ubuntu-24.04`, private repository | cold: no cache | 30 min 07 s, accepted | 2026-11-07 |
| CI, `ubuntu-24.04`, private repository | cold: the first run on `main`, no cache for it | 23 min 01 s; the job 23 min 46 s | 2026-11-07 |
| CI, `ubuntu-24.04`, private repository | warm: the cache of the run before | 1 min 51 s; the job 3 min 00 s | 2026-11-07 |

How: the wall time of `nix develop --no-update-lock-file --command scripts/check.sh`, the Nix
shell already built; cold with `CARGO_TARGET_DIR` set to an empty directory. In the cold run,
clippy's build took 2 min 13 s, and most of the rest went to building the tests. cargo-deny
fetches the advisory database each time, which needs the network.

In CI, the script's step of the workflow, from its log
([run 42337377344](https://github.com/HoneySlatt/Pluto/actions/runs/42337377344), commit
`8d3761a`): the Nix shell 35 s, clippy 8 min, building and running the tests 21 min 30 s, cargo-deny
and the links 3 s. Saving the cache took 35 s more. Most of the time goes to the dependencies,
built at `opt-level = 2` (`Cargo.toml`, `[profile.dev.package."*"]`) on a runner far smaller than
the desktop.

The first run on `main` ([run 42340373676](https://github.com/HoneySlatt/Pluto/actions/runs/42340373676),
commit `d0d79f5`) restored no cache: a run restores only those of its own branch and of the default
branch, and every run before it was on another branch.

The warm run ([run 42339784714](https://github.com/HoneySlatt/Pluto/actions/runs/42339784714),
commit `c0d0671`): restoring the cache 59 s, the Nix shell 38 s, clippy 17 s, the tests 54 s,
cargo-deny and the links 2 s.

## How the client and the fork are measured

`scripts/measure.sh`, in `nix develop .#measure`, with `scripts/fixture-server.sh` running, takes
every figure below except the database's ([ADR-0014](adr/0014-how-today-is-measured.md)). Its head
says what each step does; it writes its raw output to `/tmp/pluto-measure`. The client runs in a
headless weston, 1920 × 1080 at scale 1, in its default window of 1440 × 900, with a throwaway home
and session bus; turns are blocked. The frame bench, `examples/bench.rs`, runs without a window, in
a screen of 1440 × 900 points at scale 1, and counts each frame's CPU time only: neither the GPU's
work nor the compositor's. A window of the high tier's 3840 × 2160 lays out more rows a frame, so
the costs there are likely higher than these.

The fixture, since M1.2: the threads of before, plus "Fil de 5 000 messages" (2,500 turns of a
prompt, a command and a Markdown answer with a list, a code block and a table), and "Flux 01" to
"Flux 15", three turns each, for the streams (`scripts/seed-fixture.mjs`).

Every figure of the sections below: the owner's desktop, the high tier, 2026-11-07, the client at
`d0d79f5` with M1.2's measuring hooks.

## The client at rest

Budget: 0 frames and 0 wake-ups of its own a second; at most 100 MB resident with 15 threads open.
At rest here: the 15 "Flux" threads and the 5,000-message one opened in turn, 5 seconds to settle,
then 60 seconds counted. The long thread is held as it opens, on its last ten turns. Timers of a
longer period, such as the core's five-minute retention of threads left, fall outside the 60
seconds.

| What | Today |
|---|---|
| Frames | 4 in 60 s, one of them the measurement's own: 0.05 a second |
| Wake-ups | 3.2 a second: AccessKit's D-Bus thread `blocking-1` 2.0, the core's connection thread 1.1, the clipboard's 0.07, the interface's thread 0.05 |
| Resident | 143 MB: 95 MB of mapped files, 48 MB anonymous |
| The heap, live | 34 MB (32.1 MiB), split below |

Of the 4 frames, one is the measurement's own timer; which of the client's 46
`request_repaint_after` calls (counted in `src/`) asked for the 3 others is not known. The connection
thread's wake-up a second is known debt ([PLAN](PLAN.md#known-debt)). `blocking-1` belongs to zbus,
which only AccessKit, egui's accessibility, brings in; it woke as often in the first runs, before
the sandbox had a session bus of its own, as with one.

The mapped files are the binary, 74.5 MB unstripped, and the libraries, the GPU driver's among
them. The anonymous memory is the heap, plus the allocator's slack, the threads' stacks and the
driver's own mappings.

The heap at rest, by what holds it (heaptrack, `heap_split` in the script):

| What holds it | MiB | Share |
|---|---|---|
| Fonts: their files, read and copied, and the glyph atlas | 12.69 | 39.6 % |
| Syntax highlighting: syntect's syntaxes and their compiled expressions | 8.12 | 25.3 % |
| The GPU driver and wgpu, their CPU side | 6.35 | 19.8 % |
| Text laid out (egui's galleys) | 1.84 | 5.8 % |
| The timeline and the server's state | 1.65 | 5.1 % |
| Images and icons | 1.04 | 3.2 % |
| The rest | 0.37 | 1.1 % |
| In all | 32.05 | |

The fonts' share holds the same bytes several times: `install_fonts` (`src/theme.rs`) reads font
files, then egui copies each one again (`blob_from_font_data`, 4.9 MiB). The timeline's share is
small because each thread opens on its last ten turns.

## The start

Budget: the first useful frame warm in 100 ms, cold in 500 ms, high tier. The first useful frame is
the first one drawn with the server's projects and threads; its time ends when its painting does,
before the compositor presents it.

| Part | Warm | Cold |
|---|---|---|
| Before `main`: the binary loaded and linked | 2.0 ms | 11.4 ms |
| From `main`: the window and the GPU ready | 66 ms | 125 ms |
| From `main`: the first useful frame painted, median of 10 | 123 ms | 184 ms |
| In all | 125 ms | 195 ms |

Warm is a start after the last one, every file cached. Cold drops from the page cache every file
the client maps, those other programs map excepted; it is not a reboot. Files the client reads
without mapping them stay cached: the system's symbol fonts read by `src/theme.rs`, the `fc-match`
it runs, fontconfig's caches. The cold figure is therefore optimistic. Between the GPU ready and
the useful frame, the frame bench's `startup` lines put 19.5 ms in `App::new` and 31 ms more until
the shell is shown: the connection and the server's first answers. (In the run before, the frame
bench ran before anything had filled the throwaway home's font cache: `App::new` took 212 ms. The
script now starts the client once first.) In each of two runs of the script, two warm starts of ten
painted it near 330 to 365 ms, and one cold start once. Their likely cause, not proven: a probe of
the frames' times found the frame after the useful one beginning 183 ms after it, its own CPU work
done in 5 ms, which points at the headless weston's pacing. The median is not moved by them.

## A long thread and streams

Budget: 5,000 messages scroll at the display's rate, p99 within 4.17 ms on the high tier and 16.7
ms on the HP; 15 threads streaming at once, no frame over budget at p99. The frame bench's figures
are each frame's UI pass in milliseconds, at 1440 × 900; tessellation adds 0.1 to 0.4 ms.

| Scenario | p50 | p99 | max |
|---|---|---|---|
| The 60-turn thread, scrolled | 0.47 | 0.88 | 1.84 |
| The 5,000-message thread, its pages loading | 2.76 | 15.27 | 28.09 |
| The 5,000-message thread, scrolled at 120 points a frame | 7.07 | 8.52 | 14.16 |
| 15 streams, the open thread "Flux 01" | 0.49 | 2.41 | 3.24 |
| 15 streams, scrolling meanwhile | 0.47 | 3.63 | 7.14 |
| 15 streams, the 5,000-message thread open | 0.51 | 1.74 | 2.40 |
| 15 streams, the 5,000-message thread open and scrolled | 0.54 | 3.54 | 5.53 |

The streams replay what a server sends while 15 threads run: the open thread's answer grows ten
times a second to 15,000 characters, and the 14 others' sidebar rows change as often, 150 events a
second in all. The owner's busiest day averaged about one event a second
([the database](#the-forks-database)). The frame bench resident at its end: 256 MB, the 5,000
messages loaded. Loading all their pages took 22.4 s in this run and 5.0 s in the one before; where
that time goes, the fork or the client, is not known. The loading row's p99 also depends on how
many cheap frames fall between two pages.

The long thread misses its budget: scrolling it costs 7 to 8.5 ms a frame, against 0.5 ms for the
60-turn thread; loading its pages costs up to 28 ms a frame. The streams stay within it.

Until 2026-11-07 the bench turned the wheel to each thread's end before measuring it, to work
around a fault of the client since fixed ([CHANGELOG](../CHANGELOG.md)). It no longer does. Run
alternately three times on the desktop at rest, with the fix, the long thread scrolled at a p50 of
7.60 to 7.68 ms with that step and 7.50 to 7.67 ms without: no difference. Without the step, the
streams' answer is on screen in 594 of 600 frames, and was in none before the fix; why the 6 others
miss is not known. The streams' times stay within the table's.

## The fork

Budget: at most 80 MB resident at rest, agents' processes and Chromium excluded; 0 wake-ups of its
own.

| Which fork | Resident | Threads | Wake-ups a second |
|---|---|---|---|
| The fixture's, before the client, served nothing since its start | 394 MB | 13 | 4.4 |
| The fixture's, restarted, a minute idle (by hand, before the script read it) | 474 MB | 13 | not counted |
| The fixture's, after serving the measurements | 889 to 892 MB | 13 | 6 to 30 over five readings, the main thread most |
| The owner's service, up an hour, five agents' processes attached | 455 MB | 13 | 228: the main thread 133, libuv's workers 85 |

The fixture's fork keeps the memory of what it served: the 5,000-message thread's pages above all.
Right after the import that builds the fixture it held 300 MB more, which is why
`scripts/fixture-server.sh` restarts it. The owner's service was in use, so its figure is not one at
rest; it was read from `/proc/<pid>/status` and `pidstat` only. Its agents' processes held 1.1 GB
more.

## The fork's database

`statev2.sqlite` of the owner's fork, 2,990,153,728 bytes (2.79 GiB) on 2026-11-07, examined on a
copy taken with `sqlite3 -readonly … ".backup"`, then deleted. Only its tables' sizes and the kinds
and sizes of its rows were read.

| Part | Tables | Indexes | In all |
|---|---|---|---|
| The event log, `orchestration_events` | 1,597 MiB | 318 MiB | 1,915 MiB, 67 % |
| The projections, `orchestration_v2_projection_*` and the turn items' positions | 770 MiB | 157 MiB | 927 MiB, 33 % |
| Everything else | 4 MiB | 4 MiB | 8 MiB |

The event log holds 508,717 events of 596 threads; their payloads weigh 1,035 MiB, their metadata
104 MiB. By kind:

| Event | Events | Payloads | Average |
|---|---|---|---|
| `turn-item.updated` | 222,462 | 708 MiB | 3.3 KB |
| `node.updated` | 132,501 | 123 MiB | 1.0 KB |
| `provider-turn.updated` | 70,971 | 67 MiB | 1.0 KB |
| `message.updated` | 23,049 | 57 MiB | 2.6 KB |
| `subagent.updated` | 17,778 | 25 MiB | 1.5 KB |
| `context-handoff.updated` | 128 | 14 MiB | 112 KB |
| The other kinds | 41,828 | 42 MiB | |

What fills it:

- **Every update repeats the whole item.** The 104,891 turn items were written 222,462 times: an
  item's states before its last weigh 299 MiB, its last 409 MiB. The projections then keep the last
  state a second time, 554 MiB of turn items.
- **Tools' output.** Command runs, MCP tools and file changes are 313 MiB of the projected turn
  items; one MCP tool's output weighs 1 MiB.
- **Reasoning while it runs**: 126 MiB of `reasoning` items in their running states, superseded by
  their final text.

Its growth: nearly every event was written in the last 12 days of use, since the fork imported
the history (3,670 are older). Those days wrote about 95 MiB of payloads and metadata a day, 198
MiB on the busiest. The whole file laid over them, indexes and projections included, is about 240
MiB a day. The threads hold 17,053 messages; the longest thread 3,785.

## The cuts of M1.4

The client before and after [M1.4](PLAN.md#m14--the-cuts) removed what the triage cut, on the
desktop, by `scripts/measure.sh` on the fixture, as above: before at `86a601b`, after at `d688416`,
on 2026-11-10. The code loses about 7,500 lines of Rust net (9,089 removed, 1,628 added).

| What | Before | After |
|---|---|---|
| Frames at rest | 4 in 60 s | 4 in 60 s |
| Wake-ups at rest | 3.25 a second: `blocking-1` 2.03, the connection 1.10 | 3.28 a second: `blocking-1` 2.00, the connection 1.07 |
| Resident at rest | 139.5 MB: 92.8 MB mapped files, 46.6 MB anonymous | 139.4 MB: 92.8 MB mapped files, 46.5 MB anonymous |
| The heap at rest | 32.09 MiB | 32.19 MiB |
| The binary | 74,669,104 bytes | 73,913,504 bytes, 58,967,912 stripped |

The cuts leave memory and wake-ups where they were: what they removed was code never run at
rest, and the heap is the fonts, syntax highlighting and the GPU driver's, as before. The binary
loses 0.76 MB, 1 %. The wake-ups differ within the noise of one run.

The start is not compared. On the second run the machine was loaded by other builds, and the
first useful frame came after 1.7 s in the median, against 150 ms before. Run in turn in the same
sandbox, the client of `86a601b` and that of `d688416` both took 0.7 to 2.3 s, a start of the one
as slow as a start of the other. That shows the load dominated, not that the cuts left the start
unchanged. The start is to be measured again on an idle machine. The binary's stripped size before
the cuts was not taken.

## The rivals' bench, M1.6a

Pluto and two rivals measured in the rivals' bench on 2026-11-11, on the desktop, by
`scripts/rivals/run.sh` ([its README](../scripts/rivals/README.md),
[ADR-0026](adr/0026-the-rivals-measured-how.md)): each run in a sandbox of its own, a headless sway
at 3840 × 2160 on the GPU, the fake model's workload (`workload.json`, sha256 `5435fbb0310993d5`),
`pluto-measure` sampling the subject from outside once a second. Five runs of each scenario,
interleaved; the median, then the range in brackets.

| Subject | Version | What runs |
|---|---|---|
| Pluto | client `04087f4`, the fork `pluto-server` 0.1.0 | the egui client and the fork; Claude Code 2.1.311 as its agent; Codex 0.164 writes the threads' titles |
| T3 Code | 0.1.8, the vendor's AppImage (sha256 `e117698be4cc59e5`), through nixpkgs' `appimage-run` | Electron, its bundled server; Claude Code 2.1.311 as its agent |
| Zed | 1.27.0, the vendor's archive (sha256 `719f6bf649ea8d87`), in the same FHS environment | Zed's own agent, on an Anthropic-compatible provider |

The machine: kernel 7.3.1, Mesa 26.2.6, governor `powersave` with `balance_performance`. Every
subject's defaults are kept, except: telemetry off where a documented switch exists (T3 Code, the
fork, Claude Code, Zed), Zed's tools allowed without asking and its worktrees trusted, and two
shortcuts of the bench's own in Zed's keymap. A run is marked noisy when the rest of the machine
used more than 0.6 of a core (`BUSY_LIMIT=0.6`); the owner worked on the machine meanwhile.

### The first campaign, not kept

A first campaign on 2026-11-11 measured the three subjects five times at rest and five times
streaming. Its review found its resource figures unfit to keep, so they are left out until the
runs are made again:

- T3 Code's welcome page was not dismissed in 9 runs of 10: the keys opened its privacy link in a
  browser inside the sandbox, whose processes the figures count.
- The subjects did not stream alike during the measure: about 15 streams on average for T3 Code,
  13.5 for Pluto, 7 for Zed, whose prompts were typed one by one.
- Pluto's figures did not agree with M1.2's: the fork's resident memory, 342 MiB in the median, is
  below M1.2's range, and the client's frames at rest, 6 every 1.25 s, are not yet explained.

What each request carries, below, does not depend on these: the fake model journals it whatever
the load.

### What each request carries

From the fake model's journal of the same runs, in bytes, which stand for tokens until Stage 2
([ADR-0026](adr/0026-the-rivals-measured-how.md)). A turn's first request; then every other
request of the 15 short turns, titles and summaries among them.

| Subject | A turn's first request | Of it: instructions | Of it: tools | Tools offered | Other requests for 15 turns |
|---|---|---|---|---|---|
| Pluto (Claude Code through the fork) | 62,595 | 14,618 | 33,859 | 13 | 135 requests, 3,366,871 bytes |
| T3 Code (Claude Code) | 61,722 | 11,640 | 35,529 | 14 | 90 requests, 1,174,185 bytes |
| Zed (its own agent) | 47,092 | 15,993 | 30,884 | 16 | 15 requests, 13,960 bytes |

Pluto's other requests are 45 asked of Codex, which writes the fork's titles, about 54 KB each,
three a thread; and 90 of Claude Code's own, about 10 KB each, two a thread, which T3 Code's 90
(about 13 KB each) match. Zed asks one title of about 0.9 KB a thread.

### The packages

| Subject | Download | On disk |
|---|---|---|
| Pluto | the client's binary, 58,967,144 bytes stripped; the fork's package, 367,246,223 bytes in the Nix store | 426 MB, Node in the fork's 2.8 GB closure |
| T3 Code | AppImage, 214,364,906 bytes | 612,654,994 bytes unpacked |
| Zed | archive, 90,705,601 bytes | 239,519,318 bytes unpacked |

### What is not measured yet

- The start to the first frame and the profile's growth: the bench does not take them yet.
- Frames on time: the headless compositor's clock is no display's
  ([ADR-0026](adr/0026-the-rivals-measured-how.md)); frames are counted only.
- Energy: it waits for the root sampler of M1.6c.

## The budgets M1.2 set

Six budgets of the [quality bar](ROADMAP.md#quality-bar) had no figure; M1.2 set them, and the owner
validated them on 2026-11-07 ([ADR-0015](adr/0015-the-budgets-set-from-todays-figures.md)). Today:

| Budget | Today |
|---|---|
| Input to photon | not measured: the client does not ask for the compositor's presentation times |
| The binary's size | the client, 73.9 MB, 59.0 MB stripped, its icons and fonts within ([M1.4](#the-cuts-of-m14)) |
| An event's latency, server to client | not measured: the fixture runs no turn |
| The history's import | 2.79 GiB, 596 threads, 508,717 events; the final states, which [ADR-0008](adr/0008-storage-and-the-history-moved.md) imports, are 927 MiB of it |
| An idle agent's resume | not measured: it needs a real agent |
| The database's growth | about 240 MiB a day of use, 95 MiB of it events ([the database](#the-forks-database)) |

## The HP

The owner runs these on the HP Laptop 17-ca2xxx; the figures go in the sections above, one row
each, marked HP. The first two steps download and build much (the fixture's server is 3.5 GB);
with 4 GB of memory, build with one job at a time. `scripts/fixture-server.sh` needs `node` and
`pluto-server` on the PATH, as on the desktop.

```sh
nix develop .#measure                        # the tools, once
scripts/fixture-server.sh                    # the fixture, on :3999
CARGO_BUILD_JOBS=1 SKIP_HEAP=1 scripts/measure.sh /tmp/pluto-measure-hp
```

Then the budget "the client, a server and three agents without swap", by hand, in a working
session: the owner's server, the client, and three agents running turns.

```sh
grep -E '^(MemTotal|MemAvailable|SwapTotal|SwapFree)' /proc/meminfo   # before
vmstat 5 120 > /tmp/hp-vmstat.txt                                     # ten minutes of work
awk 'NR > 3 { si += $7; so += $8 } END { print "swapped in", si, "out", so, "(KiB/s, summed)" }' /tmp/hp-vmstat.txt
grep -E '^(MemAvailable|SwapFree)' /proc/meminfo                      # after
```

And what the quality bar does not know of it yet, its display and its storage:

```sh
for m in /sys/class/drm/card*-*/modes; do echo "$m: $(head -1 "$m")"; done
lsblk -d -o NAME,ROTA,TRAN,MODEL,SIZE
```

None of these has been run yet: they were not tried on the HP.
