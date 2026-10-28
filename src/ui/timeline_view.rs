//! Virtualization of the messages timeline: upstream renders it through
//! `LegendList` (`MessagesTimeline.tsx`), which mounts only the rows near
//! the viewport. Each row's height is kept from when it was last drawn
//! (an estimate stands in before that), so a frame lays out and paints only
//! the rows around the visible part; heights that change above the visible
//! part move the scroll offset with them, keeping what is on screen in place.
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use eframe::egui::Id;

use crate::model::{Detail, Thread};
use crate::timeline::{self, Expansion, Row};

#[derive(Default)]
pub(crate) struct TimelineView {
    rows: Option<RowsCache>,
    /// Each row's height when last drawn, the space below it included.
    heights: HashMap<Id, f32>,
    /// The thread `offset` and `follow` belong to.
    pub thread_id: String,
    /// The scroll offset for the next pass.
    pub offset: f32,
    /// The view sticks to the end as rows grow or arrive.
    pub follow: bool,
    /// Rows drawn while text is selected stay drawn until the selection
    /// ends: egui drops a selection whose ends it does not see.
    pub pinned: HashSet<Id>,
    /// The viewport height of the last pass.
    pub viewport: f32,
    /// The height of what closes the timeline (queued follow-ups).
    pub tail: f32,
    /// The wheel or the scroll bar moved the view this pass (the wheel
    /// delta is used up by then).
    pub scrolled: bool,
    /// The first row and the history control's height of the last pass:
    /// rows loaded above move the offset with them.
    pub first: Option<Id>,
    pub header: f32,
    /// The first message or plan in view (find's reading position).
    pub first_entry: Option<String>,
}

struct RowsCache {
    thread_id: String,
    revision: u64,
    expansion: Expansion,
    images: Vec<crate::generated_images::GeneratedImage>,
    rows: Arc<Vec<Row>>,
}

impl TimelineView {
    /// The thread's rows, rebuilt only when the thread or the folds change.
    pub fn rows(
        &mut self,
        thread: &Thread,
        detail: &Detail,
        expansion: &Expansion,
        revision: u64,
        images: &[crate::generated_images::GeneratedImage],
    ) -> Arc<Vec<Row>> {
        if let Some(cache) = &self.rows
            && cache.thread_id == thread.id
            && cache.revision == revision
            && cache.expansion == *expansion
            && cache.images == images
        {
            return cache.rows.clone();
        }
        let mut rows = timeline::rows(thread, detail, expansion);
        timeline::insert_generated_images(&mut rows, images);
        let rows = Arc::new(rows);
        self.rows = Some(RowsCache {
            thread_id: thread.id.clone(),
            revision,
            expansion: expansion.clone(),
            images: images.to_vec(),
            rows: rows.clone(),
        });
        rows
    }

    pub fn first_visible_entry(&self) -> Option<String> {
        self.first_entry.clone()
    }

    /// Starts over on another thread: it opens at its end.
    pub fn open(&mut self, thread_id: &str) {
        if self.thread_id == thread_id {
            return;
        }
        self.thread_id = thread_id.to_owned();
        self.follow = true;
        self.pinned.clear();
        self.first = None;
        self.header = 0.0;
        // Heights are kept by message and turn ids, so threads opened again
        // come back measured; the map is dropped once it grows large.
        if self.heights.len() > 5000 {
            self.heights.clear();
        }
    }

    /// The row's last drawn height, else an estimate (kept until it is drawn).
    pub fn height(&mut self, key: Id, row: &Row, width: f32) -> f32 {
        *self
            .heights
            .entry(key)
            .or_insert_with(|| estimate(row, width))
    }

    pub fn measured(&mut self, key: Id, height: f32) {
        self.heights.insert(key, height);
    }
}

/// A stable identity for a row across rebuilds.
pub(crate) fn row_key(row: &Row) -> Id {
    match row {
        Row::User(message) => Id::new(("user", &message.id)),
        Row::Assistant { message, .. } => Id::new(("assistant", &message.id)),
        Row::TurnFold { run_id, .. } => Id::new(("fold", run_id)),
        Row::Work { id, .. } => Id::new(("work", id)),
        Row::WorkToggle { group_id, .. } => Id::new(("toggle", group_id)),
        Row::WorkDetails { group_id, .. } => Id::new(("details", group_id)),
        Row::Working { .. } => Id::new("working"),
        Row::GeneratedImage { image } => Id::new(("generated", &image.path)),
        Row::Plan(plan) => Id::new(("plan", &plan.id)),
        Row::HtmlRender(page) => Id::new(("html-render", &page.id)),
    }
}

/// A row's height before it is drawn, from its line count at the chat's
/// 14px type; drawing it corrects the guess.
fn estimate(row: &Row, width: f32) -> f32 {
    const LINE: f32 = 22.75;
    let lines = |text: &str, width: f32| -> f32 {
        let per_line = (width / 7.5).max(8.0);
        text.lines()
            .map(|line| (line.chars().count() as f32 / per_line).ceil().max(1.0))
            .sum()
    };
    let height = match row {
        Row::User(message) => {
            let images = if message.attachments.is_empty() {
                0.0
            } else {
                width * 0.3
            };
            lines(message.text.trim_end(), width * 0.8 - 24.0) * LINE + images + 24.0 + 28.0 + 16.0
        }
        Row::Assistant {
            message, show_meta, ..
        } => {
            let meta = if *show_meta { 30.0 + 16.0 } else { 8.0 };
            lines(&message.text, width - 8.0) * LINE + 4.0 + meta
        }
        Row::TurnFold { .. } => 4.0 + LINE + 8.0 + 1.0 + 6.0,
        Row::Work { .. } => 9.25 + LINE + 8.0,
        Row::WorkToggle { .. } => 6.0 + LINE + 8.0,
        Row::WorkDetails { entries, .. } => entries.len() as f32 * (5.25 + LINE) + 4.0,
        Row::Working { .. } => LINE + 4.0 + 6.0,
        Row::GeneratedImage { .. } => width.min(384.0) * 0.75 + 16.0,
        // The frame (`px-1`) at its measured height, then `pb-2`.
        Row::HtmlRender(page) => page.render.frame_height(width - 8.0, None) + 8.0,
        // The card: padding, header, its text (a long plan folded to 26rem
        // and the Expand button) and the row's space.
        Row::Plan(plan) => {
            let text = (lines(&plan.plan_markdown, width - 48.0) * LINE).min(416.0 + 60.0);
            4.0 + 40.0 + 24.0 + 16.0 + text + 16.0
        }
    };
    // Whole points, like the layout the guess stands in for.
    height.round()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_saved_image_refreshes_the_rows_without_a_server_event() {
        let mut view = TimelineView::default();
        let thread = Thread {
            id: "t".into(),
            ..Default::default()
        };
        let detail = Detail::default();
        let expansion = Expansion::default();
        let empty = view.rows(&thread, &detail, &expansion, 1, &[]);
        assert!(empty.is_empty());
        let images = vec![crate::generated_images::GeneratedImage {
            path: "/fixture/native/exec-image.png".into(),
            modified: std::time::SystemTime::UNIX_EPOCH,
            bytes: 128,
        }];
        let loaded = view.rows(&thread, &detail, &expansion, 1, &images);
        assert!(matches!(&loaded[..], [Row::GeneratedImage { .. }]));
        assert!(estimate(&loaded[0], 768.0) > 0.0);
        let unchanged = view.rows(&thread, &detail, &expansion, 1, &images);
        assert!(Arc::ptr_eq(&loaded, &unchanged));
    }
}
