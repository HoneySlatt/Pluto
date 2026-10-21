//! Usage page (`routes/usage.tsx`, `components/usage/UsagePage.tsx`,
//! `UsageLimitsPooled.tsx`, `shared/usageLimits.ts`).
//!
//! Ported: the header (metric and period groups, refresh) and the Limits view:
//! provider quota windows pooled per driver from the server config, with the
//! remaining share, pace and reset countdown, refreshed through
//! `server.refreshProviders`. The Cost and Tokens views live in
//! `usage_summary.rs`. Not ported: segment popovers, reset credits, usage
//! limit sources (hubs), limit notices and keyboard shortcuts. Like upstream,
//! the limits clock only advances on refresh.
use eframe::egui::{self, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};
use serde_json::Value;

use crate::app::App;
use crate::theme::{self, Weight, font};
use crate::ui::widgets;

const METRICS: [&str; 3] = ["Cost", "Tokens", "Limits"];
pub(crate) const PERIODS: [(&str, u32); 4] = [
    ("Past 24h", 1),
    ("7 days", 7),
    ("30 days", 30),
    ("90 days", 90),
];
const COLUMN: f32 = 976.0;
const CARD_HEIGHT: f32 = 94.0;
const LABEL_COLUMN: f32 = 176.0; // 11rem
const CLAUDE: Color32 = Color32::from_rgb(0xd9, 0x77, 0x57);

/// Usage page state (`usagePagePreferences.ts`; Limits and 30 days by default).
pub struct UsageState {
    pub metric: usize,
    pub period: usize,
    /// The page's clock, advanced on open and refresh.
    pub now: chrono::DateTime<chrono::Utc>,
    /// The Cost/Tokens request window (`makeWindow`), made on first use.
    pub window: Option<Value>,
    /// `server.getUsageSummary` results by request key, and keys in flight.
    pub summaries: std::collections::HashMap<String, Result<Value, String>>,
    pub requested: std::collections::HashSet<String>,
    /// The breakdown table groups by period instead of model.
    pub by_time: bool,
    /// When limits were last refreshed (`refreshUsageLimits` waits 5 minutes
    /// between automatic refreshes).
    pub limits_refreshed: Option<std::time::Instant>,
}

impl Default for UsageState {
    fn default() -> Self {
        Self {
            metric: 2,
            period: 2,
            now: chrono::Utc::now(),
            window: None,
            summaries: Default::default(),
            requested: Default::default(),
            by_time: false,
            limits_refreshed: None,
        }
    }
}

/// One window pooled over a driver's accounts (`LimitPoolWindow`).
struct PoolWindow {
    label: String,
    remaining: i64,
    pace: Option<&'static str>,
    /// Per account column: remaining share and reset label.
    columns: Vec<(i64, Option<String>, bool)>,
}

struct Pool {
    driver: String,
    windows: Vec<PoolWindow>,
}

fn kind_rank(kind: &str) -> u8 {
    match kind {
        "session" => 0,
        "weekly" => 1,
        "monthly" => 2,
        _ => 3,
    }
}

fn millis(iso: Option<&str>) -> Option<i64> {
    iso.and_then(crate::timeline::parse_time)
        .map(|d| d.timestamp_millis())
}

/// `formatDuration`: "2h 13m", "3d 4h", "12m".
fn format_duration(ms: i64) -> String {
    let ms = ms.max(0);
    let (day, hour, minute) = (86_400_000, 3_600_000, 60_000);
    let (days, hours, minutes) = (ms / day, (ms % day) / hour, (ms % hour) / minute);
    if days > 0 {
        format!("{days}d {hours}h")
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else {
        format!("{minutes}m")
    }
}

/// The segment's countdown: "↻ 2h 46m" or "resets now".
fn reset_label(window: &Value, now: i64) -> Option<String> {
    let at = millis(window["resetsAt"].as_str())?;
    Some(if at <= now {
        "resets now".into()
    } else {
        format!("\u{21bb} {}", format_duration(at - now))
    })
}

fn elapsed_share(window: &Value, now: i64) -> Option<f64> {
    let at = millis(window["resetsAt"].as_str())?;
    let length = window["windowDurationMins"].as_f64()? * 60_000.0;
    if length <= 0.0 {
        return None;
    }
    Some(((length - (at - now) as f64) / length).clamp(0.0, 1.0))
}

/// `paceOfShares`: within five points of an even spend is on pace.
fn pace(used: f64, elapsed: f64) -> &'static str {
    let gap = used - elapsed * 100.0;
    if gap > 5.0 {
        "trending-up"
    } else if gap < -5.0 {
        "trending-down"
    } else {
        "gauge"
    }
}

