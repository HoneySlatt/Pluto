# Changelog

What changed for users. Releases before 1.0 are private ([ROADMAP](docs/ROADMAP.md)).

## Unreleased

### Fixed

- A thread could open away from its end, often near its top, and then not follow the answer
  streaming in until scrolled down. It happened when the mouse wheel turned over the conversation
  as the thread opened, as when switching threads by a shortcut right after scrolling. A thread
  now opens at its end, and a wheel step taken as its messages arrive starts from that end.
