//! `pluto-input`: the bench's virtual keyboard and pointer, driven by commands read from a pipe.
//!
//! ```text
//! pluto-input --commands FIFO --size WIDTHxHEIGHT
//! ```
//!
//! It connects to the compositor `WAYLAND_DISPLAY` names, creates one virtual keyboard (its keymap
//! sent once) and one virtual pointer on the seat, prints `pluto-input: ready` on stderr, then
//! plays each line of `FIFO` ([`pluto_input`] lists the commands) until it is killed. `FIFO` is
//! opened for reading and writing, so that a writer closing it is no end: the tool waits, blocked,
//! for the next command, and wakes for nothing in between. `--size` is the output's size in
//! pixels, the extent of the pointer's positions. Exit codes: 2 for a bad command line, 1 when the
//! compositor cannot be reached or goes away.

use pluto_input::{Button, Command, DEFAULT_DELAY, KEYMAP, Stroke, parse_line, parse_size};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::os::fd::AsFd;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};
use wayland_client::globals::{BindError, GlobalListContents, registry_queue_init};
use wayland_client::protocol::wl_pointer::ButtonState;
use wayland_client::protocol::wl_registry::WlRegistry;
use wayland_client::protocol::wl_seat::WlSeat;
use wayland_client::{Connection, Dispatch, EventQueue, QueueHandle, delegate_noop};
use wayland_protocols_misc::zwp_virtual_keyboard_v1::client::zwp_virtual_keyboard_manager_v1::ZwpVirtualKeyboardManagerV1;
use wayland_protocols_misc::zwp_virtual_keyboard_v1::client::zwp_virtual_keyboard_v1::ZwpVirtualKeyboardV1;
use wayland_protocols_wlr::virtual_pointer::v1::client::zwlr_virtual_pointer_manager_v1::ZwlrVirtualPointerManagerV1;
use wayland_protocols_wlr::virtual_pointer::v1::client::zwlr_virtual_pointer_v1::ZwlrVirtualPointerV1;

/// `wl_keyboard`'s keymap format for XKB's text form.
const KEYMAP_FORMAT_XKB_V1: u32 = 1;
const KEY_RELEASED: u32 = 0;
const KEY_PRESSED: u32 = 1;

/// The tool keeps no state of its own from the compositor's events: it only sends.
struct State;

