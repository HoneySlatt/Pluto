//! Interaction checks against synthetic data, rendered headlessly.
use eframe::egui;
use egui_kittest::Harness;
use pluto::app::{App, Options};
use serde_json::{Value, json};

fn at(minutes_ago: i64) -> String {
    (chrono::Utc::now() - chrono::Duration::minutes(minutes_ago))
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn thread(id: &str, title: &str, minutes_ago: i64) -> Value {
    json!({"id": id, "projectId": "p", "title": title, "createdAt": at(minutes_ago),
        "updatedAt": at(minutes_ago), "latestUserMessageAt": at(minutes_ago), "branch": "main",
        "modelSelection": {"instanceId": "codex", "model": "gpt"}, "runtimeMode": "full-access",
        "hasPendingApprovals": false, "hasPendingUserInput": false})
}

fn harness() -> Harness<'static, App> {
    harness_with(
        json!([{"id": "p", "title": "Demo", "workspaceRoot": "/fixture/demo"}]),
        json!([
            thread("a", "Newest thread", 5),
            thread("b", "Older thread", 90)
        ]),
    )
}

fn harness_with(projects: Value, threads: Value) -> Harness<'static, App> {
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1440.0, 900.0))
        .with_step_dt(1.0 / 60.0)
        .build_eframe(|cc| {
            App::new(
                cc,
                Options {
                    server: None,
                    token: None,
                    client_settings_path: None,
                    block_turn_starts: true,
                },
            )
        });
    let config = json!({"type": "snapshot", "config": {"providers": [], "settings": {}}});
    let shell = json!({"kind": "snapshot", "snapshot": {"snapshotSequence": 1,
        "projects": projects, "threads": threads}});
    harness.state_mut().load_offline(&config, &shell).unwrap();
    harness.run_steps(3);
    harness
}

fn click(harness: &mut Harness<'static, App>, pos: egui::Pos2) {
    harness.event(egui::Event::PointerMoved(pos));
    harness.run_steps(1);
    for pressed in [true, false] {
        harness.event(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
    }
    harness.run_steps(2);
}

#[test]
fn clicking_a_sidebar_card_opens_its_thread() {
    let mut harness = harness();
    // Cards start at y = 102 and repeat every 83px; the newest thread leads.
    click(&mut harness, egui::pos2(60.0, 102.0 + 83.0 + 40.0));
    assert_eq!(harness.state().selected_thread_id(), Some("b"));
    click(&mut harness, egui::pos2(60.0, 140.0));
    assert_eq!(harness.state().selected_thread_id(), Some("a"));
}

#[test]
fn worked_for_row_toggles_the_turn_fold() {
    let mut harness = harness();
    click(&mut harness, egui::pos2(60.0, 140.0));
    let item = |id: &str, kind: &str, minutes_ago: i64, extra: Value| {
        let mut item = json!({"id": id, "threadId": "a", "runId": "t", "ordinal": 0,
            "status": "completed", "startedAt": at(minutes_ago), "updatedAt": at(minutes_ago),
            "type": kind});
        for (key, value) in extra.as_object().unwrap() {
            item[key] = value.clone();
        }
        json!({"position": 0, "visibility": "local", "sourceThreadId": "a",
            "sourceItemId": id, "item": item})
    };
    let detail = json!({"kind": "snapshot", "snapshotSequence": 2, "projection": {
        "thread": {"id": "a", "projectId": "p", "title": "Newest thread"},
        "visibleTurnItems": [
            item("u", "user_message", 10, json!({"messageId": "u", "text": "Hi",
                "inputIntent": "turn_start", "attachments": []})),
            item("x", "command_execution", 8, json!({"input": "ls"})),
            item("m", "assistant_message", 6, json!({"messageId": "m", "text": "Done",
                "streaming": false})),
        ],
        "updatedAt": at(5)}});
    harness.state_mut().load_offline_thread(&detail).unwrap();
    harness.run_steps(3);
    // The fold row sits under the user bubble (y ≈ 74..149) at y ≈ 165..201.
    click(&mut harness, egui::pos2(500.0, 180.0));
    assert_eq!(harness.state().expanded_turns(), 1);
    click(&mut harness, egui::pos2(500.0, 180.0));
    assert_eq!(harness.state().expanded_turns(), 0);
}

#[test]
fn double_clicking_a_card_starts_renaming() {
    let mut harness = harness();
    // egui counts clicks against time since start; let the clock pass the
    // double-click window first.
    harness.run_steps(60);
    let pos = egui::pos2(60.0, 140.0);
    harness.event(egui::Event::PointerMoved(pos));
    harness.run_steps(1);
    for _ in 0..2 {
        for pressed in [true, false] {
            harness.event(egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
        }
        harness.run_steps(1);
    }
    harness.run_steps(2);
    assert_eq!(harness.state().renaming_thread(), Some("a"));
}

#[test]
fn project_scope_filters_the_thread_list() {
    let mut other = thread("c", "Other project thread", 30);
    other["projectId"] = json!("q");
    let mut harness = harness_with(
        json!([{"id": "p", "title": "Demo", "workspaceRoot": "/fixture/demo"},
            {"id": "q", "title": "Second", "workspaceRoot": "/fixture/second"}]),
        json!([thread("a", "Newest thread", 5), other]),
    );
    // Folder button, then the second project row: "All projects" spans
    // y = 144..172 and project rows (most recent first) follow every 32px.
    click(&mut harness, egui::pos2(177.0, 76.0));
    click(&mut harness, egui::pos2(100.0, 220.0));
    click(&mut harness, egui::pos2(60.0, 140.0));
    assert_eq!(harness.state().selected_thread_id(), Some("c"));
    // Back to all projects: the newest thread leads again.
    click(&mut harness, egui::pos2(177.0, 76.0));
    click(&mut harness, egui::pos2(100.0, 158.0));
    click(&mut harness, egui::pos2(60.0, 140.0));
    assert_eq!(harness.state().selected_thread_id(), Some("a"));
}

#[test]
fn drafts_start_on_their_project_default_model() {
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1440.0, 900.0))
        .build_eframe(|cc| {
            App::new(
                cc,
                Options {
                    server: None,
                    token: None,
                    client_settings_path: None,
                    block_turn_starts: true,
                },
            )
        });
    let model = |slug: &str| json!({"instanceId": "codex", "model": slug});
    let config = json!({"type": "snapshot", "config": {
    "providers": [{"instanceId": "codex", "driver": "codex", "enabled": true, "status": "ready"}],
    "settings": {
        "defaultModelSelection": model("environment"),
        "projectSettingsOverrides": {"overridden": {"defaultModelSelection": model("override")}},
    }}});
    let projects = json!([
        {"id": "plain", "title": "Plain", "workspaceRoot": "/fixture/plain"},
        {"id": "overridden", "title": "Overridden", "workspaceRoot": "/fixture/overridden"},
        {"id": "legacy", "title": "Legacy", "workspaceRoot": "/fixture/legacy",
         "defaultModelSelection": model("legacy")},
    ]);
    let shell = json!({"kind": "snapshot", "snapshot": {"snapshotSequence": 1,
        "projects": projects, "threads": []}});
    let app = harness.state_mut();
    app.load_offline(&config, &shell).unwrap();
    assert_eq!(app.draft_model_in("plain"), "codex/environment");
    assert_eq!(app.draft_model_in("overridden"), "codex/override");
    assert_eq!(app.draft_model_in("legacy"), "codex/legacy");
}
