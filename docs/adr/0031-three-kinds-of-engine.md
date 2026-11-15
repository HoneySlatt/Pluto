# 0031. Three kinds of engine under one thread model

- Status: Accepted; amends [0006](0006-agents-drivers.md) (a driver becomes an engine; three kinds)
- Date: 2026-11-15

## Context

Pluto becomes a harness ([ADR-0030](0030-pluto-becomes-a-harness.md)). An *engine* is what runs a
thread's turns: Pluto's own loop, or another program's. Which engine may run which model is set by
each provider's terms, read on 2026-11-15:

- **Anthropic.** A subscription may drive only the unmodified `claude` binary, signed in by the
  user through Anthropic's own flow. Third parties may not route requests through plan credentials,
  nor collect or intermediate them. The Agent SDK needs an API key unless approved
  ([legal and compliance](https://code.claude.com/docs/en/legal-and-compliance),
  [Agent SDK](https://code.claude.com/docs/en/agent-sdk/overview)).
- **OpenAI.** "Sign in with ChatGPT" lets an open-source, locally run application use the user's
  plan through the public Responses API. It is a preview: `store: false` and `stream: true` on
  every request, no hosted tools, the app refreshes its token
  ([guide](https://developers.openai.com/siwc/token-sharing-open-source.md),
  [limits](https://developers.openai.com/siwc/token-sharing-open-source/preview-limitations.md)).
  Codex's app-server also signs in with ChatGPT, never for commercial or hosted services
  ([app-server](https://developers.openai.com/codex/app-server)).
- **Google** calls direct access with Gemini CLI's sign-in from third-party software "a violation"
  ([terms](https://geminicli.com/docs/resources/tos-privacy/)).
- **GitHub Copilot**'s SDK drives GitHub's own runtime with the user's login
  ([copilot-sdk](https://github.com/github/copilot-sdk)).
- **Z.ai**'s plan works only "within officially supported tools"
  ([policy](https://docs.z.ai/devpack/usage-policy)). **Kimi**'s plan key works in third-party
  developer tools, and "tampering with the client identifier (User-Agent) is considered a
  violation" ([Kimi Code](https://www.kimi.com/code/docs/en/)).

Claude Code's mods, official since 2026-10-01, are plugins that change Claude Code without
modifying its binary ([mods](https://code.claude.com/docs/en/plugins/mods/overview.md),
[reference](https://code.claude.com/docs/en/plugins/mods/reference.md)). A mod can:
- rewrite or omit system-prompt sections, reminders and tool descriptions;
- answer a tool call with Pluto's own result, so the model keeps its trained tool names while
  Pluto runs the tool in its sandbox;
- read each request's usage, and the plan's limits and reset times;
- rewrite tool results before they are stored; withhold subagents.

It cannot replace the loop or its compaction, nor remove or summarise a past turn.

Claude Code with Opus 5.5 leads Terminal-Bench 4.1, at 65.8 ± 3.1
([board](https://www.tbench.ai/leaderboard/terminal-bench/2.0), 2026-11-14). Codex with GPT-6.1 Sol
is fifth, at 60.9 ± 3.0. Claude Code's documentation says `--bare` "will become the default for
`-p`"; bare mode never reads OAuth credentials and skips plugins
([headless](https://code.claude.com/docs/en/headless.md)).

## Decision

**One thread model, three kinds of engine.** Every engine writes the same session log
([ADR-0033](0033-one-pluto-agent-process-per-session.md)), so threads look and behave alike
whatever runs them. What an engine cannot give is shown as absent.

1. **Pluto's own loop** ([ADR-0032](0032-plutos-loop-and-its-provider-clients.md)). It runs
   everything the terms allow it: local models, DeepSeek, API keys and credits, and ChatGPT's plan
   through Sign in with ChatGPT.
2. **Vendors' engines:**
   - Claude Code, driven through a mod of Pluto's own, the binary unmodified, signed in by the
     owner. Pluto never reads its credentials.
   - Codex, through its app-server.
   - Copilot, through its SDK.
3. **ACP** for the other agents.

**The main work stays on Claude Code with the mod.** Pluto's loop is measured against it on the
bench ([ADR-0037](0037-how-the-harness-is-judged.md)). It takes over a kind of work only where it
matches or beats it.

**Claude Code under the mod**, the lead's implementation choices from the challenge, shown to the
owner with this record:
- Pluto appends to Claude's system prompt by default. It replaces the prompt only per thread, on
  the owner's choice, and both ways are measured.
- Pluto serves its tools to Claude over MCP, from its own process, and uses Claude Code's hooks.
- A replay test fails the day `-p` turns bare by default, so the change is seen before it breaks
  the owner's work.

**What the terms rule out.** Pluto never puts a proxy on Claude's API address to watch its
traffic: that would intermediate the plan's credentials. Pluto's loop sends Pluto's own client
identifier, as Kimi's terms require.

**Rust without Node, with one exception.** Pluto's own code has no Node. `claude` is a Node
program, and its mod is a small TypeScript program. Both remain while Claude Code is an engine.

## Alternatives considered

- **Pluto's loop only, Claude through an API key.** Full control. But Claude Code's lead on the
  board is lost, and heavy users report paying 10 to 40 times a subscription's price at API rates
  (estimates from users' reports, not measured).
- **Claude Code relayed bare, with no mod.** No TypeScript in Pluto. But no control of its prompt
  or tools, no plan limits, and the subscription stops working the day `-p` turns bare.
- **Two kinds of engine, without ACP.** Less to maintain. But the plans that work only in official
  tools, such as Z.ai's, would be lost; whether Google's may be driven through Gemini CLI is still
  unanswered.
- **Codex only where Pluto's loop falls short.** Less to build. But Codex is OpenAI's own engine on
  ChatGPT's plan, and it reaches the board's top five far more cheaply.
- **A proxy on the vendors' API addresses**, to feed the inspector. Forbidden by Anthropic's
  terms, and by Pluto's rule on other programs' credentials.

## Consequences

- [ADR-0006](0006-agents-drivers.md) holds for vendors' engines and ACP: native where it brings
  what ACP lacks, Cursor dropped, every engine tested without a real turn, idle processes stopped.
- Claude Code's mod features date from versions 2.1.289 to 2.1.323. The CLIs are to be pinned in
  the Nix shell, so that their changes show in the replay tests; today they are not
  ([`debt.md`](../debt.md)).
- The quotas come from each engine's own interface, Claude's from the mod
  ([ADR-0022](0022-the-triage-row-by-row.md)).
- Whether Sign in with ChatGPT covers a general application with its own loop, beyond the open-source
  examples OpenAI lists, is open; a first test, on the owner's account, or OpenAI's interest form
  settles it.
- Whether DeepSeek Harness stays as an engine beside DeepSeek's models in Pluto's loop is open
  ([ADR-0021](0021-deepseek-harness-in-todays-pluto.md) covers only the fork).
