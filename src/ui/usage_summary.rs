//! Usage Cost and Tokens views (`UsagePage.tsx`, `UsageProviderChart.tsx`,
//! `usageProviders.ts`, `usageBreakdown.ts`, `shared/usageMerge.ts`,
//! `shared/usageFormat.ts`).
//!
//! The connected environment's `server.getUsageSummary` is merged like
//! `mergeUsage` and shown as the provider summary, the daily (or hourly)
//! chart with its hover readout, the totals and the model/period breakdown.
//! Not ported: the environment filter (one environment), the Cursor Keychain
//! rows, ChatGPT shared usage, price overrides and the zone name on repeated
//! daylight-saving hours.
use std::collections::HashMap;

use chrono::{DateTime, Local, NaiveDate, Timelike, Utc};
use eframe::egui::{
    self, Color32, CornerRadius, Mesh, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, Ui, Vec2,
};
use serde_json::{Value, json};

use crate::app::App;
use crate::theme::{self, Weight, font};
use crate::ui::usage::segmented;
use crate::ui::widgets;

const CONTRACT_VERSION: i64 = 6;
const COMPATIBLE_SINCE: i64 = 4;
const COLUMN: f32 = 976.0;
const SUMMARY_WIDTH: f32 = 288.0; // 18rem
const VIEW_WIDTH: f32 = 960.0;
const VIEW_HEIGHT: f32 = 260.0;
const PLOT_TOP: f32 = 8.0;
const PLOT_HEIGHT: f32 = 224.0; // h-56
const TICK_COUNT: f64 = 4.0;

/// `PROVIDER_PRESENTATION`, in its reading order: kind, label, color
/// (`PLACEHOLDER` for Codex's `--contrast-foreground`).
const PROVIDERS: [(&str, &str, Color32); 6] = [
    ("codex", "Codex", Color32::PLACEHOLDER),
    ("claude", "Claude Code", Color32::from_rgb(0xd9, 0x77, 0x57)),
    // `color-mix(in oklab, var(--contrast-foreground) 72%, var(--background))`.
    ("grok", "Grok Build", Color32::from_rgb(179, 179, 179)),
    ("cursor", "Cursor", Color32::from_rgb(0x8b, 0x8b, 0x8b)),
    ("opencode", "OpenCode", Color32::from_rgb(0x5b, 0x9b, 0xbd)),
    (
        "antigravity",
        "Antigravity",
        Color32::from_rgb(0x8c, 0x7b, 0xd1),
    ),
];

fn presentation(provider: &str) -> (&'static str, Color32) {
    PROVIDERS
        .iter()
        .find(|p| p.0 == provider)
        .map(|p| {
            let color = if p.2 == Color32::PLACEHOLDER {
                theme::foreground()
            } else {
                p.2
            };
            (p.1, color)
        })
        .unwrap_or(("Unknown", theme::muted_foreground()))
}

/// The provider's mark (`ProviderMark`), centered in a `size` square.
/// `current` is the text color the `fill-current` marks inherit.
fn paint_mark(ui: &Ui, provider: &str, min: Pos2, size: f32, current: Color32) {
    let rect = Rect::from_min_size(min, Vec2::splat(size));
    match provider {
        "codex" => theme::paint_icon(ui, "openai", rect, size, current),
        "claude" => theme::paint_icon(ui, "claude", rect, size, presentation("claude").1),
        // A 32x40 glyph fitted into the square.
        "opencode" => theme::tinted_icon("opencode", size, Color32::WHITE).paint_at(
            ui,
            Rect::from_center_size(rect.center(), Vec2::new(size * 0.8, size)),
        ),
        _ => {}
    }
}

/// `makeWindow`: the last `days` calendar days in the local zone, or the
/// rolling 24 hours in hourly buckets.
pub(crate) fn make_window(days: u32, now: DateTime<Utc>) -> Value {
    let time_zone = iana_time_zone::get_timezone().unwrap_or_else(|_| "UTC".into());
    let local_day = |t: DateTime<Utc>| t.with_timezone(&Local).date_naive();
    let day = |d: NaiveDate| d.format("%Y-%m-%d").to_string();
    if days == 1 {
        let until = now
            .with_second(0)
            .and_then(|t| t.with_nanosecond(0))
            .unwrap_or(now);
        let since = until - chrono::Duration::hours(24);
        json!({
            "sinceDay": day(local_day(since)),
            "untilDay": day(local_day(until)),
            "timeZone": time_zone,
            "resolution": "hour",
            "sinceTime": iso(since),
            "untilTime": iso(until),
        })
    } else {
        let until = local_day(now);
        let since = until - chrono::Duration::days(i64::from(days) - 1);
        json!({
            "sinceDay": day(since),
            "untilDay": day(until),
            "timeZone": time_zone,
            "resolution": "day",
        })
    }
}

/// `Date.prototype.toISOString`.
fn iso(t: DateTime<Utc>) -> String {
    t.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
}

pub(crate) fn summary_key(window: &Value) -> String {
    format!("usage-summary:{window}")
}

#[derive(Clone, Copy, Default)]
struct Totals {
    cost: f64,
    tokens: f64,
}

struct ProviderTotals {
    provider: String,
    cost: f64,
    tokens: f64,
    sessions: f64,
    cost_share: f64,
    token_share: f64,
}

struct ModelTotals {
    model: String,
    provider: String,
    cost: f64,
    tokens: f64,
    records: f64,
    unpriced: f64,
    cost_share: f64,
}

impl ModelTotals {
    /// `isModelCostUnknown`: every record lacked rates.
    fn cost_unknown(&self) -> bool {
        self.records > 0.0 && self.unpriced >= self.records
    }
}

#[derive(Default)]
struct Period {
    key: String,
    cost: f64,
    tokens: f64,
    by_provider: HashMap<String, Totals>,
}

#[derive(Default)]
struct Merged {
    cost: f64,
    uncached: f64,
    cached: f64,
    output: f64,
    tokens: f64,
    sessions: f64,
    unpriced_share: f64,
    cache_savings: f64,
    providers: Vec<ProviderTotals>,
    models: Vec<ModelTotals>,
    daily: Vec<Period>,
    hourly: Vec<Period>,
}

impl Merged {
    /// `providersWithUsage`: active providers in presentation order.
    fn active_providers(&self) -> Vec<&'static str> {
        PROVIDERS
            .iter()
            .map(|p| p.0)
            .filter(|kind| {
                self.providers
                    .iter()
                    .any(|p| p.provider == *kind && (p.tokens > 0.0 || p.cost > 0.0))
            })
            .collect()
    }
}

