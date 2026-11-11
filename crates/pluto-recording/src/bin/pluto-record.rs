//! Records an agent's stdio to a masked recording, and checks recordings. See
//! `tests/binaries.rs` for its command line.
//!
//! ```text
//! pluto-record record --out F --agent A --scenario S --workspace W --home H
//!     [--meta K=V]... [--keep-env NAME]... -- PROGRAM ARGS...
//! pluto-record check FILE...
//! pluto-record review FILE
//! pluto-record mask-document --workspace W --home H [--home H]...
//! ```
//!
//! `record` runs the agent with an empty environment but `PATH`, `HOME` (the disposable home),
//! `LANG=C.UTF-8` and the variables `--keep-env` names, in the recorder's own folder. It forwards
//! every byte both ways at once, and writes each line to the recording, masked, before it
//! forwards it, so that the file's order follows cause and effect. It forwards SIGINT, SIGTERM,
//! SIGHUP and SIGQUIT to the agent and ends like it. Once the agent has ended, it waits 2 seconds
//! at most for the agent's output to close (a process the agent left may hold it), then records
//! the exit; a forwarded signal in that wait ends the recorder at once, without the exit. A
//! second `record` on the same file appends the next process under the same header. The
//! folders must be absolute paths.
//!
//! Exit codes: `record` ends like the agent, or with 2 when it cannot start; `check` with 1 when
//! it finds something or cannot read a file; every command with 2 for a bad command line.

use std::collections::{BTreeMap, BTreeSet};
use std::convert::Infallible;
use std::ffi::{OsStr, OsString};
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::mem::MaybeUninit;
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::ptr;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use pluto_recording::format::{self, Agent, Header, Record, Recording, Side, Stream};
use pluto_recording::mask::{self, Masker, Places};
use pluto_recording::program::{self, CommandLine};
use pluto_recording::scan::{self, Finding};
use serde_json::Value;

const NAME: &str = "pluto-record";
const USAGE: &str =
    "usage: pluto-record record --out F --agent A --scenario S --workspace W --home H \
                     [--meta K=V]... [--keep-env NAME]... -- PROGRAM ARGS...
       pluto-record check FILE...
       pluto-record review FILE
       pluto-record remask FILE...
       pluto-record mask-document --workspace W --home H [--home H]...";

const EXIT_FOUND: i32 = 1;
const EXIT_CANNOT_START: i32 = 2;

fn main() {
    let mut args = std::env::args_os().skip(1);
    let command = args.next();
    let outcome = match command.as_deref().and_then(OsStr::to_str) {
        Some("record") => record(args),
        Some("check") => check(args),
        Some("review") => review(args),
        Some("remask") => remask(args),
        Some("mask-document") => mask_document(args),
        _ => Err(Failure::Usage("a command is needed".to_owned())),
    };
    match outcome {
        Ok(code) => std::process::exit(code),
        Err(Failure::Usage(message)) => {
            program::report(NAME, &message);
            program::report(NAME, USAGE);
            std::process::exit(EXIT_CANNOT_START);
        }
        Err(Failure::CannotRun(message)) => {
            program::report(NAME, &message);
            std::process::exit(EXIT_CANNOT_START);
        }
    }
}

/// Why a command could not do its work.
enum Failure {
    Usage(String),
    CannotRun(String),
}

impl From<io::Error> for Failure {
    fn from(error: io::Error) -> Failure {
        Failure::CannotRun(error.to_string())
    }
}

fn usage(message: String) -> Failure {
    Failure::Usage(message)
}

// --- record ------------------------------------------------------------------------------------

struct RecordOptions {
    out: PathBuf,
    agent: Agent,
    scenario: String,
    meta: BTreeMap<String, String>,
    workspace: String,
    home: String,
    keep_env: Vec<OsString>,
    program: OsString,
    args: Vec<OsString>,
}

fn read_record_options(args: impl IntoIterator<Item = OsString>) -> Result<RecordOptions, String> {
    let names = [
        "--out",
        "--agent",
        "--scenario",
        "--workspace",
        "--home",
        "--meta",
        "--keep-env",
    ];
    let command_line = CommandLine::parse(args, &names)?;
    let text = |name: &str| program::text(command_line.required(name)?, name);
    let agent_name = text("--agent")?;
    let agent: Agent = serde_json::from_value(Value::String(agent_name.clone()))
        .map_err(|_| format!("unknown agent {agent_name:?}"))?;
    let mut meta = BTreeMap::new();
    for pair in command_line.all("--meta") {
        let pair = program::text(pair, "--meta")?;
        let (key, value) = pair
            .split_once('=')
            .ok_or_else(|| format!("--meta takes KEY=VALUE, not {pair:?}"))?;
        meta.insert(key.to_owned(), value.to_owned());
    }
    let out = command_line.required("--out")?.into();
    let scenario = text("--scenario")?;
    let workspace = absolute(text("--workspace")?, "--workspace")?;
    let home = absolute(text("--home")?, "--home")?;
    let keep_env = command_line
        .all("--keep-env")
        .map(OsStr::to_owned)
        .collect();
    let mut command = command_line.arguments.into_iter();
    let program = command
        .next()
        .ok_or("the program to record is missing after --")?;
    Ok(RecordOptions {
        out,
        agent,
        scenario,
        meta,
        workspace,
        home,
        keep_env,
        program,
        args: command.collect(),
    })
}

/// A folder given on the command line, refused unless absolute: the masker replaces it in every
/// text, and a relative one, such as `.`, would match all over.
fn absolute(folder: String, name: &str) -> Result<String, String> {
    if Path::new(&folder).is_absolute() {
        Ok(folder)
    } else {
        Err(format!("{name} takes an absolute path, not {folder:?}"))
    }
}

