# 0005. Pluto's own typed protocol

- Status: Accepted
- Date: 2026-11-06

## Context

The client speaks T3's protocol today: Effect RPC envelopes over a WebSocket, and orchestration
protocol 2 (`crates/pluto-core/src/protocol.rs`). It is untyped on Pluto's side: messages are
`serde_json::Value`, built by 1,047 `json!` calls in 125 files. T3 changes its protocol every week.

Several clients will speak to Pluto's server: the egui client, the Android app through UniFFI, then
uwui's client on the desktop, the phone and the web. Several servers, on machines that do not update
together, will speak to each client ([ADR-0007](0007-several-servers-and-a-relay.md)).

## Decision

**Pluto has its own protocol, described once in Rust types** in a crate shared by the server and
every client. The Kotlin app gets them through UniFFI. No IDL is invented.

**It is versioned from its first message.** A client and a server state their versions and
capabilities on connecting. Each side uses only what both have, and a mismatch says plainly what is
missing.

**Results from several servers may be partial.** Every list, search and count says which servers
answered.

**Its wire format and its transports are decided when the protocol is built.** The candidates are
in [`ROADMAP.md`](../ROADMAP.md#open).

## Alternatives considered

- **T3's protocol, imitated by the Rust server**: the egui client unchanged, but tied to a protocol
  that changes weekly, and untyped.
- **ACP as the client protocol**: a standard, but it describes one agent's session, not a server
  of many threads, machines, terminals and files. Its session model may still shape the agent's
  part of Pluto's protocol.

## Consequences

- The egui client and the Android app move from T3's protocol to Pluto's, once. This is the
  largest change the frozen client receives ([ADR-0010](0010-todays-features-triaged.md)).
- The web client loses nothing: T3's web client stops working with Pluto's server, and Pluto's own
  web client comes through uwui.