impl Dispatch<WlRegistry, GlobalListContents> for State {
    fn event(
        _: &mut Self,
        _: &WlRegistry,
        _: <WlRegistry as wayland_client::Proxy>::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

delegate_noop!(State: ignore WlSeat);
delegate_noop!(State: ZwpVirtualKeyboardManagerV1);
delegate_noop!(State: ZwpVirtualKeyboardV1);
delegate_noop!(State: ZwlrVirtualPointerManagerV1);
delegate_noop!(State: ZwlrVirtualPointerV1);

fn bind_error(interface: &'static str) -> impl Fn(BindError) -> String {
    move |error| format!("the compositor offers no {interface}: {error}")
}

/// The keyboard and the pointer, for the whole run.
struct Devices {
    queue: EventQueue<State>,
    keyboard: ZwpVirtualKeyboardV1,
    pointer: ZwlrVirtualPointerV1,
    started: Instant,
    /// The pause after each key.
    delay: Duration,
    /// The output's size, the extent of the pointer's positions.
    size: (u32, u32),
}

impl Devices {
    fn connect(size: (u32, u32)) -> Result<Devices, String> {
        let connection = Connection::connect_to_env()
            .map_err(|error| format!("cannot reach the compositor: {error}"))?;
        let (globals, queue) = registry_queue_init::<State>(&connection)
            .map_err(|error| format!("cannot list the compositor's globals: {error}"))?;
        let handle = queue.handle();
        let seat: WlSeat = globals
            .bind(&handle, 1..=1, ())
            .map_err(bind_error("wl_seat"))?;
        let keyboards: ZwpVirtualKeyboardManagerV1 = globals
            .bind(&handle, 1..=1, ())
            .map_err(bind_error("zwp_virtual_keyboard_manager_v1"))?;
        let pointers: ZwlrVirtualPointerManagerV1 = globals
            .bind(&handle, 1..=1, ())
            .map_err(bind_error("zwlr_virtual_pointer_manager_v1"))?;
        let keyboard = keyboards.create_virtual_keyboard(&seat, &handle, ());
        let pointer = pointers.create_virtual_pointer(Some(&seat), &handle, ());
        let mut devices = Devices {
            queue,
            keyboard,
            pointer,
            started: Instant::now(),
            delay: DEFAULT_DELAY,
            size,
        };
        devices.send_keymap()?;
        Ok(devices)
    }

    /// Sends the keymap through a file, which the compositor maps; the file is removed once the
    /// compositor has read it. Its text ends with a NUL, as the compositor reads a C string.
    fn send_keymap(&mut self) -> Result<(), String> {
        let folder = std::env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let path = folder.join(format!("pluto-input-keymap-{}", std::process::id()));
        let result = (|| {
            // Readable too: the compositor maps it for reading, and refuses a file it cannot map.
            let mut file = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(true)
                .open(&path)
                .map_err(|error| format!("cannot write the keymap: {error}"))?;
            file.write_all(KEYMAP.as_bytes())
                .and_then(|()| file.write_all(&[0]))
                .map_err(|error| format!("cannot write the keymap: {error}"))?;
            let size = u32::try_from(KEYMAP.len() + 1).expect("the keymap is a few hundred bytes");
            self.keyboard
                .keymap(KEYMAP_FORMAT_XKB_V1, file.as_fd(), size);
            self.sync()
        })();
        let _ = std::fs::remove_file(&path);
        result
    }

    /// Milliseconds since the start, the events' time; it wraps after 49 days, as the protocol's.
    fn now(&self) -> u32 {
        (self.started.elapsed().as_millis() % (1 << 32)) as u32
    }

    /// Waits until the compositor has handled every request sent so far.
    fn sync(&mut self) -> Result<(), String> {
        self.queue
            .roundtrip(&mut State)
            .map(|_| ())
            .map_err(|error| format!("the compositor went away: {error}"))
    }

    fn flush(&self) -> Result<(), String> {
        self.queue
            .flush()
            .map_err(|error| format!("the compositor went away: {error}"))
    }

    /// Presses a key with its modifiers held, then releases them all. The compositor does not
    /// derive the modifiers' state from the keys of a virtual keyboard: it is sent with them.
    fn stroke(&mut self, stroke: &Stroke) -> Result<(), String> {
        let mut mask = 0;
        for modifier in &stroke.modifiers {
            self.keyboard
                .key(self.now(), modifier.keycode(), KEY_PRESSED);
            mask |= modifier.mask();
            self.keyboard.modifiers(mask, 0, 0, 0);
        }
        self.keyboard.key(self.now(), stroke.keycode, KEY_PRESSED);
        self.keyboard.key(self.now(), stroke.keycode, KEY_RELEASED);
        for modifier in stroke.modifiers.iter().rev() {
            self.keyboard
                .key(self.now(), modifier.keycode(), KEY_RELEASED);
            mask &= !modifier.mask();
            self.keyboard.modifiers(mask, 0, 0, 0);
        }
        self.flush()?;
        std::thread::sleep(self.delay);
        Ok(())
    }

    fn move_to(&mut self, x: u32, y: u32) -> Result<(), String> {
        let (width, height) = self.size;
        self.pointer
            .motion_absolute(self.now(), x.min(width), y.min(height), width, height);
        self.pointer.frame();
        self.flush()
    }

    fn click(&mut self, button: Button) -> Result<(), String> {
        self.pointer
            .button(self.now(), button.code(), ButtonState::Pressed);
        self.pointer.frame();
        self.pointer
            .button(self.now(), button.code(), ButtonState::Released);
        self.pointer.frame();
        self.flush()
    }

    /// Plays one command. An error is the compositor's: the run cannot go on.
    fn play(&mut self, command: Command) -> Result<(), String> {
        match command {
            Command::Type(strokes) | Command::Keys(strokes) => {
                for stroke in &strokes {
                    self.stroke(stroke)?;
                }
            }
            Command::Delay(delay) => self.delay = delay,
            Command::Sleep(duration) => std::thread::sleep(duration),
            Command::Move { x, y } => self.move_to(x, y)?,
            Command::Click(button) => self.click(button)?,
            Command::Mark(path) => {
                self.sync()?;
                if let Err(error) = File::create(&path) {
                    eprintln!("pluto-input: cannot create {}: {error}", path.display());
                }
            }
        }
        Ok(())
    }
}

struct Options {
    commands: PathBuf,
    size: (u32, u32),
}

fn parse_options() -> Result<Options, String> {
    let mut commands = None;
    let mut size = None;
    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        let value = args
            .next()
            .ok_or_else(|| format!("{} needs a value", arg.to_string_lossy()))?;
        match arg.to_str() {
            Some("--commands") => commands = Some(PathBuf::from(value)),
            Some("--size") => {
                size = Some(parse_size(
                    value.to_str().ok_or("the size is not valid Unicode")?,
                )?);
            }
            _ => return Err(format!("unknown option {}", arg.to_string_lossy())),
        }
    }
    Ok(Options {
        commands: commands.ok_or("--commands is required")?,
        size: size.ok_or("--size is required")?,
    })
}

/// Opens the commands' pipe for reading and writing: with a writer of its own, it never reaches
/// its end, however many writers come and go.
fn open_pipe(commands: &Path) -> Result<File, String> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .open(commands)
        .map_err(|error| format!("cannot open {}: {error}", commands.display()))
}

/// Plays every line of the pipe, for as long as the compositor lives. A line that is not valid
/// UTF-8 is reported and skipped, as any command that cannot be read.
fn serve(pipe: File, devices: &mut Devices) -> Result<(), String> {
    let mut reader = BufReader::new(pipe);
    let mut bytes = Vec::new();
    loop {
        bytes.clear();
        let read = reader
            .read_until(b'\n', &mut bytes)
            .map_err(|error| format!("cannot read the commands: {error}"))?;
        if read == 0 {
            return Ok(());
        }
        let line = String::from_utf8_lossy(&bytes);
        match parse_line(&line) {
            Ok(Some(command)) => devices.play(command)?,
            Ok(None) => {}
            Err(message) => eprintln!("pluto-input: {line:?}: {message}; skipped"),
        }
    }
}

fn main() -> ExitCode {
    let options = match parse_options() {
        Ok(options) => options,
        Err(message) => {
            eprintln!("pluto-input: {message}");
            eprintln!("usage: pluto-input --commands FIFO --size WIDTHxHEIGHT");
            return ExitCode::from(2);
        }
    };
    // Ready once both the devices and the pipe are: a command written then is played.
    let result = Devices::connect(options.size).and_then(|mut devices| {
        let pipe = open_pipe(&options.commands)?;
        eprintln!("pluto-input: ready");
        serve(pipe, &mut devices)
    });
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("pluto-input: {message}");
            ExitCode::FAILURE
        }
    }
}
