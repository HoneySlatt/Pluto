# 0017. Pluto measured against the market

- Status: Accepted; amends [0015](0015-the-budgets-set-from-todays-figures.md) (budgets that
  rivals may tighten, never loosen) and [0016](0016-uwuis-decisions-of-2026-11-07-taken-by-pluto.md)
  (rivals measured from Stage 1, and the margin set now, not at Stage 6); amended by
  [0022](0022-the-triage-row-by-row.md) (safe images and icons of answers loaded) and
  [0026](0026-the-rivals-measured-how.md) (the vendors' applications without the owner's accounts;
  energy by a root sampler) and [0027](0027-the-rivals-measures-set-aside.md) (Stage 2 no longer
  waits for the rivals' figures)
- Date: 2026-11-08

## Context

On 2026-11-08 the owner asked that Pluto be the best product of its market, and held an interview
on its quality bar, its budgets, its ambitions and its features, one question at a time, as
SlimeEngine did in its ADR-0215. This ADR, [0018](0018-the-markets-features-and-the-triage.md),
[0019](0019-a-code-editor-in-pluto.md) and [0020](0020-the-chat.md) are its record.

The quality bar of the [ROADMAP](../ROADMAP.md#quality-bar) held absolute budgets, set from Pluto's
own figures ([ADR-0015](0015-the-budgets-set-from-todays-figures.md)). No rival was measured, and
the margin over them was left to Stage 6 ([ADR-0016](0016-uwuis-decisions-of-2026-11-07-taken-by-pluto.md)).
SlimeEngine found that such a bar is too low: it was rewritten against measured rivals.

A survey of the field on 2026-11-08 found, from vendors' pages and users' reports, mostly
unverified:
- Every application for coding agents is built on Electron, except Zed and Warp, both in Rust.
- The vendors' applications grow to gigabytes. Users report Claude's desktop application at about
  2 GB over 18 hours, and the Codex application up to 13 GB. Zed is reported at about 140 MB at
  rest and 0.12 s to start, its agent panel growing from 0.3 to 3 GB.
- No vendor publishes budgets. No battery or frame figure exists for any of them.
- On Linux, Claude's and Codex's applications are a beta and a preview.
- In Zed and Copilot, external agents lose steering, checkpoints or resume.

A challenger agent attacked the first recommendations before they were put. It found that a lead
over a mediocre rival can be looser than a budget already set, that Pluto (client and server) and
Zed (one process) are not counted alike, and that a rival signed out shows only its sign-in screen.

## Decision

The owner's decisions of 2026-11-08.

**Judged criterion by criterion.** On each criterion, the best rival measured sets the bar. Pluto
beats it by the margin of the criterion's family. Beating each line is the least.

**The stricter wins.** On each criterion, the bar is the stricter of the budget already validated
and the figure drawn from the best rival and its margin. A rival never loosens a budget.

**Two comparisons.** Pluto's own share, client and server, against the rival's own share, agents
excluded: what Pluto controls. And the whole system over a day of 15 threads, agents included:
where Pluto wins by stopping idle agents ([ADR-0006](0006-agents-drivers.md)).

**The rivals, named only to be measured.** Each is named with its version and date, in the quality bar and
in [`measurements.md`](../measurements.md):
- the vendors' applications: Claude Code's desktop application, Codex in ChatGPT's desktop
  application;
- the native ones: Zed and Warp;
- the multi-agent ones: T3 Code and today's fork, OpenCode Desktop, Emdash or Nimbalyst;
- the editor's, the chat's and the phone's, in [0019](0019-a-code-editor-in-pluto.md),
  [0020](0020-the-chat.md) and [0018](0018-the-markets-features-and-the-triage.md).

Cursor, Copilot and Antigravity are compared on their features only. Conductor runs on macOS only
and is compared on paper. The bare agents in a terminal are not a rival.

**Measured on one workload.** A scripted fake model service sends every application the same
work, a long thread and 15 threads streaming, with no quota spent and no variation between runs.
Each rival runs in a disposable, isolated profile, its network cut unless the measure needs it.
The Claude and ChatGPT applications, which need an account, are measured only with the owner's go,
account by account. The owner gives one go for downloading the list of rivals, outside Nix. Each
licence is checked for a clause against benchmarks before any figure is published.

**When.** A milestone of Stage 1 measures the rivals first, beside the cuts and the recorded
sessions, without blocking the triage, so that Stage 2's server starts from their figures. Then
again at the end of each stage, at the rivals' versions of the day.

**Resources: twice better.** Memory and processor at rest, wake-ups, start, binary size, disk and
energy: at most half of the best rival's, Pluto's share against its own. Energy is measured on the
HP by its battery and on the desktop by the processor's counters, at rest and over an hour of 15
threads streaming. Where a figure turns out out of reach, as a cold start bounded by the graphics
driver, it is debt until met, with its reason.

**Smoothness.** At least 99.9 % of frames on time at the display's rate, none over one and a half
times the frame, in every scenario: a long thread, 15 threads streaming, scrolling, opening a
thread, the editor. Input to photon is timed from outside with the light sensor bought for
SlimeEngine, so that rivals compare: at most one frame at p99, and twice better than the best
rival. The compositor's presentation times stay for Pluto's own traces.

**Long sessions.** After 24 hours and after 7 days of use with 15 threads open, memory and wake-ups,
client and server, stay within 10 % of a fresh start. A long session replayed on the fake model,
accelerated, checks it at the end of each stage.

**Reliability.** At least 99.9 % of hours of use without a crash, and no work lost after one, as
before. Added, each with its test:
- an update or a restart of the server never stops a running agent;
- a dropped connection recovers within one second on the local network, losing nothing;
- a worktree with uncommitted work is never deleted;
- an update of Pluto costs no thread and no draft.

**Every agent equal.** Pluto supplies what an agent lacks, for each first-rank agent: git
checkpoints at each turn, steering by interrupt and resend, its own counts of tokens, approvals and
the plan. A grid of tests on recorded sessions says, per agent and per ability: native, supplied by
Pluto, or impossible, with the reason. The agents' versions are pinned, and their sessions recorded
again at each update.

**Tokens.** For the same work, a turn started by Pluto spends at most 5 % more tokens than the same
agent started alone. Everything Pluto adds, tools, instructions and memory, is measured and can be
turned off per thread. The fake model counts what it receives.

**Promises**, each checked by a test:
- **Instant**: no common action on the desktop shows a loading state; a thread visited before
  shows whole in the frame after the click; a thread never opened, within 50 ms from a server on
  the same machine, a figure to confirm by measure.
- **Nothing is ever lost**: no draft, thread, worktree with uncommitted work or history, after a
  crash, a cut, an update or a migration.
- **No account and no cloud for Pluto itself**: it needs no account, no internet and no vendor's
  service, on the local network and offline, local models fully. Only the agents' own traffic
  leaves, and Pluto shows which.
- **Linux first-rank**: Wayland, sharp fractional scales, the HP smooth, and Linux checked first at
  each release.

**The disk.** Pluto shows the size of each worktree, offers to delete those of finished or merged
threads, never one with uncommitted work, and shares what can be shared, such as build caches. A new
worktree costs only the files git tracks, measured, and inactive worktrees never grow unseen. The
database keeps its budget of 25 MiB a day.

## Alternatives considered

- **Absolute budgets, rivals at Stage 6, as ADR-0016**: nothing more to do now, but the server of
  Stages 2 to 5 would be built without knowing whether it beats the market. SlimeEngine had to
  correct exactly that.
- **Judged as a whole, by use**: the simplest, but nothing measured.
- **The rivals' figures replacing the budgets**: one rule, but the bar would fall where the market
  is poor, in memory and wake-ups.
- **Pluto's share alone, or the whole system alone**: the first ignores what the machine pays for
  the agents; the second measures mostly programs Pluto does not control.
- **The bare agents in a terminal as a floor**: proposed by the challenger, with a budget on Pluto's
  cost over them; not chosen by the owner.
- **Rivals measured signed out**: free, but a sign-in screen is not a workspace.
- **Rivals measured on the owner's accounts only**: real use, but quota spent, runs that vary, and
  no equal workload.
- **A go per rival**: full control, at the cost of a question at each rival's new version.
- **Measured before each private release, or at each push in CI**: the first lags the market; the
  second needs downloads and noisy measures on GitHub's machines, at a private repository's cost.
- **A lead of four times on resources, as SlimeEngine against Unreal**: against Zed, about 35 MB for
  client and server together, out of reach with GPU rendering and fonts. **At least equal**: no
  lead over the native rivals.
- **99 % of frames on time, kept**: one stutter in a hundred shows at 240 Hz.
- **Long sessions over 24 hours only, or no budget**: a slow leak would pass, the very fault of the
  rivals.
- **99.99 % of hours without a crash**: years of one person's use to check it.
- **Tiers of agents, or what each agent offers**: less upkeep, but "every agent first-rank" would
  hold in name only.
- **Tokens shown, without a budget**: nothing would stop Pluto's cost growing.
- **Worktrees' size shown only**: the disk would still fill.

## Consequences

- The quality bar of the [ROADMAP](../ROADMAP.md#quality-bar) is rewritten: the rule, the rivals,
  the margins, and the new budgets (long sessions, energy, tokens, the disk, the reliability's
  guarantees, the phone's).
- Stage 1 gains a milestone, [M1.6](../PLAN.md#m16--the-rivals-measured): the fake model service,
  the rivals' sandbox, the first figures of the agents' applications and the chats. The editor's
  rivals are measured when Stage 7 starts, the phone's with Stage 5. Every figure missed is debt
  until met.
- The quality bar's rows on a long thread and on streaming join one row on frames; input to
  present becomes input to photon, timed by the light sensor.
- The light sensor that the owner buys for SlimeEngine times Pluto and its rivals too.
- Rivals are measured again at each stage's end; each figure keeps its rival's version and date.
- The grid of agents' abilities joins the drivers' tests ([ADR-0006](0006-agents-drivers.md)),
  over the recorded sessions of M1.5.
