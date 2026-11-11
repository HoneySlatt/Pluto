//! `pluto-measure`: samples a cgroup for a while and writes its figures.
//!
//! ```text
//! pluto-measure --cgroup DIR --seconds N [--interval-ms 1000] [--memory-every 10]
//!               [--agent NAME]... [--out FILE]
//! ```
//!
//! It samples every `--interval-ms` milliseconds for `--seconds` seconds, the first sample being at
//! 0 and sample *n* at *n* intervals from it, whatever the earlier reads cost. It reads memory in
//! the first sample, every `--memory-every` samples after it and in the last one, then writes the
//! figures as JSON to `--out` (or stdout) and a one-line summary to stderr.
//!
//! A flag given twice is an error, except `--agent`. If the subject ends before the run does, its
//! cgroup goes: the figures of what was sampled are written all the same, with
//! `ended_early_at_ms` set. `--out` is checked before the first sample, so that a run is not lost
//! at its end to a path that cannot be written.
//!
//! Exit codes: 2 for a bad command line; 1 if the cgroup cannot be read at the first sample, or
//! the output cannot be written; else 0.

use pluto_measure::sample::Reader;
use pluto_measure::series::{Schedule, take_series};
use pluto_measure::summary::{Figures, summarize};
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::{fs, io};

/// The cgroup is read through the real `/proc`.
const PROC_ROOT: &str = "/proc";

const USAGE: &str = "usage: pluto-measure --cgroup DIR --seconds N [--interval-ms 1000] \
[--memory-every 10] [--agent NAME]... [--out FILE]";

struct Options {
    cgroup: PathBuf,
    schedule: Schedule,
    agents: Vec<String>,
    out: Option<PathBuf>,
}

fn main() -> ExitCode {
    let options = match parse_options(std::env::args().skip(1)) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("pluto-measure: {message}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    let output = match options.out.as_deref().map(PendingFile::create) {
        Some(Err(error)) => {
            eprintln!("pluto-measure: cannot write the output: {error}");
            return ExitCode::from(1);
        }
        Some(Ok(file)) => Some(file),
        None => None,
    };
    let reader = Reader::new(Path::new(PROC_ROOT), &options.cgroup);
    let series = match take_series(&reader, &options.schedule) {
        Ok(series) => series,
        Err(error) => {
            eprintln!(
                "pluto-measure: cannot read {}: {error}",
                options.cgroup.display()
            );
            return ExitCode::from(1);
        }
    };
    let mut figures = summarize(&series.samples, &options.agents);
    figures.ended_early_at_ms = series.ended_early_at_ms;
    if let Err(error) = write_figures(&figures, output) {
        eprintln!("pluto-measure: cannot write the figures: {error}");
        return ExitCode::from(1);
    }
    if let Some(at_ms) = figures.ended_early_at_ms {
        let because = series.ended_early_because.as_deref().unwrap_or("unknown");
        eprintln!("pluto-measure: the cgroup could no longer be read after {at_ms} ms: {because}");
    }
    eprintln!("{}", summary_line(&figures));
    ExitCode::SUCCESS
}

fn parse_options(args: impl Iterator<Item = String>) -> Result<Options, String> {
    let mut args = args;
    let mut cgroup = None;
    let mut seconds = None;
    let mut interval_ms = None;
    let mut memory_every = None;
    let mut agents = Vec::new();
    let mut out = None;
    while let Some(flag) = args.next() {
        let mut value = || args.next().ok_or_else(|| format!("{flag} needs a value"));
        match flag.as_str() {
            "--cgroup" => set_once(&flag, &mut cgroup, PathBuf::from(value()?))?,
            "--seconds" => set_once(&flag, &mut seconds, positive_number(&flag, &value()?)?)?,
            "--interval-ms" => {
                set_once(&flag, &mut interval_ms, positive_number(&flag, &value()?)?)?
            }
            "--memory-every" => {
                set_once(&flag, &mut memory_every, positive_number(&flag, &value()?)?)?
            }
            "--agent" => agents.push(value()?),
            "--out" => set_once(&flag, &mut out, PathBuf::from(value()?))?,
            _ => return Err(format!("unknown argument {flag}")),
        }
    }
    let seconds = seconds.ok_or("--seconds is required")?;
    if seconds.checked_mul(1000).is_none() {
        return Err("--seconds is too large".to_string());
    }
    Ok(Options {
        cgroup: cgroup.ok_or("--cgroup is required")?,
        schedule: Schedule {
            seconds,
            interval_ms: interval_ms.unwrap_or(1000),
            memory_every: memory_every.unwrap_or(10),
        },
        agents,
        out,
    })
}

fn set_once<T>(flag: &str, slot: &mut Option<T>, value: T) -> Result<(), String> {
    if slot.replace(value).is_some() {
        return Err(format!("{flag} is given twice"));
    }
    Ok(())
}

fn positive_number(flag: &str, text: &str) -> Result<u64, String> {
    match text.parse::<u64>() {
        Ok(number) if number > 0 => Ok(number),
        _ => Err(format!(
            "{flag} needs a positive whole number, not {text:?}"
        )),
    }
}

/// The output file, made beside its place at the start, so that a path that cannot be written
/// is known before the run. It is written whole and renamed, so that a reader never sees half of
/// it; the temporary is removed if that does not happen.
struct PendingFile {
    file: File,
    temporary: PathBuf,
    target: PathBuf,
}

impl PendingFile {
    fn create(target: &Path) -> io::Result<PendingFile> {
        let mut temporary = target.as_os_str().to_owned();
        temporary.push(format!(".{}.tmp", std::process::id()));
        let temporary = PathBuf::from(temporary);
        Ok(PendingFile {
            file: File::create(&temporary)?,
            temporary,
            target: target.to_path_buf(),
        })
    }

    fn commit(mut self, text: &str) -> io::Result<()> {
        self.file.write_all(text.as_bytes())?;
        self.file.sync_all()?;
        fs::rename(&self.temporary, &self.target)
    }
}

impl Drop for PendingFile {
    fn drop(&mut self) {
        // After a rename there is nothing left to remove, and the error says so.
        let _ = fs::remove_file(&self.temporary);
    }
}

/// The figures as JSON, to the file or to stdout.
fn write_figures(figures: &Figures, output: Option<PendingFile>) -> io::Result<()> {
    let json = serde_json::to_string_pretty(figures).map_err(io::Error::other)?;
    match output {
        Some(file) => file.commit(&format!("{json}\n")),
        None => {
            println!("{json}");
            Ok(())
        }
    }
}

fn summary_line(figures: &Figures) -> String {
    format!(
        "{:.1} s, {} samples: {:.1} MiB memory, {:.2} cores, {:.0} wake-ups/s, {} processes",
        figures.seconds,
        figures.samples,
        figures
            .app
            .memory
            .headline
            .saturating_add(figures.agents.memory.headline) as f64
            / (1024.0 * 1024.0),
        figures.cpu_cores,
        figures.wakeups_per_second,
        figures
            .app
            .processes
            .saturating_add(figures.agents.processes),
    )
}