/// The owner's real home folders: `$HOME` when absolute, and the account's home from the user
/// database. A server may start the recorder with `HOME` set to the disposable home, as the
/// harness does, so `$HOME` alone would miss the real one.
fn owner_homes() -> Vec<String> {
    std::env::var("HOME")
        .ok()
        .into_iter()
        .chain(user_entry().map(|entry| entry.home))
        .filter(|home| Path::new(home).is_absolute())
        .collect()
}

fn record(args: impl IntoIterator<Item = OsString>) -> Result<i32, Failure> {
    let options = read_record_options(args).map_err(usage)?;
    // Read before any thread starts: `login_name` relies on it.
    let places = Places {
        workspace: Some(options.workspace.clone()),
        homes: std::iter::once(options.home.clone())
            .chain(owner_homes())
            .collect(),
        user: login_name(),
        host: host_name(),
    };
    let (mut file, existing) = open_to_append(&options)?;

    // The forwarded signals stay blocked until the agent's pid is stored: one that comes sooner
    // waits, then reaches the agent. Threads started meanwhile keep them blocked, so they all go
    // to the main thread.
    set_forwarded_signals_blocked(true)?;
    install_signal_forwarding()?;
    let mut agent = spawn_agent(&options).map_err(|error| {
        Failure::CannotRun(format!("cannot start {:?}: {error}", options.program))
    })?;
    let started = Instant::now();
    let agent_pid = i32::try_from(agent.id()).unwrap_or(0);
    AGENT_PID.store(agent_pid, Ordering::SeqCst);

    let mut masker = Masker::new(options.agent, places);
    let argv: Vec<String> = std::iter::once(&options.program)
        .chain(&options.args)
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    let (argv, _) = masker.argv(&argv);
    let written = write_spawn(&mut file, existing.as_ref(), &options, argv);
    let unlocked = file.unlock();
    let process = match written.and_then(|process| unlocked.map(|()| process)) {
        Ok(process) => process,
        Err(error) => {
            stop_agent(&mut agent);
            let out = options.out.display();
            return Err(Failure::CannotRun(format!("cannot write {out}: {error}")));
        }
    };

    let recorder = Arc::new(Mutex::new(Recorder {
        file,
        masker,
        process,
        started,
        closed: false,
    }));
    let output_ended = start_pumps(&mut agent, &recorder);
    set_forwarded_signals_blocked(false)?;

    let status = wait_for_agent(&mut agent)?;
    wait_for_output_to_end(&output_ended);
    lock(&recorder).write_exit(&status);
    program::exit_like(status.code(), status.signal())
}

/// Opens the recording to append to and locks it; returns it with what it holds already.
fn open_to_append(options: &RecordOptions) -> Result<(File, Option<Recording>), Failure> {
    let out = options.out.display();
    let mut file = OpenOptions::new()
        .read(true)
        .append(true)
        .create(true)
        .open(&options.out)
        .map_err(|error| Failure::CannotRun(format!("cannot open {out}: {error}")))?;
    file.lock()
        .map_err(|error| Failure::CannotRun(format!("cannot lock {out}: {error}")))?;
    let existing = read_existing(&mut file, options)
        .map_err(|message| Failure::CannotRun(format!("{out}: {message}")))?;
    Ok((file, existing))
}

/// Writes the header when the file is new, then the spawn record, in one write; returns the
/// process's number: the count of spawns the file held already.
fn write_spawn(
    file: &mut File,
    existing: Option<&Recording>,
    options: &RecordOptions,
    argv: Vec<String>,
) -> io::Result<u32> {
    let mut lines = String::new();
    if existing.is_none() {
        let mut header = Header::new(options.agent, &options.scenario, &today_utc());
        header.meta = options.meta.clone();
        lines.push_str(&format::header_line(&header));
        lines.push('\n');
    }
    let process = existing.map_or(0, count_spawns);
    lines.push_str(&format::record_line(&Record::Spawn {
        process,
        t_ms: 0,
        argv,
    }));
    lines.push('\n');
    file.write_all(lines.as_bytes())?;
    Ok(process)
}

/// Reads what the file holds already: nothing, or a recording of the same agent and scenario
/// to append a process to.
fn read_existing(file: &mut File, options: &RecordOptions) -> Result<Option<Recording>, String> {
    let mut text = String::new();
    file.read_to_string(&mut text)
        .map_err(|error| format!("cannot read it: {error}"))?;
    if text.trim().is_empty() {
        return Ok(None);
    }
    let recording =
        format::read(&text).map_err(|error| format!("not a recording to append to: {error}"))?;
    let header = &recording.header;
    if header.agent != options.agent || header.scenario != options.scenario {
        return Err(format!(
            "it records the scenario {:?} of {:?}, not {:?} of {:?}",
            header.scenario, header.agent, options.scenario, options.agent
        ));
    }
    Ok(Some(recording))
}

fn count_spawns(recording: &Recording) -> u32 {
    let spawns = recording
        .records
        .iter()
        .filter(|record| matches!(record, Record::Spawn { .. }))
        .count();
    u32::try_from(spawns).unwrap_or(u32::MAX)
}

