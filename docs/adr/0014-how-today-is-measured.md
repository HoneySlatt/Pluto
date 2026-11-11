# 0014. How today is measured

- Status: Accepted; amended by [0026](0026-the-rivals-measured-how.md) (the rivals' bench beside
  `scripts/measure.sh`)
- Date: 2026-11-07

## Context

[M1.2](../PLAN.md#m12--today-measured) asks for today's figure beside every budget of the quality
bar, taken on the fixture server and never on the owner's data. The client is frozen
([ADR-0010](0010-todays-features-triaged.md)): measuring may add tools beside it, not change what
it does. Until then the only tool was `examples/bench.rs`, which runs the client without a window,
and the fixture could no longer be built: its seed needs T3 Code v0.0.45's server, which had left
the Nix store.

## Decision

**A shell of its own for the measurements.** `nix develop .#measure` adds heaptrack
(LGPL-2.1-or-later) for the heap by call stack, hyperfine (MIT or Apache-2.0) for repeated starts,
sysstat's pidstat (GPL-2.0-or-later) for each thread's wake-ups, and weston (MIT) as a headless
compositor. They run beside the client; none is linked into it, so their licenses do not reach
Pluto. CI never enters this shell, so it does not download them.

**The fixture's old server pinned.** The flake's input `nixpkgs-t3-fixture` is the last nixpkgs
with T3 Code v0.0.45, pinned to a revision so that updates never move it. Its package,
`fixture-t3`, is built only by `scripts/fixture-server.sh`, and the link `target/fixture-t3` keeps
it from garbage collection.

**One script takes the figures.** `scripts/measure.sh` runs the frame bench, the start, the rest and the
heap against the fixture, and measures the fork serving it. The owner runs the same script on the
HP, without the heap (`SKIP_HEAP=1`). The frame bench runs in a screen of 1440 × 900 points, not
the high tier's 3840 × 2160.

**The real client in a sandbox.** `examples/live.rs` runs the client in eframe's real window, as
`pluto` does, with turns blocked and no settings file. The script gives it a headless weston, a
throwaway home, configuration and runtime directory, a session bus of its own, and no `DISPLAY`:
it reaches neither the owner's session nor the owner's files.

**The first useful frame** is the first one drawn with the server's projects and threads. The start
is the time before `main` (hyperfine on `live` without arguments, which exits at once), plus the
time from `main` to the end of that frame's painting: its UI pass, plus the CPU time eframe reports
for it. The compositor's presentation is not counted: the headless weston's pacing is no guide to
a real compositor's.

**A cold start** drops the files the client maps from the page cache (`dd iflag=nocache`) before
each run. Files that other running programs map stay cached, as after a reboot into a running
desktop; so do the GPU's shader caches on disk, and the files the client reads without mapping
them (the system's symbol fonts, `fc-match`, fontconfig's caches).

**At rest** is the client after it opened the fixture's 15 "Flux" threads and its 5,000-message
thread in turn, let them settle 5 seconds, then did nothing for 60. A wake-up is a context switch,
counted per thread by `pidstat -w -t`; the example counts its frames itself.

**The heap at rest** is what heaptrack sees still allocated when the client exits from its rest. A
pattern table in the script sorts each allocation's stack into fonts, syntax highlighting, the GPU's
own, the timeline and the server's state, text laid out, images, and the rest.

**Streams are replayed in the frame bench**, `examples/bench.rs`. The fixture has no provider, and
no recorded stream exists until [M1.5](../PLAN.md#m15--agents-sessions-recorded). It feeds the client the events a
server sends while 15 threads stream: the open thread's answer growing, the 14 others' shell rows,
each ten times a second.

## Alternatives considered

- **dhat-rs as the allocator of an example**: the same split by allocation site, but a new
  dependency in the client's build; heaptrack needs no change to the code.
- **valgrind's massif**: tens of times slower, which distorts a client driven by a server's timing.
- **A script over `/proc` instead of pidstat, a shell loop instead of hyperfine**: two tools fewer,
  but two scripts to maintain that redo what those tools do; the owner chose the tools.
- **Measuring without a window, as the frame bench does**: no wgpu, no driver, no event loop; the memory
  and the start would be underestimated.
- **cage or sway, headless**: weston's headless backend takes its size on the command line and
  needs no configuration file.
- **A seed writing protocol 2 events directly**: no 3.5 GB download, but a rewrite of the seed
  against the fork's schema, which Pluto's own server replaces at Stage 2.
- **The present counted in the start**: only meaningful on a real compositor, which the rules keep
  the measurements away from.

## Consequences

- The first `nix develop .#measure` downloads about 2 GB, heaptrack's graphical side and weston
  among it; the first fixture, 3.5 GB more.
- The start's figure stops at the painted frame: what the owner's compositor adds is not measured.
- A cold start here is not a reboot: the shared libraries other programs map stay cached.
- The streams are synthetic until M1.5 records real ones; their rate is an upper bound.
- The figures, and how to read them, are in [`measurements.md`](../measurements.md).
