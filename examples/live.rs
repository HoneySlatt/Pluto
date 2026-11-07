//! Runs the client in a real window, as `pluto` does, for what a bench
//! without a window cannot measure: the start, and the memory and wake-ups
//! at rest (scripts/measure.sh, docs/measurements.md). It never starts a
//! provider turn, and writes no settings.
//!
//! usage: live <server> <token-file> start
//!        live <server> <token-file> rest [seconds]
//!
//! `start` exits after the first useful frame: the first one drawn with the
//! server's projects and threads. It prints when that frame was painted,
//! after `main` began. The part before `main` is hyperfine's time for `live`
//! without arguments, which exits at once.
//!
//! `rest` opens the fixture's 15 "Flux" threads and its 5,000-message thread
//! in turn, lets them settle for 5 seconds, then rests for `seconds` (60 by
//! default). It counts the frames drawn meanwhile, prints its memory, and
//! exits. One wake-up of the rest is its own: the timer that ends it.
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::{Duration, Instant};

use eframe::egui;
use pluto::app::{App, Options};
use zeroize::Zeroizing;

/// The threads `rest` opens, in order: the client keeps each one it left
/// in the last five minutes, so all of them are held at rest.
fn threads_to_open() -> Vec<String> {
    let mut titles: Vec<String> = (1..=15).map(|n| format!("Flux {n:02}")).collect();
    titles.push("Fil de 5 000".into());
    titles
}

enum Phase {
    /// Waiting for the shell; `start` exits on the frame after it shows.
    Connecting,
    /// Opening `threads_to_open()[index]`, waiting for it to load.
    Opening(usize),
    /// Images, highlighting and layout finishing.
    Settling,
    Resting {
        since: Instant,
        frames: u64,
    },
}

const SETTLING: u8 = 0;
const RESTING: u8 = 1;
const OVER: u8 = 2;

struct Live {
    app: App,
    rest: Option<Duration>,
    phase: Phase,
    /// When the first useful frame began, after `main` (`start`).
    useful_frame: Option<Duration>,
    /// Where the timer of `rest` is: `SETTLING`, `RESTING` or `OVER`.
    clock: Arc<AtomicU8>,
    began: Instant,
    /// When eframe had the window and the GPU ready, and called `App::new`.
    ready: Duration,
}

impl eframe::App for Live {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if let Some(useful) = self.useful_frame {
            // The useful frame's CPU work, its UI pass and its painting, is
            // what eframe reports now for the frame before. How long the
            // compositor then takes to show it is not counted: a headless
            // weston waits about 180 ms before the next frame.
            let painted = useful + Duration::from_secs_f32(frame.info().cpu_usage.unwrap_or(0.0));
            println!(
                "window and GPU ready {:.1} ms, first useful frame painted {:.1} ms after main",
                self.ready.as_secs_f64() * 1000.0,
                painted.as_secs_f64() * 1000.0
            );
            std::process::exit(0);
        }
        if let Phase::Resting { frames, .. } = &mut self.phase {
            *frames += 1;
        }
        let frame_began = self.began.elapsed();
        self.app.ui(ui, frame);
        match &mut self.phase {
            Phase::Connecting if self.app.is_loaded() => match self.rest {
                None => {
                    self.useful_frame = Some(frame_began);
                    ctx.request_repaint();
                }
                Some(_) => self.open(0, &ctx),
            },
            Phase::Opening(index) if self.app.is_loaded() => {
                let next = *index + 1;
                self.open(next, &ctx);
            }
            Phase::Settling if self.clock.load(Ordering::Relaxed) == RESTING => {
                eprintln!("resting, pid {}", std::process::id());
                self.phase = Phase::Resting {
                    since: Instant::now(),
                    frames: 0,
                };
            }
            Phase::Resting { since, frames } if self.clock.load(Ordering::Relaxed) == OVER => {
                println!(
                    "rest: {frames} frames in {:.1} s",
                    since.elapsed().as_secs_f64()
                );
                print_memory();
                std::process::exit(0);
            }
            _ => {}
        }
        if matches!(self.phase, Phase::Connecting | Phase::Opening(_)) {
            // Loading is driven by the connection's events, which repaint;
            // this only bounds a wait the script could otherwise miss.
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }
}

impl Live {
    fn open(&mut self, index: usize, ctx: &egui::Context) {
        let titles = threads_to_open();
        if let Some(title) = titles.get(index) {
            assert!(self.app.open_thread_titled(title), "no thread {title:?}");
            self.phase = Phase::Opening(index);
            return;
        }
        let rest = self.rest.unwrap_or_default();
        let clock = self.clock.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            for (wait, stage) in [(Duration::from_secs(5), RESTING), (rest, OVER)] {
                std::thread::sleep(wait);
                clock.store(stage, Ordering::Relaxed);
                ctx.request_repaint();
            }
        });
        self.phase = Phase::Settling;
    }
}

/// The resident memory and the threads, from `/proc/self/status`.
fn print_memory() {
    let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    for line in status.lines().filter(|l| {
        [
            "VmRSS", "VmHWM", "RssAnon", "RssFile", "RssShmem", "Threads",
        ]
        .iter()
        .any(|key| l.starts_with(key))
    }) {
        println!("{line}");
    }
}

fn main() -> eframe::Result<()> {
    let began = Instant::now();
    let args: Vec<String> = std::env::args().collect();
    // Without arguments it exits at once: hyperfine's time for it is the
    // start's part before `main`, the binary loaded and linked.
    if args.len() < 4 {
        eprintln!("usage: live <server> <token-file> start|rest [seconds]");
        std::process::exit(2);
    }
    let token = std::fs::read_to_string(&args[2]).expect("token file");
    let rest = match args[3].as_str() {
        "start" => None,
        "rest" => Some(Duration::from_secs(
            args.get(4).and_then(|s| s.parse().ok()).unwrap_or(60),
        )),
        other => panic!("unknown mode {other:?}"),
    };
    let options = Options {
        server: Some(args[1].clone()),
        token: Some(Zeroizing::new(token.trim().to_owned())),
        client_settings_path: None,
        block_turn_starts: true,
    };
    // As `pluto` opens its window the first time (src/main.rs).
    let viewport = egui::ViewportBuilder::default()
        .with_title("Pluto")
        .with_app_id("pluto-measure")
        .with_inner_size([1440.0, 900.0]);
    eframe::run_native(
        "pluto-measure",
        eframe::NativeOptions {
            viewport,
            wgpu_options: eframe::egui_wgpu::WgpuConfiguration {
                surface: eframe::egui_wgpu::SurfaceConfig::LOW_LATENCY,
                ..Default::default()
            },
            ..Default::default()
        },
        Box::new(move |cc| {
            let ready = began.elapsed();
            Ok(Box::new(Live {
                ready,
                app: App::new(cc, options),
                rest,
                phase: Phase::Connecting,
                useful_frame: None,
                clock: Arc::new(AtomicU8::new(SETTLING)),
                began,
            }))
        }),
    )
}