/// `collectLimitPools` over the providers that report windows: accounts
/// grouped by driver, windows pooled by kind and id, ordered by kind.
fn limit_pools(providers: &[Value], now: i64) -> Vec<Pool> {
    let mut pools: Vec<(String, Vec<&Value>)> = Vec::new();
    for provider in providers {
        let limits = &provider["usageLimits"];
        let usable = provider["enabled"] != false
            && provider["installed"] != false
            && limits.is_object()
            && limits["unavailable"].is_null()
            && limits["windows"].as_array().is_some_and(|w| !w.is_empty());
        if !usable {
            continue;
        }
        let driver = provider["driver"].as_str().unwrap_or_default().to_owned();
        match pools.iter_mut().find(|(d, _)| *d == driver) {
            Some((_, accounts)) => accounts.push(limits),
            None => pools.push((driver, vec![limits])),
        }
    }
    pools
        .into_iter()
        .map(|(driver, accounts)| {
            let mut keys: Vec<(String, String, String)> = Vec::new();
            for account in &accounts {
                for window in account["windows"].as_array().into_iter().flatten() {
                    let kind = window["kind"].as_str().unwrap_or("other").to_owned();
                    let id = window["id"].as_str().unwrap_or_default().to_owned();
                    if !keys.iter().any(|(k, i, _)| *k == kind && *i == id) {
                        let label = window["label"].as_str().unwrap_or_default().to_owned();
                        keys.push((kind, id, label));
                    }
                }
            }
            keys.sort_by_key(|(kind, _, _)| kind_rank(kind));
            let windows = keys
                .into_iter()
                .map(|(kind, id, label)| {
                    let members: Vec<Option<&Value>> = accounts
                        .iter()
                        .map(|account| {
                            account["windows"]
                                .as_array()
                                .into_iter()
                                .flatten()
                                .find(|w| {
                                    w["kind"].as_str().unwrap_or("other") == kind
                                        && w["id"] == id.as_str()
                                })
                        })
                        .collect();
                    let present: Vec<&Value> = members.iter().flatten().copied().collect();
                    let used = |w: &Value| w["usedPercent"].as_f64().unwrap_or(0.0);
                    let mean_used =
                        present.iter().map(|w| used(w)).sum::<f64>() / present.len() as f64;
                    let timed: Vec<(f64, f64)> = present
                        .iter()
                        .filter_map(|w| elapsed_share(w, now).map(|e| (used(w), e)))
                        .collect();
                    let pace = (!timed.is_empty()).then(|| {
                        let n = timed.len() as f64;
                        pace(
                            timed.iter().map(|t| t.0).sum::<f64>() / n,
                            timed.iter().map(|t| t.1).sum::<f64>() / n,
                        )
                    });
                    let columns = members
                        .iter()
                        .flatten()
                        .map(|w| {
                            let remaining = (100.0 - used(w).clamp(0.0, 100.0)).round() as i64;
                            (remaining, reset_label(w, now), !w["resetsAt"].is_null())
                        })
                        .collect();
                    PoolWindow {
                        label,
                        remaining: (100.0 - mean_used).round() as i64,
                        pace,
                        columns,
                    }
                })
                .collect();
            Pool { driver, windows }
        })
        .collect()
}

fn driver_label(driver: &str) -> &str {
    match driver {
        "codex" => "Codex",
        "claudeAgent" => "Claude",
        "cursor" => "Cursor",
        "grok" => "Grok",
        "opencode" => "OpenCode",
        "antigravity" => "Antigravity",
        other => other,
    }
}

/// `barColor`: Claude's orange, otherwise the foreground.
fn bar_color(driver: &str) -> Color32 {
    if driver == "claudeAgent" {
        CLAUDE
    } else {
        theme::foreground()
    }
}

impl App {
    pub(crate) fn open_usage(&mut self) {
        self.usage_open = true;
        self.pull_requests_open = false;
        self.usage.now = chrono::Utc::now();
        self.open_menu = None;
        if self.usage.metric == 2 {
            self.refresh_limits(true);
        }
    }

