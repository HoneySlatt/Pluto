//! Plays one process of a recording in an agent CLI's place. See `tests/binaries.rs` for its
//! command line.
//!
//! ```text
//! pluto-replay-agent --recording F --state D [--workspace W] [--home H] [--timeout-ms N] -- ARGS...
//! ```
//!
//! Each start takes the next process of the recording, counted in `D/next-process`, so a driver
//! that spawns the CLI twice gets processes 0 and 1. When it stops it writes `D/process-<n>.json`:
//! how far the replay went, and its error if any.
//!
//! It exits like the recorded process when the replay reaches its exit (code, or signal). Else:
//! 2 for a bad command line or a state folder it cannot use, 3 when the replay fails (the
//! message on stderr), 4 when the client sends nothing for longer than the timeout (30 seconds
//! by default), 5 when the client closes stdin before the recorded exit.

use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::{self, BufRead, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::Duration;

use pluto_recording::format;
use pluto_recording::program::{self, CommandLine};
use pluto_recording::replay::{LivePlaces, Output, Replayer, Status};
use serde_json::json;

const NAME: &str = "pluto-replay-agent";
const USAGE: &str = "usage: pluto-replay-agent --recording F --state D [--workspace W] [--home H] \
                     [--timeout-ms N] -- ARGS...";
const DEFAULT_TIMEOUT_MS: u64 = 30_000;

const EXIT_USAGE: i32 = 2;
const EXIT_REPLAY_FAILED: i32 = 3;
const EXIT_CLIENT_SILENT: i32 = 4;
const EXIT_STDIN_ENDED_EARLY: i32 = 5;

struct Options {
    recording: PathBuf,
    state: PathBuf,
    places: LivePlaces,
    timeout: Duration,
    args: Vec<String>,
}

/// How the replay stopped.
enum Ending {
    /// The recorded exit was reached.
    Exited {
        code: Option<i32>,
        signal: Option<i32>,
    },
    /// Ended early, with this exit code and message.
    Stopped { exit_code: i32, message: String },
}

fn main() {
    let options = match read_options(std::env::args_os().skip(1)) {
        Ok(options) => options,
        Err(message) => {
            program::report(NAME, &message);
            program::report(NAME, USAGE);
            std::process::exit(EXIT_USAGE);
        }
    };
    let process = match take_process_number(&options.state) {
        Ok(process) => process,
        Err(error) => {
            let state = options.state.display();
            program::report(
                NAME,
                &format!("cannot use the state folder {state}: {error}"),
            );
            std::process::exit(EXIT_USAGE);
        }
    };
    let (ending, status) = replay(&options, process);
    let error = match &ending {
        Ending::Exited { .. } => None,
        Ending::Stopped { message, .. } => {
            program::report(NAME, message);
            Some(message.as_str())
        }
    };
    if let Err(error) = write_status(&options.state, process, status.as_ref(), error) {
        program::report(NAME, &format!("cannot write the replay's status: {error}"));
    }
    match ending {
        Ending::Exited { code, signal } => program::exit_like(code, signal),
        Ending::Stopped { exit_code, .. } => std::process::exit(exit_code),
    }
}

fn read_options(args: impl IntoIterator<Item = OsString>) -> Result<Options, String> {
    let names = [
        "--recording",
        "--state",
        "--workspace",
        "--home",
        "--timeout-ms",
    ];
    let command_line = CommandLine::parse(args, &names)?;
    let optional_text = |name: &str| {
        command_line
            .optional(name)?
            .map(|value| program::text(value, name))
            .transpose()
    };
    let timeout_ms = match optional_text("--timeout-ms")? {
        Some(text) => text
            .parse()
            .map_err(|_| format!("--timeout-ms takes milliseconds, not {text:?}"))?,
        None => DEFAULT_TIMEOUT_MS,
    };
    let args = command_line
        .arguments
        .iter()
        .map(|arg| program::text(arg, "an argument"))
        .collect::<Result<_, _>>()?;
    Ok(Options {
        recording: command_line.required("--recording")?.into(),
        state: command_line.required("--state")?.into(),
        places: LivePlaces {
            workspace: optional_text("--workspace")?,
            home: optional_text("--home")?,
        },
        timeout: Duration::from_millis(timeout_ms),
        args,
    })
}

/// Takes this start's process number from the state folder's counter, under a lock, so that
/// agents started at once get different numbers.
fn take_process_number(state: &Path) -> io::Result<u32> {
    fs::create_dir_all(state)?;
    // The counter is rewritten in place, not through a rename: a rename would hand the next
    // start a new file, outside the lock this start holds.
    let mut counter = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(state.join("next-process"))?;
    counter.lock()?;
    let mut text = String::new();
    counter.read_to_string(&mut text)?;
    let process: u32 = match text.trim() {
        "" => 0,
        number => number.parse().map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("next-process holds {number:?}, not a process number"),
            )
        })?,
    };
    let next = process
        .checked_add(1)
        .ok_or_else(|| io::Error::other("next-process is at its largest number"))?;
    counter.set_len(0)?;
    counter.seek(SeekFrom::Start(0))?;
    counter.write_all(format!("{next}\n").as_bytes())?;
    // Closing the file releases the lock.
    Ok(process)
}

