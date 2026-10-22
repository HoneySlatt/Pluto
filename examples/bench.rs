//! Measures the client's per-frame CPU cost (UI pass and tessellation)
//! against a server, over scripted scenarios: idle, hover, scrolling a long
//! thread, switching threads and opening settings. It never starts a
//! provider turn.
//! usage: cargo run --release --example bench -- <server> <token-file> [thread-title-prefix] [other-thread-prefix]
use std::time::{Duration, Instant};

use eframe::egui;
use t3_code_rust::app::{App, Options};
use zeroize::Zeroizing;

struct Bench {
    ctx: egui::Context,
    app: App,
    frame: eframe::Frame,
    time: f64,
    events: Vec<egui::Event>,
    /// Whether the last frame asked for another one right away.
    busy: bool,
}

#[derive(Default)]
struct Samples {
    ui: Vec<f64>,
    tessellate: Vec<f64>,
    busy: usize,
}

impl Bench {
    fn step(&mut self, samples: &mut Samples) {
        self.time += 1.0 / 60.0;
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1440.0, 900.0),
            )),
            time: Some(self.time),
            predicted_dt: 1.0 / 60.0,
            events: std::mem::take(&mut self.events),
            focused: true,
            ..Default::default()
        };
        let started = Instant::now();
        let output = self.ctx.run_ui(input, |ui| {
            eframe::App::ui(&mut self.app, ui, &mut self.frame)
        });
        let ui = started.elapsed().as_secs_f64();
        let started = Instant::now();
        let primitives = self.ctx.tessellate(output.shapes, output.pixels_per_point);
        std::hint::black_box(primitives);
        let tessellate = started.elapsed().as_secs_f64();
        self.busy = output
            .viewport_output
            .get(&egui::ViewportId::ROOT)
            .is_some_and(|v| v.repaint_delay.is_zero());
        samples.ui.push(ui);
        samples.tessellate.push(tessellate);
        samples.busy += usize::from(self.busy);
    }

    /// Runs frames, sleeping between them, until `done` holds or `limit` passes.
    fn settle(&mut self, limit: Duration, mut done: impl FnMut(&App) -> bool) {
        let started = Instant::now();
        let mut ignored = Samples::default();
        while started.elapsed() < limit {
            self.step(&mut ignored);
            if done(&self.app) {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

fn report(name: &str, samples: &Samples) {
    let stats = |values: &[f64]| {
        let mut sorted = values.to_vec();
        sorted.sort_by(f64::total_cmp);
        let at = |q: f64| sorted[((sorted.len() - 1) as f64 * q) as usize] * 1000.0;
        let mean = sorted.iter().sum::<f64>() / sorted.len() as f64 * 1000.0;
        format!(
            "mean {mean:6.2}  p50 {:6.2}  p95 {:6.2}  max {:6.2}",
            at(0.5),
            at(0.95),
            at(1.0)
        )
    };
    println!(
        "{name:<18} ui   {}  ({} frames, {} asked to repaint)",
        stats(&samples.ui),
        samples.ui.len(),
        samples.busy
    );
    println!("{:<18} tess {}", "", stats(&samples.tessellate));
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let token = std::fs::read_to_string(&args[2]).expect("token file");
    let title = args.get(3).cloned().unwrap_or("Long fil".into());
    let other = args.get(4).cloned().unwrap_or("Styles markdown".into());
    let options = Options {
        server: Some(args[1].clone()),
        token: Some(Zeroizing::new(token.trim().to_owned())),
        client_settings_path: None,
        // Benches never start provider turns.
        block_turn_starts: true,
    };
    let ctx = egui::Context::default();
    let started = Instant::now();
    let app = App::new(&eframe::CreationContext::_new_kittest(ctx.clone()), options);
    let created = started.elapsed();
    let mut bench = Bench {
        ctx,
        app,
        frame: eframe::Frame::_new_kittest(),
        time: 0.0,
        events: Vec::new(),
        busy: false,
    };
    let mut first = Samples::default();
    bench.step(&mut first);
    println!(
        "startup: App::new {:.1} ms, first frame {:.1} ms",
        created.as_secs_f64() * 1000.0,
        first.ui[0] * 1000.0
    );
    bench.settle(Duration::from_secs(15), |app| app.is_loaded());
    assert!(bench.app.open_thread_titled(&title), "no thread {title:?}");
    bench.settle(Duration::from_secs(15), |app| app.is_loaded());
    // Let images, highlighting and layout settle.
    bench.settle(Duration::from_secs(1), |_| false);

    // `BENCH_SPIN=idle|hover|scroll|settingsN` repeats one scenario for 20 s,
    // open to any same-user debugger, for sampling with `eu-stack`.
    if let Ok(spin) = std::env::var("BENCH_SPIN") {
        unsafe { libc::prctl(libc::PR_SET_PTRACER, libc::PR_SET_PTRACER_ANY) };
        eprintln!("spinning {spin}, pid {}", std::process::id());
        let started = Instant::now();
        let mut samples = Samples::default();
        let mut i = 0u32;
        while started.elapsed() < Duration::from_secs(20) {
            i += 1;
            match spin.as_str() {
                "hover" => bench.events.push(egui::Event::PointerMoved(egui::pos2(
                    500.0 + (i % 600) as f32,
                    120.0 + (i * 7 % 600) as f32,
                ))),
                "scroll" => bench.events.push(egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(
                        0.0,
                        if (i / 200).is_multiple_of(2) {
                            60.0
                        } else {
                            -60.0
                        },
                    ),
                    phase: egui::TouchPhase::Move,
                    modifiers: egui::Modifiers::NONE,
                }),
                // `settingsN` opens page N every other frame (cold layout).
                page if page.starts_with("settings") => {
                    let n = page[8..].parse().unwrap_or(0);
                    bench.app.show_settings(i.is_multiple_of(2).then_some(n));
                }
                _ => {}
            }
            bench.step(&mut samples);
        }
        report(&spin, &samples);
        return;
    }

    let mut idle = Samples::default();
    for _ in 0..120 {
        bench.step(&mut idle);
    }
    report("idle", &idle);

    let mut hover = Samples::default();
    for i in 0..240 {
        let t = i as f32 / 240.0;
        bench.events.push(egui::Event::PointerMoved(egui::pos2(
            500.0 + 600.0 * t,
            120.0 + 600.0 * (t * 7.0).fract(),
        )));
        bench.step(&mut hover);
    }
    report("hover", &hover);

    let mut scroll = Samples::default();
    for i in 0..480 {
        // Up for the first half, then back down.
        let dy = if i < 240 { 60.0 } else { -60.0 };
        bench.events.push(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, dy),
            phase: egui::TouchPhase::Move,
            modifiers: egui::Modifiers::NONE,
        });
        bench.step(&mut scroll);
    }
    report("scroll", &scroll);

    // Switches: the selection frame, then frames until the thread loads.
    let mut switch = Samples::default();
    let mut first = Samples::default();
    let mut latency = Vec::new();
    for i in 0..10 {
        let target = if i % 2 == 0 { &other } else { &title };
        bench.app.open_thread_titled(target);
        let started = Instant::now();
        bench.step(&mut first);
        while !bench.app.is_loaded() && started.elapsed() < Duration::from_secs(5) {
            bench.step(&mut switch);
            std::thread::sleep(Duration::from_millis(2));
        }
        latency.push(started.elapsed().as_secs_f64());
        for _ in 0..10 {
            bench.step(&mut switch);
        }
    }
    report("switch (1st)", &first);
    report("switch (after)", &switch);
    latency.sort_by(f64::total_cmp);
    println!(
        "{:<18} load latency median {:.1} ms, max {:.1} ms",
        "",
        latency[latency.len() / 2] * 1000.0,
        latency[latency.len() - 1] * 1000.0
    );

    let mut settings = Samples::default();
    let mut settings_first = Samples::default();
    for page in 0..10 {
        bench.app.show_settings(Some(page));
        bench.step(&mut settings_first);
        println!(
            "{:<18} page {page}: first frame {:.2} ms",
            "",
            settings_first.ui.last().unwrap() * 1000.0
        );
        let busy = settings.busy;
        for _ in 0..20 {
            bench.step(&mut settings);
        }
        println!(
            "{:<18} page {page}: {} of 20 frames asked to repaint",
            "",
            settings.busy - busy
        );
    }
    bench.app.show_settings(None);
    report("settings (1st)", &settings_first);
    report("settings", &settings);

    let mut after = Samples::default();
    for _ in 0..60 {
        bench.step(&mut after);
    }
    report("idle (end)", &after);

    // The diff panel on the fixture's large turn, its files expanded.
    if bench.app.open_thread_titled("Grand diff") {
        bench.settle(Duration::from_secs(5), |app| app.is_loaded());
        bench.app.open_last_diff();
        bench.settle(Duration::from_secs(2), |_| false);
        let expand_all = egui::pos2(1247.0, 66.0);
        bench.events.push(egui::Event::PointerMoved(expand_all));
        for pressed in [true, false] {
            bench.events.push(egui::Event::PointerButton {
                pos: expand_all,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
            bench.settle(Duration::from_millis(50), |_| false);
        }
        bench.settle(Duration::from_secs(2), |_| false);
        let mut diff = Samples::default();
        for _ in 0..120 {
            bench.step(&mut diff);
        }
        report("diff idle", &diff);
        let mut diff_scroll = Samples::default();
        for i in 0..240 {
            bench
                .events
                .push(egui::Event::PointerMoved(egui::pos2(1200.0, 450.0)));
            bench.events.push(egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, if i < 120 { -40.0 } else { 40.0 }),
                phase: egui::TouchPhase::Move,
                modifiers: egui::Modifiers::NONE,
            });
            bench.step(&mut diff_scroll);
        }
        report("diff scroll", &diff_scroll);
    }
    let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    for line in status
        .lines()
        .filter(|l| l.starts_with("VmRSS") || l.starts_with("VmHWM"))
    {
        println!("{line}");
    }
}