    /// `refreshLimits`: asks the server to re-read provider status and quotas;
    /// automatic refreshes skip a check made in the last five minutes.
    pub(crate) fn refresh_limits(&mut self, automatic: bool) {
        if automatic
            && self
                .usage
                .limits_refreshed
                .is_some_and(|at| at.elapsed() < std::time::Duration::from_secs(300))
        {
            return;
        }
        self.usage.limits_refreshed = Some(std::time::Instant::now());
        self.send_request(
            "usage-limits".into(),
            "server.refreshProviders",
            serde_json::json!({}),
        );
    }

    /// The Usage page in the workspace area.
    pub(crate) fn usage_page(&mut self, ui: &mut Ui, rect: Rect) {
        ui.painter().rect_filled(rect, 0.0, theme::background());
        // Header: "Usage / All environments", the metric and period groups, refresh.
        let mut x = rect.left() + 20.0;
        for (text, weight, color) in [
            ("Usage", Weight::Medium, theme::muted_foreground()),
            ("/", Weight::Regular, theme::muted_foreground()),
            ("All environments", Weight::Medium, theme::foreground()),
        ] {
            let width = widgets::text_width(ui, text, font(14.0, weight));
            widgets::text_in(
                ui,
                Rect::from_min_max(Pos2::new(x, 16.0), Pos2::new(x + width + 1.0, 36.0)),
                text,
                font(14.0, weight),
                color,
            );
            x += width + 12.0;
        }
        let refresh = Rect::from_min_size(Pos2::new(rect.right() - 48.0, 12.0), Vec2::splat(28.0));
        if widgets::icon_button(
            ui,
            refresh,
            "usage-refresh",
            "refresh-cw",
            14.0,
            theme::muted_foreground(),
            "",
        )
        .clicked()
        {
            if self.usage.metric == 2 {
                self.refresh_limits(false);
            } else {
                self.refresh_usage_summary();
            }
        }
        let periods: Vec<&str> = PERIODS.iter().map(|p| p.0).collect();
        let (left, clicked) = segmented(
            ui,
            refresh.left() - 8.0,
            12.0,
            &periods,
            self.usage.period,
            self.usage.metric == 2,
            "usage-period",
        );
        if let Some(index) = clicked {
            self.usage.period = index;
            self.usage.window = None;
        }
        let (_, clicked) = segmented(
            ui,
            left - 8.0,
            12.0,
            &METRICS,
            self.usage.metric,
            false,
            "usage-metric",
        );
        if let Some(index) = clicked {
            self.usage.metric = index;
            if index == 2 {
                self.usage.now = chrono::Utc::now();
                self.refresh_limits(true);
            }
        }

        let body = Rect::from_min_max(Pos2::new(rect.left(), theme::TOPBAR_HEIGHT), rect.max);
        if self.usage.metric != 2 {
            self.usage_summary(ui, body);
            return;
        }
        let now = self.usage.now.timestamp_millis();
        let pools = limit_pools(&self.providers(), now);
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(body));
        egui::ScrollArea::vertical()
            .id_salt("usage-limits")
            .auto_shrink([false, false])
            .show(&mut child, |ui| {
                let left = body.center().x - COLUMN / 2.0;
                let top = ui.max_rect().top();
                let mut y = top + 24.0;
                if pools.is_empty() {
                    widgets::text(
                        ui,
                        Pos2::new(left, y),
                        "No provider on the selected environments reports subscription limits.",
                        font(14.0, Weight::Regular),
                        theme::muted_foreground(),
                        20.0,
                    );
                    y += 20.0;
                }
                for pool in &pools {
                    // Heading: the provider glyph (size-5 slot) and its name.
                    if let Some((icon, tint)) = crate::ui::pickers::provider_icon(&pool.driver) {
                        let tint = if pool.driver == "codex" {
                            theme::alpha(theme::foreground(), 0.8)
                        } else {
                            tint
                        };
                        theme::paint_icon(
                            ui,
                            icon,
                            Rect::from_min_size(Pos2::new(left + 2.0, y + 2.0), Vec2::splat(16.0)),
                            16.0,
                            tint,
                        );
                    }
                    widgets::text_in(
                        ui,
                        Rect::from_min_size(Pos2::new(left + 28.0, y), Vec2::new(COLUMN, 20.0)),
                        driver_label(&pool.driver),
                        font(14.0, Weight::Medium),
                        theme::foreground(),
                    );
                    y += 20.0 + 12.0;
                    for window in &pool.windows {
                        let card =
                            Rect::from_min_size(Pos2::new(left, y), Vec2::new(COLUMN, CARD_HEIGHT));
                        self.pool_window_card(ui, card, window, bar_color(&pool.driver));
                        y += CARD_HEIGHT + 12.0;
                    }
                    y += 32.0 - 12.0;
                }
                ui.allocate_rect(
                    Rect::from_min_max(Pos2::new(left, top), Pos2::new(left + COLUMN, y + 24.0)),
                    Sense::hover(),
                );
            });
    }

    /// `PoolWindowCard`: label, the pooled share left with its pace, and the bar.
    fn pool_window_card(&self, ui: &Ui, card: Rect, window: &PoolWindow, color: Color32) {
        ui.painter().rect(
            card,
            CornerRadius::same(10),
            Color32::TRANSPARENT,
            Stroke::new(1.0, theme::overlay(0.036)),
            StrokeKind::Inside,
        );
        let left = card.left() + 17.0;
        let top = card.top() + 17.0;
        widgets::text_in(
            ui,
            Rect::from_min_size(Pos2::new(left, top), Vec2::new(LABEL_COLUMN, 20.0)),
            &window.label,
            font(14.0, Weight::Medium),
            theme::foreground(),
        );
        let percent = format!("{}%", window.remaining);
        let big = font(30.0, Weight::SemiBold);
        let percent_width = widgets::tabular_text(
            ui,
            Pos2::new(left, top + 24.0),
            &percent,
            big,
            theme::foreground(),
            36.0,
        );
        // "left" shares the big number's baseline (`items-baseline`).
        let left_x = left + percent_width + 8.0;
        widgets::text(
            ui,
            Pos2::new(left_x, top + 37.0),
            "left",
            font(14.0, Weight::Regular),
            theme::muted_foreground(),
            20.0,
        );
        if let Some(icon) = window.pace {
            let x = left_x + widgets::text_width(ui, "left", font(14.0, Weight::Regular)) + 8.0;
            theme::paint_icon(
                ui,
                icon,
                Rect::from_min_size(Pos2::new(x, top + 38.0), Vec2::splat(14.0)),
                14.0,
                theme::muted_foreground(),
            );
        }
        // The bar: one segment per account, 4px apart.
        let bar = Rect::from_min_max(
            Pos2::new(left + LABEL_COLUMN + 24.0, card.center().y - 16.0),
            Pos2::new(card.right() - 17.0, card.center().y + 16.0),
        );
        let count = window.columns.len().max(1) as f32;
        let segment_width = (bar.width() - 4.0 * (count - 1.0)) / count;
        for (index, (remaining, reset, has_reset)) in window.columns.iter().enumerate() {
            let segment = Rect::from_min_size(
                Pos2::new(bar.left() + index as f32 * (segment_width + 4.0), bar.top()),
                Vec2::new(segment_width, bar.height()),
            );
            paint_segment(ui, segment, *remaining, reset.as_deref(), *has_reset, color);
        }
    }
}

