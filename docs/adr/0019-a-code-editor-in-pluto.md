# 0019. A code editor in Pluto

- Status: Accepted; amends [0003](0003-what-pluto-is-to-become.md) (real editing in Pluto, no
  longer only in the owner's editor; going public moves to Stage 8; the server runs on Windows from
  Stage 7) and [0009](0009-secrets-privacy-and-the-sandbox.md) (the sandbox on Windows decided
  when Stage 7 starts)
- Date: 2026-11-08

## Context

[ADR-0003](0003-what-pluto-is-to-become.md) decided that files are "viewed and lightly edited, and
commented for the agent", and that "real editing happens in the owner's editor, which Pluto opens
at the line". During the interview of 2026-11-08 ([ADR-0017](0017-pluto-measured-against-the-market.md),
which names its record) the owner asked whether an editor better than Zed
and Neovim should be part of Pluto or a program of its own, then asked for the editor to be
questioned as thoroughly as the rest.

What weighed:
- **uwui** plans a code-editor component, optional and heavy: Tree-sitter, LSP, several carets,
  folding, a minimap (SlimeEngine's ADR-0190). Its ROADMAP builds it after its first stage, by its
  1.0. Pluto's client moves to uwui at Stage 6.
- **The market** converges from the editors' side: Zed 1.0 and Cursor 3 took in parallel agents.
  The Codex application and Claude's desktop application have no editor.
- **New editors rarely arrive.** JetBrains stopped Fleet on 2025-12-22, after four years of
  preview. Lapce is still before its 1.0. Helix has waited for plugins since 2023. Zed is about 1.5
  to 2 million lines of Rust, an estimate, built by 45 to 57 people. Pluto's client is about 110,000
  lines.
- **The figures** of a survey on 2026-11-08, mostly a vendor's benchmark on macOS (peeknext.com),
  to be measured again:
  - typing, median and p95: Vim 14.5 and 17.5 ms, Sublime 18 and 30, VS Code 17 and 35, Zed 29 and
    38, Neovim 49 and 52, mostly its terminal's;
  - a file of 5 million lines takes Zed 13.5 GB and Vim 170 MB;
  - Zed does not open 290 MB within 30 s, and hangs on 5 GB;
  - Fresh, a terminal editor in Rust, opens a log of 2 GB in 600 ms and 36 MB.
- **What no editor does well:**
  - huge files without giving anything up;
  - a typing delay steady at p95, measured on Linux;
  - project search at ripgrep's speed;
  - faithful modal editing: Zed's Helix mode is layered on its Vim mode;
  - a screen reader in an editor rendered by the GPU;
  - agents' edits as first-class;
  - extensions that draw interface and also run on the remote host.
- **The owner's habits:**
  - Neovim (nixvim) with about 25 plugins;
  - Zed in its Helix mode;
  - half code written by hand, half agents' work reviewed;
  - Rust, Kotlin, TypeScript, Nix, shell, Markdown, C, C++, Go, Python and Zig.
- **Challenger agents** recommended no editor now. They then found two problems in the first
  answers: agents editing live is editing by several at once, and the HP cannot hold rust-analyzer
  (reported at 1 to 4 GB, up to 13 GB on a large workspace).

## Decision

The owner's decisions of 2026-11-08.

**An editor in Pluto, as an optional part.** In ADR-0003's sense, its code is loaded only when a
file is opened to be edited, and each language's grammar on demand. Pluto at rest is measured with
the editor closed, and keeps its budgets.

**On uwui's code-editor component, at Stage 7, before going public.** It is built once, after the
client on uwui, never on egui. Pluto's files and diffs use the same component in a light mode
before then. Meanwhile Pluto keeps opening the owner's editor at the line. Going public becomes
Stage 8, so that Pluto opens to others with its editor.

**Its core runs in Pluto's server, everywhere.** Open files, language servers, search and git run
in the server, on the owner's machine as on a remote one; the window only draws. Local and remote
take one path, and agents see the same open files as the owner. On a machine reached by SSH
without Pluto, a minimal Pluto server is placed for the session, then removed. The same holds in a
container, a virtual machine and on Windows, so the server runs on Windows from Stage 7. Remote
targets: Pluto's servers, any SSH host, containers and virtual machines, the owner's Windows
machine.

**A terminal mode too.** The same editor, its engines and settings, also in a terminal, as Helix,
with an interface reduced to what a terminal can draw.

**Two modal engines of its own.** A real Vim engine and a real Helix engine, apart, on one core of
selections: a Vim cursor is a Helix selection of one character. A test in CI holds each against the
real program, Neovim and Helix run headless: the same keys must give the same text, cursor,
registers and undo. The costly parts are planned as such: Vim's regular expressions, `:g`, `:norm`,
the dot, the undo tree and macros. Besides them, keymaps without modes, standard ones close to Zed's
and VS Code's, and JetBrains' keymap.

**The owner's Neovim imported, not run.** Pluto starts the owner's Neovim once, headless, asks it
for the mappings and options really in effect, imports what has an equivalent, and lists what has
none. No Lua runs in Pluto.

**What the owner's plugins do, the editor does itself, in its first version:**
- finding and moving: fuzzy search of files, text and symbols; the file tree; tabs of files; the
  keys' help shown on hesitation;
- the code's tools: completion, formatting on save, linters' errors, the problems' panel, TODOs
  found;
- typing: surround, pairs closed, emmet, indentation guides;
- around the code: the terminal, direnv and the project's Nix environment, Python's environments,
  Markdown rendered in place with its preview, the debugger.

**Editing power.** Multibuffers, virtual so that 100,000 results hold, as Zed. Several carets.
Search and replace across a project with ripgrep's libraries, on the files' machine, results shown
as they come. Huge files, a log of 10 GB or a JSON of 1 GB on one line, opened at once in bounded
memory, read in pieces. Past set sizes, Tree-sitter and language servers stop, and each analysis
runs under a limit of time and memory.

**Language intelligence: the best server per language, its limits written.** For each language,
the best language server, several where needed, the debugger through DAP, and Tree-sitter. Large
refactorings go to agents and come back through the review view. Pluto builds no code model of its
own. Servers, debuggers and formatters come from the project's environment (its Nix devShell,
direnv), then the machine's PATH. Else Pluto offers to install them, through the system's package
manager or by a download, with the owner's go. Offline, everything installed works. Each language
server runs under a memory limit.

