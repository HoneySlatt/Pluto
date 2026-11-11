# 0028. How the open rivals and the web chats are measured

- Status: Accepted; amends [0026](0026-the-rivals-measured-how.md) (the runs per figure, LibreChat's
  source, the profiles' network)
- Date: 2026-11-11

## Context

[M1.6b2](../PLAN.md#m16b2--the-open-rivals-and-the-chats) measures six more rivals in the rivals'
bench ([ADR-0026](0026-the-rivals-measured-how.md)): OpenCode Desktop, Emdash, Nimbalyst, Jan,
Open WebUI and LibreChat. A design session on 2026-11-11 surveyed them from their own
documentation, releases and sources. A challenger agent then attacked the first recommendation.
What they found, as of that day:

- **Four desktop applications.**
  - OpenCode Desktop 2.2.7 is Electron. It starts the OpenCode server it bundles, which has an
    HTTP API that creates a session and sends a prompt.
  - Emdash 1.2.11 and Nimbalyst 0.83.0 are Electron. They drive the agents' CLIs (Claude Code among
    them) and can be driven only by the keyboard. Their updaters have no switch, and both send
    analytics (PostHog); Nimbalyst sends its start event even when analytics is refused.
  - Jan 0.8.8 is Tauri, a WebKitGTK window. It accepts an OpenAI- or Anthropic-compatible
    endpoint, but keeps its key only in the system's keyring. Its keyboard shortcuts are not
    documented. It starts its llama.cpp worker even when only a remote model is used, and
    downloads a small embedding model from Hugging Face at its first start.
- **Two web applications: a server, and a page in a browser.**
  - Open WebUI 0.13.3 is a Python server over SQLite. Its standard image loads PyTorch and an
    embedding model at start; its "slim" image, documented by the vendor, does not.
  - LibreChat 0.8.8 is a Node server that needs MongoDB, whose licence (SSPL) is not free.
    FerretDB, a stand-in, is not supported by LibreChat. LibreChat's default deployment is six
    services. The `.env` file its install copies allows two messages in flight per user.
  - nixpkgs rebuilds Open WebUI with its version pins relaxed (and, its licence being unfree,
    the build would run on our machines), and ships MongoDB 8.3 where LibreChat pins 8.0.20.
- **Every one of the six takes an endpoint**, directly or through an agent's CLI: none is measured
  at rest only for want of one.
- **The fake model has no chat turn.** It plays a turn only for a request that offers a shell or
  read tool. A chat sends no tool, so every answer would be a title, and the bench would never
  see a turn end. Inferring a chat turn from the prompt's text would also catch the agents' own
  side requests.
- **Each `wtype` or `wlrctl` call makes a new virtual input device.** Zed lost the first keys of
  2 prompts in 15 in one run, and clicks are lost. sway runs with no input device, so its own
  seat cursor would likely reach no window.
- **Nothing checks that a run's workload was equal**: that 15 streams were in flight, or that the
  subject kept one window.

## Decision

The owner's decisions of 2026-11-11.

**The browser is Chromium, counted whole.** Open WebUI and LibreChat are served by nixpkgs'
Chromium in `--app` mode: one window, a fresh profile from the frozen one, and only the switches
that stop its own background traffic and its first-run pages. A web subject's figure is the sum of
its parts (browser, server, database), each part also shown apart. A control subject,
`chromium-blank`, runs the same Chromium on an empty local page in the same campaigns. The whole
is the cost of a dedicated window; the whole less the control approaches the cost in a browser
already open. Chromium is the engine of the Electron rivals, so the difference left is mostly the
application's. No automation protocol (DevTools, WebDriver) runs during a measured run.

**The lean configuration feeds the bar; the default stands beside it.** Each server rival is
measured in the configuration its vendor documents for one user chatting with a remote
OpenAI-compatible endpoint (Open WebUI's slim image; LibreChat's API and MongoDB), at rest and
streaming. Its default deployment (Open WebUI's standard image; LibreChat's compose file) is
measured at rest only, labelled, where it differs by more than the noise. Every change from the
vendor's own template is listed in the subject's file; telemetry is turned off wherever a
documented switch exists, as for every subject. A change the workload needs is recorded as a
deviation: LibreChat's limit of two messages in flight is lifted.

**The servers come from their vendors' images, without Docker.** Open WebUI's images, LibreChat's,
`mongo:8.0.20` and the default deployment's other services are fetched once by skopeo, unpacked
by umoci into a folder named by the image's digest, and each run in bubblewrap with that folder as
its root, inside the run's network namespace and cgroups. This replaces ADR-0026's LibreChat from
nixpkgs. The desktop rivals stay the vendors' AppImages.

**One input tool holds the keyboard and the pointer for the whole run.** A small Rust program of
the bench, started before the subject, keeps one virtual keyboard and one virtual pointer, and
reads its commands from a pipe. It replaces `wtype` and `wlrctl` in every driver. It uses
`wayland-client`, `wayland-protocols-misc` and `wayland-protocols-wlr`, already built into Pluto
through eframe, as direct dependencies.

**The profiles are prepared with the network.** As ADR-0026 first said, each rival's profile is
prepared once with a route out, then frozen; measured runs stay offline. A prepared profile keeps
no pending update of its application.

**The fake model gets an explicit chat mode.** Started with `--chat`, which only the chat
subjects set, it plays a turn for a request whose last item is a user message that begins with
`[turn N]`: the turn's texts joined, without its tool calls, at the script's pace. Anything else is
a side request. Without `--chat`, nothing changes. The journal notes when each model request
begins, so that a long single answer is seen as soon as it starts.

**Every run checks that its workload was equal.** A run counts only if the 15 streams were in
flight for its whole measured window (stream scenario) and the subject showed exactly one window,
like the existing checks for noise, the GPU and a browser.

**Three runs a figure.** For M1.6b, three calm and valid runs per scenario and subject, as the
owner set when splitting M1.6b, instead of ADR-0026's five (ten for the start); each figure keeps
its spread.

**M1.6b2 is split in two.** M1.6b2a makes the bench ready and probes the six; M1.6b2b runs the
campaigns.

**Pluto's own chat is not measured now.** It is measured once the chat of
[ADR-0020](0020-the-chat.md) exists. Until then the chats are compared with Pluto's agents'
threads, which do more work; their figures say so.

## Alternatives considered

- **Firefox**: most Linux distributions' default browser, but another engine than the Electron
  rivals'. **GNOME Web (WebKitGTK)**: the lightest, Jan's engine, but rare, its background traffic
  not documented. **The server alone**, as Open WebUI publishes its figure: not comparable with a
  desktop application, which includes its interface.
- **Only the lean configuration**: fewer runs, but what most users run unknown. **Only the
  default**: what most users run, but rivals measured above their best, which loosens the bar.
- **nixpkgs for the servers**: no new tool, but not the vendors' builds, relaxed pins, an unfree
  build on every machine, MongoDB 8.3. **podman running the containers**: the simplest, but its
  processes live outside the bench's sandbox and cgroups. **FerretDB for MongoDB**: free, but not
  supported by LibreChat.
- **`wtype` per call, a written reason otherwise**: nothing to build, but Jan, Emdash and Nimbalyst
  likely without streaming figures. **DevTools or WebDriver**: precise for the web pages, but only
  for them, and a port and processes that change what is measured.
- **Offline preparation with a log of the hosts each subject tries to reach**: nothing leaves the
  machine, but Jan measured as for a user without a network.
- **A chat turn inferred from the prompt**: no flag, but it would turn the agents' own side
  requests into turns, and an existing test forbids it.
- **A `pluto-chat` subject now**: chats compared at equal work, but today's chat will be replaced.
- **One milestone**: one review, but days of work and 8 to 10 hours of campaigns without a step to
  verify. **A milestone per rival**: short steps, but six reviews and the controls measured each
  time.

## Consequences

- The bench gains tools of its own, beside Pluto: skopeo and umoci (Apache-2.0), a Secret Service
  keyring started for every subject outside its cgroup, a route out for preparing profiles, an
  input tool in Rust, and an `/etc/passwd` of the sandbox's own, so that a rival's login shell is
  not the owner's.
- About 3 GB of images, and the four AppImages ADR-0026 allowed, are downloaded once, each into a
  folder of its own with its digest.
- Preparing a profile online sends the owner's address to the vendors whose applications call home
  at start; runs stay offline, where the updaters fail as they would for a user without a network.
- Jan's, Emdash's and Nimbalyst's figures depend on drivers that type and click; each driver
  checks that each prompt reached the fake model.
- The web rivals' figures carry two readings, and the control's own figure beside them.
- Which web rival the HP runs is left to M1.6c.
