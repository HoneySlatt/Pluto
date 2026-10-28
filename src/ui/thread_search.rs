//! Searching every thread's messages (`state/queries.ts`'s `useThreadSearch`,
//! `orchestration.searchThreads`, `ThreadSearchMatch.tsx`): once typing
//! pauses (200ms) on two characters or more, the server returns the threads
//! whose messages match, with an excerpt. The command palette lists them
//! with their thread rows and the sidebar's search keeps them after the
//! title matches, both showing "You:" or "Agent:" and the excerpt.
//!
//! Not ported: searching several environments (this client has one).

use std::collections::HashMap;
use std::time::Duration;

use eframe::egui::{self, Color32, Pos2, Rect, Ui};
use serde_json::{Value, json};

use crate::app::App;
use crate::theme::{self, Weight, font};
use crate::ui::widgets;

/// `THREAD_SEARCH_DEBOUNCE_MS`.
const DEBOUNCE: Duration = Duration::from_millis(200);

/// A thread's best message match.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Match {
    /// The user wrote it (else the agent).
    pub(crate) user: bool,
    pub(crate) snippet: String,
}

/// Who searches: the sidebar's field and the palette's query search apart.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Slot {
    Sidebar,
    Palette,
}

impl Slot {
    fn index(self) -> usize {
        self as usize
    }
}

#[derive(Default)]
pub(crate) struct ThreadSearch {
    /// The query typed, and when (egui's clock).
    typed: Option<(String, f64)>,
    asked: String,
    /// The query `matches` answer.
    settled: String,
    matches: HashMap<String, Match>,
}

impl App {
    /// The threads whose messages match `query`, by thread id; asks the
    /// server once typing pauses.
    pub(crate) fn thread_search(
        &mut self,
        ctx: &egui::Context,
        slot: Slot,
        query: &str,
    ) -> HashMap<String, Match> {
        let query = query.trim().to_owned();
        let now = ctx.input(|i| i.time);
        let search = &mut self.thread_search[slot.index()];
        if query.chars().count() < 2 {
            *search = ThreadSearch::default();
            return HashMap::new();
        }
        if query != search.asked {
            match &search.typed {
                Some((typed, since)) if *typed == query => {
                    if now - since >= DEBOUNCE.as_secs_f64() {
                        search.typed = None;
                        search.asked = query.clone();
                        self.send_request(
                            format!("thread-search:{}:{query}", slot.index()),
                            "orchestration.searchThreads",
                            json!({"query": query}),
                        );
                    } else {
                        ctx.request_repaint_after(DEBOUNCE);
                    }
                }
                _ => {
                    search.typed = Some((query.clone(), now));
                    ctx.request_repaint_after(DEBOUNCE);
                }
            }
        }
        // A query still settling shows no stale matches.
        let search = &self.thread_search[slot.index()];
        if search.settled == query {
            search.matches.clone()
        } else {
            HashMap::new()
        }
    }

    /// The settled match of a thread for the sidebar's search, when its
    /// title does not match already.
    pub(crate) fn sidebar_excerpt(&self, thread: &crate::model::Thread) -> Option<&Match> {
        let query = self.search.trim();
        let search = &self.thread_search[Slot::Sidebar.index()];
        if query.is_empty()
            || search.settled != query
            || thread.title.to_lowercase().contains(&query.to_lowercase())
        {
            return None;
        }
        search.matches.get(&thread.id)
    }

    /// `orchestration.searchThreads`' answer; true when `key` was one.
    pub(crate) fn thread_search_result(
        &mut self,
        key: &str,
        result: &Result<Value, String>,
    ) -> bool {
        let Some((slot, query)) = key
            .strip_prefix("thread-search:")
            .and_then(|rest| rest.split_once(':'))
        else {
            return false;
        };
        let Some(search) = slot
            .parse::<usize>()
            .ok()
            .and_then(|slot| self.thread_search.get_mut(slot))
        else {
            return true;
        };
        if query == search.asked {
            search.settled = query.to_owned();
            search.matches = matches(result.as_ref().ok());
        }
        true
    }
}

/// The user's and the agent's matches, the best first per thread.
fn matches(value: Option<&Value>) -> HashMap<String, Match> {
    let mut out = HashMap::new();
    for m in value
        .and_then(|v| v["matches"].as_array())
        .into_iter()
        .flatten()
    {
        let (Some(thread), Some(source)) = (m["threadId"].as_str(), m["source"].as_str()) else {
            continue;
        };
        if source != "user" && source != "assistant" {
            continue;
        }
        out.entry(thread.to_owned()).or_insert_with(|| Match {
            user: source == "user",
            snippet: m["snippet"].as_str().unwrap_or_default().to_owned(),
        });
    }
    out
}

/// `ThreadSearchMatchExcerpt`: "You:" or "Agent:", then the excerpt, on one
/// 12px line.
pub(crate) fn paint_excerpt(ui: &Ui, rect: Rect, found: &Match) {
    let small = font(12.0, Weight::Regular);
    let (label, color) = if found.user {
        ("You:", theme::BLUE_500)
    } else {
        ("Agent:", Color32::from_rgb(0, 188, 125))
    };
    let width = widgets::text_in(ui, rect, label, small.clone(), color);
    let snippet: String = found
        .snippet
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    widgets::text_in(
        ui,
        Rect::from_min_max(Pos2::new(rect.left() + width + 4.0, rect.top()), rect.max),
        &snippet,
        small,
        theme::alpha(theme::muted_foreground(), 0.85),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_keep_each_thread_best_message() {
        let value = json!({"matches": [
            {"threadId": "a", "source": "assistant", "snippet": "the parser"},
            {"threadId": "a", "source": "user", "snippet": "later"},
            {"threadId": "b", "source": "title", "snippet": "x"},
            {"threadId": "c", "source": "user", "snippet": "fix the parser"},
        ]});
        let found = matches(Some(&value));
        assert_eq!(found.len(), 2);
        assert!(!found["a"].user);
        assert_eq!(found["c"].snippet, "fix the parser");
    }
}
