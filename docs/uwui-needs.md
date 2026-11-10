# What Pluto asks of uwui

Pluto's desktop client is drawn with egui today. It will move to uwui, the owner's UI framework,
once uwui runs applications ([ADR-0001](adr/0001-a-native-client-on-egui-until-uwui.md),
[ADR-0003](adr/0003-what-pluto-is-to-become.md)). Pluto's client is one of the programs of uwui's
first stage, and uwui's API is not declared stable before it runs Pluto at its bar (SlimeEngine's
ADR-0203). SlimeEngine's M6d.1b3b chose how an application's state changes its interface,
signals, from three lists like this one: Opium's, Pluto's and a DAW's (SlimeEngine's ADR-0205).
This file is Pluto's. SlimeEngine's M6d.10 weighs it again for uwui's first stage, and its M6d.9a
and M6d.9b design what agents may do in an application.

Because Pluto is a pilot, the components it needs belong to uwui's first stage. uwui's ROADMAP
lists several of them only "to its 1.0":
- Markdown, rich text, the code view and the terminal;
- the web view, charts, and conversations with AIs.

The diff view below is new to uwui altogether.

uwui takes an application's need only if it is general, never to win one benchmark (SlimeEngine's
ADR-0170). The needs are therefore split in two: the general ones, which uwui may take, and
Pluto's own, which stay here. The third column of the table says whether uwui's requirements
(SlimeEngine's ADR-0190 and ADR-0199, uwui's ROADMAP) already hold the need, as read on 2026-11-06,
and again against SlimeEngine's ADR-0213 to ADR-0220 on 2026-11-07:

- *Yes*: they hold it.
- *Precision*: they hold it in general terms, and Pluto says what it needs exactly.
- *New*: they do not mention it.

The needs come from the egui client as it is (`src/`, `crates/pluto-core`) and from the
requirements of 2026-11-06 (ADR-0003). Module paths are under `src/` unless they name a crate.

## Pluto's state, for M6d.1b3

These are the changes Pluto makes to its interface, from the most frequent to the rarest. Each must
leave the interface's thread free, and draw nothing once it is over.

- **Every frame while agents work.** Several threads stream at once, up to 15 on a busy day. Each
  one appends text to the last block of a long timeline. The changes arrive from a network thread,
  per server, several times per frame. A terminal's grid changes under a running command. A
  browser tab's screencast replaces a texture 30 to 60 times a second. A spinner and a "Working
  for" timer move.
- **Every second.** Elapsed times and relative times ("2 min ago") in the sidebar and the
  timeline. The dashboard's resource figures: memory and processor per agent and per machine.
- **In bursts, from other threads.**
  - A thread's history loads, thousands of items, page by page.
  - A reconnection replays the events missed.
  - A diff of 200 files lists them all, then loads each patch when it opens
    (`ui/diff_panel.rs`).
  - Search results arrive while the user types (`ui/palette.rs`, `ui/content_search.rs`).
  - Scheduled tasks finish.
  - Several servers send at once.
- **From another device or server.**
  - A thread started or answered from the phone.
  - A thread's state on another machine.
  - A quota that drops.
- **By the user.**
  - Typing in the composer: rich text with inline chips (`ui/composer.rs`,
    `ui/prompt_editor.rs`).
  - Switching thread: the whole timeline replaced within a frame.
  - Filtering the sidebar, opening and closing panels, dragging tabs, selecting text across
    messages.
- **Rarely.** The theme, the density, the language, a setting.

In each case the screen changes in place. A timeline that follows the end stays at the end while
text streams in. A timeline the user scrolled up keeps the item they read in place when content
arrives below or above. The sidebar reorders threads as their state changes, animated, and keeps
the selection by identity.

Two cases are the most likely to separate the state models that M6d.1b compares:
- fifteen streams appending to fifteen timelines, only one of them visible, without building views
  again for the hidden ones;
- a timeline of 5,000 items of variable height that grows at its end while the user reads the
  middle.

SlimeEngine's ADR-0202 does not script either case yet.

## General needs

