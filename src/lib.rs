pub mod app;
pub mod diff;
pub mod highlight;
pub mod preview;
pub mod requests;
pub mod snooze;
pub mod subagents;
pub mod terminal;
pub mod theme;
pub mod ui;

pub use pluto_core::{generated_images, html_render, http, mcp_app, model, protocol, timeline};

/// The core's redraw requests, answered by egui.
pub fn repaint(ctx: &eframe::egui::Context) -> pluto_core::Repaint {
    let ctx = ctx.clone();
    pluto_core::Repaint::new(move |delay| match delay {
        Some(delay) => ctx.request_repaint_after(delay),
        None => ctx.request_repaint(),
    })
}
