# Changelog

What changed for users. Releases before 1.0 are private ([ROADMAP](docs/ROADMAP.md)).

## Unreleased

### Added

- DeepSeek Harness is available as a provider beside Claude Code, Codex and the others, through
  the official local Web protocol of its preview 0.2.1-rc.3 release. It streams answers and
  reasoning, shows tools and permission questions, and keeps a separate Harness home per
  instance ([ADR-0021](docs/adr/0021-deepseek-harness-in-todays-pluto.md)). Sign-in and limits are
  in the server's provider guide.
- DeepSeek Harness reports its token usage: each turn's tokens and the context's fill in the
  thread, and its history on the usage page, under its own name and color.

### Changed

- An answer's images, its links' icons and its tools' icons load by themselves only from a site
  its web searches and fetches returned, over HTTPS, without a query string, and never from a local or private
  address. Any other image shows a "Load image from …" chip and loads on a click, on the desktop
  and the phone ([ADR-0022](docs/adr/0022-the-triage-row-by-row.md)). A link's icon comes from the
  site itself: Google's favicon service is no longer used.

### Removed

- The legacy sidebar, with its project trees, sorts, project menu, rename and grouping dialogs,
  thread glyphs and Mod+digit chips; the setting that chose it. Projects are no longer grouped by
  a setting: the new thread's project picker gathers a repository's checkouts, newest first.
- The "Legacy features" fold of the General settings: plan mode and the context window indicator
  are now plain rows of its Behavior section.
- Bitbucket and Azure DevOps: their settings row, their pull requests and their links. GitHub,
  GitLab and Forgejo (Codeberg) stay.
- The pull requests page's search and filters: it lists the open pull requests, sorted as chosen.
- The Providers page's usage limit hubs, which could not be added, and its health check interval;
  the server keeps checking providers at the interval it has.
- Cursor as a provider. "Open in Cursor" stays.
- The right panel's Agents, Device and pull request surfaces, and the Device settings.
- The keybindings' when-clause builder. Bindings keep the conditions they have.
- Reading the repository's `t3.json`: its scripts are no longer offered for import, and a project
  that does not set where threads start or how submodules are handled uses the defaults (local,
  recursive). Project actions are still added and edited from the action editor.
- T3's environment themes: Pluto always wears its own dark look.
- The advanced typography: the interface font and one monospace font remain, as with Advanced off.
- The "Working section" and "Provider update checks" switches; the server keeps the value it has.
- The sidebar's own search field in Code mode: the palette (Ctrl+K) searches threads, their
  messages included. Chat mode keeps its search.

### Fixed

- A task delegated to DeepSeek Harness reported its first message as its result instead of its
  last one.
- The server's log showed a provider error's cause as `[Object]`; it now prints the whole chain.

- A thread could open away from its end, often near its top, and then not follow the answer
  streaming in until scrolled down. It happened when the mouse wheel turned over the conversation
  as the thread opened, as when switching threads by a shortcut right after scrolling. A thread
  now opens at its end, and a wheel step taken as its messages arrive starts from that end.
