# 0011. Pluto's own harness, minimal and measured

- Status: Accepted; amends [0003](0003-what-pluto-is-to-become.md) (the agent loop kept in view is
  decided) and [0006](0006-agents-drivers.md) (one driver is Pluto's own agent)
- Date: 2026-11-07

## Context

Local models code through a companion OpenCode instance today. That needs workarounds:
- derived `pluto/<model>:<k>k` models, because Ollama's OpenAI-compatible API ignores the
  context length;
- Pluto's MCP tools, which OpenCode 2 hides from the model.

On 2026-10-28, one bug-fixing task with local models at 32K of context gave:
- OpenCode: 4/4, with a prompt of about 4,700 tokens;
- Claude Code: 5/5, only once its output and auto-compaction were tuned, with about 10,000 tokens;
- Pi: 3/4, with about 1,300 tokens. Its nested edit schema broke gemma4's tool parser;
- Codex: 3/5.

The owner asked whether Pluto should be a harness of its own, like Claude Code, Pi or OpenCode. The
field was surveyed on 2026-11-07. Measured, from the sources named in the alternatives below:

- **The harness matters most for small models.**
  - Among frontier models, the harness moves results by about 3 to 8 points.
  - For small models, by up to 27 points (Claw-SWE-Bench, arXiv 2606.12344).
- **Three components pay at 30B and 32K of context** (arXiv 2609.20804):
  - managing the context, by cutting old tool outputs and then summarising: SWE-bench Verified from
    9.4 % to about 21–24 %; without it, 79 % of failures are context overflows;
  - a plan tool: +11.6 points;
  - file tools rather than bash alone: +15 points. Bash alone pays only for large models.
  
  A tool to recall cut content gave nothing.
- **Unproven for coding.** Subagents, MCP, evaluator agents, skills, hooks and LSP beyond a syntax
  check have no measured gain. Some have a measured cost: an MCP browser takes 13,000 to 18,000
  tokens, and an evaluator agent costs 20 times more.
- **Failures on local models come from fit, not features.**
  - Tool calls are misparsed (Ollama's qwen and gemma bugs).
  - Tool names differ from the ones the model expects (`read_file` for `read`).
  - Formats are converted lossily: gpt-oss-20b reaches about 60 % on SWE-bench Verified in its
    native Harmony format, and about 3 % on Terminal-Bench 2.0 in general harnesses.
- **The edit format depends on the model** (Aider's benchmarks):
  - whole files are the most reliable for small models;
  - search and replace suits large ones;
  - `apply_patch` suits models trained by OpenAI.

Reuse was weighed:
- codex-rs and goose are in Rust, but their cores are unpublished and change weekly.
- codex-rs speaks only OpenAI's Responses API, with prompts tuned for GPT.
- goose routes every tool through MCP, and is being rewritten.
- Pi has the cleanest RPC, but needs Node.

## Decision

**Pluto gets its own agent harness, minimal and measured.** It is a driver of Pluto's Rust server,
for local models and for providers reached by API key or by plans that allow third-party tools.
Claude's and ChatGPT's subscriptions stay on their official CLIs.

**Its core is small.**
- A short system prompt.
- Read, write, edit and bash, with grep, glob and ls for small models.
- An agent loop that can be steered and interrupted.

**It holds what the measurements require.**
- Context management in two steps: old tool outputs replaced by a stub, then a summary under fixed
  headings near the limit. The real context length is checked, and silent truncation refused.
- Tool output truncated.
- A plan tool.
- Loop detection.
- Tolerant edits, with a syntax check after each one.

**Each model has a profile.** It sets the edit format, the aliases of tool names, a parser for tool
calls written as text, the native message format where one exists, and which components are on. A
component whose effect changes sign between models is switched per profile.

**What Pluto's server already does stays in the server.** Subagents, delegation, the browser,
memory, the sandbox, terminals and worktrees are offered as tools. They are not rebuilt in the
harness.

**Every component earns its place on Pluto's bench.** The bench is a small fixed set of tasks, run
on the owner's local models.
- A component is kept only if the bench shows its gain.
- The harness must beat OpenCode on the owner's local models before it replaces the companion.
- Agents may run the bench on local models through Ollama by hand during a milestone, never in CI.
- Runs on models reached by API key or subscription are the owner's alone.

**It comes after Stage 3** ([ROADMAP](../ROADMAP.md#4-stages)), once the driver interface and the
sandbox exist. A command-line client of Pluto's protocol, usable in a terminal like `claude` or
`pi`, comes later.

## Alternatives considered

- **A complete harness, like Claude Code**: subagents, plan mode, MCP, skills, hooks and LSP. None
  has a measured gain in coding. Pluto's server already provides most of them. It would be a second
  large product to maintain.
- **A pure minimal harness, like Pi**: the smallest. But small models with 16K to 64K of context
  fail without context management and a plan tool.
- **Embedding codex-rs**: in Rust, but unpublished, about 100 commits a month to its core,
  Responses API only, GPT's prompts.
- **Embedding goose**: many providers. But its core is unpublished, every tool goes through MCP, and
  a rewrite is under way. Its declarative provider files may be reused as data, under Apache-2.0,
  with their notice.
- **Driving Pi over RPC**: little to build, but Node returns, and each model's fixes would be made
  outside Pluto.
- **Keeping the OpenCode companion**: nothing to build, but the workarounds stay and its prompt
  weighs on 32K of context.

## Consequences

- The harness is one more driver to maintain. Its churn is the providers' APIs: Chat Completions,
  Anthropic's Messages and Ollama's native API, all stable.
- Its quality is a figure on the bench, not a claim. The bench's tasks are chosen when the harness
  is designed.
- AGENTS.md allows agents to run the bench on local models.
- The OpenCode companion stays until the harness beats it.
