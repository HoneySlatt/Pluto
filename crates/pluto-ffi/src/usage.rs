//! Usage (`#usage`, after the desktop's `usage.rs` and `usage_summary.rs`):
//! Cost | Tokens | Limits, Limits first. Limits are the providers' quota
//! windows (`usageLimits` in the server config, refreshed through
//! `server.refreshProviders`), one card per provider with a meter per
//! window, "Low" under 25% left. Cost and Tokens read
//! `server.getUsageSummary` for the period (merged like `mergeUsage`: the
//! sources the server scanned own their buckets), with the total, a daily
//! (or hourly) bar chart and the split by provider.
//!
//! A period's window is fixed when the page first asks for it (its key
//! names the summary's reply), until the page opens again or refreshes.

use std::collections::BTreeMap;
use std::sync::Mutex;

use chrono::{Local, NaiveDate, Timelike, Utc};
use serde_json::{Value, json};

use crate::{Core, catalog};

/// The periods: label and days (`Past 24h` is hourly).
const PERIODS: [(&str, u32); 4] = [
    ("Past 24h", 1),
    ("7 days", 7),
    ("30 days", 30),
    ("90 days", 90),
];
const CONTRACT_VERSION: i64 = 6;
const COMPATIBLE_SINCE: i64 = 4;

/// The windows the page asked for, by period.
static WINDOWS: Mutex<[Option<Value>; 4]> = Mutex::new([None, None, None, None]);

fn period_index(period: u32) -> usize {
    (period as usize).min(PERIODS.len() - 1)
}

/// The period's window, made on first use.
fn window(period: u32, zone: &str) -> Value {
    let index = period_index(period);
    let mut windows = WINDOWS.lock().unwrap_or_else(|p| p.into_inner());
    windows[index]
        .get_or_insert_with(|| make_window(PERIODS[index].1, zone))
        .clone()
}

fn summary_key(window: &Value) -> String {
    format!("usage-summary:{window}")
}

/// `make_window`.
fn make_window(days: u32, zone: &str) -> Value {
    let now = Utc::now();
    let zone = if zone.is_empty() { "UTC" } else { zone };
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

/// A bar of the chart: its day (or hour), value and value as shown.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct UsageBar {
    pub label: String,
    pub value: f64,
    pub shown: String,
}

/// A provider's part of the period.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct UsageShare {
    pub name: String,
    /// Its mark (`mark_png`).
    pub mark: Option<String>,
    pub shown: String,
    /// Percent of the total.
    pub share: f64,
}

#[derive(Clone, Debug, PartialEq, uniffi::Enum)]
pub enum UsageSummary {
    Loading,
    Failed {
        error: String,
    },
    Ready {
        /// The total as shown, and what it counts.
        total: String,
        caption: String,
        bars: Vec<UsageBar>,
        /// The axis' ends.
        start: String,
        end: String,
        providers: Vec<UsageShare>,
    },
}

/// A quota window: its label, percent left and reset line.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct LimitWindow {
    pub label: String,
    pub left: i64,
    pub low: bool,
    pub resets: String,
}

/// A provider's limits.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct LimitCard {
    pub name: String,
    pub mark: Option<String>,
    pub plan: Option<String>,
    pub windows: Vec<LimitWindow>,
}