fn spawn_agent(options: &RecordOptions) -> io::Result<Child> {
    let mut command = Command::new(&options.program);
    command
        .args(&options.args)
        .env_clear()
        .env("HOME", &options.home)
        .env("LANG", "C.UTF-8")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(path) = std::env::var_os("PATH") {
        command.env("PATH", path);
    }
    for name in &options.keep_env {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    // SAFETY: `restore_forwarded_signals` runs in the child between fork and exec, where only
    // async-signal-safe calls are allowed: it calls signal() and pthread_sigmask() on values it
    // builds on its stack, and allocates nothing.
    unsafe { command.pre_exec(restore_forwarded_signals) };
    command.spawn()
}

/// Kills an agent the recorder cannot record, and reaps it.
fn stop_agent(agent: &mut Child) {
    AGENT_PID.store(0, Ordering::SeqCst);
    // Errors mean the agent ended already; there is nothing more to stop.
    let _ = agent.kill();
    let _ = agent.wait();
}

/// The recording of one process, shared by the threads that pump its stdio.
struct Recorder {
    file: File,
    masker: Masker,
    process: u32,
    started: Instant,
    /// Set once the exit is written, or once a write failed: nothing is written after.
    closed: bool,
}

/// Where a line came from.
#[derive(Clone, Copy, PartialEq)]
enum Source {
    Client,
    Agent,
    Stderr,
}

impl Recorder {
    /// Records one line, masked: a frame when it is JSON on stdin or stdout, else a line. A
    /// blank line from the client is not recorded: a replay ignores those.
    fn write_line(&mut self, source: Source, bytes: &[u8]) {
        let bytes = bytes.strip_suffix(b"\n").unwrap_or(bytes);
        if source == Source::Client && bytes.trim_ascii().is_empty() {
            return;
        }
        let process = self.process;
        let t_ms = self.t_ms();
        let side = match source {
            Source::Client => Side::Client,
            Source::Agent | Source::Stderr => Side::Agent,
        };
        let frame = match source {
            Source::Stderr => None,
            Source::Client | Source::Agent => serde_json::from_slice::<Value>(bytes).ok(),
        };
        let record = match frame {
            Some(mut json) => {
                let masked = self.masker.frame(side, &mut json);
                Record::Frame {
                    process,
                    t_ms,
                    from: side,
                    json,
                    masked,
                }
            }
            None => {
                let (text, masked) = self.masker.line(&String::from_utf8_lossy(bytes));
                let from = match source {
                    Source::Client => Stream::Client,
                    Source::Agent => Stream::Agent,
                    Source::Stderr => Stream::Stderr,
                };
                Record::Line {
                    process,
                    t_ms,
                    from,
                    text,
                    masked,
                }
            }
        };
        self.write(&record);
    }

    fn write_stdin_closed(&mut self) {
        let record = Record::StdinClosed {
            process: self.process,
            t_ms: self.t_ms(),
        };
        self.write(&record);
    }

    fn write_exit(&mut self, status: &ExitStatus) {
        let record = Record::Exit {
            process: self.process,
            t_ms: self.t_ms(),
            code: status.code(),
            signal: status.signal(),
        };
        self.write(&record);
        self.closed = true;
    }

    /// Appends one record in a single write, so that a recorder appending another process to
    /// the same file never cuts a line in two.
    fn write(&mut self, record: &Record) {
        if self.closed {
            return;
        }
        let mut line = format::record_line(record);
        line.push('\n');
        if let Err(error) = self.file.write_all(line.as_bytes()) {
            // The agent's session goes on: stopping it would hurt the server more than a
            // recording cut short, which the owner sees here and in the missing exit.
            program::report(
                NAME,
                &format!("cannot write the recording, which stops here: {error}"),
            );
            self.closed = true;
        }
    }

    fn t_ms(&self) -> u64 {
        u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX)
    }
}

fn lock(recorder: &Mutex<Recorder>) -> MutexGuard<'_, Recorder> {
    // A pump that panicked leaves the recorder as it was between two records: still usable.
    recorder.lock().unwrap_or_else(PoisonError::into_inner)
}

/// How long the agent's output may stay open after the agent ended: a process it left running
/// may hold it open for ever.
const OUTPUT_GRACE: Duration = Duration::from_secs(2);

/// Starts the threads that pump stdin to the agent and its stdout and stderr back. Returns a
/// channel on which nothing is ever sent: it closes once both output pumps have ended, as the
/// agent's output does. The stdin pump is not waited for: it may wait on a client that keeps
/// stdin open after the agent ended.
fn start_pumps(agent: &mut Child, recorder: &Arc<Mutex<Recorder>>) -> Receiver<Infallible> {
    let (pump_running, output_ended) = mpsc::channel::<Infallible>();
    if let Some(agent_stdin) = agent.stdin.take() {
        let recorder = Arc::clone(recorder);
        thread::spawn(move || pump_stdin(agent_stdin, &recorder));
    }
    if let Some(agent_stdout) = agent.stdout.take() {
        let recorder = Arc::clone(recorder);
        let running = pump_running.clone();
        thread::spawn(move || {
            // Held until the thread ends, panicking or not.
            let _running = running;
            pump_output(agent_stdout, io::stdout(), Source::Agent, &recorder);
        });
    }
    if let Some(agent_stderr) = agent.stderr.take() {
        let recorder = Arc::clone(recorder);
        let running = pump_running;
        thread::spawn(move || {
            let _running = running;
            pump_output(agent_stderr, io::stderr(), Source::Stderr, &recorder);
        });
    }
    output_ended
}

/// Waits for the agent's output to end, for [`OUTPUT_GRACE`] at most.
fn wait_for_output_to_end(output_ended: &Receiver<Infallible>) {
    match output_ended.recv_timeout(OUTPUT_GRACE) {
        Ok(never) => match never {},
        Err(RecvTimeoutError::Disconnected) => {}
        Err(RecvTimeoutError::Timeout) => program::report(
            NAME,
            "the agent ended, but a process it left still holds its output: the recording ends \
             without what that process writes",
        ),
    }
}

