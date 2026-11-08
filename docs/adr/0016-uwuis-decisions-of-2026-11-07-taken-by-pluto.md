# 0016. uwui's decisions of 2026-11-07, taken by Pluto

- Status: Accepted; amends [0001](0001-a-native-client-on-egui-until-uwui.md) (Pluto depends on
  uwui from uwui's own repository), [0003](0003-what-pluto-is-to-become.md) (the look over uwui's
  core look; uwui's rules for applications in the bar; macOS decided when Pluto goes public) and
  [0009](0009-secrets-privacy-and-the-sandbox.md) (uwui's applications as a right of the sandbox;
  actions kept for a person); amended by [0017](0017-pluto-measured-against-the-market.md)
  (rivals measured from Stage 1, the margin set now)
- Date: 2026-11-07

## Context

On 2026-11-07 the owner took decisions on uwui, in SlimeEngine's ADR-0213 to ADR-0220. uwui's
roadmap took them the same day. Pluto stands on both sides of them: its client is one of uwui's
pilots, and the agents it drives may drive programs on uwui. What they change for Pluto:

- **Looks.** uwui's core ships one sober look: light, dark and of high contrast, in three
  densities. Glass, Material 3 Expressive and Pro become official themes, each a crate of its own.
  Nothing is chosen in silence, and a test checks that applications on uwui cannot be recognised by
  their look (SlimeEngine's ADR-0213, ADR-0215).
- **Platforms.** uwui aims at macOS for its 1.0, with packages signed under a paid Apple developer
  account. macOS's host comes after uwui's first stage, the one in which Pluto's client is a pilot.
  iOS is a trial only. An application that uses only uwui's APIs runs on Linux, Windows, macOS and
  the web with no line written for one platform, and uwui covers files, settings, the network and
  long work (ADR-0213). Pluto's server does not run on uwui. Its sandbox (bubblewrap), its keyring
  (Secret Service) and its terminals are tied to Linux today.
- **The bar.** uwui now applies its own rules to every application built on it (ADR-0215):
  - at least 99.9 % of hours of use without a crash, and no work lost after one;
  - compiling budgets;
  - input timed to the photon;
  - blind tests.

  uwui also asks that its own cost be at most a third of the best rival's (ADR-0213). Pluto's bar
  aims at "a fraction" of its rivals' memory, but no rival has been measured.
- **Agents.** uwui gets one core for agents (ADR-0213, ADR-0218):
  - a first message that names the agent, and the person's consent, agent by agent;
  - numbered requests, and events;
  - attaching to a program already running;
  - actions declared once, each with its permission;
  - rights, previews and tools found by families.

  A local MCP server lets a user's assistant call an application's actions. The agents Pluto drives
  speak MCP. They will be able to drive any application on uwui that the owner has open, and
  Pluto's own client once it runs on uwui.
- **The licence.** uwui stays under MIT OR Apache-2.0 (ADR-0214). Until SlimeEngine's M6d.11, it
  still depends on crates of the engine, which are now under the engine's own licence.

## Decision

The owner's decisions of 2026-11-07.

**Agents reach uwui's applications only where the sandbox allows it.** Reaching an application's
agent endpoint is a right of the sandbox of its own, apart from files and the network. It is closed
in the locked mode, and so for unattended tasks. The owner opens it per project or per thread. The
consent that uwui asks for, given in the application, is needed on top of it.

**What only a person may do in Pluto is never offered to agents.** When Pluto's client runs on uwui
and declares its actions, four of them stay with the person, whatever consent an agent holds:
approving an agent's request, changing a sandbox, revealing a masked secret and pairing a device.
Pluto asks uwui for a mark that keeps an action from every agent
([`uwui-needs.md`](../uwui-needs.md#pluto-as-an-agent-and-as-an-application-driven)).

**Pluto's bar takes uwui's rules for applications.** The client and the server keep at least 99.9 %
of hours of use without a crash, and lose no work after one, such as a draft in the composer. The
rivals of the quality bar are measured, on the fixture, or on the owner's accounts with the owner's
consent. The margin by which Pluto must beat them is set from those figures when Stage 6 is
designed.

**macOS is decided when Pluto goes public.** The client follows uwui's platforms. Porting the
server's sandbox, keyring and terminals to macOS joins the open decision on going public, beside
Windows. Nothing is built for it before. The code stays free of what would tie it to Linux, as
ADR-0003 asks.

**Pluto's look sits on uwui's core look.** Pluto's theme and density extend uwui's own look. Glass
is an option, through uwui's official theme Glass or the glass material that stays in uwui's core.

**Pluto depends on uwui only from uwui's own repository.** That is once SlimeEngine's M6d.11 has
moved uwui there, under MIT OR Apache-2.0 and free of the engine's crates. The licence check of
`cargo deny` in `scripts/check.sh` refuses anything else.

## Alternatives considered

- **uwui's consent alone**: simpler, but an unattended task could act on the owner's open
  applications once an agent had been accepted.
- **Agents never reaching uwui's applications**: no risk, but Pluto could never drive Opium or the
  engine's editor.
- **Every action open to agents, with an explicit right**: more automation, but an agent could widen
  its own rights, or approve its own requests.
- **Pluto's client declaring no action**: the simplest, but no assistant could start a thread or
  read Pluto's state.
- **A lead of three times at once, as uwui's**: a clear ambition, but a figure set before any rival
  is measured.
- **The bar kept as it is**: nothing more to measure, but a lead claimed, not shown.
- **macOS aimed at now, the server included**: it matters in this market, where Conductor, the Codex
  app and Claude's desktop app run. But it is work before any user, without a Mac with Apple's chips
  to check it.
- **macOS excluded**: nothing to maintain, but part of the market closed if Pluto goes public.

## Consequences

- [`uwui-needs.md`](../uwui-needs.md) asks uwui's core for four things for agents, before
  SlimeEngine's M6d.9a and M6d.9b fix rights and declarations:
  - actions kept for a person;
  - consent that is scoped and revocable;
  - an identity that names the harness;
  - a way to find the applications open on Linux.
- The sandbox's design, in Stage 3, holds a right for uwui's applications.
- The quality bar gains hours without a crash and no work lost. The rivals are measured before
  Stage 6 is designed; a measure on the owner's accounts waits for the owner's consent.
- The open decision on going public names macOS.
- An idea is kept in the open decisions, not built: Pluto as the owner's assistant on the desktop,
  calling the actions of uwui's applications, where Linux has no such mechanism. It is decided
  after Stage 4, once uwui's local MCP server exists.
