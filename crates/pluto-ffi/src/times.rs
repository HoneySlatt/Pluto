//! The drawer's and the thread header's times.

use chrono::{DateTime, Local, Utc};
use pluto_core::timeline::parse_time;

/// `formatRelativeTime`'s compact form: now, 5m, 3h, 2d.
pub fn relative_time(iso: &str, now: DateTime<Utc>) -> String {
    let Some(at) = parse_time(iso) else {
        return String::new();
    };
    let minutes = (now - at).num_minutes().max(0);
    match minutes {
        0 => "now".into(),
        1..=59 => format!("{minutes}m"),
        60..=1439 => format!("{}h", minutes / 60),
        _ => format!("{}d", minutes / 1440),
    }
}

/// A chat's day group in the drawer.
pub fn day_group(iso: &str, now: DateTime<Utc>) -> &'static str {
    let Some(at) = parse_time(iso) else {
        return "Older";
    };
    let local = |t: DateTime<Utc>| t.with_timezone(&Local).date_naive();
    match (local(now) - local(at)).num_days() {
        ..=0 => "Today",
        1 => "Yesterday",
        2..=7 => "This week",
        _ => "Older",
    }
}

/// A chat's time in the drawer: the clock today, the weekday this week,
/// else the date. `format` is the `timestampFormat` setting; its default
/// follows the phone's clock (`clock_24h`).
pub fn chat_time(iso: &str, now: DateTime<Utc>, format: &str, clock_24h: bool) -> String {
    let Some(at) = parse_time(iso) else {
        return String::new();
    };
    let local = at.with_timezone(&Local);
    match day_group(iso, now) {
        "Today" => {
            let twelve = match format {
                "12-hour" => true,
                "24-hour" => false,
                _ => !clock_24h,
            };
            if twelve {
                local.format("%-I:%M %p").to_string()
            } else {
                local.format("%H:%M").to_string()
            }
        }
        "Yesterday" | "This week" => local.format("%a").to_string(),
        _ => local.format("%b %-d").to_string(),
    }
}

/// "6m 40s" from seconds.
pub fn elapsed(seconds: i64) -> String {
    let seconds = seconds.max(0);
    if seconds >= 3600 {
        format!("{}h {:02}m", seconds / 3600, (seconds % 3600) / 60)
    } else {
        format!("{}m {:02}s", seconds / 60, seconds % 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_read_compactly() {
        let now = Utc::now();
        let ago = |minutes| {
            (now - chrono::Duration::minutes(minutes))
                .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
        };
        assert_eq!(relative_time(&ago(0), now), "now");
        assert_eq!(relative_time(&ago(5), now), "5m");
        assert_eq!(relative_time(&ago(180), now), "3h");
        assert_eq!(relative_time(&ago(2 * 1440), now), "2d");
        assert_eq!(day_group(&ago(0), now), "Today");
        assert_eq!(day_group(&ago(60 * 24 * 40), now), "Older");
        assert_eq!(elapsed(400), "6m 40s");
    }
}
