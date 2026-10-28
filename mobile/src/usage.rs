//! Usage (`#usage`, after the desktop's `usage.rs` and `usage_summary.rs`):
//! Cost | Tokens | Limits, Limits first. Limits are the providers' quota
//! windows (`usageLimits` in the server config, refreshed through
//! `server.refreshProviders`), one card per provider with a meter per
//! window, "Low" under 25% left. Cost and Tokens read
//! `server.getUsageSummary` for the period (merged like `mergeUsage`: the
//! sources the server scanned own their buckets), with the total, a daily
//! (or hourly) bar chart whose bars show their value when tapped, and the
//! split by provider.

use std::collections::BTreeMap;

use chrono::{Local, NaiveDate, Timelike, Utc};
use dioxus::prelude::*;
use serde_json::{Value, json};

use crate::catalog;
use crate::icons::Icon;
use crate::nav::Ui;
use crate::pages::PageHead;

const PERIODS: [(&str, u32); 4] = [
    ("Past 24h", 1),
    ("7 days", 7),
    ("30 days", 30),
    ("90 days", 90),
];
const CONTRACT_VERSION: i64 = 6;
const COMPATIBLE_SINCE: i64 = 4;

/// `make_window`.
fn make_window(days: u32) -> Value {
    let now = Utc::now();
    let zone = iana_time_zone::get_timezone().unwrap_or_else(|_| "UTC".into());
    let day = |d: NaiveDate| d.format("%Y-%m-%d").to_string();
    let iso = |t: chrono::DateTime<Utc>| t.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
    if days == 1 {
        let until = now
            .with_second(0)
            .and_then(|t| t.with_nanosecond(0))
            .unwrap_or(now);
        let since = until - chrono::Duration::hours(24);
        json!({"sinceDay": day(since.with_timezone(&Local).date_naive()), "untilDay": day(until.with_timezone(&Local).date_naive()),
            "timeZone": zone, "resolution": "hour", "sinceTime": iso(since), "untilTime": iso(until)})
    } else {
        let until = now.with_timezone(&Local).date_naive();
        let since = until - chrono::Duration::days(i64::from(days) - 1);
        json!({"sinceDay": day(since), "untilDay": day(until), "timeZone": zone, "resolution": "day"})
    }
}

/// The merged summary: totals, per period and per provider.
#[derive(Default, Debug, PartialEq)]
struct Merged {
    cost: f64,
    tokens: f64,
    /// Period key (day or hour start) → (cost, tokens).
    periods: BTreeMap<String, (f64, f64)>,
    /// Provider → (cost, tokens).
    providers: Vec<(String, f64, f64)>,
}

