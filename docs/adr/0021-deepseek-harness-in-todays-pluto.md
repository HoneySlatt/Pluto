# 0021. DeepSeek Harness in today's Pluto

- Status: Accepted
- Date: 2026-11-08
- Amends: [0006](0006-agents-drivers.md), the deferred DeepSeek interface decision;
  [0010](0010-todays-features-triaged.md), a targeted exception to the frozen client.

## Context

The owner wants the official DeepSeek Harness beside Claude Code and Kimi in today's Pluto,
before the Rust server replaces the TypeScript fork. DeepSeek models through OpenCode do not
provide that harness.

DeepSeek Harness 0.2.1-rc.3 exposes three interfaces. ACP supports persistent sessions, permissions
and cancellation, but publishes committed responses rather than live tokens and omits interactive
questions. The stdio SDK has no cancel operation. Its official local Web server exposes session
creation, model selection, live output, cancellation and interactive requests.

## Decision

Add a dedicated `deepseekHarness` driver to the current server fork, using the official local Web
server, as Kimi's native driver does. The client keeps its native interface; no browser UI is
embedded. Use the installed official `dsh` executable. The driver accepts the tested preview
version only; the shared compatibility manifest keeps prereleases unknown.

Keep the instance's Harness state in its own directory under the server's state directory.
Authentication stays with the official program; Pluto never imports another program's credentials.
Disable Harness session-log contributions and telemetry explicitly. No model request is made by
automated tests. Tests use fake protocol peers and disposable profiles.

Advertise only operations the adapter implements. Record unsupported behavior in the provider's
guide. This exception adds no unrelated feature to the frozen client or server and does not bring
forward Pluto's own harness.

## Alternatives

- Local ACP configuration: less code, but incomplete streaming and interaction for daily use.
- Native stdio SDK: simpler transport, but cancellation requires killing the process.
- Wait for the Rust server: no work in the fork, but the owner cannot use the harness now.

## Consequences

The Web protocol is a preview, so the driver needs a bounded compatibility range and replay or
fake-peer tests. A successful build proves packaging; real sign-in and a paid turn remain the
owner's validation.

## Sources

- [Official ACP contract](https://github.com/deepseek-ai/deepseek-harness/blob/master/packages/acp/acp/README.md)
- [Official SDK contract](https://github.com/deepseek-ai/deepseek-harness/blob/master/packages/sdk/protocol/README.md)
- [Official session controller](https://github.com/deepseek-ai/deepseek-harness/blob/master/packages/api/session-controller/README.md)
- [Official Web application](https://github.com/deepseek-ai/deepseek-harness/blob/master/packages/bundle/web-app/README.md)