/// `PoolSegment` at full width: the remaining share filled at 35%, the spent
/// share hatched at 20% (when it resets), the share and the countdown plate.
fn paint_segment(
    ui: &Ui,
    segment: Rect,
    remaining: i64,
    reset: Option<&str>,
    has_reset: bool,
    color: Color32,
) {
    let painter = ui.painter();
    painter.rect_filled(segment, CornerRadius::same(8), theme::overlay(0.03));
    let fill_width = segment.width() * remaining as f32 / 100.0;
    let fill = Rect::from_min_size(segment.min, Vec2::new(fill_width, segment.height()));
    painter.rect_filled(fill, CornerRadius::same(8), theme::alpha(color, 0.35));
    if remaining < 100 && has_reset {
        // `repeating-linear-gradient(135deg, color 0 1px, transparent 1px 5px)`.
        let hatch = Rect::from_min_max(Pos2::new(fill.right(), segment.top()), segment.max);
        let clipped = painter.with_clip_rect(hatch.intersect(painter.clip_rect()));
        let stroke = Stroke::new(1.0, theme::alpha(color, 0.2));
        let step = 5.0 * std::f32::consts::SQRT_2;
        let height = hatch.height();
        let mut x = hatch.left() - height;
        while x < hatch.right() {
            clipped.line_segment(
                [
                    Pos2::new(x, hatch.bottom()),
                    Pos2::new(x + height, hatch.top()),
                ],
                stroke,
            );
            x += step;
        }
    }
    widgets::tabular_text(
        ui,
        Pos2::new(segment.left() + 8.0, segment.center().y - 8.0),
        &format!("{remaining}%"),
        font(12.0, Weight::SemiBold),
        theme::foreground(),
        16.0,
    );
    if let Some(reset) = reset {
        let small = font(11.0, Weight::Regular);
        let width = widgets::tabular_width(ui, reset, small.clone()) + 12.0;
        let plate = Rect::from_min_size(
            Pos2::new(segment.right() - 8.0 - width, segment.center().y - 10.0),
            Vec2::new(width, 20.0),
        );
        painter.rect_filled(
            plate,
            CornerRadius::same(6),
            theme::alpha(theme::background(), 0.85),
        );
        widgets::tabular_text(
            ui,
            Pos2::new(plate.left() + 6.0, plate.top() + 2.0),
            reset,
            small,
            theme::foreground(),
            16.0,
        );
    }
}