/// `mergeUsage` over one environment: sources it scanned (not missing) own
/// their buckets; distinct sessions come from the sources, not the buckets.
fn merge(summary: &Value) -> Merged {
    let version = summary["contractVersion"].as_i64().unwrap_or(0);
    if !(COMPATIBLE_SINCE..=CONTRACT_VERSION).contains(&version) {
        return Merged::default();
    }
    let num = |v: &Value| v.as_f64().unwrap_or(0.0);
    let text = |v: &Value| v.as_str().unwrap_or_default().to_owned();
    let mut owned_providers: Vec<String> = Vec::new();
    let mut owned_sources: Vec<(String, String)> = Vec::new();
    let mut sessions_by_provider: Vec<(String, f64)> = Vec::new();
    for source in summary["sources"].as_array().into_iter().flatten() {
        if source["status"] == "missing" {
            continue;
        }
        let provider = text(&source["fingerprint"]["provider"]);
        owned_sources.push((
            provider.clone(),
            text(&source["fingerprint"]["resolvedHomePath"]),
        ));
        if !owned_providers.contains(&provider) {
            owned_providers.push(provider.clone());
        }
        let sessions = num(&source["distinctSessions"]);
        match sessions_by_provider
            .iter_mut()
            .find(|(p, _)| *p == provider)
        {
            Some((_, total)) => *total += sessions,
            None => sessions_by_provider.push((provider, sessions)),
        }
    }

    let mut merged = Merged::default();
    let mut provider_acc: Vec<(String, f64, f64, f64)> = Vec::new(); // cost, tokens, sessions
    for (provider, sessions) in &sessions_by_provider {
        merged.sessions += sessions;
        if *sessions > 0.0 {
            provider_acc.push((provider.clone(), 0.0, 0.0, *sessions));
        }
    }
    let mut models: Vec<ModelTotals> = Vec::new();
    let mut daily: HashMap<String, Period> = HashMap::new();
    let mut hourly: HashMap<String, Period> = HashMap::new();
    let (mut records, mut unpriced) = (0.0, 0.0);
    for bucket in summary["buckets"].as_array().into_iter().flatten() {
        let provider = text(&bucket["provider"]);
        let owned = match bucket["sourcePath"].as_str() {
            None => owned_providers.contains(&provider),
            Some(path) => owned_sources
                .iter()
                .any(|(p, s)| *p == provider && s == path),
        };
        if !owned {
            continue;
        }
        let totals = &bucket["totals"];
        // Reasoning tokens are a subset of output and are not added again.
        let tokens = num(&totals["uncachedInputTokens"])
            + num(&totals["cachedInputTokens"])
            + num(&totals["cacheCreationTokens"])
            + num(&totals["outputTokens"]);
        let cost = num(&bucket["costUsd"]);
        merged.cost += cost;
        merged.cache_savings += num(&bucket["cacheSavingsUsd"]);
        merged.uncached += num(&totals["uncachedInputTokens"]);
        merged.cached += num(&totals["cachedInputTokens"]);
        merged.output += num(&totals["outputTokens"]);
        merged.tokens += tokens;
        records += num(&bucket["records"]);
        unpriced += num(&bucket["unpricedRecords"]);

        match provider_acc.iter_mut().find(|p| p.0 == provider) {
            Some(entry) => {
                entry.1 += cost;
                entry.2 += tokens;
            }
            None => provider_acc.push((provider.clone(), cost, tokens, 0.0)),
        }
        let model = text(&bucket["model"]);
        match models
            .iter_mut()
            .find(|m| m.provider == provider && m.model == model)
        {
            Some(entry) => {
                entry.cost += cost;
                entry.tokens += tokens;
                entry.records += num(&bucket["records"]);
                entry.unpriced += num(&bucket["unpricedRecords"]);
            }
            None => models.push(ModelTotals {
                model,
                provider: provider.clone(),
                cost,
                tokens,
                records: num(&bucket["records"]),
                unpriced: num(&bucket["unpricedRecords"]),
                cost_share: 0.0,
            }),
        }
        let add = |map: &mut HashMap<String, Period>, key: String| {
            let period = map.entry(key.clone()).or_insert_with(|| Period {
                key,
                ..Default::default()
            });
            period.cost += cost;
            period.tokens += tokens;
            let entry = period.by_provider.entry(provider.clone()).or_default();
            entry.cost += cost;
            entry.tokens += tokens;
        };
        add(&mut daily, text(&bucket["day"]));
        if let Some(hour) = bucket["hourStart"].as_str() {
            add(&mut hourly, hour.to_owned());
        }
    }

    let (cost, tokens) = (merged.cost, merged.tokens);
    let share = |part: f64, whole: f64| if whole == 0.0 { 0.0 } else { part / whole };
    merged.providers = provider_acc
        .into_iter()
        .map(|(provider, c, t, sessions)| ProviderTotals {
            provider,
            cost: c,
            tokens: t,
            sessions,
            cost_share: share(c, cost),
            token_share: share(t, tokens),
        })
        .collect();
    merged.providers.sort_by(|a, b| b.cost.total_cmp(&a.cost));
    for model in &mut models {
        model.cost_share = share(model.cost, cost);
    }
    models.sort_by(|a, b| {
        b.cost
            .total_cmp(&a.cost)
            .then(b.tokens.total_cmp(&a.tokens))
    });
    merged.models = models;
    merged.daily = daily.into_values().collect();
    merged.daily.sort_by(|a, b| a.key.cmp(&b.key));
    merged.hourly = hourly.into_values().collect();
    merged.hourly.sort_by(|a, b| a.key.cmp(&b.key));
    merged.unpriced_share = share(unpriced, records);
    merged
}

