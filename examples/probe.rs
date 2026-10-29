//! Dumps the config, shell and first thread snapshot of a server as JSON.
//! usage: cargo run --example probe -- <server-url> <token-file> <out-dir>
use pluto::protocol::{Action, Connection, Event, Route};
use zeroize::Zeroizing;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let token = Zeroizing::new(std::fs::read_to_string(&args[2]).expect("token file"));
    let out = std::path::PathBuf::from(&args[3]);
    std::fs::create_dir_all(&out).unwrap();
    let ctx = eframe::egui::Context::default();
    let connection = Connection::open(&args[1], token, pluto::repaint(&ctx)).expect("connect");
    let started = std::time::Instant::now();
    let mut selected = false;
    let mut count = 0;
    while started.elapsed().as_secs() < 6 {
        match connection
            .events
            .recv_timeout(std::time::Duration::from_millis(200))
        {
            Ok(Event::Data(route, value)) => {
                count += 1;
                let name = match &route {
                    Route::Config => "config".to_owned(),
                    Route::Shell => "shell".to_owned(),
                    Route::Thread(_) => "thread".to_owned(),
                    _ => "other".to_owned(),
                };
                std::fs::write(
                    out.join(format!("{count:03}-{name}.json")),
                    serde_json::to_string_pretty(&value).unwrap(),
                )
                .unwrap();
                if route == Route::Shell && !selected {
                    let wanted = args.get(4).cloned().unwrap_or_default();
                    let thread = value["snapshot"]["threads"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|t| t["title"].as_str().unwrap_or("").starts_with(&wanted))
                        .cloned();
                    if let Some(id) = thread.as_ref().and_then(|t| t["id"].as_str()) {
                        connection.send(Action::Select(Some(id.to_owned())));
                        selected = true;
                    }
                }
            }
            Ok(Event::Failed(route, error)) => eprintln!("failed {route:?}: {error}"),
            Ok(Event::Disconnected(error)) => {
                eprintln!("disconnected: {error}");
                break;
            }
            Ok(Event::Slow(requests)) => eprintln!("{} slow requests", requests.len()),
            Err(_) => {}
        }
    }
    println!("{count} events");
}