/// A segmented group (`ToggleGroup variant="segmented"`) right-aligned at
/// `right`; returns its left edge and the clicked index. A disabled group
/// fades to 64% and ignores the pointer.
pub(crate) fn segmented(
    ui: &mut Ui,
    right: f32,
    top: f32,
    labels: &[&str],
    active: usize,
    disabled: bool,
    id: &str,
) -> (f32, Option<usize>) {
    let label_font = font(14.0, Weight::Medium);
    let opacity = if disabled { 0.64 } else { 1.0 };
    // Buttons are `px-2.5` with a transparent 1px border.
    let text_widths: Vec<f32> = labels
        .iter()
        .map(|l| widgets::text_width(ui, l, label_font.clone()))
        .collect();
    let total =
        text_widths.iter().map(|w| w + 22.0).sum::<f32>() + 2.0 * (labels.len() - 1) as f32 + 4.0;
    let snap = |a: f32, b: f32| {
        Rect::from_min_max(
            Pos2::new(a.round(), top + 2.0),
            Pos2::new(b.round(), top + 26.0),
        )
    };
    let group = Rect::from_min_max(
        Pos2::new((right - total).round(), top),
        Pos2::new(right, top + 28.0),
    );
    ui.painter()
        .rect_filled(group, CornerRadius::same(10), theme::overlay(0.032));
    let mut x = right - total + 2.0;
    let mut clicked = None;
    for (index, (label, text_width)) in labels.iter().zip(&text_widths).enumerate() {
        let button = snap(x, x + text_width + 22.0);
        let text_x = x + 11.0;
        x += text_width + 24.0;
        let sense = if disabled {
            Sense::hover()
        } else {
            Sense::click()
        };
        let response = ui.interact(button, ui.id().with((id, index)), sense);
        let hovered = !disabled && response.hovered();
        if index == active {
            ui.painter().rect_filled(
                button,
                CornerRadius::same(8),
                theme::alpha(theme::overlay(0.0576), opacity),
            );
        } else if hovered {
            ui.painter()
                .rect_filled(button, CornerRadius::same(8), theme::overlay(0.0256));
        }
        let color = if index == active || hovered {
            theme::foreground()
        } else {
            theme::muted_foreground()
        };
        widgets::text_in(
            ui,
            Rect::from_min_max(Pos2::new(text_x, button.top()), button.max),
            label,
            label_font.clone(),
            theme::alpha(color, opacity),
        );
        if response.clicked() {
            clicked = Some(index);
        }
    }
    (right - total, clicked)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn windows_pool_by_kind_with_pace_and_reset() {
        let now = 1_000_000_000_000i64;
        let iso = |ms: i64| {
            chrono::DateTime::from_timestamp_millis(ms)
                .unwrap()
                .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
        };
        let providers = [json!({
            "driver": "codex", "enabled": true, "installed": true,
            "usageLimits": {"windows": [
                {"id": "secondary", "kind": "weekly", "label": "Weekly", "usedPercent": 10,
                 "resetsAt": iso(now + 3 * 86_400_000), "windowDurationMins": 10080},
                {"id": "primary", "kind": "session", "label": "Session", "usedPercent": 80,
                 "resetsAt": iso(now + 3_600_000), "windowDurationMins": 300}
            ]}
        })];
        let pools = limit_pools(&providers, now);
        let windows = &pools[0].windows;
        assert_eq!(windows[0].label, "Session");
        assert_eq!(windows[0].remaining, 20);
        // 80% spent with 80% of the window gone: on pace.
        assert_eq!(windows[0].pace, Some("gauge"));
        assert_eq!(windows[0].columns[0].1.as_deref(), Some("\u{21bb} 1h 0m"));
        assert_eq!(windows[1].pace, Some("trending-down"));
    }
}
