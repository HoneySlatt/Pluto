//! Context window meter (`chat/ContextWindowMeter.tsx`, `lib/contextWindow.ts`).
//!
//! The ring and the snapshot follow upstream: the newest live provider-turn
//! usage, else the active provider thread's `contextUsage`, else the last
//! compaction item. Pluto replaces upstream's popover with Claude Code's
//! layout: a hover card with the context and the most used quota window, and
//! on click a panel with the context bar, the auto-compact budget, "Compact
//! session" (`/compact`, like upstream's `onCompact`) and the plan usage
//! limits the thread's provider reports (`usageLimits` windows).
//!
//! Not ported: the slot reserved while the thread's detail loads, the cost
//! and total processed rows, and upstream's disabled-compaction reasons.
use eframe::egui::{self, Color32, CornerRadius, Pos2, Rect, Sense, Shape, Stroke, Ui, Vec2};
use serde_json::Value;

use crate::app::App;
use crate::model::Detail;
use crate::theme::{self, Weight};
use crate::ui::widgets;

const MENU: &str = "context-meter";
const PANEL_WIDTH: f32 = 360.0;
/// The panel grows up to this width before its labels truncate.
const PANEL_MAX_WIDTH: f32 = 520.0;
const LINK: &str = "See all \u{2192}";
const COMPACT: &str = "Compact session";
const PAD: f32 = 16.0;

/// `ContextWindowSnapshot`, the fields this meter shows.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Snapshot {
    pub used: f64,
    pub max: Option<f64>,
    pub auto_compact_threshold: Option<f64>,
    pub compacts_automatically: bool,
}

impl Snapshot {
    fn percent(&self) -> Option<f64> {
        self.max
            .filter(|max| *max > 0.0)
            .map(|max| (self.used / max * 100.0).min(100.0))
    }
}

fn number(value: &Value) -> Option<f64> {
    value.as_f64().filter(|n| n.is_finite())
}

/// `deriveLatestContextWindowSnapshot` over the open thread.
pub(crate) fn snapshot(detail: &Detail) -> Option<Snapshot> {
    // `latestProviderTurnTokenUsage`: newest by `updatedAt`, ties to the later entry.
    let live = detail
        .provider_turns
        .iter()
        .map(|turn| &turn["tokenUsage"])
        .filter(|usage| usage.is_object())
        .fold(None::<&Value>, |latest, usage| match latest {
            Some(latest)
                if crate::timeline::parse_time(usage["updatedAt"].as_str().unwrap_or(""))
                    < crate::timeline::parse_time(latest["updatedAt"].as_str().unwrap_or("")) =>
            {
                Some(latest)
            }
            _ => Some(usage),
        });
    if let Some(usage) = live
        && let Some(used) = number(&usage["usedTokens"])
    {
        return Some(Snapshot {
            used: used.max(0.0),
            max: number(&usage["maxTokens"]),
            auto_compact_threshold: None,
            compacts_automatically: true,
        });
    }
    let active = &detail.thread["activeProviderThreadId"];
    if let Some(usage) = detail
        .provider_threads
        .iter()
        .find(|t| !active.is_null() && t["id"] == *active)
        .map(|t| &t["contextUsage"])
        .filter(|u| u.is_object())
        && let Some(used) = number(&usage["usedTokens"])
    {
        return Some(Snapshot {
            used,
            max: number(&usage["maxTokens"]),
            auto_compact_threshold: number(&usage["autoCompactThreshold"]),
            compacts_automatically: usage["compactsAutomatically"] == true,
        });
    }
    detail.visible.iter().rev().find_map(|row| {
        let item = &row.item;
        let used = number(item.get("afterTokenCount")).filter(|n| *n >= 0.0);
        (item.kind == "compaction")
            .then_some(used)
            .flatten()
            .map(|used| Snapshot {
                used,
                max: None,
                auto_compact_threshold: None,
                compacts_automatically: true,
            })
    })
}

