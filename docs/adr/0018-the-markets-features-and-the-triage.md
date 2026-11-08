# 0018. The market's features, and the triage by family

- Status: Accepted; amends [0002](0002-the-android-app-until-uwui.md) (the phone's notifications
  through UnifiedPush, its own models), [0003](0003-what-pluto-is-to-become.md) (review in Pluto;
  triggers, one configuration for every agent, side questions, dictation, cloud runs and computer
  use added), [0007](0007-several-servers-and-a-relay.md) (the phone's notifications decided now;
  Tailscale as one more path) and [0009](0009-secrets-privacy-and-the-sandbox.md) (computer use in
  an isolated session; triggers run locked)
- Date: 2026-11-08

## Context

The interview of 2026-11-08 ([ADR-0017](0017-pluto-measured-against-the-market.md), which names its
record) went on to the features. In late 2026 the field shares a standard set: parallel agents in worktrees, diff review
with comments sent to the agent, a browser the agent drives, scheduled and event-triggered
automations, the phone's control, cloud runs, pull requests followed to their merge, plan mode,
subagents, side chats and voice. Gaps no rival fills: one configuration of MCP servers, skills and
instructions across every agent, and a phone that needs no vendor's cloud.

[ADR-0010](0010-todays-features-triaged.md) asks that every feature of today be marked keep, cut
or rethink. The inventory groups them into 31 families.

## Decision

The owner's decisions of 2026-11-08.

**Review in Pluto.** Pluto has the best review view of the market: unified and side-by-side diffs,
thousands of lines smooth, comments on lines batched and sent at once to the thread's agent or to
another provider's, the cross-agent review of ADR-0003. Pull requests stay followed on the forge.
This replaces ADR-0003's "review happens on the forge".

**Triggers on the owner's own events.** Scheduled tasks also start on events: a CI that fails, a
comment on a pull request, a webhook. Only the owner's own events trigger, and the agent runs in
the locked sandbox. Events come through the owner's relay ([ADR-0007](0007-several-servers-and-a-relay.md)),
or, without it, from a poll of GitHub that runs only while a pull request is followed, never at
rest. A pull request can be followed to its merge, the CI fixed and remarks answered, with the
owner's go before each push.

**One configuration for every agent.** MCP servers, skills and instructions are declared once in
Pluto, per machine, project or thread. Pluto gives them to each agent in its own format when a
session starts, without touching the agent's own files. What an agent cannot receive is shown as
absent.

**Side questions.** A question asked beside a thread does not turn its agent from its work nor
fill its context.

**Local dictation**, planned rather than an idea: a model downloaded on demand, the speech
recognised on the owner's machine.

**Runs in the vendors' clouds**, optional and off by default, turned on per provider. The promise
of no account and no cloud holds for Pluto itself; a cloud run is the agent's own traffic, chosen
by the owner and shown. The recommendation was against it; the owner chose it.

**Computer use, in an isolated session by default.** An agent drives a session apart, a headless
compositor or a virtual machine, holding only the applications opened for it, and the owner watches
it in a tab of Pluto, as the browser. The owner's own desktop only with the owner's explicit leave
for one task, never an unattended one, with an emergency stop on the keyboard. The recommendation
was against computer use; the owner chose it, and chose the isolated session first.

**The triage by family.** Each of the 30 families of the desktop is marked in
[`features.md`](../features.md): 11 kept, 19 rethought, none cut. A family kept goes to the
specification as it is. A family rethought keeps its need; its form is decided row by row in
`features.md`, each row with its problem, the rival that does it best, and whether it is core or
optional.

**The phone.** A survey of the phone's market on 2026-11-08 found that no major application is
confirmed to deliver notifications in the background without Google's services, which GrapheneOS
lacks by default; that only Paseo, Happy and T3 Code reach the owner's machines without a vendor's
cloud, each with gaps; and that only one battery figure is published, about 10 % an hour for
Claude's Remote Control, by one user. The owner's decisions:

- **Its rivals:** Claude's and ChatGPT's remote control, measured with the owner's go per account;
  T3 Code's mobile application; Happy; Conductor and Nimbalyst on paper, being iOS only. Paseo,
  missing from the list put to the owner, was added after the survey and told to the owner.
- **Its budgets** take the desktop's families and margins: cold start, 99.9 % of frames on time,
  the battery over an hour of a thread streaming on the fake model, the network's data per hour,
  memory, and nothing in the background outside the system's schedulers; measured with
  Macrobenchmark and Perfetto on the owner's Pixel.
- **What it is for:** starting work, by voice too; reviewing and commenting diffs; following and
  unblocking agents; and chatting, aloud too. The chat matters most on the phone, code not
  neglected: the application opens as readily on the chat as on the dashboard, and its chat aims at
  the level of ChatGPT's and Claude's applications.
