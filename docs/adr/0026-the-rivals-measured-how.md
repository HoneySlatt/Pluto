# 0026. How the rivals are measured

- Status: Accepted; amends [0014](0014-how-today-is-measured.md) (the rivals' bench beside
  `scripts/measure.sh`), [0015](0015-the-budgets-set-from-todays-figures.md) (memory counted as
  private, swap and GPU memory) and [0017](0017-pluto-measured-against-the-market.md) (the vendors'
  applications without the owner's accounts; energy by a root sampler); amended by
  [0028](0028-the-open-rivals-and-the-web-chats-measured.md)
- Date: 2026-11-11

## Context

[ADR-0017](0017-pluto-measured-against-the-market.md) asks that every rival be measured on one
workload from a scripted fake model service, in a disposable, isolated profile, so that the quality
bar holds the stricter of Pluto's budgets and the best rival's figure with its margin.
[M1.6](../PLAN.md#m16a--the-fake-model-and-the-rivals-bench) does it first. A design session on
2026-11-11 surveyed the rivals, the mock model servers and how to measure another program from
outside; a challenger agent attacked the first recommendation. What it found:

- The fork passes its environment to every agent's CLI, so Pluto runs real turns of Claude Code,
  Codex and OpenCode against any endpoint, with no change to the fork and no account.
- The rivals speak four APIs: Anthropic's Messages, OpenAI's Responses (Codex accepts nothing
  else), OpenAI's Chat Completions, and Ollama's. One mock covers all four, aimock (TypeScript,
  a year old); none counts tokens exactly; none is in nixpkgs.
- Claude's desktop application exists on Linux in beta, with a "3P" mode that talks to a Messages
  gateway without signing in. ChatGPT's, which holds Codex, exists on Linux in preview; Codex takes
  a provider of the user's. Warp sends its custom endpoint's calls from Warp's servers and rejects
  local addresses. T3 Chat and TypingMind are hosted. LM Studio serves models; it is not a client.
- No rival's licence or terms forbids publishing benchmarks.
- nixpkgs lags the vendors by weeks and rebuilds some of them (Zed from source, OpenCode Desktop on
  nixpkgs' Electron).
- A headless compositor cannot judge frames on time at 240 Hz: weston runs at 60 Hz by default,
  and wlroots' headless timer counts whole milliseconds and fakes the presentation.
- An Electron application in a network namespace with only the loopback thinks it is offline, and
  without `/dev/dri` it renders in software: either would rig the comparison.
- Building a 5,000-message thread by 2,500 turns resends the history each time, about 4 GB, and
  agents compact long before the end, so each application would end with a different thread.
- The processor's energy counters (RAPL) are readable only by root since 2020, against a side
  channel attack; AMD's are a model, reported inaccurate. Summing resident memory over an Electron
  application's processes counts its shared libraries many times, and GPU memory is in no resident
  figure.
- The desktop is the owner's daily machine; a game was running during the design.

## Decision

The owner's decisions of 2026-11-11.

**Three milestones.** M1.6a builds the fake model service and the rivals' bench, calibrated on
Pluto, which must find M1.2's figures again, then on T3 Code and Zed. M1.6b measures the other
open rivals and the chats, with the long thread and real days. M1.6c measures the downloaded
rivals, energy and frames, and tightens the bar.

**The fake model service is a Rust crate of Pluto's**, `crates/pluto-fake-model`, synchronous, one
thread per connection, its requests parsed by `httparse` (MIT or Apache-2.0, already built through
tungstenite), its responses written by hand. It speaks Messages, Responses and Ollama from M1.6a,
Chat Completions from M1.6b. It plays one script, the same answer bytes to every application, and
counts what it receives, by bytes, in parts: instructions, tool definitions, history, the new
message. It stays for Stage 2's budgets on tokens and the chat's delay, and for Stage 4's adapters.

**The vendors' own packages**, at their version of the day: the AppImage, `.deb` or archive each
vendor publishes, run under NixOS through nixpkgs' `appimage-run` or an FHS environment, its hash
and version kept with each figure. The owner allowed on 2026-11-11 the download of: T3 Code, Zed,
OpenCode Desktop, Emdash, Nimbalyst; Claude's and ChatGPT's desktop applications; Jan, Msty Studio,
Open WebUI; Warp and LM Studio. LibreChat comes from nixpkgs, which has the version of the day.
Each package goes into an empty directory of its own and runs only in the bench's sandbox.

**The vendors' applications without the owner's accounts.** Claude's desktop application is
measured in its 3P mode, ChatGPT's through Codex with a provider of Pluto's, both on the fake
model, on Linux, and labelled so in every figure. ChatGPT's chat, which takes no endpoint, is
measured at rest only. Nothing runs in the owner's Windows machine.

**Each run in bubblewrap.** The rival sees neither `/home` nor the owner's files: a disposable home,
copied from a profile prepared once with the network and then frozen; a network namespace with no
route out, where an interface without a route makes Chromium believe it is online; the fake model
in the same namespace; `/dev/dri` bound, and each run checked to render on the GPU. One headless
sway, at 3840 × 2160, for every subject, Pluto included. A systemd scope with a cgroup per part
(the rival, the fake model, the compositor) counts everything. The rival's own sandboxes are kept;
any `--no-sandbox` is recorded as a deviation.

**Memory is private memory, swap and GPU memory**, summed over every process of the application.
Proportional (PSS) and resident (RSS) figures are given beside it. The quality bar's memory budgets
are read with this definition.

**Frames: a stand-in now, the light sensor later.** In the headless compositor, the bench measures
the frames drawn at rest, which must be none, and each frame's production time, from the
compositor's frame callback to the application's commit, in runs apart from the resource figures.
Frames on time at the display's rate wait for the light sensor; it is debt until then.

**Energy by a root sampler.** A small root service in the owner's NixOS reads RAPL and writes only
each run's totals; the lead prepares it, the owner builds it. The HP's battery measures the laptop.
Desktop figures carry AMD's inaccuracy, stated beside them.

**Runs.** At least 5 runs per figure, 10 for the start, in interleaved order, the median and its
spread kept; real days of 15 threads, several rivals side by side, each in its sandbox. The lead
starts runs during a session when the machine is calm; the bench stops a run when other work
loads the processor. Each figure keeps its fingerprint: the rival's version and hash, the kernel,
Mesa, the compositor's settings, the processor's governor, the fake model's script and the bench's
version. On the HP, the owner runs one command: Pluto and the best rivals, client, server and three
agents, at rest and under load, on battery.

## Alternatives considered

- **One milestone, or two**: fewer reviews, but days of work without a verifiable step.
- **aimock**: done and covering the four APIs, but young, under Node outside Nix, estimating
  tokens, and not Pluto's. **tiny_http**: less HTTP by hand, but unreleased since October 2022.
- **nixpkgs only**: no download, but versions weeks old, some rebuilt, four rivals missing.
- **The owner's accounts, ChatGPT in the Windows machine**: real use, but quota spent, no equal
  workload, and Windows is not the reference platform.
- **The home redirected only, as `measure.sh`**: simpler, but a closed application could read the
  owner's files by absolute path. **A virtual machine per rival**: the GPU virtualised, the figures
  distorted.
- **Resident memory, as M1.2**: what users quote, but it inflates applications of many processes
  and ignores the GPU. **PSS and the GPU**: common, but it depends on what else runs.
- **Frames by hand on the owner's compositor**: real presentations, but rivals' windows on the
  owner's desktop. **No frame figure until Stage 6**: the bar without any smoothness figure.
- **RAPL readable by a group**: reopens the side channel to every program the owner runs.
  **A wall meter and the HP's battery**: the whole machine, recommended, but a purchase.
  **The HP alone**: no desktop figure.
- **Unattended nights started by the owner**: steadier figures, but the milestone waits on the
  owner each night. **A day composed from measured parts**: much shorter, but it hides the slow
  leaks and the idle agents that the rivals are reported for.
- **The long thread built by turns**: real, but 4 GB of requests and compaction mid-way; it is
  seeded in each application's own format instead.

## Consequences

- The ROADMAP's memory budgets are read as private, swap and GPU memory; Pluto's own figure is
  measured again that way by M1.6a.
- Run during sessions, figures are noisier than on an idle night; the bench's guard and the medians
  bound it, and each figure says when it was taken.
- Frames on time and input to photon of the rivals wait for the light sensor.
- The energy sampler is a change to the owner's NixOS, built by the owner; until then no desktop
  energy figure exists.
- Each rival's profile is prepared once with the network, then frozen; preparing it again with
  each new version is part of each stage's measurement.
- Bytes stand for tokens until Stage 2 brings a tokenizer.
