//! A series of samples taken over time, at a fixed interval.
//!
//! Sample *n* is taken at *n* intervals from the first, whatever the earlier reads cost: each one
//! waits for its own deadline, not for an interval after the last, so a slow read does not push
//! the following ones back.
//!
//! A subject may end before the series does (a scope whose command is done): its cgroup is then
//! removed. After the first sample, a cgroup that cannot be read ends the series with what was
//! taken, and says when ([`Series::ended_early_at_ms`]).

use crate::sample::{Reader, Sample};
use std::io;
use std::time::{Duration, Instant};

/// How long, how often, and how often with memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Schedule {
    /// Whole seconds from the first sample to the last; at most `u64::MAX / 1000`.
    pub seconds: u64,
    /// Milliseconds between samples; at least 1.
    pub interval_ms: u64,
    /// Memory is read at the first sample, every this many samples after it and at the last one;
    /// at least 1.
    pub memory_every: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Series {
    pub samples: Vec<Sample>,
    /// When the cgroup could no longer be read, in milliseconds from the first sample; `None` if
    /// the series ran to its end.
    pub ended_early_at_ms: Option<u64>,
    /// Why it ended early: usually the cgroup gone with its last process, but a permission error
    /// must not be taken for the subject's end.
    pub ended_early_because: Option<String>,
}

/// Takes the samples of the schedule, from `seconds × 1000 / interval_ms + 1` of them. An error
/// only if the first sample fails: the cgroup is not there to measure, or holds no process. A
/// later sample that fails, or finds the cgroup empty, ends the series early.
pub fn take_series(reader: &Reader, schedule: &Schedule) -> io::Result<Series> {
    let last_index = schedule.seconds.saturating_mul(1000) / schedule.interval_ms;
    let start = Instant::now();
    let mut samples = Vec::new();
    for index in 0..=last_index {
        let deadline = start + Duration::from_millis(index.saturating_mul(schedule.interval_ms));
        std::thread::sleep(deadline.saturating_duration_since(Instant::now()));
        let at_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
        let with_memory = index % schedule.memory_every == 0 || index == last_index;
        let read = reader.sample(at_ms, with_memory).and_then(|sample| {
            // A cgroup outlives its last process: an empty one is a subject that has ended.
            if sample.processes.is_empty() {
                Err(io::Error::other("no process is left in the cgroup"))
            } else {
                Ok(sample)
            }
        });
        match read {
            Ok(sample) => samples.push(sample),
            Err(error) if samples.is_empty() => return Err(error),
            Err(error) => {
                return Ok(Series {
                    samples,
                    ended_early_at_ms: Some(at_ms),
                    ended_early_because: Some(error.to_string()),
                });
            }
        }
    }
    Ok(Series {
        samples,
        ended_early_at_ms: None,
        ended_early_because: None,
    })
}
