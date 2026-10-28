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
fn sidebar_cards_draw_provider_marks_for_all_supported_drivers() {
    let drivers = [
        "codex",
        "claudeAgent",
        "cursor",
        "grok",
        "opencode",
        "antigravity",
        "kimiCode",
        "mistralVibe",
        "zcode",
    ];
    let providers: Vec<Value> = drivers
        .iter()
        .map(|driver| json!({"instanceId": format!("custom-{driver}"), "driver": driver}))
        .collect();
    let threads: Vec<Value> = drivers
        .iter()
        .enumerate()
        .map(|(i, driver)| {
            let mut thread = thread(driver, driver, i as i64 + 1);
            thread["modelSelection"]["instanceId"] = json!(format!("custom-{driver}"));
            thread
        })
        .collect();
    let mut harness = harness();
    let config = json!({"type": "snapshot", "config": {"providers": providers, "settings": {}}});
    let shell = json!({"kind": "snapshot", "snapshot": {"snapshotSequence": 2,
        "projects": [{"id": "p", "title": "Demo", "workspaceRoot": "/fixture/demo"}],
        "threads": threads}});
    harness.state_mut().load_offline(&config, &shell).unwrap();
    harness.run_steps(5);
    for (i, driver) in drivers.iter().enumerate() {
        let slot = egui::Rect::from_min_size(
            egui::pos2(216.0, 150.0 + i as f32 * 83.0),
            egui::vec2(28.0, 25.0),
        );
        assert!(
            harness.output().shapes.iter().any(|shape| {
                matches!(&shape.shape, egui::Shape::Rect(rect)
                    if rect.brush.is_some() && slot.contains_rect(rect.rect))
            }),
            "missing sidebar provider mark for {driver}"
        );
    }
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

#[test]
fn a_bounded_thread_offers_its_earlier_turns() {
    let mut harness = harness();
    click(&mut harness, egui::pos2(60.0, 140.0));
    let item = json!({"id": "u", "threadId": "a", "runId": "t", "ordinal": 40,
        "status": "completed", "startedAt": at(10), "updatedAt": at(10),
        "type": "user_message", "messageId": "u", "text": "Hi", "inputIntent": "turn_start",
        "attachments": []});
    let detail = json!({"kind": "snapshot", "snapshotSequence": 2, "projection": {
        "thread": {"id": "a", "projectId": "p", "title": "Newest thread"},
        "turnItems": [item],
        "visibleTurnItems": [{"position": 0, "visibility": "local", "sourceThreadId": "a",
            "sourceItemId": "u", "item": item}],
        "updatedAt": at(5)},
        "historyCursor": "c1", "hasMoreHistory": true, "latestLocalTurnOrdinal": 40});
    harness.state_mut().load_offline_thread(&detail).unwrap();
    harness.run_steps(3);
    let texts = |harness: &Harness<'static, App>| -> Vec<(String, egui::Rect)> {
        harness
            .output()
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) => Some((
                    text.galley.text().to_owned(),
                    text.galley.rect.translate(text.pos.to_vec2()),
                )),
                _ => None,
            })
            .collect()
    };
    let (_, button) = texts(&harness)
        .into_iter()
        .find(|(text, _)| text == "Load earlier turns")
        .expect("the load-earlier control is drawn");
    // Offline, the load fails and says why; the control stays to retry.
    click(&mut harness, button.center());
    harness.run_steps(2);
    let shown: Vec<String> = texts(&harness).into_iter().map(|(text, _)| text).collect();
    assert!(
        shown.iter().any(|t| t == "Environment is not connected."),
        "{shown:?}"
    );
    assert!(shown.iter().any(|t| t == "Load earlier turns"));
}

#[test]
fn find_in_thread_marks_and_counts_the_server_matches() {
    let mut harness = harness();
    let config = json!({"type": "snapshot", "config": {"providers": [], "settings": {},
        "threadFind": true, "threadFindProgressive": true,
        "keybindings": [{"command": "chat.find", "shortcut": {"key": "f", "modKey": true},
            "whenAst": {"type": "not", "node": {"type": "identifier", "name": "terminalFocus"}}}]}});
    harness
        .state_mut()
        .load_offline(
            &config,
            &json!({"kind": "snapshot", "snapshot": {
        "snapshotSequence": 1,
        "projects": [{"id": "p", "title": "Demo", "workspaceRoot": "/fixture/demo"}],
        "threads": [thread("a", "Newest thread", 5)]}}),
        )
        .unwrap();
    click(&mut harness, egui::pos2(60.0, 140.0));
    let item = |id: &str, kind: &str, ordinal: u64, extra: Value| {
        let mut item = json!({"id": id, "threadId": "a", "runId": "t", "ordinal": ordinal,
            "status": "completed", "startedAt": at(10), "updatedAt": at(10), "type": kind});
        for (key, value) in extra.as_object().unwrap() {
            item[key] = value.clone();
        }
        json!({"position": 0, "visibility": "local", "sourceThreadId": "a",
            "sourceItemId": id, "item": item})
    };
    let detail = json!({"kind": "snapshot", "snapshotSequence": 2, "projection": {
        "thread": {"id": "a", "projectId": "p", "title": "Newest thread"},
        "runs": [{"id": "t", "threadId": "a", "ordinal": 1, "providerInstanceId": "codex",
            "modelSelection": {"instanceId": "codex", "model": "gpt"}, "userMessageId": "u",
            "status": "completed", "requestedAt": at(10)}],
        "visibleTurnItems": [
            item("u", "user_message", 1, json!({"messageId": "u", "text": "Write a plan",
                "inputIntent": "turn_start", "attachments": []})),
            item("m", "assistant_message", 2, json!({"messageId": "m",
                "text": "Here is the plan.", "streaming": false})),
        ],
        "updatedAt": at(5)}});
    harness.state_mut().load_offline_thread(&detail).unwrap();
    harness.run_steps(3);
    let ctrl = egui::Modifiers {
        ctrl: true,
        command: true,
        ..egui::Modifiers::NONE
    };
    harness.key_press_modifiers(ctrl, egui::Key::F);
    harness.run_steps(2);
    harness.event(egui::Event::Text("plan".into()));
    // Past the 100ms debounce, the search goes out.
    harness.run_steps(12);
    harness.state_mut().load_offline_reply(
        "thread-find:1",
        json!({"snapshotSequence": 2, "totalMatches": 2, "activeIndex": 1,
            "match": {"entryId": "m", "runId": "t", "occurrence": 0},
            "navigation": [{"entryId": "u", "runId": "t", "startIndex": 0, "count": 1},
                {"entryId": "m", "runId": "t", "startIndex": 1, "count": 1}]}),
    );
    harness.run_steps(3);
    let output = harness.output();
    let texts: Vec<String> = output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Text(text) => Some(text.galley.text().to_owned()),
            _ => None,
        })
        .collect();
    assert!(texts.iter().any(|t| t == "2/2"), "{texts:?}");
    let fills: Vec<egui::Color32> = output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Rect(rect) => Some(rect.fill),
            _ => None,
        })
        .collect();
    // The active occurrence and the other one.
    assert!(fills.contains(&egui::Color32::from_rgb(238, 165, 43)));
    assert!(fills.contains(&egui::Color32::from_rgb(140, 108, 31)));
}
