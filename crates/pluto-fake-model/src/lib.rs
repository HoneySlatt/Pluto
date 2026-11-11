//! A scripted fake model service ([ADR-0026]).
//!
//! It answers like a model over four APIs: Anthropic's Messages, OpenAI's Responses and Chat
//! Completions, and Ollama's chat. Every application that takes a custom endpoint, Pluto included,
//! receives the same answer bytes from one script, paced like a model, so that they are measured
//! on one workload with no quota spent. It counts what each request carries, by bytes and by part.
//!
//! It keeps no conversation state: where a request stands in the script is read from the request
//! itself. The prompt that starts a turn carries a marker, `[turn N]`; each tool result sent back
//! after it moves to the turn's next segment.
//!
//! - [`script`]: the script's file, versioned, and where a request stands in it.
//! - [`conversation`]: what a request carries, read the same way for each API.
//! - [`structured`]: JSON that fits a schema, for the side requests that ask for it.
//! - [`tools`]: the script's tool calls mapped to the tools a client offers.
//! - [`wire`]: the answer encoded for each API, streamed or whole.
//! - [`http`]: the little HTTP/1.1 the service needs, on `httparse`.
//! - [`server`]: the listener, the routes, the pacing and the journal.
//!
//! [ADR-0026]: ../../../docs/adr/0026-the-rivals-measured-how.md

pub mod conversation;
pub mod http;
mod json;
pub mod script;
pub mod server;
pub mod structured;
pub mod tools;
pub mod wire;
