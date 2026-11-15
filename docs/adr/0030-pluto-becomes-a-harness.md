# 0030. Pluto becomes a harness: one kernel, two products

- Status: Accepted; amends [0003](0003-what-pluto-is-to-become.md) (a harness of Pluto's own at its
  core; a personal assistant as a second product); amended by [0041](0041-the-assistant-on-every-plan-opened-to-third-parties.md) (what the assistant runs on) and [0042](0042-a-cli-first-the-tui-with-the-assistant.md) (the TUI at Stage 7)
- Date: 2026-11-15

## Context

Until 2026-11-15 Pluto was to relay other vendors' agents through a server of its own
([ADR-0004](0004-plutos-own-server-in-rust.md)), with a small harness of its own for local models
after Stage 3 of the ROADMAP of 2026-11-08 ([ADR-0011](0011-plutos-own-harness-minimal-and-measured.md)). A harness is the loop,
tools, prompts and context management that make a model an agent. In a design session on
2026-11-15, the owner asked for a real harness at Pluto's core. The owner gave four reasons:
- control over the brain: prompts, tools, context, memory;
- no dependence on vendors' churn;
- less weight;
- agents that behave alike.

The owner's needs, from the interview:
- everything configurable; extensions; separable pieces; skills, hooks and agents as text;
- design as a first-class space: interface mockups, design systems, graphics and logos, a canvas;
- owner first, public later; surfaces: the uwui desktop client, a complete TUI, a phone that does
  everything;
- the owner's delegation method native at the core;
- judged on public benchmarks, context cost, speed and resources, local models equal for all;
- every mode of autonomy: followed closely, overnight milestones, triggered, always there;
- a context inspector at every turn;
- spaces: deep research with replayable sources, browser and computer use, publishing a page;
  editing the real running app or mockup visually, tools that show their own UI, recording a
  know-how as a skill;
- a full personal assistant: a morning brief and monitoring; mail, calendar and lists; a second
  brain of notes as files; the machine and the NAS; full voice conversation, local.

Still held from earlier decisions: the market bar
([ADR-0017](0017-pluto-measured-against-the-market.md)), the triage and the
[specification](../specification.md), the chat ([ADR-0020](0020-the-chat.md)), Rust without Node,
a server with clients and a relay ([ADR-0007](0007-several-servers-and-a-relay.md)), the history
imported, Android in Kotlin first.

What the surveys of 2026-11-15 found:
- **The field converged** on one local service under thin clients: OpenCode 2.0, Codex's
  app-server, Claude Code's headless mode, Pi's RPC, and Anthropic's split of session, harness and
  sandbox ([Managed Agents](https://www.anthropic.com/engineering/managed-agents)).
- **No stable Rust harness library exists.** Codex's core is unpublished and released four stable
  minors in 15 days; goose's is an alpha every two weeks. The stable surfaces are protocols: MCP,
  ACP, Codex's app-server.
- **The personal agents** (OpenClaw, Hermes, Claude Duet, ChatGPT Dot, Grok Bot) are popular for
  being reachable, persistent and scheduled. Their weak points are trust, cost and reliability:
  - 341 of 2,857 skills on OpenClaw's registry were malicious
    ([eSecurityPlanet](https://www.esecurityplanet.com/threats/hundreds-of-malicious-skills-found-in-openclaws-clawhub/));
  - one Dot spent over 300,000 tokens in a day with no prompt
    ([The Register](https://www.theregister.com/ai-and-ml/2026/10/13/openais-dot-assistant-burned-300k-tokens-in-a-day-without-a-single-prompt/5302913));
  - a finance agent posted bank balances to a company channel, because all its owner's agents
    shared one set of connections
    ([Business Insider](https://www.businessinsider.com/personal-ai-agent-grok-bot-posted-bank-details-company-slack-2026-10)).
- **Anthropic's consumer terms** forbid access "through automated or non-human means" without an
  API key or permission ([Consumer Terms](https://www.anthropic.com/legal/consumer-terms)). An
  always-on assistant is such a use.
- **Two assistants are written in Rust**, ZeroClaw and IronClaw; they are to be read before
  Pluto's is built.

## Decision

**Pluto's core is a harness kernel of its own.** Seven pieces:
- the session's log ([ADR-0033](0033-one-pluto-agent-process-per-session.md));
- Pluto's loop, as a crate ([ADR-0032](0032-plutos-loop-and-its-provider-clients.md));
- the engines ([ADR-0031](0031-three-kinds-of-engine.md));
- tools in a sandbox, with a vault outside it ([ADR-0035](0035-the-sandbox-and-the-vault.md));
- the delegation object ([ADR-0034](0034-the-methods-mechanisms-native.md));
- a scheduler, whose jobs may be scripts alone;
- the bench, the gate on each component ([ADR-0037](0037-how-the-harness-is-judged.md)).

**Two products on that kernel.**
- **The work comes first**: code, chat, design and orchestration.
- **The personal assistant comes later**, as its own daemon, with its own system user, vault,
  budget and rights. It hands coding work to the first product.

Both show in the same application.

**The assistant's rules**, set now so the kernel allows them:
- It never deletes without a way back.
- It spends money or sends a message on the owner's behalf only after an explicit approval. The
  approval shows the action, the target and every recipient.
- It runs on API keys, DeepSeek or local models, never on a Claude subscription.
- Mail goes through IMAP. Tuta's mail goes through TutaBridge, a separate program.
- Its notes are plain files.
- It runs on the desktop first, and on a NAS or a server later.
- It speaks through Pluto's own applications.

**The lead's implementation choices**, from the challenge, shown to the owner with this record:
- the approval is checked outside the model, as Claude Duet does;
- the assistant wakes on schedules and events, never by polling; its watchers are scripts until
  one signals, and only then is a model called.

## Alternatives considered

- **Keep relaying the vendors' agents** ([ADR-0004](0004-plutos-own-server-in-rust.md),
  [ADR-0011](0011-plutos-own-harness-minimal-and-measured.md)). Less to build. But none of the
  owner's four reasons is met: the brain, the churn and the weight stay the vendors'.
- **One product: the assistant in the work's daemon.** One process. But the two differ in trust
  (mail is untrusted input to an agent that can send), in rest (the work draws nothing at rest; an
  assistant never sleeps for long), in terms and in cost. One boundary for both repeats the finance
  agent's incident.
- **The assistant first.** It is what made the personal agents popular. But the owner's daily
  friction is in the work: delegation's bookkeeping, usage limits, context read again.
- **A base taken from another harness.**
  - A Codex fork repeats the T3 trap: the fork never merged upstream.
  - goose is an alpha every two weeks.
  - Pi's RPC needs Node.

## Consequences

- [ADR-0003](0003-what-pluto-is-to-become.md) holds, except where this ADR and the next ones
  change it. Pluto's differences gain one: a harness of its own, measured.
- The ROADMAP's stages follow the order of [ADR-0039](0039-the-order-of-the-stages.md). The
  assistant is the last stage before going public.
- "Nothing runs at rest" holds for the work. For the assistant it reads: nothing runs between its
  triggers.
- Open for the assistant's stage: Tuta's calendar (TutaBridge serves only mail); where it runs once
  it leaves the desktop; Signal and Matrix as channels, an open path, not planned.
