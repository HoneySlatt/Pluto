//! `pluto-fake-model`: serves a script until stopped.
//!
//! ```text
//! pluto-fake-model --script FILE [--listen ADDR]... [--journal FILE] [--speed N|max]
//!                  [--model NAME]...
//! ```
//!
//! `--listen` defaults to `127.0.0.1:11435`; give it twice for IPv4 and IPv6 (`[::1]:11435`).
//! `--speed` defaults to 1: a finite number above zero, or `max`, which waits for nothing.
//! `--model` defaults to `fake-model`. It prints `pluto-fake-model: listening on http://ADDR` on
//! stderr for each address, then serves until killed. Exit codes: 2 for a bad command line, 1 for a script or a listener that fails.

use pluto_fake_model::script::Script;
use pluto_fake_model::server::{self, Config};
use std::ffi::OsString;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::ExitCode;

const DEFAULT_LISTEN: &str = "127.0.0.1:11435";
const DEFAULT_MODEL: &str = "fake-model";

/// The command line, read and checked.
struct Options {
    script: PathBuf,
    listen: Vec<SocketAddr>,
    journal: Option<PathBuf>,
    speed: f64,
    models: Vec<String>,
}

impl Options {
    /// Reads the arguments; the error says what is wrong with them.
    fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Options, String> {
        let mut script = None;
        let mut listen = Vec::new();
        let mut journal = None;
        let mut speed = 1.0;
        let mut models = Vec::new();
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            let name = arg.to_str().ok_or("an option name is not valid Unicode")?;
            let mut value = || args.next().ok_or(format!("{name} needs a value"));
            match name {
                "--script" => script = Some(PathBuf::from(value()?)),
                "--listen" => listen.push(parse_address(&text(value()?, name)?)?),
                "--journal" => journal = Some(PathBuf::from(value()?)),
                "--speed" => speed = parse_speed(&text(value()?, name)?)?,
                "--model" => models.push(text(value()?, name)?),
                _ => return Err(format!("unknown option {name}")),
            }
        }
        if listen.is_empty() {
            listen.push(parse_address(DEFAULT_LISTEN)?);
        }
        if models.is_empty() {
            models.push(DEFAULT_MODEL.to_owned());
        }
        Ok(Options {
            script: script.ok_or("--script is required")?,
            listen,
            journal,
            speed,
            models,
        })
    }
}

fn text(value: OsString, option: &str) -> Result<String, String> {
    value
        .into_string()
        .map_err(|_| format!("the value of {option} is not valid Unicode"))
}

fn parse_address(text: &str) -> Result<SocketAddr, String> {
    text.parse()
        .map_err(|_| format!("{text:?} is not an address like 127.0.0.1:11435 or [::1]:11435"))
}

/// `max`, or a finite number above zero.
fn parse_speed(text: &str) -> Result<f64, String> {
    if text == "max" {
        return Ok(f64::INFINITY);
    }
    match text.parse::<f64>() {
        Ok(speed) if speed.is_finite() && speed > 0.0 => Ok(speed),
        _ => Err(format!(
            "{text:?} is not a speed: give a finite number above zero, or max"
        )),
    }
}

/// Reads the script and starts listening.
fn start(options: Options) -> Result<server::Handle, String> {
    let text = std::fs::read_to_string(&options.script)
        .map_err(|error| format!("cannot read {}: {error}", options.script.display()))?;
    let script = Script::from_json(&text)
        .map_err(|error| format!("{}: {error}", options.script.display()))?;
    let config = Config {
        script,
        speed: options.speed,
        journal_path: options.journal,
        models: options.models,
        connection_timeout: server::CONNECTION_TIMEOUT,
    };
    server::spawn(config, &options.listen).map_err(|error| format!("cannot listen: {error}"))
}

fn main() -> ExitCode {
    let options = match Options::parse(std::env::args_os().skip(1)) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("pluto-fake-model: {message}");
            eprintln!(
                "usage: pluto-fake-model --script FILE [--listen ADDR]... [--journal FILE] [--speed N|max] [--model NAME]..."
            );
            return ExitCode::from(2);
        }
    };
    let handle = match start(options) {
        Ok(handle) => handle,
        Err(message) => {
            eprintln!("pluto-fake-model: {message}");
            return ExitCode::FAILURE;
        }
    };
    for addr in &handle.addrs {
        eprintln!("pluto-fake-model: listening on http://{addr}");
    }
    // The server's threads do the work; this one waits to be killed.
    loop {
        std::thread::park();
    }
}