fn group_thousands(digits: &str) -> String {
    let mut out = String::new();
    for (index, c) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// `formatUsd`: `$6,191.56`.
fn format_usd(value: f64) -> String {
    let fixed = format!("{:.2}", value.abs());
    let (int, frac) = fixed.split_once('.').unwrap_or((&fixed, "00"));
    let sign = if value < 0.0 && fixed != "0.00" {
        "-"
    } else {
        ""
    };
    format!("{sign}${}.{frac}", group_thousands(int))
}

/// `formatCount`: `1,234`.
fn format_count(value: f64) -> String {
    let rounded = value.round();
    let digits = format!("{}", rounded.abs() as u64);
    let sign = if rounded < 0.0 { "-" } else { "" };
    format!("{sign}{}", group_thousands(&digits))
}

/// `formatTokens`: three significant figures with a unit (`19.9B`, `804K`).
fn format_tokens(value: f64) -> String {
    let trim = |v: f64| {
        let digits = if v.abs() >= 100.0 {
            0
        } else if v.abs() >= 10.0 {
            1
        } else {
            2
        };
        let fixed = format!("{v:.digits$}");
        match fixed.split_once('.') {
            Some((int, frac)) if frac.chars().all(|c| c == '0') => int.to_owned(),
            _ => fixed,
        }
    };
    let abs = value.abs();
    if abs >= 1e12 {
        format!("{}T", trim(value / 1e12))
    } else if abs >= 1e9 {
        format!("{}B", trim(value / 1e9))
    } else if abs >= 1e6 {
        format!("{}M", trim(value / 1e6))
    } else if abs >= 1e3 {
        format!("{}K", trim(value / 1e3))
    } else {
        format_count(value)
    }
}

/// `formatPercent` with one digit: `0.4%`, `<0.1%`.
fn format_percent(share: f64) -> String {
    let percent = share * 100.0;
    if percent > 0.0 && percent < 0.1 {
        "<0.1%".into()
    } else {
        format!("{percent:.1}%")
    }
}

/// `formatDayShort`: `2026-08-07` to `Aug 7`.
fn format_day_short(day: &str) -> String {
    NaiveDate::parse_from_str(day, "%Y-%m-%d")
        .map(|d| d.format("%b %-d").to_string())
        .unwrap_or_else(|_| day.to_owned())
}

fn local_time(instant: &str) -> Option<DateTime<Local>> {
    crate::timeline::parse_time(instant).map(|t| t.with_timezone(&Local))
}

/// `formatHourShort`: `2 PM` in the local zone.
fn format_hour_short(instant: &str) -> String {
    local_time(instant)
        .map(|t| t.format("%-I %p").to_string())
        .unwrap_or_else(|| instant.to_owned())
}

/// `formatRelativeHourShort`: `2 PM today`, `2 PM yesterday` or `Aug 11, 2 PM`.
fn format_relative_hour(instant: &str, reference: &str) -> String {
    let (Some(at), Some(reference)) = (local_time(instant), local_time(reference)) else {
        return instant.to_owned();
    };
    let hour = at.format("%-I %p");
    match (reference.date_naive() - at.date_naive()).num_days() {
        0 => format!("{hour} today"),
        1 => format!("{hour} yesterday"),
        _ => at.format("%b %-d, %-I %p").to_string(),
    }
}

/// `niceScale`: a 1/2/5 x 10^n step whose top is at or above the peak.
fn nice_scale(peak: f64, count: f64) -> (f64, Vec<f64>) {
    if peak <= 0.0 {
        return (0.0, vec![0.0]);
    }
    let raw = peak / count;
    let magnitude = 10f64.powf(raw.log10().floor());
    let normalized = raw / magnitude;
    let step = if normalized > 5.0 {
        10.0
    } else if normalized > 2.0 {
        5.0
    } else if normalized > 1.0 {
        2.0
    } else {
        1.0
    } * magnitude;
    let max = (peak / step).ceil() * step;
    let mut ticks = Vec::new();
    let mut value = 0.0;
    while value <= max + step * 1e-6 {
        ticks.push(value);
        value += step;
    }
    (max, ticks)
}

/// `monotoneTangents`: cubic tangents that cannot overshoot spiky data.
fn monotone_tangents(points: &[Pos2]) -> Vec<f32> {
    let count = points.len();
    if count < 2 {
        return vec![0.0];
    }
    let slopes: Vec<f32> = points
        .windows(2)
        .map(|w| {
            let dx = w[1].x - w[0].x;
            if dx == 0.0 {
                0.0
            } else {
                (w[1].y - w[0].y) / dx
            }
        })
        .collect();
    let mut tangents = vec![0.0; count];
    tangents[0] = slopes[0];
    tangents[count - 1] = slopes[count - 2];
    for index in 1..count - 1 {
        let (previous, next) = (slopes[index - 1], slopes[index]);
        tangents[index] = if previous * next <= 0.0 {
            0.0
        } else {
            (previous + next) / 2.0
        };
    }
    for index in 0..count - 1 {
        let slope = slopes[index];
        if slope == 0.0 {
            tangents[index] = 0.0;
            tangents[index + 1] = 0.0;
            continue;
        }
        let a = tangents[index] / slope;
        let b = tangents[index + 1] / slope;
        let magnitude = a * a + b * b;
        if magnitude > 9.0 {
            let scale = 3.0 / magnitude.sqrt();
            tangents[index] = scale * a * slope;
            tangents[index + 1] = scale * b * slope;
        }
    }
    tangents
}

/// The `smoothCurve` path through `points` (view units), flattened to about
/// one point per screen pixel by `to_screen`.
fn smooth_curve(points: &[Pos2], to_screen: impl Fn(Pos2) -> Pos2) -> Vec<Pos2> {
    if points.len() < 2 {
        return Vec::new();
    }
    let tangents = monotone_tangents(points);
    let mut out = vec![to_screen(points[0])];
    for index in 0..points.len() - 1 {
        let (from, to) = (points[index], points[index + 1]);
        let dx = to.x - from.x;
        let (y0, y3) = (from.y, to.y);
        let y1 = from.y + tangents[index] * dx / 3.0;
        let y2 = to.y - tangents[index + 1] * dx / 3.0;
        // The control points sit at thirds of dx, so x is linear in t.
        let width = (to_screen(to).x - to_screen(from).x).abs();
        let steps = width.ceil().max(1.0) as usize;
        for step in 1..=steps {
            let t = step as f32 / steps as f32;
            let u = 1.0 - t;
            let y = u * u * u * y0 + 3.0 * u * u * t * y1 + 3.0 * u * t * t * y2 + t * t * t * y3;
            out.push(to_screen(Pos2::new(from.x + dx * t, y)));
        }
    }
    out
}

/// The chart's periods: the window's days, or its hourly bucket starts.
fn window_periods(window: &Value) -> Vec<String> {
    if let (Some(since), Some(until)) = (
        window["sinceTime"]
            .as_str()
            .and_then(crate::timeline::parse_time),
        window["untilTime"]
            .as_str()
            .and_then(crate::timeline::parse_time),
    ) {
        let mut hours = Vec::new();
        let mut cursor = since;
        while cursor < until {
            hours.push(iso(cursor));
            cursor += chrono::Duration::hours(1);
        }
        return hours;
    }
    let parse = |key: &str| {
        window[key]
            .as_str()
            .and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
    };
    let (Some(since), Some(until)) = (parse("sinceDay"), parse("untilDay")) else {
        return Vec::new();
    };
    since
        .iter_days()
        .take_while(|d| *d <= until)
        .map(|d| d.format("%Y-%m-%d").to_string())
        .collect()
}

fn skeleton(ui: &Ui, rect: Rect, radius: u8) {
    ui.painter().rect_filled(
        rect,
        CornerRadius::same(radius),
        theme::alpha(theme::muted_foreground(), 0.15),
    );
}

fn h2(ui: &Ui, pos: Pos2, text: &str) {
    widgets::text(
        ui,
        pos,
        text,
        font(14.0, Weight::Medium),
        theme::foreground(),
        20.0,
    );
}

/// Right-aligned `tabular-nums` text ending at `right`.
fn tabular_right(ui: &Ui, right: f32, top: f32, text: &str, size: f32, color: Color32) {
    let font = font(size, Weight::Regular);
    let width = widgets::tabular_width(ui, text, font.clone());
    widgets::tabular_text(
        ui,
        Pos2::new(right - width, top),
        text,
        font,
        color,
        widgets::line_height_for(size),
    );
}

impl App {
    /// The window the page reads, made on first use and on period changes.
    fn usage_window(&mut self) -> Value {
        let days = crate::ui::usage::PERIODS[self.usage.period].1;
        self.usage
            .window
            .get_or_insert_with(|| make_window(days, Utc::now()))
            .clone()
    }

    /// Refresh: re-anchor the window, refresh pricing, then rescan.
    pub(crate) fn refresh_usage_summary(&mut self) {
        let days = crate::ui::usage::PERIODS[self.usage.period].1;
        let window = make_window(days, Utc::now());
        let key = summary_key(&window);
        self.usage.window = Some(window);
        self.usage.requested.insert(key.clone());
        self.send_request(
            format!("usage-rates:{key}"),
            "server.refreshUsageRates",
            json!({}),
        );
    }

    /// The Cost or Tokens view in `body`.
    pub(crate) fn usage_summary(&mut self, ui: &mut Ui, body: Rect) {
        let window = self.usage_window();
        let key = summary_key(&window);
        if !self.usage.requested.contains(&key) {
            self.usage.requested.insert(key.clone());
            self.send_request(key.clone(), "server.getUsageSummary", window.clone());
        }
        let tokens = self.usage.metric == 1;
        let hourly = window["resolution"] == "hour";
        let summary = self.usage.summaries.get(&key).map(|r| r.as_ref().ok());
        let merged = summary.map(|s| s.map(merge).unwrap_or_default());
        let messages: Vec<String> = summary
            .flatten()
            .and_then(|s| s["sources"].as_array())
            .into_iter()
            .flatten()
            .filter(|s| {
                s["action"].is_null()
                    && (s["status"] == "partial"
                        || s["status"] == "failed"
                        || s["fingerprint"]["provider"] == "cursor")
            })
            .filter_map(|s| s["message"].as_str().map(str::to_owned))
            .fold(Vec::new(), |mut all, m| {
                if !all.contains(&m) {
                    all.push(m);
                }
                all
            });
        let periods = window_periods(&window);
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(body));
        egui::ScrollArea::vertical()
            .id_salt("usage-summary")
            .auto_shrink([false, false])
            .show(&mut child, |ui| {
                let left = body.center().x - COLUMN / 2.0;
                let top = ui.max_rect().top();
                let bottom = match &merged {
                    None => self.usage_skeleton(ui, left, top + 24.0),
                    Some(merged) => {
                        let mut y = top + 24.0;
                        for message in &messages {
                            widgets::text(
                                ui,
                                Pos2::new(left, y),
                                message,
                                font(14.0, Weight::Regular),
                                theme::muted_foreground(),
                                20.0,
                            );
                            y += 36.0;
                        }
                        let hover_reference = window["untilTime"].as_str().map(str::to_owned);
                        self.usage_report(
                            ui,
                            left,
                            y,
                            merged,
                            &periods,
                            tokens,
                            hourly,
                            hover_reference.as_deref(),
                        )
                    }
                };
                ui.allocate_rect(
                    Rect::from_min_max(
                        Pos2::new(left, top),
                        Pos2::new(left + COLUMN, bottom + 48.0),
                    ),
                    Sense::hover(),
                );
            });
    }

    /// The loaded report from `top`; returns its bottom edge.
    #[allow(clippy::too_many_arguments)]
    fn usage_report(
        &mut self,
        ui: &mut Ui,
        left: f32,
        top: f32,
        merged: &Merged,
        periods: &[String],
        tokens: bool,
        hourly: bool,
        reference: Option<&str>,
    ) -> f32 {
        let providers = merged.active_providers();
        // Summary column: the total, its sessions, one row per provider.
        let total = if tokens {
            format_tokens(merged.tokens)
        } else {
            format_usd(merged.cost)
        };
        widgets::tabular_text(
            ui,
            Pos2::new(left, top),
            &total,
            font(36.0, Weight::SemiBold),
            theme::foreground(),
            40.0,
        );
        let mut subline = format!("{} sessions", format_count(merged.sessions));
        if !tokens {
            subline.push_str(" · API estimate");
        }
        let small = font(12.0, Weight::Regular);
        widgets::text(
            ui,
            Pos2::new(left, top + 44.0),
            &subline,
            small.clone(),
            theme::muted_foreground(),
            16.0,
        );
        if !tokens && merged.unpriced_share > 0.0 {
            let x = left
                + widgets::text_width(ui, &subline, small.clone())
                + widgets::text_width(ui, " ", small.clone());
            let icon = Rect::from_min_size(Pos2::new(x, top + 44.0), Vec2::splat(12.0));
            theme::paint_icon(ui, "info", icon, 12.0, theme::muted_foreground());
            ui.interact(icon, ui.id().with("usage-unpriced"), Sense::hover())
                .on_hover_text(format!(
                    "API estimate excludes {} unpriced records.",
                    format_percent(merged.unpriced_share)
                ));
        }
        let mut y = top + 80.0;
        for provider in &providers {
            let totals = merged.providers.iter().find(|p| p.provider == *provider);
            let (label, color) = presentation(provider);
            let (cost, token_count, sessions) = totals
                .map(|t| (t.cost, t.tokens, t.sessions))
                .unwrap_or_default();
            ui.painter()
                .circle_filled(Pos2::new(left + 4.0, y + 11.0), 4.0, color);
            paint_mark(
                ui,
                provider,
                Pos2::new(left + 16.0, y + 3.0),
                16.0,
                theme::foreground(),
            );
            let label_font = font(14.0, Weight::Regular);
            widgets::text(
                ui,
                Pos2::new(left + 40.0, y + 1.0),
                label,
                label_font.clone(),
                theme::foreground(),
                20.0,
            );
            let session_label = format!(
                "{} {}",
                format_count(sessions),
                if sessions == 1.0 {
                    "session"
                } else {
                    "sessions"
                }
            );
            widgets::tabular_text(
                ui,
                Pos2::new(
                    left + 46.0 + widgets::text_width(ui, label, label_font),
                    y + 4.0,
                ),
                &session_label,
                font(11.0, Weight::Regular),
                theme::muted_foreground(),
                16.0,
            );
            let value = if tokens {
                format_tokens(token_count)
            } else {
                format_usd(cost)
            };
            let value_font = font(14.0, Weight::Medium);
            let width = widgets::tabular_width(ui, &value, value_font.clone());
            widgets::tabular_text(
                ui,
                Pos2::new(left + SUMMARY_WIDTH - width, y),
                &value,
                value_font,
                theme::foreground(),
                20.0,
            );
            let detail = match (tokens, totals) {
                (false, _) => format!(
                    "{} of cost · {} tokens",
                    format_percent(totals.map_or(0.0, |t| t.cost_share)),
                    format_tokens(token_count)
                ),
                (true, _) => format!(
                    "{} of tokens · {}",
                    format_percent(totals.map_or(0.0, |t| t.token_share)),
                    format_usd(cost)
                ),
            };
            widgets::text(
                ui,
                Pos2::new(left, y + 25.0),
                &detail,
                small.clone(),
                theme::muted_foreground(),
                16.0,
            );
            y += 61.0;
        }
        let summary_bottom = if providers.is_empty() {
            top + 60.0
        } else {
            y - 20.0
        };

        // Chart column.
        let chart_left = left + SUMMARY_WIDTH + 24.0;
        h2(
            ui,
            Pos2::new(chart_left, top),
            &format!(
                "{} {}",
                if hourly { "Hourly" } else { "Daily" },
                if tokens { "processed tokens" } else { "cost" }
            ),
        );
        let chart = Rect::from_min_max(
            Pos2::new(chart_left, top + 32.0),
            Pos2::new(left + COLUMN, top + 32.0 + PLOT_HEIGHT),
        );
        let by_period: HashMap<&str, &Period> = if hourly {
            merged.hourly.iter().map(|p| (p.key.as_str(), p)).collect()
        } else {
            merged.daily.iter().map(|p| (p.key.as_str(), p)).collect()
        };
        paint_chart(
            ui, chart, &providers, periods, &by_period, tokens, hourly, reference,
        );
        let mut y = summary_bottom.max(top + 274.0) + 24.0;

        // Totals.
        h2(ui, Pos2::new(left, y), "Totals");
        let metrics = [
            ("Processed tokens", format_tokens(merged.tokens)),
            ("Cached input", format_tokens(merged.cached)),
            ("Uncached input", format_tokens(merged.uncached)),
            ("Output", format_tokens(merged.output)),
            ("Cache savings", format_usd(merged.cache_savings)),
        ];
        for (index, (label, value)) in metrics.iter().enumerate() {
            let x = left + index as f32 * 200.0;
            widgets::text(
                ui,
                Pos2::new(x, y + 32.0),
                label,
                small.clone(),
                theme::muted_foreground(),
                16.0,
            );
            widgets::tabular_text(
                ui,
                Pos2::new(x, y + 50.0),
                value,
                font(16.0, Weight::Medium),
                theme::foreground(),
                24.0,
            );
        }
        y += 78.0 + 24.0;

        // Breakdown.
        h2(ui, Pos2::new(left, y + 4.0), "Breakdown");
        let (_, clicked) = segmented(
            ui,
            left + COLUMN,
            y,
            &["Model", if hourly { "Hour" } else { "Day" }],
            usize::from(self.usage.by_time),
            false,
            "usage-breakdown",
        );
        if let Some(index) = clicked {
            self.usage.by_time = index == 1;
        }
        let table_top = y + 40.0;
        if self.usage.by_time {
            breakdown_by_period(ui, left, table_top, merged, &providers, hourly)
        } else {
            breakdown_by_model(ui, left, table_top, merged, tokens)
        }
    }

    /// `UsageSkeleton`; returns its bottom edge.
    fn usage_skeleton(&self, ui: &Ui, left: f32, top: f32) -> f32 {
        skeleton(
            ui,
            Rect::from_min_size(Pos2::new(left, top), Vec2::new(144.0, 40.0)),
            4,
        );
        skeleton(
            ui,
            Rect::from_min_size(Pos2::new(left, top + 44.0), Vec2::new(128.0, 16.0)),
            4,
        );
        let mut y = top + 80.0;
        for _ in PROVIDERS {
            skeleton(
                ui,
                Rect::from_min_size(Pos2::new(left, y + 6.0), Vec2::splat(8.0)),
                4,
            );
            skeleton(
                ui,
                Rect::from_min_size(Pos2::new(left + 16.0, y + 2.0), Vec2::splat(16.0)),
                8,
            );
            skeleton(
                ui,
                Rect::from_min_size(Pos2::new(left + 40.0, y + 3.0), Vec2::new(80.0, 14.0)),
                4,
            );
            skeleton(
                ui,
                Rect::from_min_size(
                    Pos2::new(left + SUMMARY_WIDTH - 56.0, y + 3.0),
                    Vec2::new(56.0, 14.0),
                ),
                4,
            );
            skeleton(
                ui,
                Rect::from_min_size(Pos2::new(left, y + 24.0), Vec2::new(144.0, 16.0)),
                4,
            );
            y += 60.0;
        }
        let summary_bottom = y - 20.0;
        let chart_left = left + SUMMARY_WIDTH + 24.0;
        skeleton(
            ui,
            Rect::from_min_size(Pos2::new(chart_left, top), Vec2::new(96.0, 20.0)),
            4,
        );
        skeleton(
            ui,
            Rect::from_min_max(
                Pos2::new(chart_left + 64.0, top + 32.0),
                Pos2::new(left + COLUMN, top + 256.0),
            ),
            4,
        );
        skeleton(
            ui,
            Rect::from_min_max(
                Pos2::new(chart_left + 64.0, top + 260.0),
                Pos2::new(left + COLUMN, top + 276.0),
            ),
            4,
        );
        let mut y = summary_bottom.max(top + 276.0) + 24.0;
        h2(ui, Pos2::new(left, y), "Totals");
        for (index, label) in [
            "Processed tokens",
            "Cached input",
            "Uncached input",
            "Output",
            "Cache savings",
        ]
        .iter()
        .enumerate()
        {
            let x = left + index as f32 * 200.0;
            widgets::text(
                ui,
                Pos2::new(x, y + 32.0),
                label,
                font(12.0, Weight::Regular),
                theme::muted_foreground(),
                16.0,
            );
            skeleton(
                ui,
                Rect::from_min_size(Pos2::new(x, y + 50.0), Vec2::new(64.0, 24.0)),
                4,
            );
        }
        y += 78.0 + 24.0;
        h2(ui, Pos2::new(left, y + 4.0), "Breakdown");
        skeleton(
            ui,
            Rect::from_min_size(Pos2::new(left + COLUMN - 112.0, y), Vec2::new(112.0, 28.0)),
            10,
        );
        skeleton(
            ui,
            Rect::from_min_size(Pos2::new(left, y + 40.0), Vec2::new(COLUMN, 176.0)),
            4,
        );
        y + 40.0 + 176.0
    }
}