| Need | Pluto's use | In uwui's requirements |
|---|---|---|
| A virtual list of items of variable height (Markdown, code, images, cards), 5,000 and more, heights measured when first shown; appended at the end at stream rate; anchored to the end while following, to the item read otherwise; jumped to any item at once | The timeline (`ui/chat.rs`, `ui/minimap.rs`) | Precision: ADR-0190 has a table of millions of rows, of one height |
| Text that streams in: a block appended many times a second, at the cost of the appended part and not of the block; Markdown parsed incrementally, the open code block highlighted as it grows | Every agent's answer (`ui/markdown.rs`, `highlight.rs`) | Precision: ADR-0190 lists "text that streams in" and Markdown shown, not their incremental cost |
| Many streams changing state that is not on screen, at the cost of the change and with nothing built or drawn for hidden views | 15 threads at once, one visible | Precision: the state model |
| State changed from other threads in bursts (one network thread per server), gathered into the next frame, the interface's thread never waiting | Every server connection (`crates/pluto-core/src/protocol.rs`) | Precision: ADR-0190's two executors; the state model decides how |
| Timers bound to what is visible: a clock that ticks each second only while its text is on screen, and at the granularity it shows ("2 min ago" wakes once a minute) | Elapsed and relative times; today 46 `request_repaint_after` calls | New |
| A list changed in place by key: items reordered, inserted and removed, animated, the selection kept by identity | The sidebar, the dashboard | Precision: Opium asks the same of its table |
| Text selected and copied across many elements, including items of a virtual list that are not built, copied as Markdown or plain text | Selecting part of a conversation | Precision: ADR-0190 has selection across elements, not across a virtual list |
| Find in a long view: matches highlighted over virtualized content, next and previous, the count | Find in a thread, in a diff, in a file | New |
| A rich text input: inline atomic chips (a file, a thread, a skill), a popover of suggestions at the caret, IME, undo across chips, a paste of 100,000 characters without a freeze, images pasted and dropped | The composer | Precision: ADR-0190 has rich text, not atomic chips and caret popovers |
| A view of code, read-mostly with light edits: syntax colours, line numbers, a gutter of comments, folding, a small edit saved; cheap in memory when many are open | The Files panel (`ui/file_editor.rs`) | Precision: ADR-0190's code editor; Pluto needs its light mode first |
| The whole code editor: a buffer that merges concurrent edits (a CRDT) from the user and agents, with a caret per author; Vim's and Helix's modal engines on one core of selections; LSP and DAP; files held by a remote core, the window only drawing; huge files read in pieces in bounded memory; multibuffers; extensions in WebAssembly that may draw under a frame budget; a terminal rendering of the same editor; a virtual document a screen reader can read | Pluto's editor, Stage 7 ([ADR-0019](adr/0019-a-code-editor-in-pluto.md)) | Precision: ADR-0190's code editor (Tree-sitter, LSP, several carets, folding, a minimap); the merge, the modal engines, the remote core, huge files, the terminal rendering and screen readers on a virtual document are new |
| A diff view: unified and side by side, colours, words changed within a line, thousands of lines, comments anchored to lines, files that load when opened | The diff panel, cross-agent review | New: ADR-0190 has a code editor, not a diff view. An editor, a git tool and any review tool need it too |
| A terminal fed by a stream of bytes from anywhere, not only from a local process: mouse reporting, wide characters, links, the Kitty keyboard; many open, only the visible ones drawn | Terminals whose process runs on a server, maybe another machine (`terminal.rs`) | Precision: ADR-0190's terminal crate, fed by the network |
| A texture replaced at each frame from decoded images, decoded away from the interface's thread, input forwarded from the element | The browser on the server, seen through its screencast (`server_browser.rs`) | Yes: uwui's ROADMAP, "textures updated at each frame" |
| Docking of panels with tabs, splits and tab drag; their layout kept per context (per thread) and across restarts | The right panel: diff, files, terminal, browser | Precision: ADR-0190 has docking and restores panels, not per context |
| A palette of commands with fuzzy matching and results that arrive while typing | Ctrl+K, Go to file, search | Yes: ADR-0190 |
| Shortcuts remapped by the user, with chords, and bound to a context (active only in the composer, in the terminal, in a dialog); written with the primary modifier, which Pluto writes `Mod` | Keyboard first (`ui/keybindings.rs`) | Precision: ADR-0190 has remapping and chords, not contexts; the primary modifier, Ctrl or Cmd, comes with SlimeEngine's M6d.3 (ADR-0220) |
| An optional modal navigation of lists, timelines and panels (j/k, gg, /, a leader key), on the code editor's modal core | Keyboard first, modal navigation outside the editor ([ADR-0022](adr/0022-the-triage-row-by-row.md)) | New |
| Popovers, hover cards, context menus with submenus, toasts with actions, sheets and dialogs | Everywhere | Yes: ADR-0190 |
| Notifications with actions answered from the notification (approve, reply), an icon in the tray, a start with the session, one instance, deep links (`pluto://`), files dropped in and dragged out | The server works while the window is closed | Precision: ADR-0190 has all of it except actions in notifications |
| Charts, live ones | The dashboard: quotas, usage, resources | Yes: ADR-0190's optional crate |
| Embedded web content: an HTML page or an MCP app shown inside a message, sandboxed, sized to its content, and whole in a panel; no network and no files by default | `html_render` pages, MCP apps (`ui/html_render.rs`, `ui/mcp_apps.rs`) | Precision: ADR-0190's web view, inside a scrolled list |
| SVG, PDF, video and audio in place | Mermaid diagrams, files of the workspace (`ui/media.rs`) | Yes: ADR-0190 |
| Every writing system, colour emoji, fallback fonts | Agents answer in Chinese, Japanese, with emoji | Yes: ADR-0190 |
| An element marked sensitive: never in an agent's capture or a screenshot taken by uwui, never in logs, readable by the user's screen reader only on request | Secrets shown masked (ADR-0009) | Precision: ADR-0190 keeps secrets out of agents' captures; the mark on an element is new |
| Text that streams announced to screen readers politely, batched by sentence, never flooding | Accessibility of the timeline | New: ADR-0190 has screen readers, not live regions that stream |
| The window's state restored: the open thread, each list's scroll by the item shown, panels and tabs per thread | Pluto reopens where the user was | Precision: Opium asks the same |
| One interface's code for the desktop, the phone and the web, with screens of one platform where needed | Android and the web client move to uwui (ADR-0003) | Yes: ADR-0190 |
| A whole Android application in Rust, with notifications pushed through UnifiedPush without Google's service, Android's live notifications answered in place, a widget, two panes on large screens | The phone app ([ADR-0018](adr/0018-the-markets-features-and-the-triage.md)) | Precision: ADR-0190 has whole Android applications; ADR-0213 gives uwui long work, but names neither UnifiedPush nor live notifications |
| Glass, motion interrupted without a jump, effects lightened on low tiers and on battery | Pluto's look (ADR-0003) | Yes: ADR-0190 |
| Themes in files, a theme and a density of Pluto's own that extend uwui's core look; Glass taken from uwui's official theme Glass, or its glass material | Pluto's look ([ADR-0016](adr/0016-uwuis-decisions-of-2026-11-07-taken-by-pluto.md)) | Yes: ADR-0213, the core's look and the official themes |
| Settings kept where the system keeps them, written atomically, versioned and migrated | Client settings | Yes: ADR-0190; ADR-0213's services of files and settings |
| Accessibility that wakes nothing at rest | AccessKit's D-Bus thread wakes today's client twice a second, the largest of its wake-ups ([measured](measurements.md#the-client-at-rest)) | Precision: uwui's budget at rest, which AccessKit's adapter must keep too |
| Fonts held once in memory, each loaded when a text first needs it | Today's client holds 12.7 MiB of fonts, some of their bytes twice | Precision: uwui's ROADMAP loads fonts when first needed, and does not say they are held once |
| Microphone capture, and audio played from its first bytes and cut at once, for dictation and voice | Local dictation and a conversation aloud ([ADR-0020](adr/0020-the-chat.md)) | New |
| A global shortcut that opens a small window over the desktop, and a window or part of the screen captured at the user's request, through Wayland's portals | The chat outside Pluto's window ([ADR-0020](adr/0020-the-chat.md)) | New |

All are indispensable: the port would lose a function Pluto has, or an ADR asks for it. The whole
code editor is needed by Stage 7, after uwui's first stage.

## Pluto as an agent, and as an application driven

Pluto stands on both sides of uwui's core for agents (SlimeEngine's ADR-0213 and ADR-0218). The
agents it drives may attach to any application on uwui that the owner has open. And Pluto's client
on uwui is itself an application that agents may drive, including the agents running inside it.
Pluto's sandbox keeps uwui's applications closed to agents unless the owner opens them, and keeps
what only a person may do from every agent ([ADR-0016](adr/0016-uwuis-decisions-of-2026-11-07-taken-by-pluto.md)).

What Pluto asks of uwui's core for agents, for SlimeEngine's M6d.9a and M6d.9b. All are general
needs:

| Need | Pluto's use | In uwui's decisions |
|---|---|---|
| An action kept for a person: never offered to an agent, never run for one, whatever consent the agent holds | Approving an agent's request, changing a sandbox, revealing a masked secret, pairing a device: an agent inside Pluto must never approve itself | New: ADR-0213 gives each action the permission it asks, not one that no agent may hold. A password manager or a bank's application needs it too |
| Consent scoped and revocable: given to one session of an agent, or for a time, and withdrawn at once | An overnight task must not inherit the consent given to a session the owner watched | Precision: ADR-0218 has consent agent by agent |
| An identity that names the harness and its session beside the agent, bound to the consent given rather than only declared by the agent | "Claude Code, in Pluto, thread X"; any program on the machine can call itself Claude Code | Precision: ADR-0218's first message names the agent |
| A way to find the applications on uwui that are open and accept agents, on Linux, where the system offers none | An agent in Pluto reaching Opium or SlimeEngine's editor | New: ADR-0213 notes that Linux has no such mechanism |

## Pluto's own

These stay in Pluto. uwui gives them only the general pieces above.

- The server, whole: it draws nothing and has no uwui at all. That covers the protocol, the
  agents' drivers, the storage, the relay, the sandbox, the server's own keyring access, the
  pseudo-terminals and the browser driven through CDP.
- The timeline's model: tool calls grouped into work folds, plans, approvals, questions, subagents,
  checkpoints (`crates/pluto-core`).
- Pluto's Markdown: thread links, alerts, footnotes, prompt links. uwui's Markdown crate needs only
  a way to add inline and block kinds.
- Finding the secrets to mask, and the masking.
- What the dashboard shows, the quotas and the suggested switch between agents.
- The terminal's emulation state, if uwui's terminal crate does not own one: today
  `alacritty_terminal`.
- On Android, the foreground service that keeps the connection, and the notifications through a
  push service without Google's, unless uwui's long work comes to cover them. Today both are in
  the Kotlin layer, which the system draws, not uwui.
- Pluto's theme and its density.