/// Replays the process, and returns how it ended and where it stood, if it started.
fn replay(options: &Options, process: u32) -> (Ending, Option<Status>) {
    let failed = |message: String| Ending::Stopped {
        exit_code: EXIT_REPLAY_FAILED,
        message,
    };
    let recording = match fs::read_to_string(&options.recording) {
        Ok(text) => format::read(&text).map_err(|error| error.to_string()),
        Err(error) => Err(error.to_string()),
    };
    let recording = match recording {
        Ok(recording) => recording,
        Err(error) => {
            let path = options.recording.display();
            return (
                failed(format!("cannot read the recording {path}: {error}")),
                None,
            );
        }
    };
    let started = Replayer::new(&recording, process, &options.args, options.places.clone());
    let (mut replayer, outputs) = match started {
        Ok(started) => started,
        Err(error) => return (failed(error.to_string()), None),
    };
    let client = read_client_lines();
    // The status is saved after every step, not only at the end: a client may kill the fake
    // agent, as the fork's ACP runtime kills an agent's processes when it closes a session.
    let save = |status: &Status| {
        if let Err(error) = write_status(&options.state, process, Some(status), None) {
            program::report(NAME, &format!("cannot write the replay's status: {error}"));
        }
    };
    save(&replayer.status());
    let ending = match give_back(&outputs) {
        Ok(Some(ending)) => ending,
        Ok(None) => play(&mut replayer, &client, options.timeout, &save),
        Err(error) => failed(format!("cannot write the agent's output: {error}")),
    };
    (ending, Some(replayer.status()))
}

/// What the client's stdin gives the replay.
enum ClientInput {
    Line(String),
    Closed,
}

/// Reads stdin on a thread of its own, so that the replay can wait on it with a timeout.
fn read_client_lines() -> Receiver<ClientInput> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let mut stdin = io::stdin().lock();
        let mut line = Vec::new();
        loop {
            line.clear();
            // A read error ends the input as its end does: either way no line will come.
            match stdin.read_until(b'\n', &mut line) {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    let text = line.strip_suffix(b"\n").unwrap_or(&line);
                    let text = String::from_utf8_lossy(text).into_owned();
                    if sender.send(ClientInput::Line(text)).is_err() {
                        return;
                    }
                }
            }
        }
        // The replay may have stopped already, and then nothing waits for this.
        let _ = sender.send(ClientInput::Closed);
    });
    receiver
}