/// `mergeUsage` over one environment.
fn merge(summary: &Value) -> Merged {
    let mut merged = Merged::default();
    let version = summary["contractVersion"].as_i64().unwrap_or(0);
    if !(COMPATIBLE_SINCE..=CONTRACT_VERSION).contains(&version) {
        return merged;
    }
    let num = |v: &Value| v.as_f64().unwrap_or(0.0);
    let text = |v: &Value| v.as_str().unwrap_or_default().to_owned();
    let mut providers: Vec<String> = Vec::new();
    let mut sources: Vec<(String, String)> = Vec::new();
    for source in summary["sources"].as_array().into_iter().flatten() {
        if source["status"] == "missing" {
            continue;
        }
        let provider = text(&source["fingerprint"]["provider"]);
        sources.push((
            provider.clone(),
            text(&source["fingerprint"]["resolvedHomePath"]),
        ));
        if !providers.contains(&provider) {
            providers.push(provider);
        }
    }
    let hourly = summary["buckets"]
        .as_array()
        .is_some_and(|b| b.iter().any(|b| b["hourStart"].is_string()));
    for bucket in summary["buckets"].as_array().into_iter().flatten() {
        let provider = text(&bucket["provider"]);
        let owned = match bucket["sourcePath"].as_str() {
            None => providers.contains(&provider),
            Some(path) => sources.iter().any(|(p, s)| *p == provider && s == path),
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
        merged.tokens += tokens;
        let key = if hourly {
            text(&bucket["hourStart"])
        } else {
            text(&bucket["day"])
        };
        let period = merged.periods.entry(key).or_default();
        period.0 += cost;
        period.1 += tokens;
        match merged.providers.iter_mut().find(|p| p.0 == provider) {
            Some(entry) => {
                entry.1 += cost;
                entry.2 += tokens;
            }
            None => merged.providers.push((provider, cost, tokens)),
        }
    }
    merged
}

/// The chart's bars: one per day (or hour) of the window, empty ones too.
fn series(merged: &Merged, window: &Value, tokens: bool) -> Vec<(String, f64)> {
    let pick = |(cost, count): (f64, f64)| if tokens { count } else { cost };
    if window["resolution"] == "hour" {
        let since = window["sinceTime"]
            .as_str()
            .and_then(pluto_core::timeline::parse_time);
        let Some(since) = since else {
            return Vec::new();
        };
        (1..=24)
            .map(|h| {
                let at = since + chrono::Duration::hours(h);
                let value = merged
                    .periods
                    .iter()
                    .filter(|(k, _)| {
                        pluto_core::timeline::parse_time(k)
                            .is_some_and(|t| t.timestamp() / 3600 == at.timestamp() / 3600 - 1)
                    })
                    .map(|(_, v)| pick(*v))
                    .sum();
                (at.with_timezone(&Local).format("%H:00").to_string(), value)
            })
            .collect()
    } else {
        let parse = |k: &str| {
            NaiveDate::parse_from_str(window[k].as_str().unwrap_or_default(), "%Y-%m-%d").ok()
        };
        let (Some(since), Some(until)) = (parse("sinceDay"), parse("untilDay")) else {
            return Vec::new();
        };
        since
            .iter_days()
            .take_while(|d| *d <= until)
            .map(|d| {
                let key = d.format("%Y-%m-%d").to_string();
                (
                    d.format("%b %-d").to_string(),
                    merged.periods.get(&key).copied().map(pick).unwrap_or(0.0),
                )
            })
            .collect()
    }
}

fn money(v: f64) -> String {
    format!("${v:.2}")
}

fn tok(v: f64) -> String {
    if v >= 1e6 {
        format!("{:.1}M", v / 1e6)
    } else {
        format!("{}k", (v / 1e3).round())
    }
}

/// `formatDuration`.
fn duration(ms: i64) -> String {
    let ms = ms.max(0);
    let (days, hours, minutes) = (
        ms / 86_400_000,
        (ms % 86_400_000) / 3_600_000,
        (ms % 3_600_000) / 60_000,
    );
    if days > 0 {
        format!("{days}d {hours}h")
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else {
        format!("{minutes}m")
    }
}

#[component]
pub fn Usage() -> Element {
    let ui = use_context::<Ui>();
    let mut metric = use_signal(|| 2usize);
    let mut period = use_signal(|| 2usize);
    let mut spinning = use_signal(|| false);
    let window = use_memo(move || make_window(PERIODS[period()].1));
    let key = use_memo(move || format!("usage-summary:{}", window()));
    use_effect(move || {
        let key = key();
        if metric() != 2 && !ui.ctx.session.peek().replies.contains_key(&key) {
            ui.ctx.request(key, "server.getUsageSummary", window());
        }
    });
    let refresh = move |_| {
        spinning.set(true);
        ui.ctx
            .request("usage-limits", "server.refreshProviders", json!({}));
        ui.ctx.session.clone().write().replies.remove(&key());
        if metric() != 2 {
            ui.ctx
                .request("usage-rates", "server.refreshUsageRates", json!({}));
            ui.ctx.request(key(), "server.getUsageSummary", window());
        }
        spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(800)).await;
            spinning.set(false);
        });
    };
    let refresh_class = if spinning() { "spin" } else { "" };
    rsx! {
        section { class: "page",
            PageHead {
                title: "Usage",
                action: rsx! {
                    button { class: "disc", "aria-label": "Refresh", onclick: refresh,
                        Icon { name: "refresh-cw", class: refresh_class }
                    }
                },
            }
            div { class: "pscroll",
                div { class: "seg top", role: "radiogroup", "aria-label": "Metric",
                    for (index, label) in ["Cost", "Tokens", "Limits"].into_iter().enumerate() {
                        button { role: "radio", "aria-checked": "{metric() == index}", onclick: move |_| metric.set(index), "{label}" }
                    }
                }
                if metric() == 2 {
                    Limits {}
                } else {
                    div { class: "periods", role: "group", "aria-label": "Period",
                        for (index, (label, _)) in PERIODS.into_iter().enumerate() {
                            button { "aria-pressed": "{period() == index}", onclick: move |_| period.set(index), "{label}" }
                        }
                    }
                    Summary { tokens: metric() == 1, window: window(), summary_key: key(), label: PERIODS[period()].0 }
                }
            }
        }
    }
}