fn pump_stdin(mut agent_stdin: ChildStdin, recorder: &Mutex<Recorder>) {
    let mut stdin = io::stdin().lock();
    let mut line = Vec::new();
    loop {
        line.clear();
        // A read error ends the client's input as its end does.
        match stdin.read_until(b'\n', &mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        lock(recorder).write_line(Source::Client, &line);
        if agent_stdin
            .write_all(&line)
            .and_then(|()| agent_stdin.flush())
            .is_err()
        {
            // The agent closed its stdin or ended; its exit tells the rest.
            return;
        }
    }
    lock(recorder).write_stdin_closed();
    drop(agent_stdin);
}

/// Records and forwards each line of one of the agent's outputs until it ends. Forwarding stops
/// at the first write that fails, the reader being gone, but recording goes on, so that the
/// agent never blocks on a full pipe.
fn pump_output(
    agent_output: impl Read,
    mut sink: impl Write,
    source: Source,
    recorder: &Mutex<Recorder>,
) {
    let mut reader = BufReader::new(agent_output);
    let mut line = Vec::new();
    let mut forwarding = true;
    loop {
        line.clear();
        match reader.read_until(b'\n', &mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        lock(recorder).write_line(source, &line);
        if forwarding && let Err(error) = sink.write_all(&line).and_then(|()| sink.flush()) {
            program::report(NAME, &format!("cannot forward the agent's output: {error}"));
            forwarding = false;
        }
    }
}

// --- Signals -----------------------------------------------------------------------------------

/// The agent's pid while it may be signalled: set once it is spawned, cleared before it is
/// reaped, so that a signal never reaches a process that took its pid afterwards.
static AGENT_PID: AtomicI32 = AtomicI32::new(0);

const FORWARDED_SIGNALS: [libc::c_int; 4] =
    [libc::SIGINT, libc::SIGTERM, libc::SIGHUP, libc::SIGQUIT];

/// Forwards a signal to the agent. Once the agent has ended, the recorder obeys the signal
/// itself, as it would without this handler, rather than wait on output a process the agent
/// left may hold: it then ends without an exit record.
extern "C" fn forward_to_agent(signal: libc::c_int) {
    let pid = AGENT_PID.load(Ordering::SeqCst);
    if pid > 0 {
        // SAFETY: kill is async-signal-safe and takes two integers. `pid` is the agent's and the
        // agent is not reaped yet (see `wait_for_agent`), so it names no other process; kill to
        // one's own child, alive or a zombie, does not fail, so it leaves errno as the
        // interrupted code had it.
        unsafe { libc::kill(pid, signal) };
    } else {
        // SAFETY: signal() and raise() are async-signal-safe and take plain integers. The raised
        // signal stays pending while this handler runs, then takes its default action, which
        // for each forwarded signal ends the process.
        unsafe {
            libc::signal(signal, libc::SIG_DFL);
            libc::raise(signal);
        }
    }
}

/// Forwards each signal to the agent, except one the recorder started with ignored, as a
/// shell's background job or `nohup` leaves it: the agent inherits it ignored as well, as it
/// would without the recorder.
fn install_signal_forwarding() -> io::Result<()> {
    for signal in FORWARDED_SIGNALS {
        if disposition(signal)? == libc::SIG_IGN {
            continue;
        }
        let mut action = MaybeUninit::<libc::sigaction>::zeroed();
        // SAFETY: a zeroed sigaction is a valid one (no flags, an empty mask, the default
        // handler); the handler set here only loads an atomic and calls kill, both
        // async-signal-safe. SA_RESTART lets an interrupted system call go on.
        let result = unsafe {
            let action = action.as_mut_ptr();
            (*action).sa_sigaction =
                forward_to_agent as extern "C" fn(libc::c_int) as libc::sighandler_t;
            (*action).sa_flags = libc::SA_RESTART;
            libc::sigemptyset(&mut (*action).sa_mask);
            libc::sigaction(signal, action, ptr::null_mut())
        };
        if result != 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

fn set_forwarded_signals_blocked(blocked: bool) -> io::Result<()> {
    let how = if blocked {
        libc::SIG_BLOCK
    } else {
        libc::SIG_UNBLOCK
    };
    let mut signals = MaybeUninit::<libc::sigset_t>::uninit();
    // SAFETY: sigemptyset initialises the set that sigaddset and pthread_sigmask then read; they
    // touch nothing else. The old mask is not asked for.
    let result = unsafe {
        libc::sigemptyset(signals.as_mut_ptr());
        for signal in FORWARDED_SIGNALS {
            libc::sigaddset(signals.as_mut_ptr(), signal);
        }
        libc::pthread_sigmask(how, signals.as_ptr(), ptr::null_mut())
    };
    if result != 0 {
        return Err(io::Error::from_raw_os_error(result));
    }
    Ok(())
}

/// In the agent's process, before the agent runs: gives the forwarded signals the recorder
/// handles back their default action, leaves the ignored ones ignored, and unblocks them all.
/// A child inherits the blocked mask the recorder holds around the spawn, and the agent would
/// otherwise never see the signals forwarded to it.
fn restore_forwarded_signals() -> io::Result<()> {
    for signal in FORWARDED_SIGNALS {
        if disposition(signal)? == libc::SIG_IGN {
            continue;
        }
        // SAFETY: signal() with SIG_DFL is async-signal-safe and installs no code.
        if unsafe { libc::signal(signal, libc::SIG_DFL) } == libc::SIG_ERR {
            return Err(io::Error::last_os_error());
        }
    }
    set_forwarded_signals_blocked(false)
}

/// What a signal does now: `SIG_DFL`, `SIG_IGN` or a handler's address.
fn disposition(signal: libc::c_int) -> io::Result<libc::sighandler_t> {
    let mut current = MaybeUninit::<libc::sigaction>::zeroed();
    // SAFETY: with no new action, sigaction only writes the current one into `current`, which
    // has its size. It is async-signal-safe, so the child may call it before exec.
    let result = unsafe { libc::sigaction(signal, ptr::null(), current.as_mut_ptr()) };
    if result != 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: a zeroed sigaction is already a valid one, and sigaction filled it in.
    Ok(unsafe { current.assume_init() }.sa_sigaction)
}

/// Waits for the agent to end, clears its pid while it is still a zombie, then reaps it.
fn wait_for_agent(agent: &mut Child) -> io::Result<ExitStatus> {
    let pid = agent.id();
    loop {
        let mut info = MaybeUninit::<libc::siginfo_t>::zeroed();
        // SAFETY: waitid writes only into `info`. WNOWAIT leaves the agent waitable, so its pid
        // stays its own while the signal handler may still use it.
        let result = unsafe {
            libc::waitid(
                libc::P_PID,
                pid,
                info.as_mut_ptr(),
                libc::WEXITED | libc::WNOWAIT,
            )
        };
        if result == 0 || io::Error::last_os_error().kind() != io::ErrorKind::Interrupted {
            // On another error, `wait` below says what it is.
            break;
        }
    }
    AGENT_PID.store(0, Ordering::SeqCst);
    agent.wait()
}

// --- Names and dates ---------------------------------------------------------------------------

/// The user's login name: `$USER`, or the password database's entry for this user.
fn login_name() -> Option<String> {
    if let Some(user) = std::env::var("USER").ok().filter(|user| !user.is_empty()) {
        return Some(user);
    }
    user_entry().map(|entry| entry.name)
}

/// This process's user in the user database.
struct UserEntry {
    name: String,
    home: String,
}

fn user_entry() -> Option<UserEntry> {
    // SAFETY: getuid cannot fail. getpwuid returns null or an entry in static storage that
    // stays valid until the next getpw* call; `record` and `mask-document` call this before
    // starting any thread, and the strings are copied out at once.
    unsafe {
        let entry = libc::getpwuid(libc::getuid());
        if entry.is_null() || (*entry).pw_name.is_null() || (*entry).pw_dir.is_null() {
            return None;
        }
        let text = |pointer| {
            std::ffi::CStr::from_ptr(pointer)
                .to_string_lossy()
                .into_owned()
        };
        let name = text((*entry).pw_name);
        let home = text((*entry).pw_dir);
        Some(UserEntry { name, home }).filter(|entry| !entry.name.is_empty())
    }
}

fn host_name() -> Option<String> {
    let mut buffer = [0u8; 256];
    // SAFETY: the pointer and length describe `buffer`, and gethostname writes at most that many
    // bytes into it.
    let result = unsafe { libc::gethostname(buffer.as_mut_ptr().cast(), buffer.len()) };
    if result != 0 {
        return None;
    }
    // A name that fills the buffer may lack its terminating zero.
    let length = buffer
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(buffer.len());
    let name = String::from_utf8_lossy(&buffer[..length]).into_owned();
    Some(name).filter(|name| !name.is_empty())
}

/// Today's date in UTC, `YYYY-MM-DD`.
fn today_utc() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());
    let (year, month, day) = civil_date(i64::try_from(seconds / 86_400).unwrap_or(0));
    format!("{year:04}-{month:02}-{day:02}")
}

/// The proleptic Gregorian date of a day counted from 1970-01-01 (Howard Hinnant's
/// `civil_from_days`).
fn civil_date(days: i64) -> (i64, u32, u32) {
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_from_march = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_from_march + 2) / 5 + 1;
    let month = if month_from_march < 10 {
        month_from_march + 3
    } else {
        month_from_march - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    // Both are in range by construction: a day of 1 to 31, a month of 1 to 12.
    (year, month as u32, day as u32)
}

// --- check, review, mask-document --------------------------------------------------------------

fn check(args: impl IntoIterator<Item = OsString>) -> Result<i32, Failure> {
    let files = CommandLine::parse(args, &[]).map_err(usage)?.arguments;
    if files.is_empty() {
        return Err(usage("check needs at least one file".to_owned()));
    }
    let mut stdout = io::stdout().lock();
    let mut clean = true;
    for file in files {
        let file = PathBuf::from(file);
        match check_one(&file) {
            Ok(findings) => {
                for (path, finding) in &findings {
                    print_finding(&mut stdout, path, finding)?;
                }
                clean &= findings.is_empty();
            }
            Err(message) => {
                program::report(NAME, &format!("{}: {message}", file.display()));
                clean = false;
            }
        }
    }
    Ok(if clean { 0 } else { EXIT_FOUND })
}

/// Scans a recording, and its reference when it has one, with lines as the file numbers them.
fn check_one(file: &Path) -> Result<Vec<(PathBuf, Finding)>, String> {
    let text = fs::read_to_string(file).map_err(|error| error.to_string())?;
    let recording = format::read(&text).map_err(|error| error.to_string())?;
    let file_lines = non_blank_line_numbers(&text);
    let mut findings: Vec<(PathBuf, Finding)> = scan::recording(&recording)
        .into_iter()
        .map(|mut finding| {
            finding.line = file_lines
                .get(finding.line - 1)
                .copied()
                .unwrap_or(finding.line);
            (file.to_owned(), finding)
        })
        .collect();
    if let Some(reference) = reference_of(file).filter(|reference| reference.exists()) {
        let text = fs::read_to_string(&reference)
            .map_err(|error| format!("its reference {}: {error}", reference.display()))?;
        let value: Value = serde_json::from_str(&text)
            .map_err(|error| format!("its reference {}: {error}", reference.display()))?;
        findings.extend(
            scan::document(&value)
                .into_iter()
                .map(|finding| (reference.clone(), finding)),
        );
    }
    Ok(findings)
}

fn print_finding(out: &mut impl Write, path: &Path, finding: &Finding) -> io::Result<()> {
    writeln!(
        out,
        "{}:{}: {} {}",
        path.display(),
        finding.line,
        finding.detector,
        finding.hint
    )
}

/// The file's line number of each non-blank line, in order: the reader skips blank lines, and
/// the scan counts the lines it read.
fn non_blank_line_numbers(text: &str) -> Vec<usize> {
    text.lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty())
        .map(|(index, _)| index + 1)
        .collect()
}

/// `x.jsonl`'s reference, `x.thread.json`.
fn reference_of(file: &Path) -> Option<PathBuf> {
    let name = file.file_name()?.to_str()?;
    let stem = name.strip_suffix(".jsonl")?;
    Some(file.with_file_name(format!("{stem}.thread.json")))
}

fn review(args: impl IntoIterator<Item = OsString>) -> Result<i32, Failure> {
    let files = CommandLine::parse(args, &[]).map_err(usage)?.arguments;
    let [file] = &files[..] else {
        return Err(usage("review takes one file".to_owned()));
    };
    let file = Path::new(file);
    let failed = |message: String| Failure::CannotRun(format!("{}: {message}", file.display()));
    let text = fs::read_to_string(file).map_err(|error| failed(error.to_string()))?;
    let recording = format::read(&text).map_err(|error| failed(error.to_string()))?;
    let findings = check_one(file).map_err(failed)?;
    let mut out = io::stdout().lock();
    write_review(&mut out, file, text.len(), &recording, &findings)?;
    Ok(0)
}

fn write_review(
    out: &mut impl Write,
    file: &Path,
    size: usize,
    recording: &Recording,
    findings: &[(PathBuf, Finding)],
) -> io::Result<()> {
    let header = &recording.header;
    writeln!(out, "{}: {size} bytes", file.display())?;
    writeln!(
        out,
        "{:?} speaking {:?}, scenario {:?}, recorded on {}",
        header.agent, header.protocol, header.scenario, header.recorded_on
    )?;
    for (key, value) in &header.meta {
        writeln!(out, "  meta {key} = {value}")?;
    }
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    let mut masked = 0;
    let mut exits: BTreeMap<u32, Option<String>> = BTreeMap::new();
    for record in &recording.records {
        let kind = match record {
            Record::Spawn { process, .. } => {
                exits.entry(*process).or_insert(None);
                "spawn"
            }
            Record::Frame {
                from: Side::Client, ..
            } => "client frame",
            Record::Frame {
                from: Side::Agent, ..
            } => "agent frame",
            Record::Line {
                from: Stream::Client,
                ..
            } => "client line",
            Record::Line {
                from: Stream::Agent,
                ..
            } => "agent line",
            Record::Line {
                from: Stream::Stderr,
                ..
            } => "stderr line",
            Record::Action { .. } => "action",
            Record::StdinClosed { .. } => "stdin closed",
            Record::Exit {
                process,
                code,
                signal,
                ..
            } => {
                let how = match (code, signal) {
                    (_, Some(signal)) => format!("killed by signal {signal}"),
                    (Some(code), None) => format!("exit code {code}"),
                    (None, None) => "exit".to_owned(),
                };
                exits.insert(*process, Some(how));
                "exit"
            }
        };
        *counts.entry(kind).or_default() += 1;
        if matches!(
            record,
            Record::Frame { masked: true, .. } | Record::Line { masked: true, .. }
        ) {
            masked += 1;
        }
    }
    writeln!(out, "records:")?;
    for (kind, count) in &counts {
        writeln!(out, "  {count:>6} {kind}")?;
    }
    writeln!(out, "masked frames and lines: {masked}")?;
    for (process, exit) in &exits {
        let exit = exit.as_deref().unwrap_or("no exit recorded");
        writeln!(out, "process {process}: {exit}")?;
    }
    writeln!(out, "findings: {}", findings.len())?;
    for (path, finding) in findings {
        print_finding(out, path, finding)?;
    }
    write_values_to_read(out, recording)
}

/// Keys whose strings are a conversation's text or opaque data, left out of the values to read:
/// the scan covers them, and they would bury the rest. `result` is Claude's final answer.
const UNLISTED_KEYS: [&str; 8] = [
    "text",
    "thinking",
    "content",
    "result",
    "partial_json",
    "signature",
    "data",
    "encrypted_content",
];

/// Keys of the protocols' own per-frame ids and times: the review counts their values rather than
/// listing them, since every frame brings new ones. An account's id is under no such key.
const COUNTED_KEYS: [&str; 8] = [
    "uuid",
    "timestamp",
    "request_id",
    "tool_use_id",
    "parent_tool_use_id",
    "toolUseID",
    "traceId",
    "spanId",
];

/// The paths of a message's id and of its parts' ids, counted as the keys above are.
const COUNTED_PATHS: [&str; 2] = [".message.id", ".message.content[].id"];

/// The longest a value is shown in the review.
const LISTED_VALUE_CHARS: usize = 120;

/// The values found, by frame kind and key path.
type ValuesByPlace = BTreeMap<(String, String), BTreeSet<String>>;

/// Lists every string of 8 characters or more the recording holds outside the conversation's text,
/// once each: arguments, stderr, and the frames' fields by frame kind and key path; the protocols'
/// per-frame ids and times are only counted. This is what the owner reads before accepting a
/// recording: an account's id or name that no rule knows shows here.
fn write_values_to_read(out: &mut impl Write, recording: &Recording) -> io::Result<()> {
    let mut values = ValuesByPlace::new();
    for record in &recording.records {
        match record {
            Record::Spawn { argv, .. } => {
                for arg in argv.iter().skip(1) {
                    listed_value(&mut values, "spawn", "argv", arg);
                }
            }
            Record::Line { from, text, .. } => {
                let source = match from {
                    Stream::Client => "client line",
                    Stream::Agent => "agent line",
                    Stream::Stderr => "stderr",
                };
                listed_value(&mut values, source, "", text);
            }
            Record::Frame { json, .. } if !is_streamed_text(json) => {
                let kind = frame_kind(json);
                collect_values(&mut values, &kind, "", json);
            }
            _ => {}
        }
    }
    writeln!(
        out,
        "values to read (every string of 8 characters or more outside the conversation's text):"
    )?;
    for ((kind, path), found) in &values {
        if is_counted(path) {
            writeln!(out, "  {kind} {path}: {} ids, counted", found.len())?;
            continue;
        }
        for value in found {
            writeln!(out, "  {kind} {path} = {value}")?;
        }
    }
    Ok(())
}

fn is_counted(path: &str) -> bool {
    let key = path.rsplit('.').next().unwrap_or(path);
    COUNTED_KEYS.contains(&key) || COUNTED_PATHS.contains(&path)
}

/// A Claude `stream_event`: deltas of text the scan reads joined.
fn is_streamed_text(frame: &Value) -> bool {
    frame.get("type").and_then(Value::as_str) == Some("stream_event")
}

/// A frame's kind for the review: Claude's `type` and `subtype`, ACP's method, or `response`.
fn frame_kind(frame: &Value) -> String {
    let field = |name| frame.get(name).and_then(Value::as_str);
    match (field("type"), field("subtype"), field("method")) {
        (Some(kind), Some(subtype), _) => format!("{kind}/{subtype}"),
        (Some(kind), None, _) => kind.to_owned(),
        (None, _, Some(method)) => method.to_owned(),
        (None, _, None) => "response".to_owned(),
    }
}

fn collect_values(values: &mut ValuesByPlace, kind: &str, path: &str, value: &Value) {
    match value {
        Value::String(text) => listed_value(values, kind, path, text),
        Value::Array(items) => {
            for item in items {
                collect_values(values, kind, &format!("{path}[]"), item);
            }
        }
        Value::Object(fields) => {
            for (key, field) in fields {
                if field.is_string() && UNLISTED_KEYS.contains(&key.as_str()) {
                    continue;
                }
                collect_values(values, kind, &format!("{path}.{key}"), field);
            }
        }
        _ => {}
    }
}

/// Whether a whole string is one of the recording's own placeholders, which hide nothing.
fn is_placeholder(text: &str) -> bool {
    let Some(inner) = text
        .strip_prefix('<')
        .and_then(|rest| rest.strip_suffix('>'))
    else {
        return false;
    };
    let numbered = |prefix: &str| {
        inner
            .strip_prefix(prefix)
            .is_some_and(|rest| !rest.is_empty() && rest.bytes().all(|byte| byte.is_ascii_digit()))
    };
    matches!(inner, "workspace" | "home" | "host" | "account")
        || numbered("id:")
        || inner.strip_prefix("masked:").is_some_and(|kind| {
            !kind.is_empty()
                && kind
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte == b'-')
        })
}