**The HP is a thin client of the desktop.** On the HP the editor is the window, and its core runs
on the desktop, rust-analyzer with it. Alone away from home without the relay, the HP keeps editing,
search and the light servers (clangd, gopls, zls), each under a memory limit. The HP is measured both
ways.

**Agents and the owner edit at once.** An open file merges every edit live, the owner's and the
agents', in a structure that merges without conflict (a CRDT), as Zed's shared editing.
- Agents that speak ACP write through the editor, their caret shown.
- For agents' CLIs, which write to the disk, Pluto sees the file change and merges the change,
  shown as a diff to keep or reject; it never overwrites what the owner is typing.
- Editing with other people is not built. The merge is designed so that a person could join later,
  an idea decided when Pluto goes public.

**Agents in the editor:**
- edits predicted, and where to make them, by a local model first (Ollama, on the desktop when on
  the HP) or a provider of the owner's choice; no code leaves the machine without the owner's go,
  project by project ([ADR-0009](0009-secrets-privacy-and-the-sandbox.md));
- each agent writing in an open file has its caret, colour and thread's name; no editor does it;
- an agent's changes reviewed in the file itself, hunk by hunk, kept, rejected or commented, the
  comment going back to its thread;
- a selection sent to a thread, a question asked in place beside the thread, and from any line an
  agent wrote, its thread and its moment.

**Git in the editor:** changes in the gutter, opened in place, reverted or staged alone; blame and a
file's or a selection's history, with the Pluto thread that wrote a line; conflicts resolved in a
view of three panes, with an agent's proposal.

**More of an IDE:** tests and tasks run from the code, their results beside each test; spelling in
English and French, on the machine; libraries' documentation offline; a profiler's results on the
code, line by line.

**Beyond code: the binary.** A hexadecimal editor. Markdown and notes, and notebooks, are not aimed
at: Markdown is edited as code, with its rendering and preview.

**Extensions, in steps, isolated.** First, languages declared without code: grammars, queries,
servers; the queries taken from Helix and nvim-treesitter with their licences. Then extensions in
WebAssembly for commands, which also run on the remote host. Then extensions that draw interface,
under a frame budget, once uwui has its own. Always: no network and no files by default, each right
asked for, extensions signed. No host of VS Code's extensions, no Neovim plugins.

**Configuration.** Pluto's settings in its interface and in readable TOML files, per machine and per
project, which the owner's NixOS can write. Behaviour that needs code goes through extensions. A
project's configuration asks for the owner's trust the first time, since naming a server or a task
runs code.

**Accessibility**, at WCAG 2.2 AA with a screen reader, is part of the bar, as for all of Pluto.
The editor gives a screen reader the document though it is drawn by the GPU and virtual.