/// Claude Code's token counts: "850", "712.5k", "1M", "1.2M".
pub(crate) fn format_tokens(value: f64) -> String {
    let trim = |n: f64, unit: &str| {
        let text = format!("{n:.1}");
        format!("{}{unit}", text.strip_suffix(".0").unwrap_or(&text))
    };
    if value < 1_000.0 {
        format!("{}", value.max(0.0).round())
    } else if value < 999_950.0 {
        trim(value / 1_000.0, "k")
    } else {
        trim(value / 1_000_000.0, "M")
    }
}

/// "712.5k / 1M (71%)", or the bare count without a window size.
fn usage_text(snapshot: &Snapshot) -> String {
    match (snapshot.max, snapshot.percent()) {
        (Some(max), Some(percent)) => format!(
            "{} / {} ({}%)",
            format_tokens(snapshot.used),
            format_tokens(max),
            percent.round()
        ),
        _ => format_tokens(snapshot.used),
    }
}

/// The line under the bar: the room left before auto-compaction.
fn budget_text(snapshot: &Snapshot) -> Option<String> {
    if let Some(threshold) = snapshot.auto_compact_threshold {
        return Some(format!(
            "{} until auto-compact",
            format_tokens((threshold - snapshot.used).max(0.0))
        ));
    }
    if let Some(max) = snapshot.max {
        return Some(format!(
            "{} left",
            format_tokens((max - snapshot.used).max(0.0))
        ));
    }
    snapshot
        .compacts_automatically
        .then(|| "Compacts automatically when needed".to_owned())
}

/// A quota window as the panel lists it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Window {
    pub label: String,
    pub used: f64,
    pub resets_at: Option<String>,
}

/// The provider's usable windows, session first (`kindRank`).
pub(crate) fn windows(provider: &Value) -> Vec<Window> {
    let limits = &provider["usageLimits"];
    if !limits["unavailable"].is_null() {
        return Vec::new();
    }
    let rank = |kind: &str| match kind {
        "session" => 0,
        "weekly" => 1,
        "monthly" => 2,
        _ => 3,
    };
    let mut windows: Vec<(u8, Window)> = limits["windows"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|w| {
            let label = w["label"].as_str()?.trim();
            let label = if label == "Session" {
                "Session limit"
            } else {
                label
            };
            Some((
                rank(w["kind"].as_str().unwrap_or("")),
                Window {
                    label: label.to_owned(),
                    used: number(&w["usedPercent"])?.clamp(0.0, 100.0),
                    resets_at: w["resetsAt"].as_str().map(str::to_owned),
                },
            ))
        })
        .collect();
    windows.sort_by_key(|(rank, _)| *rank);
    windows.into_iter().map(|(_, w)| w).collect()
}

/// "Resets at 5:00 PM", "Resets tomorrow at 9:10 AM", "Resets 11/3 5:00 PM".
fn reset_text(iso: &str, now: chrono::DateTime<chrono::Utc>) -> String {
    let when = crate::ui::usage::upcoming_time(iso, now);
    if when.starts_with(|c: char| c.is_ascii_digit()) && !when.contains('/') {
        format!("Resets at {when}")
    } else {
        format!("Resets {when}")
    }
}

/// The part of a row left of a `right_width` text at its end, with a 12px gap,
/// so long labels truncate instead of running under it.
fn before(line: Rect, right_width: f32) -> Rect {
    let end = if right_width > 0.0 {
        line.right() - right_width - 12.0
    } else {
        line.right()
    };
    Rect::from_min_max(line.min, Pos2::new(end.max(line.left()), line.bottom()))
}

/// Red past 90%, like upstream's ring.
fn level_color(percent: f64, normal: Color32) -> Color32 {
    if percent > 90.0 {
        theme::error_foreground()
    } else {
        normal
    }
}