#[component]
fn Summary(tokens: bool, window: Value, summary_key: String, label: &'static str) -> Element {
    let key = summary_key;
    let ui = use_context::<Ui>();
    let mut tip = use_signal(|| None::<usize>);
    let reply = ui.ctx.session.read().replies.get(&key).cloned();
    let summary = match reply {
        None => return rsx! { div { class: "loading-note", "Loading usage…" } },
        Some(Err(error)) => {
            return rsx! { div { class: "empty-page", b { "Couldn’t load usage" } "{error}" } };
        }
        Some(Ok(summary)) => summary,
    };
    let merged = merge(&summary);
    let bars = series(&merged, &window, tokens);
    let total = if tokens { merged.tokens } else { merged.cost };
    let fmt = move |v: f64| {
        if tokens {
            format!("{} tokens", tok(v))
        } else {
            money(v)
        }
    };
    let hourly = window["resolution"] == "hour";
    let max = bars.iter().map(|b| b.1).fold(0.0, f64::max) * 1.1;
    let (w, h, top) = (334.0, 168.0, 8.0);
    let n = bars.len().max(1) as f64;
    let gap = if n > 40.0 { 1.0 } else { 2.0 };
    let bw = (w - gap * (n - 1.0)) / n;
    let paths: Vec<(usize, String)> = bars
        .iter()
        .enumerate()
        .map(|(i, (_, v))| {
            let bh = if max > 0.0 {
                ((h - top) * v / max).max(2.0)
            } else {
                2.0
            };
            let (x, y, r) = (i as f64 * (bw + gap), h - bh, (bw / 2.0).min(4.0));
            (
                i,
                format!(
                    "M{x},{h}V{}Q{x},{y} {},{y}H{}Q{},{y} {},{}V{h}Z",
                    y + r,
                    x + r,
                    x + bw - r,
                    x + bw,
                    x + bw,
                    y + r
                ),
            )
        })
        .collect();
    let mut providers = merged.providers.clone();
    providers.sort_by(|a, b| {
        let (va, vb) = if tokens { (a.2, b.2) } else { (a.1, b.1) };
        vb.total_cmp(&va)
    });
    let caption = if hourly {
        "past 24 hours".to_owned()
    } else {
        format!("past {label}")
    };
    let tip_text = tip().and_then(|i| bars.get(i).map(|(day, v)| (i, day.clone(), fmt(*v))));
    rsx! {
        div { class: "hero-num",
            b { if tokens { "{tok(total)}" } else { "{money(total)}" } }
            span { if tokens { "Input and output, {caption}" } else { "At model list prices, {caption}" } }
        }
        div { class: "chart",
            if let Some((i, day, value)) = tip_text {
                div { class: "tip on", style: "left:{((i as f64 + 0.5) / n * 100.0).clamp(18.0, 82.0)}%",
                    span { "{day}" } " " b { "{value}" }
                }
            }
            svg { view_box: "0 0 {w} {h}", preserve_aspect_ratio: "none", role: "img",
                line { x1: "0", x2: "{w}", y1: "{h - (h - top) * 0.5}", y2: "{h - (h - top) * 0.5}", stroke: "var(--border)", stroke_width: "1" }
                line { x1: "0", x2: "{w}", y1: "{top}", y2: "{top}", stroke: "var(--border)", stroke_width: "1" }
                line { x1: "0", x2: "{w}", y1: "{h}", y2: "{h}", stroke: "var(--input)" }
                for (i, d) in paths {
                    path {
                        key: "{i}",
                        d: "{d}",
                        fill: "var(--accent)",
                        opacity: if tip() == Some(i) { "1" } else if tip().is_some() { ".45" } else { ".85" },
                        onclick: move |_| tip.set(if tip() == Some(i) { None } else { Some(i) }),
                    }
                }
            }
            div { class: "axis",
                span { if hourly { "24h ago" } else { "{bars.len()}d ago" } }
                span { if hourly { "now" } else { "today" } }
            }
        }
        if !providers.is_empty() {
            div { class: "glabel", style: "padding-top:0", "By provider" }
            div { class: "group brk",
                for (provider, cost, count, share) in providers.into_iter().map(|(p, c, t)| {
                    let share = if total > 0.0 { (if tokens { t } else { c }) / total * 100.0 } else { 0.0 };
                    (p, c, t, share)
                }) {
                    div { key: "{provider}", class: "prow",
                        if let Some(mark) = catalog::mark(&provider) {
                            img { src: mark, alt: "" }
                        }
                        span { class: "grow",
                            b { "{catalog::driver_label(&provider)}" }
                            span { class: "share",
                                i { style: "width:{share:.1}%" }
                            }
                        }
                        span { class: "val", if tokens { "{tok(count)}" } else { "{money(cost)}" } }
                    }
                }
            }
        }
    }
}

