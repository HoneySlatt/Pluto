//! Renders the client headlessly against a server and saves a PNG, for
//! side-by-side comparison with `scripts/reference/shot.sh`.
//! usage: cargo run --example snapshot -- <server> <token-file> <out.png> [thread-title-prefix] [--expand]
use std::time::{Duration, Instant};

use eframe::egui;
use egui_kittest::Harness;
use t3_code_rust::app::{App, Options};
use zeroize::Zeroizing;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    // A server of "-" renders the connection screen.
    let offline = args[1] == "-";
    let token = if offline {
        String::new()
    } else {
        std::fs::read_to_string(&args[2]).expect("token file")
    };
    let out = args[3].clone();
    let title = args.get(4).filter(|a| !a.starts_with("--")).cloned();
    // Values that follow flags are not titles.
    let title = title.filter(|t| !t.starts_with("--"));
    let expand = args.iter().any(|a| a == "--expand");
    let options = Options {
        server: (!offline).then(|| args[1].clone()),
        token: (!offline).then(|| Zeroizing::new(token.trim().to_owned())),
        client_settings_path: None,
    };
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1440.0, 900.0))
        .with_pixels_per_point(1.0)
        .with_step_dt(1.0 / 60.0)
        .build_eframe(|cc| App::new(cc, options));
    // Start past egui's double-click window so scripted clicks count normally.
    harness.run_steps(60);
    let started = Instant::now();
    let mut opened = title.is_none();
    while !offline && started.elapsed() < Duration::from_secs(15) {
        harness.step();
        std::thread::sleep(Duration::from_millis(50));
        let app = harness.state_mut();
        if !opened && app.is_loaded() {
            opened = app.open_thread_titled(title.as_deref().unwrap_or_default());
            continue;
        }
        if opened && app.is_loaded() {
            if expand {
                app.expand_turns();
            }
            break;
        }
    }
    // Let images load and layout settle.
    for _ in 0..20 {
        harness.step();
        std::thread::sleep(Duration::from_millis(30));
    }
    // `--click x,y` clicks once, then waits for the server to answer.
    // Interactions, in order: --click/--rclick/--dclick/--move x,y, --drag x1,y1,x2,y2,
    // --scroll dy, --type text, --key Enter|Escape.
    let mut ops = args.iter().skip(4).peekable();
    while let Some(op) = ops.next() {
        let value = match op.as_str() {
            "--click" | "--rclick" | "--dclick" | "--move" | "--drag" | "--scroll" | "--type"
            | "--key" => ops.next().cloned().unwrap_or_default(),
            _ => continue,
        };
        let point = || {
            let (x, y) = value.split_once(',').expect("x,y");
            egui::pos2(x.parse().unwrap(), y.parse().unwrap())
        };
        // Press and release land on separate frames, as with a real pointer.
        let press = |harness: &mut Harness<App>, pos, button| {
            for pressed in [true, false] {
                harness.event(egui::Event::PointerButton {
                    pos,
                    button,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                });
                harness.step();
            }
        };
        match op.as_str() {
            "--drag" => {
                let n: Vec<f32> = value.split(',').map(|v| v.parse().unwrap()).collect();
                let (from, to) = (egui::pos2(n[0], n[1]), egui::pos2(n[2], n[3]));
                harness.event(egui::Event::PointerMoved(from));
                harness.step();
                harness.event(egui::Event::PointerButton {
                    pos: from,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                });
                harness.step();
                for step in 1..=8 {
                    harness.event(egui::Event::PointerMoved(
                        from + (to - from) * (step as f32 / 8.0),
                    ));
                    harness.step();
                }
                harness.event(egui::Event::PointerButton {
                    pos: to,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                });
                harness.step();
            }
            "--click" | "--rclick" | "--dclick" => {
                let pos = point();
                harness.event(egui::Event::PointerMoved(pos));
                harness.step();
                let button = if op == "--rclick" {
                    egui::PointerButton::Secondary
                } else {
                    egui::PointerButton::Primary
                };
                press(&mut harness, pos, button);
                if op == "--dclick" {
                    harness.step();
                    press(&mut harness, pos, button);
                }
                if op != "--rclick" {
                    harness.event(egui::Event::PointerGone);
                }
            }
            "--move" => harness.event(egui::Event::PointerMoved(point())),
            // `--scroll dy` scrolls the content under the pointer down by dy points.
            "--scroll" => harness.event(egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, -value.parse::<f32>().expect("dy")),
                phase: egui::TouchPhase::Move,
                modifiers: egui::Modifiers::NONE,
            }),
            "--type" => harness.event(egui::Event::Text(value.clone())),
            "--key" => {
                let (mods, name) = value.rsplit_once('+').unwrap_or(("", value.as_str()));
                let mut modifiers = egui::Modifiers::NONE;
                if mods.contains("Ctrl") {
                    modifiers |= egui::Modifiers::COMMAND | egui::Modifiers::CTRL;
                }
                if mods.contains("Shift") {
                    modifiers |= egui::Modifiers::SHIFT;
                }
                if mods.contains("Alt") {
                    modifiers |= egui::Modifiers::ALT;
                }
                let key = egui::Key::from_name(name).expect("key name");
                for pressed in [true, false] {
                    harness.event(egui::Event::Key {
                        key,
                        physical_key: None,
                        pressed,
                        repeat: false,
                        modifiers,
                    });
                }
            }
            _ => {}
        }
        for _ in 0..30 {
            harness.step();
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    let image = harness.render().expect("render");
    image.save(&out).expect("save png");
    println!("saved {out}");
}