fn listed_value(values: &mut ValuesByPlace, kind: &str, path: &str, text: &str) {
    if text.chars().count() < 8 || is_placeholder(text) {
        return;
    }
    let mut shown: String = text.chars().take(LISTED_VALUE_CHARS).collect();
    if shown.len() < text.len() {
        shown.push('…');
    }
    let shown = shown.replace('\n', "⏎");
    values
        .entry((kind.to_owned(), path.to_owned()))
        .or_default()
        .insert(shown);
}

// --- remask ------------------------------------------------------------------------------------

/// Masks recordings again with today's rules, so that a rule added after a recording reaches it
/// without recording again. Masking an already masked recording changes only what a new rule
/// covers: placeholders, masked values and stripped fields stay as they are. Each file is
/// replaced through a temporary file and a rename.
fn remask(args: impl IntoIterator<Item = OsString>) -> Result<i32, Failure> {
    let files: Vec<OsString> = args.into_iter().collect();
    if files.is_empty() {
        return Err(usage("remask takes one file or more".to_owned()));
    }
    // Read before any work: the owner's places a recording may name.
    let places = Places {
        workspace: None,
        homes: owner_homes(),
        user: login_name(),
        host: host_name(),
    };
    for file in &files {
        let file = Path::new(file);
        let failed = |message: String| Failure::CannotRun(format!("{}: {message}", file.display()));
        let text = fs::read_to_string(file).map_err(|error| failed(error.to_string()))?;
        let mut recording = format::read(&text).map_err(|error| failed(error.to_string()))?;
        let changed = remask_records(&mut recording, &places);
        let mut out = format::header_line(&recording.header);
        out.push('\n');
        for record in &recording.records {
            out.push_str(&format::record_line(record));
            out.push('\n');
        }
        let temporary = file.with_extension("jsonl.remask");
        fs::write(&temporary, out).map_err(|error| failed(error.to_string()))?;
        fs::rename(&temporary, file).map_err(|error| failed(error.to_string()))?;
        println!("{}: {changed} records masked further", file.display());
    }
    Ok(0)
}