/// A provider's limits: driver, name, plan, then label, percent left and
/// reset line per window.
type LimitCard = (String, String, Option<String>, Vec<(String, i64, String)>);

#[component]
fn Limits() -> Element {
    let ui = use_context::<Ui>();
    let providers = ui.ctx.session.read().providers().to_vec();
    let now = Utc::now().timestamp_millis();
    let cards: Vec<LimitCard> = providers
        .iter()
        .filter(|p| p["enabled"] != false && p["installed"] != false && p["status"] == "ready")
        .filter(|p| p["usageLimits"].is_object())
        .map(|p| {
            let driver = p["driver"].as_str().unwrap_or_default().to_owned();
            let name = p["displayName"]
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| catalog::driver_label(&driver));
            let plan = p["auth"]["label"].as_str().map(str::to_owned);
            let windows = p["usageLimits"]["windows"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|w| {
                    let left = (100.0 - w["usedPercent"].as_f64().unwrap_or(0.0).clamp(0.0, 100.0))
                        .round() as i64;
                    let resets = w["resetsAt"]
                        .as_str()
                        .and_then(|at| pluto_core::timeline::parse_time(at).map(|t| (at, t)))
                        .map(|(_, t)| {
                            let local = t.with_timezone(&Local);
                            let when = if local.date_naive() == Local::now().date_naive() {
                                format!("at {}", local.format("%H:%M"))
                            } else {
                                local.format("%a %H:%M").to_string()
                            };
                            format!(
                                "Resets {when} · in {}",
                                duration(t.timestamp_millis() - now)
                            )
                        })
                        .unwrap_or_default();
                    (
                        w["label"].as_str().unwrap_or("Limit").to_owned(),
                        left,
                        resets,
                    )
                })
                .collect();
            (driver, name, plan, windows)
        })
        .collect();
    rsx! {
        if cards.is_empty() {
            div { class: "empty-page", b { "No limits reported" } "Providers report their quota windows after a refresh." }
        }
        for (driver, name, plan, windows) in cards {
            div { key: "{name}", class: "limit",
                header {
                    if let Some(mark) = catalog::mark(&driver) { img { src: mark, alt: "" } }
                    b { "{name}" }
                    if let Some(plan) = plan { span { "{plan}" } }
                }
                if windows.is_empty() {
                    p { class: "none", "No limits reported." }
                }
                for (label, left, resets) in windows {
                    div { key: "{label}", class: if left < 25 { "win low" } else { "win" },
                        div { class: "l1",
                            span { class: "name", "{label}" }
                            if left < 25 { span { class: "lowtag", Icon { name: "circle-alert" } "Low" } }
                            span { class: "pct", "{left}% left" }
                        }
                        div { class: "bar", role: "meter", "aria-valuemin": "0", "aria-valuemax": "100", "aria-valuenow": "{left}",
                            i { style: "width:{left}%" }
                        }
                        div { class: "l2", "{resets}" }
                    }
                }
            }
        }
        p { class: "note", "Updated when you refresh." }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_sources_own_no_buckets() {
        let summary = json!({
            "contractVersion": 6,
            "sources": [
                {"fingerprint": {"provider": "claude", "resolvedHomePath": "/c"}, "status": "ok"},
                {"fingerprint": {"provider": "grok", "resolvedHomePath": "/g"}, "status": "missing"}
            ],
            "buckets": [
                {"day": "2026-10-01", "provider": "claude", "sourcePath": "/c", "costUsd": 3.0,
                 "totals": {"uncachedInputTokens": 10, "cachedInputTokens": 80, "cacheCreationTokens": 5, "outputTokens": 5}},
                {"day": "2026-10-02", "provider": "grok", "sourcePath": "/g", "costUsd": 9.0, "totals": {}}
            ]
        });
        let merged = merge(&summary);
        assert_eq!(merged.cost, 3.0);
        assert_eq!(merged.tokens, 100.0);
        assert_eq!(merged.providers.len(), 1);
        assert_eq!(duration(3_600_000 * 3 + 60_000 * 12), "3h 12m");
    }
}