/// `UsageProviderChart` in `rect` (axis column, gap and plot).
#[allow(clippy::too_many_arguments)]
fn paint_chart(
    ui: &mut Ui,
    rect: Rect,
    providers: &[&str],
    periods: &[String],
    by_period: &HashMap<&str, &Period>,
    tokens: bool,
    hourly: bool,
    reference: Option<&str>,
) {
    let plot = Rect::from_min_max(Pos2::new(rect.left() + 64.0, rect.top()), rect.max);
    let axis_right = rect.left() + 56.0;
    let format = |v: f64| {
        if tokens {
            format_tokens(v)
        } else {
            format_usd(v)
        }
    };
    let value = |period: &str, provider: &str| {
        by_period
            .get(period)
            .and_then(|p| p.by_provider.get(provider))
            .map_or(0.0, |t| if tokens { t.tokens } else { t.cost })
    };
    let peak = periods
        .iter()
        .flat_map(|p| PROVIDERS.iter().map(move |(kind, _, _)| value(p, kind)))
        .fold(0.0, f64::max);
    let (max, ticks) = nice_scale(peak, TICK_COUNT);
    let step = if periods.len() <= 1 {
        0.0
    } else {
        VIEW_WIDTH / (periods.len() - 1) as f32
    };
    let to_y = |v: f64| {
        if max == 0.0 {
            VIEW_HEIGHT
        } else {
            VIEW_HEIGHT - (v / max) as f32 * (VIEW_HEIGHT - PLOT_TOP)
        }
    };
    let to_screen = |p: Pos2| {
        Pos2::new(
            plot.left() + p.x / VIEW_WIDTH * plot.width(),
            plot.top() + p.y / VIEW_HEIGHT * plot.height(),
        )
    };
    let painter = ui.painter().with_clip_rect(plot.intersect(ui.clip_rect()));
    let tick_font = font(10.0, Weight::Regular);
    for tick in &ticks {
        let y = to_screen(Pos2::new(0.0, to_y(*tick))).y;
        painter.hline(plot.x_range(), y, Stroke::new(1.0, theme::border()));
        let label = if *tick == 0.0 {
            "0".into()
        } else {
            format(*tick)
        };
        tabular_right(
            ui,
            axis_right,
            y - 8.0,
            &label,
            10.0,
            theme::muted_foreground(),
        );
    }
    // Heavier series first so the lighter one is not buried; fills, then lines.
    let mut series: Vec<(&str, f64, Vec<Pos2>)> = providers
        .iter()
        .map(|provider| {
            let points: Vec<Pos2> = periods
                .iter()
                .enumerate()
                .map(|(i, p)| Pos2::new(i as f32 * step, to_y(value(p, provider))))
                .collect();
            let total = periods.iter().map(|p| value(p, provider)).sum::<f64>();
            (*provider, total, smooth_curve(&points, to_screen))
        })
        .collect();
    series.sort_by(|a, b| b.1.total_cmp(&a.1));
    for (provider, _, line) in &series {
        let color = theme::alpha(presentation(provider).1, 0.12);
        let mut mesh = Mesh::default();
        for pair in line.windows(2) {
            let base = mesh.vertices.len() as u32;
            mesh.colored_vertex(pair[0], color);
            mesh.colored_vertex(Pos2::new(pair[0].x, plot.bottom()), color);
            mesh.colored_vertex(pair[1], color);
            mesh.colored_vertex(Pos2::new(pair[1].x, plot.bottom()), color);
            mesh.add_triangle(base, base + 1, base + 2);
            mesh.add_triangle(base + 1, base + 3, base + 2);
        }
        painter.add(Shape::mesh(mesh));
    }
    for (provider, _, line) in &series {
        if line.len() > 1 {
            painter.add(Shape::line(
                line.clone(),
                Stroke::new(2.0, presentation(provider).1),
            ));
        }
    }

    // Period labels: first, middle and last, spaced like `justify-between`.
    if !periods.is_empty() {
        let label = |p: &str| {
            if hourly {
                format_hour_short(p)
            } else {
                format_day_short(p)
            }
            .to_uppercase()
        };
        let labels = [
            label(&periods[0]),
            label(&periods[periods.len() / 2]),
            label(&periods[periods.len() - 1]),
        ];
        let widths: Vec<f32> = labels
            .iter()
            .map(|l| widgets::text_width(ui, l, tick_font.clone()))
            .collect();
        let gap = (plot.width() - widths.iter().sum::<f32>()) / 2.0;
        let mut x = plot.left();
        for (text, width) in labels.iter().zip(&widths) {
            widgets::text(
                ui,
                Pos2::new(x, plot.bottom() + 4.0),
                text,
                tick_font.clone(),
                theme::muted_foreground(),
                14.0,
            );
            x += width + gap;
        }
    }

    // Hover: the period's guide and readout.
    let response = ui.interact(plot, ui.id().with("usage-chart"), Sense::hover());
    let Some(pointer) = response.hover_pos() else {
        return;
    };
    if periods.is_empty() {
        return;
    }
    let local = pointer - plot.min;
    let fraction = (local.x / plot.width()).clamp(0.0, 1.0);
    let index = ((fraction * (periods.len() - 1) as f32).round() as usize).min(periods.len() - 1);
    let x = to_screen(Pos2::new(index as f32 * step, 0.0)).x;
    painter.vline(
        x,
        to_screen(Pos2::new(0.0, PLOT_TOP)).y..=plot.bottom(),
        Stroke::new(1.0, theme::muted_foreground()),
    );
    let period = &periods[index];
    let title = match (hourly, reference) {
        (true, Some(reference)) => format_relative_hour(period, reference),
        (true, None) => format_hour_short(period),
        _ => format_day_short(period),
    };
    let small = font(12.0, Weight::Regular);
    let rows: Vec<(Option<&str>, String, String)> = providers
        .iter()
        .map(|p| {
            (
                Some(*p),
                presentation(p).0.to_owned(),
                format(value(period, p)),
            )
        })
        .collect();
    let total = format(providers.iter().map(|p| value(period, p)).sum());
    let row_width = |label: &str, value: &str, mark: bool| {
        widgets::text_width(ui, label, small.clone())
            + if mark { 18.0 } else { 0.0 }
            + 12.0
            + widgets::tabular_width(ui, value, small.clone())
    };
    let content = rows
        .iter()
        .map(|(_, l, v)| row_width(l, v, true))
        .chain([
            row_width("Total", &total, false),
            widgets::text_width(ui, &title, small.clone()),
        ])
        .fold(0.0, f32::max);
    let size = Vec2::new(
        (content + 22.0).max(144.0).min(plot.width()),
        8.0 + 20.0 + rows.len() as f32 * 16.0 + 25.0 + 8.0,
    );
    let gap = 12.0;
    let preferred = Vec2::new(
        if local.x + gap + size.x <= plot.width() {
            local.x + gap
        } else {
            local.x - gap - size.x
        },
        if local.y + gap + size.y <= plot.height() {
            local.y + gap
        } else {
            local.y - gap - size.y
        },
    );
    let offset = Vec2::new(
        preferred.x.max(0.0).min((plot.width() - size.x).max(0.0)),
        preferred.y.max(0.0).min((plot.height() - size.y).max(0.0)),
    );
    let card = Rect::from_min_size(plot.min + offset, size);
    let painter = ui.painter();
    painter.rect(
        card,
        CornerRadius::same(12),
        // `surface-glass`: 80% background over a backdrop blur, which egui lacks.
        theme::alpha(theme::background(), 0.95),
        Stroke::new(1.0, theme::overlay(0.03)),
        StrokeKind::Inside,
    );
    let inner_left = card.left() + 11.0;
    let inner_right = card.right() - 11.0;
    let mut y = card.top() + 9.0;
    widgets::text(
        ui,
        Pos2::new(inner_left, y),
        &title,
        small.clone(),
        theme::muted_foreground(),
        16.0,
    );
    y += 20.0;
    for (provider, label, value) in &rows {
        let mut x = inner_left;
        if let Some(provider) = provider {
            paint_mark(
                ui,
                provider,
                Pos2::new(x, y + 2.0),
                12.0,
                theme::muted_foreground(),
            );
            x += 18.0;
        }
        widgets::text(
            ui,
            Pos2::new(x, y),
            label,
            small.clone(),
            theme::muted_foreground(),
            16.0,
        );
        tabular_right(ui, inner_right, y, value, 12.0, theme::foreground());
        y += 16.0;
    }
    painter.hline(
        card.left() + 11.0..=card.right() - 11.0,
        y + 4.5,
        Stroke::new(1.0, theme::border()),
    );
    y += 9.0;
    widgets::text(
        ui,
        Pos2::new(inner_left, y),
        "Total",
        small.clone(),
        theme::muted_foreground(),
        16.0,
    );
    tabular_right(ui, inner_right, y, &total, 12.0, theme::foreground());
}

