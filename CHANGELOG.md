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

### Fixed

- A task delegated to DeepSeek Harness reported its first message as its result instead of its
  last one.
- The server's log showed a provider error's cause as `[Object]`; it now prints the whole chain.

- A thread could open away from its end, often near its top, and then not follow the answer
  streaming in until scrolled down. It happened when the mouse wheel turned over the conversation
  as the thread opened, as when switching threads by a shortcut right after scrolling. A thread
  now opens at its end, and a wheel step taken as its messages arrive starts from that end.