fn bar(ui: &Ui, rect: Rect, percent: f64, fill: Color32) {
    let radius = CornerRadius::same((rect.height() / 2.0) as u8);
    ui.painter()
        .rect_filled(rect, radius, theme::alpha(theme::muted_foreground(), 0.24));
    if percent > 0.0 {
        let width = (rect.width() * (percent / 100.0) as f32).max(rect.height());
        let filled = Rect::from_min_size(rect.min, Vec2::new(width, rect.height()));
        ui.painter().rect_filled(filled, radius, fill);
    }
}

/// The ring: a 20px circle of radius 9.75/24 with a 3/24 stroke, filled
/// clockwise from the top.
fn ring(ui: &Ui, center: Pos2, percent: f64) {
    let scale = 20.0 / 24.0;
    let radius = 9.75 * scale;
    let width = 3.0 * scale;
    let track = theme::alpha(theme::muted_foreground(), 0.24);
    ui.painter()
        .circle_stroke(center, radius, Stroke::new(width, track));
    if percent <= 0.0 {
        return;
    }
    let color = level_color(percent, theme::alpha(theme::muted_foreground(), 0.72));
    let sweep = std::f32::consts::TAU * (percent / 100.0) as f32;
    let steps = ((sweep / 0.1).ceil() as usize).max(2);
    let point = |angle: f32| {
        let angle = angle - std::f32::consts::FRAC_PI_2;
        center + Vec2::new(angle.cos(), angle.sin()) * radius
    };
    let points: Vec<Pos2> = (0..=steps)
        .map(|i| point(sweep * i as f32 / steps as f32))
        .collect();
    // `strokeLinecap="round"`.
    ui.painter().circle_filled(points[0], width / 2.0, color);
    ui.painter()
        .circle_filled(points[steps], width / 2.0, color);
    ui.painter()
        .add(Shape::line(points, Stroke::new(width, color)));
}

impl App {
    fn meter_snapshot(&self) -> Option<Snapshot> {
        self.state
            .detail
            .as_deref()
            .filter(|d| Some(d.id()) == self.selected.as_deref())
            .and_then(snapshot)
    }

    /// Paints the meter in `rect` (28px) when there is something to show.
    /// Returns whether it was painted.
    pub(crate) fn context_meter(&mut self, ui: &mut Ui, rect: Rect, draft: bool) -> bool {
        if self.client_setting("contextWindowMeterEnabled") != true {
            return false;
        }
        let context = if draft { None } else { self.meter_snapshot() };
        let provider = self.composer_provider(draft).cloned();
        let windows = provider.as_ref().map(windows).unwrap_or_default();
        if context.is_none() && windows.is_empty() {
            return false;
        }
        let percent = context.as_ref().and_then(Snapshot::percent).unwrap_or(0.0);
        let response = ui.interact(rect, ui.id().with(MENU), Sense::click());
        let open = self.open_menu.as_deref() == Some(MENU);
        if response.hovered() || open {
            ui.painter()
                .rect_filled(rect, CornerRadius::same(8), theme::overlay(0.06));
        }
        ring(ui, rect.center(), percent);
        if response.clicked() {
            self.open_menu = (!open).then(|| MENU.to_owned());
            if !open {
                self.refresh_limits(true);
            }
        }
        if self.open_menu.as_deref() == Some(MENU) {
            let plan = provider
                .as_ref()
                .and_then(|p| p["auth"]["label"].as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned);
            self.context_panel(ui, rect, draft, context.as_ref(), &windows, plan);
        } else if response.hovered() {
            hover_card(ui, rect, context.as_ref(), &windows);
        }
        true
    }