/// One breakdown table: header labels and cells laid out over fractional
/// columns, numbers right-aligned.
struct Table<'a> {
    ui: &'a Ui,
    left: f32,
    /// Right edges of the columns.
    edges: Vec<f32>,
}

impl Table<'_> {
    fn new<'a>(ui: &'a Ui, left: f32, widths: &[f32]) -> Table<'a> {
        let mut edge = left;
        let edges = widths
            .iter()
            .map(|w| {
                edge += w * COLUMN;
                edge
            })
            .collect();
        Table { ui, left, edges }
    }

    fn header(&self, top: f32, labels: &[&str]) -> f32 {
        let small = font(12.0, Weight::Regular);
        for (index, label) in labels.iter().enumerate() {
            let x = if index == 0 {
                self.left
            } else {
                self.edges[index] - widgets::text_width(self.ui, label, small.clone())
            };
            widgets::text(
                self.ui,
                Pos2::new(x, top + 8.0),
                label,
                small.clone(),
                theme::muted_foreground(),
                16.0,
            );
        }
        self.ui.painter().rect_filled(
            Rect::from_min_size(Pos2::new(self.left, top + 32.0), Vec2::new(COLUMN, 1.0)),
            0.0,
            theme::border(),
        );
        top + 33.0
    }

    /// A body row; hovering lights it with `bg-muted/50`. Returns its bottom.
    fn row(&self, top: f32, id: &str, cells: &[(String, Color32)], mark: Option<&str>) -> f32 {
        let rect = Rect::from_min_size(Pos2::new(self.left, top), Vec2::new(COLUMN, 37.0));
        if self
            .ui
            .interact(rect, self.ui.id().with(("usage-row", id)), Sense::hover())
            .hovered()
        {
            self.ui
                .painter()
                .rect_filled(rect, 0.0, theme::alpha(theme::muted(), 0.5));
        }
        for (index, (text, color)) in cells.iter().enumerate() {
            if index == 0 {
                let mut x = self.left;
                if let Some(provider) = mark {
                    paint_mark(
                        self.ui,
                        provider,
                        Pos2::new(x, top + 11.0),
                        14.0,
                        theme::foreground(),
                    );
                    x += 22.0;
                }
                widgets::text_in(
                    self.ui,
                    Rect::from_min_max(
                        Pos2::new(x, top + 8.0),
                        Pos2::new(self.edges[0], top + 28.0),
                    ),
                    text,
                    font(14.0, Weight::Regular),
                    *color,
                );
            } else {
                tabular_right(self.ui, self.edges[index], top + 8.0, text, 14.0, *color);
            }
        }
        self.ui.painter().rect_filled(
            Rect::from_min_size(Pos2::new(self.left, top + 36.0), Vec2::new(COLUMN, 1.0)),
            0.0,
            theme::overlay(0.03),
        );
        top + 37.0
    }

    fn empty(&self, top: f32) -> f32 {
        let text = "No activity in this window.";
        let width = widgets::text_width(self.ui, text, font(14.0, Weight::Regular));
        widgets::text(
            self.ui,
            Pos2::new(self.left + (COLUMN - width) / 2.0, top + 24.0),
            text,
            font(14.0, Weight::Regular),
            theme::muted_foreground(),
            20.0,
        );
        top + 68.0
    }
}

