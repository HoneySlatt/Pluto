//! What the Pluto clients share: the server connection (`protocol`), the
//! conversation model it fills (`model`) and the timeline built from it
//! (`timeline`, with the published pages, MCP apps and generated images
//! it places).

use std::sync::Arc;
use std::time::Duration;

pub mod chat_requests;
pub mod commands;
pub mod context;
pub mod diff;
pub mod generated_images;
pub mod html_render;
pub mod http;
pub mod mcp_app;
pub mod model;
pub mod notifications;
pub mod protocol;
pub mod requests;
pub mod spaces;
pub mod timeline;

/// Asks the client's interface to redraw, now or after a delay; the
/// connection worker calls it when an event arrives.
#[derive(Clone)]
pub struct Repaint(Arc<dyn Fn(Option<Duration>) + Send + Sync>);

impl Repaint {
    pub fn new(redraw: impl Fn(Option<Duration>) + Send + Sync + 'static) -> Self {
        Self(Arc::new(redraw))
    }

    pub fn now(&self) {
        (self.0)(None);
    }

    pub fn after(&self, delay: Duration) {
        (self.0)(Some(delay));
    }
}