    fn context_panel(
        &mut self,
        ui: &mut Ui,
        anchor: Rect,
        draft: bool,
        context: Option<&Snapshot>,
        windows: &[Window],
        plan: Option<String>,
    ) {
        let can_compact = !draft
            && !self.running()
            && self.connected()
            && self
                .provider_list(draft, "slashCommands")
                .iter()
                .any(|c| c["name"] == "compact");
        let budget = context.and_then(budget_text);
        let now = chrono::Utc::now();
        let title = match &plan {
            Some(plan) => format!("Plan usage limits · {plan}"),
            None => "Plan usage limits".to_owned(),
        };
        let resets: Vec<String> = windows
            .iter()
            .map(|window| match &window.resets_at {
                Some(at) => format!("{}  {}%", reset_text(at, now), window.used.round()),
                None => format!("{}%", window.used.round()),
            })
            .collect();
        // Wide enough for each row's label, a 12px gap and its right-hand text.
        let body = theme::font(13.0, Weight::Regular);
        let measure = |text: &str| widgets::text_width(ui, text, body.clone());
        let mut rows: Vec<(f32, f32)> = Vec::new();
        if let Some(context) = context {
            rows.push((measure("Context window"), measure(&usage_text(context))));
            let button = if can_compact {
                widgets::outline_button_width(ui, false, COMPACT)
            } else {
                0.0
            };
            rows.push((budget.as_deref().map_or(0.0, measure), button));
        }
        if !windows.is_empty() {
            rows.push((measure(&title), measure(LINK)));
        }
        for (window, reset) in windows.iter().zip(&resets) {
            rows.push((measure(&window.label), measure(reset)));
        }
        let needed = rows
            .iter()
            .map(|(left, right)| left + 12.0 + right)
            .fold(0.0, f32::max)
            + 2.0 * PAD;
        let screen = ui.ctx().content_rect();
        let panel_width = needed
            .ceil()
            .clamp(PANEL_WIDTH, PANEL_MAX_WIDTH)
            .min(screen.width() - 8.0);
        let mut height = PAD;
        if context.is_some() {
            height += 20.0 + 10.0 + 6.0;
            if budget.is_some() || can_compact {
                height += 10.0 + 24.0;
            }
        }
        if context.is_some() && !windows.is_empty() {
            height += 16.0 + 1.0 + 16.0;
        }
        if !windows.is_empty() {
            height += 20.0 + windows.len() as f32 * (12.0 + 18.0 + 6.0 + 4.0);
        }
        height += PAD;
        let left = (anchor.right() - panel_width).max(screen.left() + 4.0);
        let popup = Rect::from_min_size(
            Pos2::new(left, anchor.top() - 6.0 - height),
            Vec2::new(panel_width, height),
        );
        let mut compact = false;
        let mut see_all = false;
        let mut open = self.open_menu.take();
        widgets::popup(ui, &mut open, MENU, popup, anchor, |ui, rect| {
            let muted = theme::muted_foreground();
            let x = rect.left() + PAD;
            let right = rect.right() - PAD;
            let width = right - x;
            let mut y = rect.top() + PAD;
            let row = |ui: &Ui, y: f32, height: f32, left: &str, right_text: &str, color| {
                let line = Rect::from_min_max(Pos2::new(x, y), Pos2::new(right, y + height));
                let w = widgets::text_width(ui, right_text, body.clone());
                widgets::text_in(ui, before(line, w), left, body.clone(), color);
                widgets::text_in(
                    ui,
                    Rect::from_min_max(Pos2::new(right - w, y), line.max),
                    right_text,
                    body.clone(),
                    muted,
                );
            };
            if let Some(context) = context {
                row(ui, y, 20.0, "Context window", &usage_text(context), muted);
                y += 20.0 + 10.0;
                let percent = context.percent().unwrap_or(0.0);
                bar(
                    ui,
                    Rect::from_min_size(Pos2::new(x, y), Vec2::new(width, 6.0)),
                    percent,
                    level_color(percent, theme::primary()),
                );
                y += 6.0;
                if budget.is_some() || can_compact {
                    y += 10.0;
                    let line = Rect::from_min_max(Pos2::new(x, y), Pos2::new(right, y + 24.0));
                    let w = if can_compact {
                        widgets::outline_button_width(ui, false, COMPACT)
                    } else {
                        0.0
                    };
                    if let Some(budget) = &budget {
                        let room = before(line, w);
                        widgets::text_in(ui, room, budget, body.clone(), theme::foreground());
                    }
                    if can_compact {
                        let button = Rect::from_min_max(Pos2::new(right - w, y), line.max);
                        compact = widgets::outline_button(
                            ui,
                            button,
                            "context-compact",
                            None,
                            COMPACT,
                            CornerRadius::same(6),
                        )
                        .clicked();
                    }
                    y += 24.0;
                }
            }
            if context.is_some() && !windows.is_empty() {
                y += 16.0;
                ui.painter().hline(
                    rect.left()..=rect.right(),
                    y + 0.5,
                    Stroke::new(1.0, theme::border()),
                );
                y += 1.0 + 16.0;
            }
            if !windows.is_empty() {
                let line = Rect::from_min_max(Pos2::new(x, y), Pos2::new(right, y + 20.0));
                let w = widgets::text_width(ui, LINK, body.clone());
                widgets::text_in(ui, before(line, w), &title, body.clone(), muted);
                let link_rect = Rect::from_min_max(Pos2::new(right - w, y), line.max);
                let response =
                    ui.interact(link_rect, ui.id().with("context-see-all"), Sense::click());
                let color = if response.hovered() {
                    theme::foreground()
                } else {
                    theme::alpha(theme::foreground(), 0.85)
                };
                widgets::text_in(ui, link_rect, LINK, body.clone(), color);
                if response.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                see_all = response.clicked();
                y += 20.0;
                for (window, reset) in windows.iter().zip(&resets) {
                    y += 12.0;
                    row(ui, y, 18.0, &window.label, reset, theme::foreground());
                    y += 18.0 + 6.0;
                    bar(
                        ui,
                        Rect::from_min_size(Pos2::new(x, y), Vec2::new(width, 4.0)),
                        window.used,
                        level_color(window.used, theme::foreground()),
                    );
                    y += 4.0;
                }
            }
        });
        self.open_menu = open;
        if compact {
            self.open_menu = None;
            self.compact_context();
        }
        if see_all {
            self.open_menu = None;
            self.usage.metric = 2;
            self.open_usage();
        }
    }