fn breakdown_by_model(ui: &Ui, left: f32, top: f32, merged: &Merged, tokens: bool) -> f32 {
    let table = Table::new(ui, left, &[0.4, 0.2, 0.2, 0.2]);
    let mut y = table.header(top, &["Model", "Cost", "Share", "Tokens"]);
    if merged.models.is_empty() {
        return table.empty(y);
    }
    let mut models: Vec<&ModelTotals> = merged.models.iter().collect();
    if tokens {
        // `sortModelsByTokens`.
        models.sort_by(|a, b| {
            b.tokens
                .total_cmp(&a.tokens)
                .then(b.cost.total_cmp(&a.cost))
        });
    }
    for model in models {
        let unknown = model.cost_unknown();
        let cells = [
            (model.model.clone(), theme::foreground()),
            if unknown {
                ("Unpriced".into(), theme::muted_foreground())
            } else {
                (format_usd(model.cost), theme::foreground())
            },
            (
                if unknown {
                    "—".into()
                } else {
                    format_percent(model.cost_share)
                },
                theme::muted_foreground(),
            ),
            (format_tokens(model.tokens), theme::muted_foreground()),
        ];
        let id = format!("{}:{}", model.provider, model.model);
        y = table.row(y, &id, &cells, Some(&model.provider));
    }
    y
}