/// Runs every record through a masker of its process again; returns how many changed.
fn remask_records(recording: &mut Recording, places: &Places) -> usize {
    let agent = recording.header.agent;
    let mut maskers: BTreeMap<u32, Masker> = BTreeMap::new();
    let mut changed = 0;
    for record in &mut recording.records {
        let Some(process) = record.process() else {
            continue;
        };
        let masker = maskers
            .entry(process)
            .or_insert_with(|| Masker::new(agent, places.clone()));
        let before = format::record_line(record);
        match record {
            Record::Spawn { argv, .. } => *argv = masker.argv(argv).0,
            Record::Frame {
                from, json, masked, ..
            } => *masked |= masker.frame(*from, json),
            Record::Line { text, masked, .. } => {
                let (remasked, was_masked) = masker.line(text);
                *text = remasked;
                *masked |= was_masked;
            }
            Record::Action { .. } | Record::StdinClosed { .. } | Record::Exit { .. } => {}
        }
        if format::record_line(record) != before {
            changed += 1;
        }
    }
    changed
}

fn mask_document(args: impl IntoIterator<Item = OsString>) -> Result<i32, Failure> {
    let places = read_mask_document_places(args).map_err(usage)?;
    let mut text = String::new();
    io::stdin().read_to_string(&mut text)?;
    let mut value: Value = serde_json::from_str(&text)
        .map_err(|error| Failure::CannotRun(format!("stdin is not JSON: {error}")))?;
    mask::document(&places, &mut value);
    let mut stdout = io::stdout().lock();
    writeln!(stdout, "{value:#}")?;
    stdout.flush()?;
    Ok(0)
}

