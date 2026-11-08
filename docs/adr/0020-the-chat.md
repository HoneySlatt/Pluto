# 0020. The chat

- Status: Accepted; amends [0003](0003-what-pluto-is-to-become.md) (the chat replaces the vendors'
  applications for the owner)
- Date: 2026-11-08

## Context

Pluto's chat mode is one of its four differences ([ADR-0003](0003-what-pluto-is-to-become.md)). The
interview of 2026-11-08 ([ADR-0017](0017-pluto-measured-against-the-market.md), which names its
record) took it in turn. Today it has:
- a chat space and chat projects, each with instructions, reference files and a memory;
- a memory that updates itself after each chat;
- search in earlier chats, a sandboxed Python tool, answers' versions, temporary chats and export.

It goes through the agents' official CLIs, with a chat charter in place of the coding prompt, and
through Ollama for local models.

A survey of the field on 2026-11-08 found, from secondary sources unless said:
- ChatGPT has the widest set of features and the best voice (GPT-Live), and Grok the fastest voice.
  Claude has the most transparent memory and its Artifacts. Gemini Live is the best with the camera.
  Perplexity is the best at search with citations. T3 Chat has the fastest interface.
- Each vendor's memory stays locked in its product. Side-by-side comparison exists only with one's
  own API keys (Open WebUI, Msty, TypingMind). No product joins the history of every vendor. No
  major vendor offers end-to-end encryption.
- Users measured Claude's desktop application at about 1.2 GB at rest on Windows, and one saw its
  main process grow from 261 MB to 47 GB in 15 hours on a Mac.
- The first token reaches an API client in 0.85 to 1.1 s for frontier models (measured). No one
  publishes what an application adds.
- The vendors' terms, from their own pages:
  - Anthropic forbids third-party applications to offer claude.ai's sign-in or to use a plan's
    credentials. It allows a user signed in to the unmodified Claude Code program.
  - OpenAI allows "Sign in with ChatGPT", with the plan's usage, for open-source projects and
    personal applications that run locally, since 2026-09-29.
  - Google calls the use of Gemini CLI's sign-in from third-party software a violation; third
    parties use an API key. Whether driving its official CLI is allowed is unanswered.

## Decision

The owner's decisions of 2026-11-08.

**The chat replaces the vendors' applications.** Pluto's chat becomes the owner's only chat, on
the desktop and the phone, with every model equal. It aims at the best of the market criterion by
criterion, as all of Pluto ([ADR-0017](0017-pluto-measured-against-the-market.md)), for questions
and learning, writing, search on the web, and images, voice and files. What a vendor allows only to
its own application is said plainly, and made up for where possible.

**Each vendor by the path its terms allow.** AGENTS.md's rule to drive the vendors' official
programs gains one exception: a sign-in a vendor offers to third-party applications, as OpenAI's.
Claude through the official Claude Code, signed in by
the owner. ChatGPT through "Sign in with ChatGPT", or Codex. Gemini through an API key, until
Google says plainly that driving its CLI is allowed. The others through their official program or
a key. A table of the paths and their sources is kept up to date and checked again at each stage's
end.

**One memory of the owner, visible, on the owner's machine.** One memory for every model, local
ones included, and one per project. It updates itself; each addition is shown and can be undone in
one gesture. All of it can be read, edited and exported. Code agents do not receive it by default,
for its tokens and its relevance; a thread can ask for it. The chat also finds earlier chats when
useful.

**Search on the web through a keyed engine.** The vendors' models search with their own tools.
Local models and Pluto's own agent search through an engine the owner chooses and keys (Brave,
Kagi, Exa or another). Sources are always cited. Deep research is offered with every model.

**Files read, a library indexed on the machine.** The chat reads PDFs (text and images), Office
documents, spreadsheets, images and code. A small file goes whole to the model. A large library is
indexed on the owner's machine with a local embedding model (Ollama), and only the useful passages
go to the model, cited. Nothing goes to an indexing service.

