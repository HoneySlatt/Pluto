# 0041. The assistant on every plan opened to third-party applications

- Status: Accepted; amends [0030](0030-pluto-becomes-a-harness.md) (what the assistant runs on)
- Date: 2026-11-15

## Context

[ADR-0030](0030-pluto-becomes-a-harness.md) set that the personal assistant runs on API keys,
DeepSeek or local models, never on a Claude subscription. It named no other subscription.

On 2026-11-15 the owner asked whether that was too strict, since Hermes, a personal agent, lets
Claude and ChatGPT subscribers "bring their plan", with Gemini said to come. What the sources say:

- **Hermes and Claude.** Its own documentation says that its Claude sign-in "only works on a Claude
  Max plan with purchased extra usage credits", that "the base Max allowance is never consumed by
  Hermes", and that the sign-in "routes as Claude Code"; by default it also borrows Claude Code's
  stored login ([providers](https://hermes-agent.nousresearch.com/docs/integrations/providers), read
  2026-11-15). The subscription's included quota is never used.
- **Anthropic.** Its legal page forbids third-party developers to offer Claude.ai's sign-in in
  their applications, or to collect, store or intermediate its credentials
  ([legal and compliance](https://code.claude.com/docs/en/legal-and-compliance)). Yet third-party
  harnesses that use it are billed as "extra usage", per token, according to an e-mail of
  Anthropic's quoted on Hacker News (2026-04-03) and Pi's documentation
  ([providers](https://raw.githubusercontent.com/earendil-works/pi/v0.86.1/packages/coding-agent/docs/providers.md)).
  No page of Anthropic's own states that rule. Extra usage is billed at API prices, as an API key
  is, and Anthropic's larger plans include monthly API credits usable with a key.
- **OpenAI.** "Sign in with ChatGPT" lets open-source, locally run applications use the user's
  plan; OpenAI lists OpenClaw, a personal assistant, among them
  ([Sign in with ChatGPT](https://developers.openai.com/codex/sign-in-with-chatgpt.md)). It is a
  preview, and its limit is reported shared with ChatGPT and Codex.
- **Google** calls the use of Gemini CLI's sign-in by third-party software a violation. A fan
  account that follows Hermes, not Nous Research itself, said on 2026-11-15 that Nous is working
  with Google on an approved sign-in, with no date.

## Decision

**The assistant may run on any subscription a provider officially opens to third-party
applications**, through the sign-in that provider offers, as
[ADR-0020](0020-the-chat.md) and `AGENTS.md` already allow for the rest of Pluto:
- today, ChatGPT's plan through Sign in with ChatGPT;
- Gemini's plan if Google publishes such a sign-in;
- any other provider that does the same, without a new decision.

**Claude, for now, through an API key**, which draws first on any monthly API credits the plan
includes. The subscription's included quota is never used by the assistant.

**Claude's extra usage the day it is official.** If Anthropic publishes a sign-in for third-party
applications billed as extra usage, Pluto takes it without a new decision.

**Never** borrowed credentials, nor a sign-in that presents Pluto as another program.

## Alternatives considered

- **ADR-0030 unchanged.** The assistant never touches a subscription the work also needs. But
  ChatGPT's plan is open to it by OpenAI's own terms, and would go unused.
- **ChatGPT only**, each later plan its own decision. Narrower, but every provider that opens its
  plan would wait for a decision the rule above already answers.
- **Claude's extra usage now, as Hermes does.** The same price as an API key. But against
  Anthropic's written terms and Pluto's rule on other programs' credentials, on a policy that
  changed five times in 2026; the owner's account would carry the risk.
- **Claude's included quota through Claude Code, for the assistant too.** The cheapest. But an
  always-on assistant is the automated use Anthropic's consumer terms name, and the owner kept the
  assistant off that quota.

## Consequences

- The assistant's use of ChatGPT's plan counts against limits the owner's work shares; the
  assistant's own budget ([ADR-0030](0030-pluto-becomes-a-harness.md)) caps it.
- Sign in with ChatGPT is a preview: no hosted tools, `store: false` on every request. Whether it
  covers a general application with its own loop stays the open question of
  [ADR-0031](0031-three-kinds-of-engine.md).
- The table of the vendors' paths ([ADR-0020](0020-the-chat.md)) records each plan opened to third
  parties, checked again at each stage's end.
