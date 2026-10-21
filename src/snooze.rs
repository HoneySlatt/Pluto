//! Thread snooze rules (`client-runtime/state/threadSettled.ts`,
//! `Sidebar.snooze.ts`): wake presets, when a thread may snooze and when a
//! snoozed thread still hides from the inbox.
use chrono::{DateTime, Datelike, Duration, Local, TimeZone, Timelike, Utc};

use crate::model::Thread;
use crate::timeline::parse_time;

/// A queued turn start stops counting after this grace window.
const QUEUED_TURN_START_GRACE_MS: i64 = 2 * 60 * 1000;

pub struct Preset {
    pub id: &'static str,
    pub label: &'static str,
    /// Menu-row time column ("9:00 AM", "Mon 9:00 AM").
    pub when: String,
    pub until: DateTime<Utc>,
}

fn iso(date: DateTime<Utc>) -> String {
    date.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

pub fn until_iso(preset: &Preset) -> String {
    iso(preset.until)
}

fn time_of_day(date: DateTime<Local>) -> String {
    date.format("%-I:%M %p").to_string()
}

/// Local `hour:00` on the day of `base`, `days` calendar days later.
fn at_hour(base: DateTime<Local>, days: i64, hour: u32) -> DateTime<Local> {
    let day = base.date_naive() + Duration::days(days);
    let naive = day.and_hms_opt(hour, 0, 0).unwrap_or_default();
    Local
        .from_local_datetime(&naive)
        .earliest()
        .unwrap_or_else(|| base.with_hour(hour).unwrap_or(base))
}

/// `resolveSnoozePresets`: in 1 and 3 hours, this evening (while it is more
/// than an hour away), tomorrow 9:00 and next Monday 9:00 unless that is
/// tomorrow.
pub fn presets(now: DateTime<Local>) -> Vec<Preset> {
    let in_an_hour = now + Duration::hours(1);
    let in_three_hours = now + Duration::hours(3);
    let mut presets = vec![
        Preset {
            id: "hour",
            label: "In 1 hour",
            when: time_of_day(in_an_hour),
            until: in_an_hour.with_timezone(&Utc),
        },
        Preset {
            id: "three-hours",
            label: "In 3 hours",
            when: time_of_day(in_three_hours),
            until: in_three_hours.with_timezone(&Utc),
        },
    ];
    let evening = at_hour(now, 0, 18);
    if evening - now > Duration::hours(1) {
        presets.push(Preset {
            id: "evening",
            label: "This evening",
            when: time_of_day(evening),
            until: evening.with_timezone(&Utc),
        });
    }
    let tomorrow = at_hour(now, 1, 9);
    presets.push(Preset {
        id: "tomorrow",
        label: "Tomorrow",
        when: time_of_day(tomorrow),
        until: tomorrow.with_timezone(&Utc),
    });
    let weekday = now.weekday().num_days_from_sunday() as i64;
    let days_until_monday = match (1 - weekday + 7) % 7 {
        0 => 7,
        days => days,
    };
    let next_week = at_hour(now, days_until_monday, 9);
    if next_week != tomorrow {
        presets.push(Preset {
            id: "next-week",
            label: "Next week",
            when: format!("{} {}", next_week.format("%a"), time_of_day(next_week)),
            until: next_week.with_timezone(&Utc),
        });
    }
    presets
}

fn ms(iso: Option<&str>) -> Option<i64> {
    iso.and_then(parse_time).map(|d| d.timestamp_millis())
}

/// `hasQueuedTurnStart`: a user message no turn has adopted yet.
fn has_queued_turn_start(thread: &Thread, now: DateTime<Utc>) -> bool {
    let Some(message_at) = ms(thread.latest_user_message_at.as_deref()) else {
        return false;
    };
    if thread.session.as_ref().is_some_and(|s| s.status == "error") {
        return false;
    }
    if (now.timestamp_millis() - message_at).abs() > QUEUED_TURN_START_GRACE_MS {
        return false;
    }
    let Some(turn) = &thread.latest_turn else {
        return true;
    };
    [
        Some(turn.requested_at.as_str()),
        turn.started_at.as_deref(),
        turn.completed_at.as_deref(),
    ]
    .into_iter()
    .all(|at| ms(at).is_none_or(|at| at < message_at))
}

/// `canSnooze`: never while the agent waits on the user.
pub fn can_snooze(thread: &Thread, now: DateTime<Utc>) -> bool {
    !thread.has_pending_approvals
        && !thread.has_pending_user_input
        && !has_queued_turn_start(thread, now)
}

/// `threadRaisedHandWhileSnoozed`: blocked on the user, a fresh failure or a
/// run completed after the snooze.
fn raised_hand(thread: &Thread) -> bool {
    if thread.has_pending_approvals || thread.has_pending_user_input {
        return true;
    }
    let snoozed_at = ms(thread.snoozed_at.as_deref());
    if let Some(session) = &thread.session
        && session.status == "error"
        && snoozed_at.is_none_or(|at| ms(Some(&session.updated_at)).is_some_and(|u| u > at))
    {
        return true;
    }
    if let (Some(at), Some(turn)) = (snoozed_at, &thread.latest_turn)
        && turn.state == "completed"
        && ms(turn.completed_at.as_deref()).is_some_and(|done| done > at)
    {
        return true;
    }
    false
}

/// `effectiveSnoozed`: hidden while the wake time is ahead and nothing
/// raised the thread's hand.
pub fn effective_snoozed(thread: &Thread, now: DateTime<Utc>) -> bool {
    match ms(thread.snoozed_until.as_deref()) {
        Some(wake) if wake > now.timestamp_millis() => !raised_hand(thread),
        _ => false,
    }
}

/// `threadWokeAt`: when a snoozed thread woke (its wake time, or what
/// raised its hand), or `None` while it still sleeps or never snoozed.
pub fn woke_at(thread: &Thread, now: DateTime<Utc>) -> Option<String> {
    let until = thread.snoozed_until.as_deref()?;
    let wake = ms(Some(until))?;
    if raised_hand(thread) {
        let snoozed_at = ms(thread.snoozed_at.as_deref());
        if let (Some(at), Some(turn)) = (snoozed_at, &thread.latest_turn)
            && turn.state == "completed"
            && let Some(done) = turn.completed_at.as_deref()
            && ms(Some(done)).is_some_and(|done| done > at)
        {
            return Some(done.to_owned());
        }
        return thread
            .session
            .as_ref()
            .map(|s| s.updated_at.clone())
            .or_else(|| thread.snoozed_at.clone());
    }
    (wake <= now.timestamp_millis()).then(|| until.to_owned())
}

/// `snoozeWakeLabel`: time left until the wake ("45m", "3h", "2d").
pub fn wake_label(until: &str, now: DateTime<Utc>) -> String {
    let Some(wake) = ms(Some(until)) else {
        return "now".into();
    };
    let remaining = wake - now.timestamp_millis();
    const HOUR: i64 = 60 * 60 * 1000;
    if remaining <= 0 {
        "now".into()
    } else if remaining < HOUR {
        format!("{}m", ((remaining + 59_999) / 60_000).max(1))
    } else if remaining < 24 * HOUR {
        format!("{}h", (remaining + HOUR - 1) / HOUR)
    } else {
        format!("{}d", (remaining + 24 * HOUR - 1) / (24 * HOUR))
    }
}

/// `CustomSnoozeInput`: a local date and time, or an elapsed duration.
pub enum CustomSnooze {
    At(chrono::NaiveDateTime),
    /// An amount of minutes, hours or days (unit 0, 1, 2).
    For(String, usize),
}

/// `resolveCustomSnooze`: the wake time in the future, or `None` for a past,
/// nonexistent (DST gap) or non-positive choice.
pub fn resolve_custom(input: &CustomSnooze, now: DateTime<Utc>) -> Option<String> {
    let wake = match input {
        CustomSnooze::For(amount, unit) => {
            let amount: f64 = amount.trim().parse().ok().filter(|a: &f64| a.is_finite())?;
            if amount <= 0.0 {
                return None;
            }
            let unit_ms = [60_000.0, 3_600_000.0, 86_400_000.0][(*unit).min(2)];
            now + Duration::milliseconds((amount * unit_ms) as i64)
        }
        CustomSnooze::At(naive) => Local
            .from_local_datetime(naive)
            .single()?
            .with_timezone(&Utc),
    };
    (wake > now).then(|| iso(wake))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_snoozes_must_lie_ahead() {
        let now = Utc.with_ymd_and_hms(2026, 10, 21, 10, 0, 0).unwrap();
        assert_eq!(
            resolve_custom(&CustomSnooze::For("2".into(), 1), now).as_deref(),
            Some("2026-10-21T12:00:00.000Z")
        );
        assert_eq!(
            resolve_custom(&CustomSnooze::For("1.5".into(), 0), now).as_deref(),
            Some("2026-10-21T10:01:30.000Z")
        );
        assert!(resolve_custom(&CustomSnooze::For("0".into(), 2), now).is_none());
        assert!(resolve_custom(&CustomSnooze::For("x".into(), 2), now).is_none());
        let past = (now - Duration::hours(30))
            .with_timezone(&Local)
            .naive_local();
        assert!(resolve_custom(&CustomSnooze::At(past), now).is_none());
        let ahead = (now + Duration::hours(30))
            .with_timezone(&Local)
            .naive_local();
        assert!(resolve_custom(&CustomSnooze::At(ahead), now).is_some());
    }

    fn local(y: i32, m: u32, d: u32, h: u32, min: u32) -> DateTime<Local> {
        Local.with_ymd_and_hms(y, m, d, h, min, 0).unwrap()
    }

    #[test]
    fn evening_preset_needs_an_hour_of_margin() {
        // Tuesday afternoon: every preset is offered.
        let ids: Vec<&str> = presets(local(2026, 10, 20, 14, 0))
            .iter()
            .map(|p| p.id)
            .collect();
        assert_eq!(
            ids,
            ["hour", "three-hours", "evening", "tomorrow", "next-week"]
        );
        let late: Vec<&str> = presets(local(2026, 10, 20, 17, 30))
            .iter()
            .map(|p| p.id)
            .collect();
        assert_eq!(late, ["hour", "three-hours", "tomorrow", "next-week"]);
    }

    #[test]
    fn sunday_collapses_next_week_into_tomorrow() {
        let ids: Vec<&str> = presets(local(2026, 10, 25, 20, 0))
            .iter()
            .map(|p| p.id)
            .collect();
        assert_eq!(ids, ["hour", "three-hours", "tomorrow"]);
        let monday = presets(local(2026, 10, 26, 20, 0));
        assert_eq!(monday.last().map(|p| p.when.as_str()), Some("Mon 9:00 AM"));
    }

    #[test]
    fn wake_labels_round_up() {
        let now = Utc::now();
        let label = |minutes: i64| wake_label(&iso(now + Duration::minutes(minutes)), now);
        assert_eq!(label(45), "45m");
        assert_eq!(label(61), "2h");
        assert_eq!(label(60 * 30), "2d");
        assert_eq!(label(-5), "now");
    }

    #[test]
    fn pending_input_wakes_a_snoozed_thread() {
        let now = Utc::now();
        let mut thread = Thread {
            snoozed_until: Some(iso(now + Duration::hours(1))),
            snoozed_at: Some(iso(now - Duration::minutes(1))),
            ..Default::default()
        };
        assert!(effective_snoozed(&thread, now));
        thread.has_pending_user_input = true;
        assert!(!effective_snoozed(&thread, now));
        assert!(!can_snooze(&thread, now));
    }
}
