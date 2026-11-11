//! Starts turns on a server the way a client does, for the rivals' bench (scripts/rivals/,
//! docs/adr/0026-the-rivals-measured-how.md): it launches threads in the server's first project,
//! each with a first message that names a turn of the fake model's script, then exits once the
//! server has accepted them all. It waits for nothing else: the bench reads the fake model's
//! journal to know when the turns are done.
//!
//! usage: drive <server> <token-file> <instance> <model> <threads> <turn>...
//!
//! Thread `n` (from 0) gets `[turn T]` with T the `n`-th of the turns given, cycling.
use std::sync::mpsc::RecvTimeoutError;
use std::time::Duration;

use pluto_core::Repaint;
use pluto_core::commands::launch_input;
use pluto_core::protocol::{Action, Connection, Event, Route};
use serde_json::json;
use zeroize::Zeroizing;

const PATIENCE: Duration = Duration::from_secs(60);

fn main() {
    if let Err(error) = run() {
        eprintln!("drive: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [server, token_file, instance, model, threads, turns @ ..] = args.as_slice() else {
        return Err(
            "usage: drive <server> <token-file> <instance> <model> <threads> <turn>...".into(),
        );
    };
    let threads: usize = threads.parse().map_err(|_| "<threads> is not a number")?;
    if turns.is_empty() {
        return Err("name at least one turn".into());
    }
    let token = std::fs::read_to_string(token_file).map_err(|e| format!("{token_file}: {e}"))?;
    let connection = Connection::open(
        server,
        Zeroizing::new(token.trim().to_owned()),
        Repaint::new(|_| {}),
    )?;
    let project = first_project(&connection)?;
    let selection = json!({"instanceId": instance, "model": model});
    for n in 0..threads {
        let turn = &turns[n % turns.len()];
        let thread_id = uuid::Uuid::new_v4().to_string();
        let title = format!("Bench {:02}", n + 1);
        let text = format!("[turn {turn}] Look at this project and tell me what you find.");
        let input = launch_input(
            &thread_id,
            &project,
            &title,
            &text,
            &selection,
            "full-access",
            "default",
        );
        connection.send(Action::Request {
            key: format!("launch-{n}"),
            method: "orchestration.launchThread",
            payload: input,
        });
    }
    let mut accepted = 0;
    while accepted < threads {
        match next_event(&connection)? {
            Event::Data(Route::Rpc(key), _) if key.starts_with("launch-") => accepted += 1,
            Event::Failed(Route::Rpc(key), error) if key.starts_with("launch-") => {
                return Err(format!("{key} refused: {error}"));
            }
            Event::Disconnected(error) => return Err(format!("disconnected: {error}")),
            _ => {}
        }
    }
    println!("drive: {accepted} threads launched in project {project}");
    Ok(())
}

/// The id of the server's first project: from the shell's snapshot, or, when a server adds its
/// project after its first snapshot, from the `project.updated` that follows.
fn first_project(connection: &Connection) -> Result<String, String> {
    loop {
        let Event::Data(Route::Shell, value) = next_event(connection)? else {
            continue;
        };
        let id = match value["kind"].as_str() {
            Some("snapshot") => value["snapshot"]["projects"][0]["id"].as_str(),
            Some("project.updated") => value["project"]["id"].as_str(),
            _ => None,
        };
        if let Some(id) = id {
            return Ok(id.to_owned());
        }
    }
}

fn next_event(connection: &Connection) -> Result<Event, String> {
    match connection.events.recv_timeout(PATIENCE) {
        Ok(event) => Ok(event),
        Err(RecvTimeoutError::Timeout) => Err("the server stopped answering".into()),
        Err(RecvTimeoutError::Disconnected) => Err("the connection ended".into()),
    }
}