    /// `onCompactContext`: sends `/compact` as its own turn.
    fn compact_context(&mut self) {
        let Some(thread) = self.current_thread() else {
            return;
        };
        let runtime = self.composer_runtime_mode(false);
        let interaction = self.composer_interaction_mode(false);
        let model = self.composer_model_json(false);
        let send = crate::ui::composer::TurnSend {
            thread_id: thread.id.clone(),
            before: self.mode_commands(&thread, &runtime, &interaction),
            message: crate::ui::composer::message_dispatch(&thread.id, "/compact", &model, "auto"),
            launch: false,
            uploads: Vec::new(),
        };
        self.start_turn(send);
    }
}

/// Claude Code's hover card: the context, then the most used quota window.
fn hover_card(ui: &Ui, anchor: Rect, context: Option<&Snapshot>, windows: &[Window]) {
    let mut lines: Vec<(String, Color32)> = Vec::new();
    if let Some(context) = context {
        lines.push((
            format!("Context {}", usage_text(context)),
            theme::foreground(),
        ));
    }
    if let Some(window) = windows.iter().max_by(|a, b| a.used.total_cmp(&b.used)) {
        let now = chrono::Utc::now();
        lines.push((format!("{}:", window.label), theme::muted_foreground()));
        let detail = match &window.resets_at {
            Some(at) => format!("{}% · {}", window.used.round(), reset_text(at, now)),
            None => format!("{}%", window.used.round()),
        };
        lines.push((detail, theme::muted_foreground()));
    }
    let galleys: Vec<_> = lines
        .iter()
        .enumerate()
        .map(|(i, (text, color))| {
            let font = theme::font(if i == 0 { 13.0 } else { 12.0 }, Weight::Regular);
            widgets::galley(ui, text, font, *color, 18.0, f32::INFINITY)
        })
        .collect();
    let width = galleys.iter().map(|g| g.size().x).fold(0.0, f32::max) + 20.0;
    let height = galleys.len() as f32 * 18.0 + 12.0;
    let screen = ui.ctx().content_rect();
    let left = (anchor.right() - width).max(screen.left() + 4.0);
    let card = Rect::from_min_size(
        Pos2::new(left, anchor.top() - 6.0 - height),
        Vec2::new(width, height),
    );
    egui::Area::new(ui.id().with("context-hover"))
        .order(egui::Order::Tooltip)
        .fixed_pos(card.min)
        .interactable(false)
        .show(ui.ctx(), |ui| {
            widgets::popup_surface(ui, card);
            let mut y = card.top() + 6.0;
            for galley in galleys {
                ui.painter().galley(
                    Pos2::new(card.left() + 10.0, y),
                    galley,
                    theme::foreground(),
                );
                y += 18.0;
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn detail(value: Value) -> Detail {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn formats_tokens_like_claude_code() {
        assert_eq!(format_tokens(850.0), "850");
        assert_eq!(format_tokens(712_500.0), "712.5k");
        assert_eq!(format_tokens(250_000.0), "250k");
        assert_eq!(format_tokens(1_000_000.0), "1M");
        assert_eq!(format_tokens(999_960.0), "1M");
        assert_eq!(format_tokens(1_234_000.0), "1.2M");
    }

    #[test]
    fn live_usage_wins_and_newest_report_counts() {
        let d = detail(json!({
            "thread": {"activeProviderThreadId": "pt"},
            "providerTurns": [
                {"tokenUsage": {"usedTokens": 900, "maxTokens": 1000, "updatedAt": "2026-10-27T10:00:00Z"}},
                {"tokenUsage": {"usedTokens": 500, "maxTokens": 1000, "updatedAt": "2026-10-27T09:00:00Z"}},
            ],
            "providerThreads": [{"id": "pt", "contextUsage": {"usedTokens": 1}}],
        }));
        let snapshot = snapshot(&d).unwrap();
        assert_eq!(snapshot.used, 900.0);
        assert_eq!(snapshot.percent(), Some(90.0));
        assert_eq!(usage_text(&snapshot), "900 / 1k (90%)");
    }

    #[test]
    fn falls_back_to_provider_thread_then_compaction() {
        let d = detail(json!({
            "thread": {"activeProviderThreadId": "pt"},
            "providerThreads": [{"id": "pt", "contextUsage": {
                "usedTokens": 712_500, "maxTokens": 1_000_000, "autoCompactThreshold": 962_500,
            }}],
        }));
        let snapshot = snapshot(&d).unwrap();
        assert_eq!(usage_text(&snapshot), "712.5k / 1M (71%)");
        assert_eq!(budget_text(&snapshot).unwrap(), "250k until auto-compact");

        let d = detail(json!({
            "visibleTurnItems": [{"item": {"type": "compaction", "afterTokenCount": 40_000}}],
        }));
        let snapshot = super::snapshot(&d).unwrap();
        assert_eq!(snapshot.used, 40_000.0);
        assert_eq!(snapshot.max, None);
        assert!(super::snapshot(&Detail::default()).is_none());
    }

    #[test]
    fn windows_skip_unavailable_and_sort_session_first() {
        let provider = json!({"usageLimits": {"windows": [
            {"id": "seven_day", "kind": "weekly", "label": "Weekly", "usedPercent": 97},
            {"id": "five_hour", "kind": "session", "label": "Session", "usedPercent": 3},
        ]}});
        let labels: Vec<String> = windows(&provider).into_iter().map(|w| w.label).collect();
        assert_eq!(labels, ["Session limit", "Weekly"]);
        let unavailable = json!({"usageLimits": {
            "windows": [{"kind": "weekly", "label": "Weekly", "usedPercent": 1}],
            "unavailable": {"reason": "unsupported"},
        }});
        assert!(windows(&unavailable).is_empty());
    }
}
