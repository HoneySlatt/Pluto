# 0045. Features from other harnesses' add-ons, adapted, never installed

- Status: Accepted
- Date: 2026-11-17

## Context

Users of other harnesses add much of what they use through add-ons: plugins, skills, hooks, MCP
servers, extensions and companion tools. A survey of 2026-11-17 saved about 1,800 pages, primary
ones first, across thirteen areas:

- the vendors' marketplaces: Claude Code's directory, Codex, Gemini CLI, Copilot;
- the open agents' hubs: Hermes, OpenClaw's ClawHub, Pi's packages, OpenCode, goose, Cline, Zed;
- companion tools: front ends, status lines, notifiers, phone apps, worktree managers;
- chat front ends and local-model apps: Open WebUI, LibreChat, AnythingLLM, Jan, LM Studio, Msty;
- what users add around agents: named subagents, rule libraries, routers, sandboxes, CI agents,
  editors' and terminals' agent features;
- connectors to outside services, across assistants and integration platforms;
- knowledge and personal tools: Obsidian's AI plugins, second brains, Home Assistant, browser
  sidebars, mail;
- the tools the owner works with: Unreal, Blender, Tripo, Bevy, Godot, OBS; DAWs, notation and
  video editors; Nix, Rust, Android, Penpot and Figma, Wayland.

Every figure of demand (installs, downloads, stars) was read on its page and checked by sample.
The owner then took each feature one at a time: keep, later, or drop.

## Decision

**Pluto builds what it keeps in itself; it installs or runs no add-on as it is.** Pluto opens no
plugin system to others; its architecture only keeps the way open. An add-on's text or code may be
adapted where its licence is MIT or Apache-2.0, its notice in `NOTICE.md`; under any other licence,
or none, only its idea is taken.

**The kept features are in [`specification.md`](../specification.md#8-features-from-other-harnesses-add-ons)**,
each with its stage and its platforms. A feature that works on one platform only is kept with that
limit stated ([ADR-0044](0044-five-platforms-at-going-public.md)). Each feature that touches the
loop or the context stays only while the bench shows its gain
([ADR-0037](0037-how-the-harness-is-judged.md)).

**The features left for later are ideas**, in the *Ideas* of [`PLAN.md`](../PLAN.md#ideas): kept
in sight, not planned.

**Refused, so that no session proposes them again:**

- **A live terminal tool for agents** (an agent typing into gdb or a database shell, with handoff):
  the owner's choice.
- **Nix builds run by an agent**, even read-only: the owner's choice; the owner builds and switches.
- **OBS driven by Pluto**: the owner's choice; engines record themselves.
- **Video read by sampled frames and a transcript; audio analysis as text; notation by music21 or
  LilyPond**: the owner's choice.
- **Chat-app bridges through a third party** (Telegram, Discord, Slack): the app's server sees
  everything, against "nothing leaves but the models' traffic"; the phone app covers the need.
- **All-day memory of the screen and the voice**: it runs all day and holds everything private.
- **An AI classifier as an approval barrier**: approvals are checked outside the model
  ([ADR-0030](0030-pluto-becomes-a-harness.md),
  [ADR-0043](0043-each-session-in-its-own-unit.md)).
- **Proxies that turn a subscription into an API**: against the providers' terms
  ([ADR-0031](0031-three-kinds-of-engine.md)).
- **Team sharing** before Pluto goes public: Pluto is for its owner first.
- **Self-learning loops**: agents write skills only as files the owner reviews
  ([ADR-0034](0034-the-methods-mechanisms-native.md)).
- **Catalogues of ready-made roles**: text paid at every turn with no measured gain; Pluto reads
  the named agents the owner writes.
- **Integration hubs such as Zapier, Make or Composio**: data and tokens pass through a third party.
- **CRM, payment and media connectors**: no use now; an MCP server the owner adds covers a need.
- **Remote images loaded by a skill**: telemetry by image.

## Alternatives considered

- **A plugin system of Pluto's own, open to others.** The ecosystem's breadth. But the survey's
  security record (malicious skills on ClawHub and Snyk's 13.4 % of skills with a critical issue,
  pins bypassed, plugins broken by every major release) and the cost of a stable public API, for a
  product made for its owner first.
- **Installing the best add-ons as they are.** The least work. But each brings its host's
  assumptions, its telemetry, its licence and its churn.
- **Everything kept at once, from the survey's ranking.** Faster. But the owner sets the scope,
  feature by feature.

## Consequences

- The specification gains a section of kept features, and the ROADMAP's stages point to it.
- The PLAN's draft list of the same day is replaced by the ideas the owner left for later.
- What the architecture must allow for extensions later is decided apart, with M2.0.
- The survey's reports and proofs stay on the owner's machine; the figures quoted in the
  specification are those of 2026-11-17.

## Validation and reconsideration

Reviewed reasoning and the owner's choices, not measures: no kept feature is built. Demand
figures are each page's own, as of 2026-11-17; several are self-reported or rounded.

A feature is reconsidered when the bench shows it costs more than it gives, when its platform's
rules change, or on the owner's word.