- **Notifications through UnifiedPush, by the relay.** Pluto's server sends each event through
  UnifiedPush to a distributor on the phone, ntfy hosted with the relay or another; the application
  no longer runs all the time. The kept connection stays while the application is open, and as a
  fallback. Their content is encrypted end to end. This takes the ROADMAP's open decision on the
  phone's notifications.
- **Reaching the machines:** the fastest path that answers, in order: the local network, a direct
  connection through iroh, the owner's relay, and Tailscale where installed. Moving from Wi-Fi to a
  mobile network loses neither the thread nor the draft. No third party's service is needed.
- **Offline,** recent threads and chats stay readable and searchable; messages and tasks are
  written and leave by themselves when the network returns, shown as waiting. What the phone keeps
  is encrypted and bounded in space.
- **Local models on the phone:** chat offline with a model running on the phone, the local voice
  too, and Pluto's own agent (Stage 4) in a light form: searching the threads and notes kept on the
  phone, writing, preparing a task that leaves for the owner's machines when the network returns.
  The models are managed from Pluto as on the desktop, downloaded on the owner's request; their
  battery and speed are measured per model.
- **Forms:** live notifications, the state of a working thread updated in place, with answers,
  approvals and dictation from the notification itself, and a widget; two panes on tablets and
  foldables.
- **Safety:** the application locks by fingerprint after an absence the owner sets; sensitive
  actions ask for the fingerprint again (approving a command, pushing, opening the sandbox). Each
  device has its rights, such as read-only, no push or no approval. Revoking a device from the
  desktop cuts it at once. Nothing is readable on the locked screen unless the owner sets it.
- **Code on the phone,** beyond starting, following, reviewing and unblocking: files browsed,
  searched, read and corrected by hand through the editor's live merge; a real terminal on the
  owner's machines, with a bar of keys above the keyboard; git and pull requests, behind the
  fingerprint. The editor itself stays on the desktop.

## Alternatives considered

- **Review on the forge, as ADR-0003**: less to build, but a round trip by the forge for each
  remark, and nothing for work not yet pushed.
- **Scheduled tasks only**: no risk of prompt injection, but a pull request that breaks its CI at
  night waits for the owner.
- **Each agent's own configuration**: nothing to maintain, but everything declared once per agent.
- **No cloud runs**: recommended, for the promise of no cloud, and because ADR-0007's several
  machines already give remote runs. The owner chose them, optional.
- **No computer use**: recommended, since the owner's rules forbid agents to inject input into the
  owner's session; agents already reach uwui's applications by their declared actions
  ([ADR-0016](0016-uwuis-decisions-of-2026-11-07-taken-by-pluto.md)). **Computer use on the owner's
  desktop directly**: it reaches everything, but sees everything, and a mistake or an injection acts
  on the real desktop. **Isolated only, never the desktop**: the safest, but an application only
  signed in on the desktop stays out of reach.
- **Cutting families now**: the owner cut none; cuts may still come row by row.
- **The phone kept as it is, or doing all the desktop does**: the first leaves it a viewer; the
  second costs much for uses rare on a phone.
- **The phone's notifications by a kept connection, as today**: nothing more to install, but a
  permanent notification, the battery, and Android's limits on foreground services, tighter at each
  version. **Firebase as an option**: works everywhere, but a service of Google's.
- **The owner's relay only, without Tailscale**: one path, but those who have Tailscale could not
  use it.
- **The phone read-only offline**, or **chat only on the phone's own model**, or **a full coding
  agent on the phone**: the last needs an Android sandbox not yet designed, for a weak model and a
  rare use.
- **The Android desktop mode and a watch**: proposed, not chosen.
- **The browser and previews on the phone**: proposed, not chosen.
- **Only ADR-0009's revocable key per device**: a phone unlocked in the wrong hands could do
  everything.

## Consequences

- The specification ([M1.3](../PLAN.md#m13--the-triage)) takes these features. Their stages, as
  proposed to the owner:
  - the review view, the one configuration and side questions: the server of Stage 2 and the client
    of Stage 6;
  - triggers and pull requests followed: the scheduled tasks of Stage 3, the relay of Stage 5;
  - computer use and cloud runs: the sandbox of Stage 3;
  - dictation and the phone's own models: once Pluto's bench can judge their models, at the earliest
    Stage 4;
  - the phone's notifications through UnifiedPush: the relay of Stage 5.
- The sandbox ([ADR-0009](0009-secrets-privacy-and-the-sandbox.md)) gains the isolated session of
  computer use, and the rule that triggers run locked.
- The ROADMAP's open decision on the phone's notifications is taken.
- The rows of the rethought families are marked next, in `features.md`, before the specification.
