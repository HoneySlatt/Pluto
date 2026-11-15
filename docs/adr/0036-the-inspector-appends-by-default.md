# 0036. The inspector appends by default

- Status: Accepted
- Date: 2026-11-15

## Context

The owner asked for a context inspector at every turn: what the model receives, and the power to
change it. Changing a model's context breaks its prompt cache, which is where most of the cost
lies:
- Most of the owner's spend on Claude is context read again from the cache.
- Changing tools, plugins or the model mid-thread restarts Claude Code's cache
  ([prompt caching](https://code.claude.com/docs/en/prompt-caching.md)).
- An edit early in a context of 150,000 tokens makes the next request's input many times dearer,
  about 20 times by an estimate from Opus's prices: everything after the edit is paid in full
  instead of read from the cache.

The field avoids editing the past:
- OpenCode 2.2 adds keyed session instructions "instead of editing the system prompt and busting
  the prompt cache" ([changelog](https://opencode.ai/changelog)).
- Hermes keeps its system prompt fixed, and adds skills as a note on the next message.
- Pi edits the model's context with an entry, without rewriting its history.

On Claude Code, a mod sees the newest 4,096 entries and what each request costs. It can pin or
remove parts of the system prompt and tools, each at the price of a cache restart, and cut new tool
outputs. It cannot remove or summarise a past turn ([ADR-0031](0031-three-kinds-of-engine.md)).

## Decision

**The inspector shows, at every turn, what the model receives**, part by part, with its tokens and
its cost.

**A change appends by default**: a small delta after what the model has seen, so the cache holds.

**Rewriting the past shows its price first and asks.** Removing or summarising earlier content, or
changing the system prompt or the tools, is shown with what the next request will cost, and done
only on the owner's go.

**What an engine cannot change is shown as absent**, as Claude Code's past turns.

## Alternatives considered

- **Free editing of the whole context.** The most control. But every edit is a cache miss, paid
  without warning.
- **An inspector that only shows.** No surprise in cost. But the owner asked to act on the context,
  not only to see it.

## Consequences

- A change is an entry in the session's log, never a rewrite of it
  ([ADR-0033](0033-one-pluto-agent-process-per-session.md)).
- The inspector's figures need each provider's usage per request: the provider clients and the mod
  report it.
