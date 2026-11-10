# 0022. The triage, row by row

- Status: Accepted; amends [0003](0003-what-pluto-is-to-become.md) (the dashboard as the home
  screen; glass not a setting of Pluto's own), [0009](0009-secrets-privacy-and-the-sandbox.md) and
  [0017](0017-pluto-measured-against-the-market.md) (safe images and icons of answers loaded), and
  [0018](0018-the-markets-features-and-the-triage.md) (the terminal kept; family 1's direction)
- Date: 2026-11-10

## Context

[ADR-0018](0018-the-markets-features-and-the-triage.md) marked today's 31 families of features:
11 kept, 20 rethought with the phone, none cut. A rethought family keeps its need; its form was
left to be decided row by row, each row with its problem, the rival that does it best, and whether
it is core or optional. Four families had been rethought against the recommendation to keep them
(2, 7, 15, 28), and the direction of design projects (25), Bitbucket and T3's environment themes
were still open.

On 2026-11-10 four reading agents listed the rows of the rethought families and of the phone from
the code, `docs/parity/`, the README and the fork's `PLUTO.md`. With the features the families'
new forms add, and the right panel that holds the surfaces, they make 358 rows. A web survey that
day named the rival that does each of 19 areas best, from the vendors' own documentation, not from
use: T3 Code in 7 areas, more than any other; it also found that Conductor closes (announced on
2026-11-04).

The owner answered an interview family by family, then confirmed in batches the marks proposed for
rows the questions had not reached, each batch naming its rows. A closing review then found that
about a hundred rows marked Keep had never been named; the owner accepted them as marked. The rows
and their marks are in [`features.md`](../features.md).

## Decision

The owner's decisions of 2026-11-10. Each row of [`features.md`](../features.md) carries its mark:
195 kept, 106 rethought, 32 cut and 25 added. What follows is what a newcomer would question.

**The terminal is kept.** Family 7 was marked Rethink by mistake on 2026-11-08; the owner meant
Keep. The families now count 12 kept and 19 rethought, the phone among them.

**The owner's reasons** for the families rethought against the recommendation:
- the timeline (2): too close to T3, too dense to follow, too heavy, things missing. A turn shows a
  summary at its head (duration, files changed, commands, result), then the answer; tools and
  thinking are folded. A running turn shows one live line;
- agents' pages and MCP apps (15): heavy, drawn through a Chromium off screen, and not powerful
  enough. A preview stays in the thread; the page opens whole in a panel, in uwui's isolated web
  view, and becomes ADR-0020's living artifact;
- projects and scripts (28): too many pages and settings. One page per project; actions found by
  Pluto in the project's own files; one project is one repository, on every machine where it is
  checked out, instead of project groups.

**Design projects** stay a kind of project, optional, for mockups of the owner's applications and
for pages and sites. Their studio is the artifacts panel; they gain several proposals, versions
compared, an element inspected or commented, OpenDesign's catalog on request, and a validated
mockup handed to a code thread.

**The dashboard is the home screen.** A grid of thread cards, each with its title, project, state
and live line, agent, model and machine, on which the owner approves, answers, stops, sends a
message, pins, settles or snoozes. A strip at the top holds the quotas, the machines and the next
scheduled tasks, updated on events and still at rest. A thread opens over the dashboard with a
compact list of threads beside it. This replaces family 1's direction of 2026-11-08, a sidebar
merged with the dashboard.

**Images and icons of answers load by themselves only when safe.** The owner first chose to load
every image and icon, knowing that the cited sites learn the machine's address. The closing review
then showed a graver risk: an agent misled by a page it reads can write an image whose address
carries a secret, and the client would fetch it at once, outside the agent's sandbox. So an image
or a link's icon loads by itself only from the hosts of the answer's cited sources or tool results,
without a query string, never to a loopback or private address, and under the thread's sandbox
network rule; any other loads on a click. Nothing goes through a third party such as Google's
favicon service. This is the one exception to ADR-0009's "Nothing leaves the owner's machines but
the agents' own traffic", and to ADR-0017's promise.

**One browser, on the server.** The client's own Chromium (`src/preview.rs`) is not in the uwui
client. Each project keeps a browser profile, its logins encrypted under the keyring, reached only
as a right of the sandbox. Emulators are driven inside computer use's isolated session, not in a
surface of their own.

**Pull requests** show in a strip of their thread, and on a page that lists only the pull requests
of Pluto's threads, without search or filters. The forges kept are GitHub, GitLab and Forgejo
(Codeberg); Bitbucket and Azure DevOps are cut.

**Claude's banked resets are not claimed by Pluto.** The fork claims them by reading the OAuth
token that Claude Code stores, which AGENTS.md's rule on credentials and Anthropic's terms forbid.
Quotas are read only through each agent's own interface.

**Keys** are bound per zone, without when-clauses. An optional modal navigation outside the editor
(j/k, gg, /) and a leader key come with the editor's modal core, at Stage 7.

**Glass is not a setting of Pluto's own.** The owner chose an accent, a density and the diffs'
colours as appearance settings, not glass; motion and transparency follow the system.

**What leaves:**
- the legacy sidebar, its project menu and its digit chips;
- the Agents surface, the Device surface and the pull request surface;
- the when-clause builder and when-clauses;
- T3's environment themes and `t3.json` (imported once into the project's own file);
- the action editor and the import of scripts;
- review guides, usage limit hubs, Claude's reset claims and the providers' periodic health check;
- the advanced typography, project grouping, the working shelf, provider update checks and the fold
  of legacy features;
- the custom model editor's option descriptors: a custom model is an id and a name;
- the visual comparison with T3's web client (`examples/snapshot.rs`), and
  `docs/release-backlog.md`, whose plan no longer exists.

The send-key setting stays: it had been cut on the wrong belief that it did nothing.

**Added beside the market's features of ADR-0018:**
- a full-window review mode; a file marked viewed; a hunk kept or rejected; find in a diff;
  cross-agent review whose remarks arrive as line comments; pending comments saved like a draft;
- a file's line commented for the agent; HTML and PDF shown in place;
- one search, in the palette, on every machine, covering messages, archived threads and imported
  history, and optionally by meaning through a local embedding model;
- local models managed (installed, GPU memory, pulled by name, a catalog with whether a model fits)
  and any OpenAI-compatible endpoint; a context length per model instead of derived models;
- a warning by pace and at 95 % before a quota runs out, with a suggested switch, never made alone;
  a run stopped by a limit resumed at its reset or continued with another agent;
- a subagent's usage summed into its parent's;
- on the phone, two tabs (Chat and Threads), pairing by a QR code, the threads and chats opened in
  the last 30 days kept offline, encrypted, under a cap the owner sets (500 MB by default), and
  agents' pages shown as previews, which ADR-0018 had not chosen for the phone.

## Alternatives considered

- **Keeping the four families against the recommendation as they are**: less to build; the owner
  gave reasons to rethink three of them.
- **The sidebar as a compact dashboard**, recommended: the list always visible. The owner chose the
  dashboard as the home screen, the list beside an open thread.
- **Images on a click always**, the first recommendation, which keeps the rule whole; **every
  image loaded**, the owner's first choice, which the review showed open to exfiltration.
- **Design projects as a real design tool**, with a free canvas and components, or **a charter
  only**: the first a large undertaking, the second loses the studio.
- **Two browsers**, the client's for the owner's own previews: one more Chromium on the client.
- **Every forge the fork reads, or GitHub alone until going public**: the owner kept GitLab and
  Forgejo too.
- **Claude's resets claimed only through Claude Code**, if it ever offers a command: possible
  later, nothing to build now.
- **Diffs inside the timeline, comments on a whole file or on unchanged context, keys shown on every
  button, a which-key panel, digit chips while Mod is held**: proposed, not chosen.
- **A score of Pluto's bench beside each local model, glass as a setting**: proposed, not chosen.

## Consequences

- [`features.md`](../features.md) holds every row, its mark and its rival; the
  [specification](../specification.md) is written from it.
- [AGENTS.md](../../AGENTS.md), the [ROADMAP](../ROADMAP.md)'s promises and
  [ARCHITECTURE.md](../ARCHITECTURE.md) state the exception for safe images and icons. M1.4
  removes Google's favicon service and brings the egui client to the rule.
- M1.4 cuts what leaves from the egui client and the Android app, where it is there. The egui
  client keeps its own Chromium while it has no web view: it draws agents' pages, MCP apps and
  designs through it.
- The fork's claim of Claude's resets is not ported; the fork is frozen and keeps it until it stops.
- The ROADMAP notes Conductor's closing beside its name among the rivals; M1.6 measures what is
  still available.