fn breakdown_by_period(
    ui: &Ui,
    left: f32,
    top: f32,
    merged: &Merged,
    providers: &[&str],
    hourly: bool,
) -> f32 {
    let value_width = 0.6 / (providers.len() + 2) as f32;
    let mut widths = vec![0.4];
    widths.extend(std::iter::repeat_n(value_width, providers.len() + 2));
    let table = Table::new(ui, left, &widths);
    let mut labels = vec![if hourly { "Hour" } else { "Day" }];
    labels.extend(providers.iter().map(|p| presentation(p).0));
    labels.extend(["Total", "Tokens"]);
    let mut y = table.header(top, &labels);
    let periods = if hourly {
        &merged.hourly
    } else {
        &merged.daily
    };
    if periods.is_empty() {
        return table.empty(y);
    }
    // Newest first.
    for period in periods.iter().rev() {
        let mut cells = vec![(
            if hourly {
                format_hour_short(&period.key)
            } else {
                format_day_short(&period.key)
            },
            theme::foreground(),
        )];
        cells.extend(providers.iter().map(|p| {
            (
                format_usd(period.by_provider.get(*p).map_or(0.0, |t| t.cost)),
                theme::muted_foreground(),
            )
        }));
        cells.push((format_usd(period.cost), theme::foreground()));
        cells.push((format_tokens(period.tokens), theme::muted_foreground()));
        y = table.row(y, &period.key, &cells, None);
    }
    y
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_format_like_intl() {
        assert_eq!(format_usd(6191.555), "$6,191.56");
        assert_eq!(format_usd(0.0), "$0.00");
        assert_eq!(format_tokens(21_300_000_000.0), "21.3B");
        assert_eq!(format_tokens(21_000_000_000.0), "21B");
        assert_eq!(format_tokens(9_970_000.0), "9.97M");
        assert_eq!(format_tokens(679_000.0), "679K");
        assert_eq!(format_tokens(812.0), "812");
        assert_eq!(format_percent(0.0004), "<0.1%");
        assert_eq!(format_percent(0.995), "99.5%");
        assert_eq!(format_count(1234.0), "1,234");
        assert_eq!(format_day_short("2026-09-21"), "Sep 21");
    }

    #[test]
    fn nice_scale_rounds_the_top_up() {
        let (max, ticks) = nice_scale(1260.0, 4.0);
        assert_eq!(max, 1500.0);
        assert_eq!(ticks, vec![0.0, 500.0, 1000.0, 1500.0]);
        assert_eq!(nice_scale(0.0, 4.0), (0.0, vec![0.0]));
    }

    #[test]
    fn merge_skips_missing_sources_and_takes_sessions_from_sources() {
        let summary = json!({
            "contractVersion": 6,
            "sources": [
                {"fingerprint": {"provider": "claude", "resolvedHomePath": "/c"}, "status": "ok", "distinctSessions": 3},
                {"fingerprint": {"provider": "grok", "resolvedHomePath": "/g"}, "status": "missing", "distinctSessions": 0}
            ],
            "buckets": [
                {"day": "2026-10-01", "provider": "claude", "model": "opus", "sourcePath": "/c",
                 "totals": {"uncachedInputTokens": 10, "cachedInputTokens": 80, "cacheCreationTokens": 5, "outputTokens": 5, "reasoningTokens": 3},
                 "costUsd": 3.0, "cacheSavingsUsd": 1.0, "records": 4, "unpricedRecords": 0, "sessions": 2},
                {"day": "2026-10-02", "provider": "claude", "model": "sonnet", "sourcePath": "/c",
                 "totals": {"uncachedInputTokens": 1, "cachedInputTokens": 0, "cacheCreationTokens": 0, "outputTokens": 1, "reasoningTokens": 0},
                 "costUsd": 1.0, "cacheSavingsUsd": 0.0, "records": 2, "unpricedRecords": 2, "sessions": 1},
                {"day": "2026-10-02", "provider": "grok", "model": "g", "sourcePath": "/g",
                 "totals": {"uncachedInputTokens": 9, "cachedInputTokens": 0, "cacheCreationTokens": 0, "outputTokens": 0, "reasoningTokens": 0},
                 "costUsd": 9.0, "cacheSavingsUsd": 0.0, "records": 1, "unpricedRecords": 0, "sessions": 1}
            ]
        });
        let merged = merge(&summary);
        assert_eq!(merged.cost, 4.0);
        assert_eq!(merged.tokens, 102.0);
        assert_eq!(merged.sessions, 3.0);
        assert_eq!(merged.active_providers(), vec!["claude"]);
        assert_eq!(merged.models[0].model, "opus");
        assert!(merged.models[1].cost_unknown());
        assert_eq!(merged.daily.len(), 2);
        assert!((merged.unpriced_share - 2.0 / 6.0).abs() < 1e-9);
    }

    #[test]
    fn daily_windows_enumerate_inclusive_days() {
        let window =
            json!({"sinceDay": "2026-09-29", "untilDay": "2026-10-02", "resolution": "day"});
        assert_eq!(
            window_periods(&window),
            vec!["2026-09-29", "2026-09-30", "2026-10-01", "2026-10-02"]
        );
        let hourly = json!({"sinceTime": "2026-10-19T21:44:00.000Z", "untilTime": "2026-10-19T23:44:00.000Z"});
        assert_eq!(
            window_periods(&hourly),
            vec!["2026-10-19T21:44:00.000Z", "2026-10-19T22:44:00.000Z"]
        );
    }
}