/// Feeds the client's lines to the replay until it reaches an exit or stops, saving its status
/// after each line.
fn play(
    replayer: &mut Replayer,
    client: &Receiver<ClientInput>,
    timeout: Duration,
    save: &dyn Fn(&Status),
) -> Ending {
    loop {
        let input = match client.recv_timeout(timeout) {
            Ok(input) => input,
            // The reading thread sends `Closed` before it ends; a sender gone silent ended too.
            Err(RecvTimeoutError::Disconnected) => ClientInput::Closed,
            Err(RecvTimeoutError::Timeout) => {
                return Ending::Stopped {
                    exit_code: EXIT_CLIENT_SILENT,
                    message: format!(
                        "the client sent nothing for {} ms; the replay waits for {}",
                        timeout.as_millis(),
                        replayer
                            .status()
                            .waiting_for
                            .as_deref()
                            .unwrap_or("nothing")
                    ),
                };
            }
        };
        let closed = matches!(input, ClientInput::Closed);
        let outputs = match input {
            ClientInput::Line(line) => replayer.client_line(&line),
            ClientInput::Closed => replayer.stdin_closed(),
        };
        let given_back = match outputs {
            // Saved before the outputs are written: a client may kill the fake agent as soon as
            // it reads them, as the fork does once `session/close` is answered, which would
            // leave the last step unsaved. A kill between the two leaves a status ahead of what
            // the client read; the client then waits for that output and its own check fails.
            Ok(outputs) => {
                save(&replayer.status());
                give_back(&outputs)
            }
            Err(error) => {
                return Ending::Stopped {
                    exit_code: EXIT_REPLAY_FAILED,
                    message: error.to_string(),
                };
            }
        };
        match given_back {
            Ok(Some(ending)) => return ending,
            // A recording without an exit ends when its last record is consumed.
            Ok(None) if closed && replayer.status().complete => {
                return Ending::Exited {
                    code: None,
                    signal: None,
                };
            }
            Ok(None) if closed => {
                return Ending::Stopped {
                    exit_code: EXIT_STDIN_ENDED_EARLY,
                    message: "the client closed stdin before the recorded exit".to_owned(),
                };
            }
            Ok(None) => {}
            Err(error) => {
                return Ending::Stopped {
                    exit_code: EXIT_REPLAY_FAILED,
                    message: format!("cannot write the agent's output: {error}"),
                };
            }
        }
    }
}

/// Writes the agent's outputs, each line flushed at once; returns the exit when one comes.
fn give_back(outputs: &[Output]) -> io::Result<Option<Ending>> {
    for output in outputs {
        match output {
            Output::Stdout(line) => write_line(&mut io::stdout().lock(), line)?,
            Output::Stderr(line) => write_line(&mut io::stderr().lock(), line)?,
            Output::Exit { code, signal } => {
                return Ok(Some(Ending::Exited {
                    code: *code,
                    signal: *signal,
                }));
            }
        }
    }
    Ok(None)
}

fn write_line(sink: &mut impl Write, line: &str) -> io::Result<()> {
    sink.write_all(line.as_bytes())?;
    sink.write_all(b"\n")?;
    sink.flush()
}

/// Writes `D/process-<n>.json` through a temporary file and a rename, so that a reader never
/// sees half of it.
fn write_status(
    state: &Path,
    process: u32,
    status: Option<&Status>,
    error: Option<&str>,
) -> io::Result<()> {
    let value = json!({
        "process": process,
        "consumed": status.map_or(0, |status| status.consumed),
        "total": status.map_or(0, |status| status.total),
        "complete": status.is_some_and(|status| status.complete),
        "waiting_for": status.and_then(|status| status.waiting_for.as_deref()),
        "error": error,
    });
    let path = state.join(format!("process-{process}.json"));
    let temporary = state.join(format!(".process-{process}.json.tmp"));
    fs::write(&temporary, format!("{value:#}\n"))?;
    fs::rename(&temporary, &path)
}
