//! What the crate's two programs, `pluto-record` and `pluto-replay-agent`, share: reading their
//! command line, and ending like the agent's process they stand for. Not meant for other users.

use std::ffi::{OsStr, OsString};
use std::io::Write;
use std::mem::MaybeUninit;
use std::ptr;

/// A command line: `--name value` options, and the other arguments, in order. Everything after
/// `--` is an argument, whatever it looks like.
#[derive(Debug, Default)]
pub struct CommandLine {
    options: Vec<(String, OsString)>,
    pub arguments: Vec<OsString>,
}

impl CommandLine {
    /// Reads `args`, refusing an option not in `names` and an option without its value.
    pub fn parse(
        args: impl IntoIterator<Item = OsString>,
        names: &[&str],
    ) -> Result<CommandLine, String> {
        let mut command_line = CommandLine::default();
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            if arg == "--" {
                command_line.arguments.extend(args);
                break;
            }
            match arg.to_str() {
                Some(name) if name.starts_with("--") => {
                    if !names.contains(&name) {
                        return Err(format!("unknown option {name}"));
                    }
                    let value = args.next().ok_or_else(|| format!("{name} needs a value"))?;
                    command_line.options.push((name.to_owned(), value));
                }
                _ => command_line.arguments.push(arg),
            }
        }
        Ok(command_line)
    }

    /// Every value given to an option that may repeat.
    pub fn all(&self, name: &str) -> impl Iterator<Item = &OsStr> {
        self.options
            .iter()
            .filter(move |(option, _)| option == name)
            .map(|(_, value)| value.as_os_str())
    }

    /// The value of an option given at most once.
    pub fn optional(&self, name: &str) -> Result<Option<&OsStr>, String> {
        match self.all(name).collect::<Vec<_>>()[..] {
            [] => Ok(None),
            [value] => Ok(Some(value)),
            _ => Err(format!("{name} is given twice")),
        }
    }

    /// The value of an option given exactly once.
    pub fn required(&self, name: &str) -> Result<&OsStr, String> {
        self.optional(name)?
            .ok_or_else(|| format!("{name} is missing"))
    }
}

/// An argument as text; the formats and the masker work on text.
pub fn text(value: &OsStr, what: &str) -> Result<String, String> {
    value
        .to_str()
        .map(str::to_owned)
        .ok_or_else(|| format!("{what} is not UTF-8: {value:?}"))
}

/// Writes one line on stderr for the owner. When stderr itself is gone there is nowhere left to
/// report to, so a failure is dropped.
pub fn report(program: &str, message: &str) {
    let _ = writeln!(std::io::stderr(), "{program}: {message}");
}

/// Signals that stop or continue a process rather than end it.
const STOP_AND_CONTINUE_SIGNALS: [libc::c_int; 5] = [
    libc::SIGSTOP,
    libc::SIGTSTP,
    libc::SIGTTIN,
    libc::SIGTTOU,
    libc::SIGCONT,
];
/// The highest signal number Linux has.
const LAST_SIGNAL: libc::c_int = 64;

/// Ends this process the way the agent's ended: with its exit code, or killed by its signal. A
/// signal that cannot end it so (one that stops or continues, one ignored by default, a number
/// no signal has, as a malformed recording may hold) ends it with the code a shell reports.
pub fn exit_like(code: Option<i32>, signal: Option<i32>) -> ! {
    let Some(signal) = signal else {
        std::process::exit(code.unwrap_or(0));
    };
    if (1..=LAST_SIGNAL).contains(&signal) && !STOP_AND_CONTINUE_SIGNALS.contains(&signal) {
        raise_with_its_default_action(signal);
    }
    std::process::exit(code_for_signal(signal))
}

/// Raises a signal on this thread with its default action, unblocked. Returns only when that
/// action does not end the process.
fn raise_with_its_default_action(signal: libc::c_int) {
    let mut unblocked = MaybeUninit::<libc::sigset_t>::uninit();
    // SAFETY: sigemptyset initialises the set that sigaddset and pthread_sigmask then read; all
    // three only touch `unblocked`. `signal` is a real signal number (the caller checks it), and
    // signal() with SIG_DFL installs no Rust code; raise sends it to this thread.
    unsafe {
        libc::sigemptyset(unblocked.as_mut_ptr());
        libc::sigaddset(unblocked.as_mut_ptr(), signal);
        libc::signal(signal, libc::SIG_DFL);
        libc::pthread_sigmask(libc::SIG_UNBLOCK, unblocked.as_ptr(), ptr::null_mut());
        libc::raise(signal);
    }
}

/// The exit code a shell reports for a signal: 128 plus its number, at most `i32::MAX`; 1 for a
/// number no signal has.
fn code_for_signal(signal: i32) -> i32 {
    if signal <= 0 {
        1
    } else {
        128_i32.saturating_add(signal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<CommandLine, String> {
        CommandLine::parse(args.iter().map(OsString::from), &["--home", "--out"])
    }

    #[test]
    fn options_repeat_and_everything_after_the_separator_is_an_argument() {
        let command_line =
            parse(&["a", "--home", "h1", "--home", "h2", "--", "--out", "x"]).unwrap();
        assert_eq!(command_line.all("--home").collect::<Vec<_>>(), ["h1", "h2"]);
        assert_eq!(command_line.arguments, ["a", "--out", "x"]);
        assert!(command_line.optional("--home").is_err());
        assert_eq!(command_line.optional("--out"), Ok(None));
        assert!(command_line.required("--out").is_err());
    }

    #[test]
    fn a_signal_becomes_a_shell_s_code_without_overflow() {
        assert_eq!(code_for_signal(libc::SIGSTOP), 128 + libc::SIGSTOP);
        assert_eq!(code_for_signal(i32::MAX), i32::MAX);
        assert_eq!(code_for_signal(0), 1);
        assert_eq!(code_for_signal(-9), 1);
    }

    #[test]
    fn an_unknown_option_and_a_missing_value_are_refused() {
        assert_eq!(parse(&["--nope"]).unwrap_err(), "unknown option --nope");
        assert_eq!(parse(&["--out"]).unwrap_err(), "--out needs a value");
    }
}