**Budgets and rivals,** under [ADR-0017](0017-pluto-measured-against-the-market.md)'s rules.
- **The rivals:** Zed, Neovim, Vim, Helix, Sublime Text, VS Code; Fresh for huge files; JetBrains
  for analysis. Each is measured again on Linux, on the same display, with the light sensor.
- **The absolute rule:** a keystroke shows in the next frame. The margin of twice the best rival
  applies to what Pluto controls, from the key's event to the frame presented.
- **The workloads:**
  - typing in `sqlite3.c` and in `gcc.c`;
  - a JSON of 1 GB on one line, and a log of 10 GB;
  - search in the Linux kernel;
  - typing on a remote machine over a link of 50 ms with losses;
  - Orca reading a file of 100,000 lines;
  - the HP as a thin client.

## Alternatives considered

- **A program of its own**, on the same component, linked to Pluto through its server: Pluto stays
  lighter and the editor serves without agents, but two windows, a shallower integration and two
  products.
- **No editor now, an idea decided after uwui's 1.0**: recommended by the challenger, for the cost
  and the record of new editors.
- **No editor at all**, Pluto offered to Zed and Neovim as an agent through ACP: the cheapest, but
  threads and code never side by side.
- **The editor after going public**: recommended, Pluto opening sooner without an editor, as the
  vendors' applications are; the owner chose the editor first. **In parallel with Stages 6 and 7**:
  two heavy undertakings at once.
- **Neovim embedded as the engine**, as vscode-neovim: perfect fidelity and the owner's plugins,
  but two models of one file to keep in step (vscode-neovim loses scrolling, jumps and undo), a
  round trip at each key, and Neovim's grid of characters. **Vim only**: half the work, but the
  owner works in Helix's mode in Zed.
- **Starting from defaults** instead of importing the owner's Neovim: no import to build, but
  everything set again by hand.
- **One file owned by one writer at a time, or merged on save**: much simpler, but the owner and the
  agent wait for each other, or conflicts are resolved by hand.
- **Editing with other people now**: pair programming, but a product for several users before
  Pluto is public.
- **The core in the client when local**: a little less delay, but two paths that drift apart.
- **No terminal mode**: recommended, one renderer only; the owner chose a terminal mode too.
- **Emacs's and Sublime Text's keymaps**: proposed, not chosen.
- **A whole git client in the editor**, as Magit: proposed, not chosen; the rest of Pluto does
  part of it.
- **A code model of Pluto's own, at JetBrains' level**: the only way to beat JetBrains on the finest
  refactorings, but an undertaking the size of an editor, per language.
- **Pluto downloading every tool itself**, as Zed and VS Code: no setting, but downloads of its own
  accord, against Pluto's rule, and broken on NixOS.
- **Language servers on the HP**: independent of the desktop, but rust-analyzer does not fit beside
  Pluto and the agents in 4 GB.
- **A host of VS Code's extensions**, as Theia: thousands of extensions, but a heavy Node runtime,
  an API that moves each month, extensions not isolated, and Microsoft's marketplace closed to
  others.
- **Lua configuration, as Neovim**: everything programmable, but a new interpreter in Pluto and a
  configuration no interface can read back.
- **Markdown and notes, notebooks**: chosen at first, then left by the owner. Jupyter's kernels
  would have brought ZeroMQ, a new dependency.
- **A margin of twice end to end**: the hardest bar, but likely impossible against Vim, the display
  taking half the time.

## Consequences

- The ROADMAP gains Stage 7, the editor; going public becomes Stage 8, and the release map moves.
- It is Pluto's largest stage, larger than all the others together, an estimate. It starts only on
  the owner's go, after Stage 6, and is split into milestones then.
- Pluto's server grows the editor's core and must run on Windows, containers and virtual machines
  from Stage 7, before going public. The sandbox on Windows, which ADR-0009 left to going public,
  is decided when Stage 7 starts.
- Stage 7 waits for uwui's code-editor component, built after uwui's first stage.
- The terminal mode is a second renderer of the whole editor, to hold at the same bar.
- [`uwui-needs.md`](../uwui-needs.md) asks uwui's code-editor component for the whole editor:
  modal editing, a merging buffer, LSP and DAP, remote files, huge files, extensions, a virtual
  document a screen reader can read.
- Pluto's keybindings share the editor's Vim and Helix modes ([`features.md`](../features.md),
  family 14).
- New dependencies, each asked of the owner when its milestone comes: a CRDT, ripgrep's crates,
  Tree-sitter's grammars, DAP.
