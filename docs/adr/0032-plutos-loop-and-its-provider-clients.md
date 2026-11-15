# 0032. Pluto's loop and its own provider clients

- Status: Accepted; supersedes [0011](0011-plutos-own-harness-minimal-and-measured.md)
- Date: 2026-11-15

## Context

[ADR-0011](0011-plutos-own-harness-minimal-and-measured.md) gave Pluto a minimal harness, a driver
of its server for local models, after Stage 3 of the ROADMAP of 2026-11-08. Pluto now becomes a harness
([ADR-0030](0030-pluto-becomes-a-harness.md)), and its loop runs every model the terms allow it
([ADR-0031](0031-three-kinds-of-engine.md)). ADR-0011's evidence still holds:
- the harness matters most for small models: up to 27 points, against 3 to 8 among frontier
  models;
- at 30B and 32K of context, three components pay: context management (79 % of failures are
  overflows without it), a plan tool (+11.6 points), file tools rather than bash alone (+15);
- failures on local models come from fit: tool calls misparsed, unexpected tool names, formats
  converted lossily;
- the edit format depends on the model.

The surveys of 2026-11-15 add:
- **Tuning moves a fixed model.** The same model rose from 52.8 to 66.5 on Terminal-Bench 2.0 by
  its harness alone ([LangChain](https://blog.langchain.com/improving-deep-agents-with-harness-engineering/),
  reported).
- **A heavy base costs.** Claude Code sends about 32,800 tokens before the prompt, OpenCode about
  6,900, and the cost per passing run differed 3.7 times
  ([Systima](https://systima.ai/blog/claude-code-vs-opencode-token-overhead)). Pi works with under
  1,000 ([Pi](https://mariozechner.at/posts/2025-11-30-pi-coding-agent/)).
- **Add-ons rarely pay.** On one model and 100 tasks, effort gave +11.3 points, the best plugin
  +2.4 to +3.3, the heaviest no clear gain at 8 times the cost
  ([Surge](https://surgehq.ai/blog/benchmarking-coding-agent-harnesses)).
- **No official Rust SDK** exists from Anthropic or OpenAI. The Rust libraries:
  - `genai` covers many providers, but has one maintainer, 157 versions, and a README that says
    "depth is secondary" ([README](https://raw.githubusercontent.com/jeremychone/rust-genai/main/README.md));
  - `rig-core` is pre-1.0 with breaking releases, now ships a coding agent of its own, and has a
    plugin that shapes requests for Anthropic's subscription sign-in, against the terms;
  - `async-openai` covers OpenAI only.
- **Codex**, the best Rust harness, writes its own provider clients.
- `crates/pluto-fake-model` already serves the four wire formats Pluto needs.
- [models.dev](https://models.dev/api.json) (MIT) lists 324 providers and 9,878 models, with costs,
  limits and capabilities; OpenCode and rig read it.

## Decision

**The loop is a crate**, used by `pluto-agent`, the TUI and later the phone.

**Its core is small, as ADR-0011 set it:**
- a short system prompt;
- read, write, edit and bash, with grep, glob and ls for small models;
- a loop that can be steered and interrupted;
- context managed in two steps: old tool outputs replaced by a stub, then a summary under fixed
  headings near the limit; the real context length checked, silent truncation refused;
- tool output truncated, a plan tool, loop detection, tolerant edits checked for syntax.

**Every component earns its place on the bench** ([ADR-0037](0037-how-the-harness-is-judged.md)),
and is removed when a model no longer needs it.

**Each model has a profile**: the edit format, tool names' aliases, a parser for tool calls written
as text, the native message format where one exists, the effort, and which components are on. A
component whose effect changes sign between models is switched per profile.

**Four provider clients of Pluto's own:** Anthropic's Messages, OpenAI's Responses, Chat
Completions and Ollama's native API, tested against the fake model service. What differs between
providers (cache breakpoints, reasoning blocks, `store`, roles, limits) lives in a table of quirks,
as data.

**A model registry from models.dev**, kept as a snapshot in the repository. It is refreshed only on
the owner's go: nothing leaves the machine of Pluto's own accord.

**The lead's implementation choices**, from the challenge, shown to the owner with this record:
- Pluto's own tools are native: an MCP browser costs 13,000 to 18,000 tokens at a session's start.
- Outside tools come through MCP, with the official Rust SDK, `rmcp`, pinned; as a new dependency,
  it is asked of the owner when its milestone comes.
- Extensions are text for now: skills, hooks and named agents as files. No WebAssembly extension
  until a second concrete use exists.

## Alternatives considered

- **genai underneath.** Many providers for little code. But the harness lives in the depth it
  leaves secondary: cache breakpoints, reasoning kept across tool turns, `store: false` for
  ChatGPT's plan. One maintainer.
- **rig-core.** The widest, with a catalog. But a framework that breaks at each release, now a
  competing harness.
- **async-openai and ollama-rs**: typed wires for two providers, nothing for Anthropic's.
- **Embedding codex-rs or goose** (from ADR-0011): unpublished or alpha cores that change weekly.
- **Driving Pi over RPC**: Node returns.
- **WebAssembly extensions now**, on wasmtime: the most isolated. But no second use yet, and
  wasmtime ships a major version every month.

## Consequences

- Four clients and a quirks table to maintain. Their churn is the providers' APIs, slower than any
  Rust library on top of them.
- The OpenCode companion stays until the loop beats it on the bench, on the owner's local models.
- Who runs the bench, on which models, is [ADR-0037](0037-how-the-harness-is-judged.md)'s.
- `rmcp` and `agent-client-protocol` change fast (ACP went from 3.6 to 5.3 in 17 days); both are
  pinned, and their upgrades are planned work.
- Each new dependency is still asked of the owner when its milestone comes.