/// `mask-document`'s places: the folders given, all absolute, the owner's home, and the user's
/// and host's names.
fn read_mask_document_places(args: impl IntoIterator<Item = OsString>) -> Result<Places, String> {
    let command_line = CommandLine::parse(args, &["--workspace", "--home"])?;
    if !command_line.arguments.is_empty() {
        return Err("mask-document reads stdin and takes no file".to_owned());
    }
    let folder = |value: &OsStr, name: &str| absolute(program::text(value, name)?, name);
    let workspace = folder(command_line.required("--workspace")?, "--workspace")?;
    let mut homes = command_line
        .all("--home")
        .map(|home| folder(home, "--home"))
        .collect::<Result<Vec<_>, _>>()?;
    if homes.is_empty() {
        return Err("--home is missing".to_owned());
    }
    homes.extend(owner_homes());
    Ok(Places {
        workspace: Some(workspace),
        homes,
        user: login_name(),
        host: host_name(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn days_become_dates() {
        assert_eq!(civil_date(0), (1970, 1, 1));
        assert_eq!(civil_date(20_404), (2025, 11, 12));
        assert_eq!(civil_date(11_016), (2000, 2, 29));
        assert_eq!(civil_date(-1), (1969, 12, 31));
    }

    #[test]
    fn a_reference_sits_beside_its_recording() {
        assert_eq!(
            reference_of(Path::new("r/claude/turn.jsonl")),
            Some(PathBuf::from("r/claude/turn.thread.json"))
        );
        assert_eq!(reference_of(Path::new("r/claude/turn.txt")), None);
    }

    #[test]
    fn the_review_lists_values_no_rule_knows_but_not_the_conversation() {
        let recording = format::read(concat!(
            r#"{"format":"pluto-agent-recording","version":1,"agent":"claude","protocol":"claude-stream-json","recorded_on":"2026-11-12","scenario":"turn","meta":{}}"#,
            "\n",
            r#"{"kind":"frame","process":0,"t_ms":1,"from":"agent","json":{"type":"system","subtype":"init","account_uuid":"0f8fad5b-d9cb-469f-a165-70867728950e","organization_name":"Some Studio Name","session_id":"<id:1>","uuid":"6c1e7a52-0b7d-4f3e-9a41-000000000010"}}"#,
            "\n",
            r#"{"kind":"frame","process":0,"t_ms":1,"from":"agent","json":{"type":"system","subtype":"init","uuid":"6c1e7a52-0b7d-4f3e-9a41-000000000011","note":"<not-a-placeholder>"}}"#,
            "\n",
            r#"{"kind":"frame","process":0,"t_ms":2,"from":"agent","json":{"type":"assistant","message":{"content":[{"type":"text","text":"a long answer the scan reads"}]}}}"#,
            "\n",
            r#"{"kind":"frame","process":0,"t_ms":3,"from":"agent","json":{"type":"stream_event","event":{"delta":{"type":"text_delta","text":"streamed deltas stay out"}}}}"#,
        ))
        .unwrap();
        let mut out = Vec::new();
        write_values_to_read(&mut out, &recording).unwrap();
        let out = String::from_utf8(out).unwrap();
        assert!(
            out.contains("system/init .account_uuid = 0f8fad5b-d9cb-469f-a165-70867728950e"),
            "{out}"
        );
        assert!(
            out.contains("system/init .organization_name = Some Studio Name"),
            "{out}"
        );
        assert!(!out.contains("<id:1>"), "{out}");
        assert!(
            out.contains("<not-a-placeholder>"),
            "only the recording's own placeholders hide: {out}"
        );
        assert!(!out.contains("long answer"), "{out}");
        assert!(!out.contains("streamed"), "{out}");
    }

    #[test]
    fn findings_get_the_file_s_line_numbers() {
        assert_eq!(non_blank_line_numbers("h\n\nr1\n  \nr2\n"), [1, 3, 5]);
    }
}