**Writing and artifacts.** A document written together beside the chat, the model's edits shown as
diffs, comments on a passage, versions kept, on the editor's live merge
([ADR-0019](0019-a-code-editor-in-pluto.md)). Living artifacts: pages, diagrams, charts and small
applications that run in the conversation, isolated, replayable and exportable, from today's agents'
pages. Exports of an answer or a document as PDF, Word, Markdown or a web page.

**Outside the window.**
- A global shortcut opens a small chat window over the desktop, through Wayland's portal of global
  shortcuts, to check with Umbriel.
- A window or a part of the screen can be shown to the model, through Wayland's capture portal,
  always at the owner's request.
- Text selected anywhere can be translated, explained or rephrased from a shortcut.
- One question sent to several models, their answers side by side, in the chat as in code.

**Voice, local, with any model.** A conversation aloud with any model. The owner's voice is
recognised on the machine. The answer is read by a local voice from its first words, and the owner
can cut it by speaking. Nothing of the voice leaves the machine. A vendor's real-time voice is an
option through an API key. Measured against GPT-Live and Grok: the delay from the end of a
sentence to the first word heard. On the HP, recognition and speech run on the desktop.

**Images, by vendors and locally.** Images generated and edited through each vendor's permitted
path and, optionally, by a local model on the desktop, managed as the other local models. A part of
an image can be selected and changed; versions are kept.

**One history.** Pluto imports the owner's exports from ChatGPT, Claude, Gemini and the others into
one history, searchable, in Pluto's versioned format, and merges their memories into the owner's,
each addition shown before it is kept. Pluto exports to the others too.

**The chat's rivals and speed.** Measured under ADR-0017's rules against:
- ChatGPT's desktop application, in the owner's Windows machine, and Claude's;
- T3 Chat, Open WebUI, LibreChat, Msty and TypingMind;
- LM Studio and Jan, on the same local models;
- Gemini and Le Chat, on their features and voice.

What Pluto adds itself, from sending to the first word shown, is at most 10 ms on the same machine.
The official program's path is measured apart and shown. Text streams word by word without a
stutter. An agent's session is kept ready so that a question does not pay its start. How many and
for how long is left open, against "nothing runs at rest": proposed, only the last models used,
stopped after a while idle; decided at Stage 2 and measured.

**And:**
- scheduled tasks and triggers for the chat, such as a morning summary or a watch on a subject;
- the MCP servers of the one configuration reach the chat, each allowed by the owner;
- chats locked by fingerprint or password, hidden from the list.

## Alternatives considered

- **A complement to the vendors' applications**: much less to build, but Pluto would be handy, not
  the best.
- **API keys everywhere**: no doubt about terms, but paid on top of the subscriptions.
- **Memory shared with code agents by default**: the agents would know the owner, at a cost in
  tokens and relevance. **Explicit memory only**: full control, less useful.
- **SearXNG hosted by the owner**: recommended, private and without an account; the owner chose a
  keyed engine.
- **Files only whole**: simple, but impossible past a few documents.
- **The vendors' voices only**: today's best quality, but paid, the voice at the vendor's, for their
  models only. **Dictation only**: answers stay written.
- **Images by vendors only, or none**: nothing offline, or nothing in the chat.
- **No import**: the history would start in Pluto.
- **Only a margin on the rivals for speed**: no absolute figure on Pluto's own delay.
- **Sharing a chat by a link through the relay**: as the vendors do, but a service open to the
  outside on the owner's machine; not chosen.

## Consequences

- The specification's chat family ([`features.md`](../features.md), family 22) grows with these
  features, and its phone takes them too.
- New services the owner runs or keys: a search engine's key; local models for embeddings, speech,
  voice and images, managed as the other local models. Each new dependency is asked when its
  milestone comes.
- The table of the vendors' paths and terms becomes a document of its own, checked at each stage's
  end.
- Keeping agents' sessions ready is weighed against "nothing runs at rest": the rule and its figure
  are set at Stage 2 and measured.
- [AGENTS.md](../../AGENTS.md) admits a vendor's sign-in for third-party applications beside its
  official programs.
