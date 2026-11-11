//! Agents' sessions recorded and replayed ([ADR-0025]).
//!
//! An agent's CLI speaks a line-based protocol on its stdio: Claude's `stream-json`, or ACP's
//! JSON-RPC. `pluto-record` sits between a server and the CLI: it masks each line, writes it to a
//! recording, then forwards it unchanged. `pluto-replay-agent` plays a recording back in the CLI's
//! place, so that a driver is tested without a real turn.
//!
//! - [`format`]: the recording's file, JSON Lines, versioned from its first line.
//! - [`mask`]: what is kept out of a recording: account fields, the owner's lists, secrets,
//!   paths; and the ids the client chose, which become placeholders.
//! - [`scan`]: the check every committed recording passes, with detectors of its own.
//! - [`replay`]: the logic of the fake agent, without its I/O.
//! - [`program`]: what the two programs share: their command lines, and ending like an agent.
//!
//! [ADR-0025]: ../../../docs/adr/0025-agents-sessions-recorded-and-replayed.md

pub mod format;
pub mod mask;
pub mod program;
pub mod replay;
pub mod scan;
