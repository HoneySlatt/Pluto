pub mod app;
pub mod highlight;
pub mod preview;
pub mod snooze;
pub mod subagents;
pub mod terminal;
pub mod theme;
pub mod ui;

pub use pluto_core::{
    commands, diff, generated_images, html_render, http, mcp_app, model, protocol, requests,
    timeline, worktree_setup,
};

/// The core's redraw requests, answered by egui.
pub fn repaint(ctx: &eframe::egui::Context) -> pluto_core::Repaint {
    let ctx = ctx.clone();
    pluto_core::Repaint::new(move |delay| match delay {
        Some(delay) => ctx.request_repaint_after(delay),
        None => ctx.request_repaint(),
    })
}