fn summary_view(
    reply: Option<&Result<Value, String>>,
    window: &Value,
    tokens: bool,
    label: &str,
) -> UsageSummary {
    let summary = match reply {
        None => return UsageSummary::Loading,
        Some(Err(error)) => {
            return UsageSummary::Failed {
                error: error.clone(),
            };
        }
        Some(Ok(summary)) => summary,
    };
    let merged = merge(summary);
    let total = if tokens { merged.tokens } else { merged.cost };
    let shown = |v: f64| {
        if tokens {
            format!("{} tokens", tok(v))
        } else {
            money(v)
        }
    };
    let hourly = window["resolution"] == "hour";
    let bars: Vec<UsageBar> = series(&merged, window, tokens)
        .into_iter()
        .map(|(label, value)| UsageBar {
            shown: shown(value),
            label,
            value,
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
    UsageSummary::Ready {
        total: if tokens { tok(total) } else { money(total) },
        caption: if tokens {
            format!("Input and output, {caption}")
        } else {
            format!("At model list prices, {caption}")
        },
        start: if hourly {
            "24h ago".into()
        } else {
            format!("{}d ago", bars.len())
        },
        end: if hourly { "now" } else { "today" }.into(),
        bars,
        providers: providers
            .into_iter()
            .map(|(provider, cost, count)| {
                let value = if tokens { count } else { cost };
                UsageShare {
                    name: catalog::driver_label(&provider),
                    mark: catalog::mark(&provider).map(str::to_owned),
                    shown: if tokens { tok(count) } else { money(cost) },
                    share: if total > 0.0 {
                        value / total * 100.0
                    } else {
                        0.0
                    },
                }
            })
            .collect(),
    }
}

fn limit_cards(providers: &[Value]) -> Vec<LimitCard> {
    let now = Utc::now().timestamp_millis();
    providers
        .iter()
        .filter(|p| p["enabled"] != false && p["installed"] != false && p["status"] == "ready")
        .filter(|p| p["usageLimits"].is_object())
        .map(|p| {
            let driver = p["driver"].as_str().unwrap_or_default().to_owned();
            let name = p["displayName"]
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| catalog::driver_label(&driver));
            let windows = p["usageLimits"]["windows"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|w| {
                    let left = (100.0 - w["usedPercent"].as_f64().unwrap_or(0.0).clamp(0.0, 100.0))
                        .round() as i64;
                    let resets = w["resetsAt"]
                        .as_str()
                        .and_then(pluto_core::timeline::parse_time)
                        .map(|t| {
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
                    LimitWindow {
                        label: w["label"].as_str().unwrap_or("Limit").to_owned(),
                        left,
                        low: left < 25,
                        resets,
                    }
                })
                .collect();
            LimitCard {
                mark: catalog::mark(&driver).map(str::to_owned),
                name,
                plan: p["auth"]["label"].as_str().map(str::to_owned),
                windows,
            }
        })
        .collect()
}

#[uniffi::export]
impl Core {
    /// The usage periods' labels.
    pub fn usage_periods(&self) -> Vec<String> {
        PERIODS
            .iter()
            .map(|(label, _)| (*label).to_owned())
            .collect()
    }

    /// The page opened: its periods' windows start from now.
    pub fn usage_open(&self) {
        *WINDOWS.lock().unwrap_or_else(|p| p.into_inner()) = [None, None, None, None];
    }

    /// Asks for the period's summary once (`zone` is the phone's time
    /// zone, `TimeZone.getDefault().id`).
    pub fn usage_follow(&self, period: u32, zone: String) {
        let window = window(period, &zone);
        let key = summary_key(&window);
        let asked = {
            let app = self.hub.lock();
            app.session.replies.contains_key(&key) || app.session.waiting(&key)
        };
        if !asked {
            self.update(|app, _| app.session.request(key, "server.getUsageSummary", window));
        }
    }

    /// Refreshes the limits, and the summary of `period` unless the page
    /// shows the limits.
    pub fn usage_refresh(&self, period: u32, limits: bool, zone: String) {
        let index = period_index(period);
        let old = WINDOWS.lock().unwrap_or_else(|p| p.into_inner())[index].take();
        let window = window(period, &zone);
        self.update(|app, _| {
            let session = &mut app.session;
            session.request("usage-limits", "server.refreshProviders", json!({}));
            if let Some(old) = old {
                session.replies.remove(&summary_key(&old));
            }
            if !limits {
                session.request("usage-rates", "server.refreshUsageRates", json!({}));
                session.request(summary_key(&window), "server.getUsageSummary", window);
            }
        });
    }

    /// The period's cost (or `tokens`) summary.
    pub fn usage_summary(&self, period: u32, tokens: bool, zone: String) -> UsageSummary {
        let window = window(period, &zone);
        let key = summary_key(&window);
        self.read(|app| {
            summary_view(
                app.session.replies.get(&key),
                &window,
                tokens,
                PERIODS[period_index(period)].0,
            )
        })
    }

    /// The providers' quota windows.
    pub fn usage_limits(&self) -> Vec<LimitCard> {
        self.read(|app| limit_cards(app.session.providers()))
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

    #[test]
    fn a_week_has_a_bar_a_day_and_its_shares() {
        let window =
            json!({"sinceDay": "2026-10-01", "untilDay": "2026-10-07", "resolution": "day"});
        let summary = json!({
            "contractVersion": 6,
            "sources": [{"fingerprint": {"provider": "codex", "resolvedHomePath": "/x"}, "status": "ok"}],
            "buckets": [{"day": "2026-10-03", "provider": "codex", "costUsd": 1.5, "totals": {"outputTokens": 2_000_000}}]
        });
        let UsageSummary::Ready {
            total,
            bars,
            providers,
            start,
            ..
        } = summary_view(Some(&Ok(summary)), &window, false, "7 days")
        else {
            panic!("not ready");
        };
        assert_eq!(
            (total.as_str(), bars.len(), start.as_str()),
            ("$1.50", 7, "7d ago")
        );
        assert_eq!(bars[2].shown, "$1.50");
        assert_eq!(providers[0].name, "Codex");
        assert_eq!(providers[0].mark.as_deref(), Some("openai"));
        assert_eq!(providers[0].share, 100.0);
        assert_eq!(
            summary_view(None, &window, true, "7 days"),
            UsageSummary::Loading
        );
    }

    #[test]
    fn limits_show_what_is_left() {
        let providers = vec![
            json!({"driver": "claudeAgent", "status": "ready", "auth": {"label": "Max"},
            "usageLimits": {"windows": [{"label": "5h", "usedPercent": 80.4}, {"label": "Week", "usedPercent": 10}]}}),
        ];
        let cards = limit_cards(&providers);
        assert_eq!(cards[0].name, "Claude");
        assert_eq!(cards[0].plan.as_deref(), Some("Max"));
        assert_eq!(
            (cards[0].windows[0].left, cards[0].windows[0].low),
            (20, true)
        );
        assert_eq!(
            (cards[0].windows[1].left, cards[0].windows[1].low),
            (90, false)
        );
    }
}
