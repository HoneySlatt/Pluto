//! The masker's contract (src/mask.rs): stripping, placeholders and shapes.

use pluto_recording::format::{Agent, Side};
use pluto_recording::mask::{self, Masker, Places};
use serde_json::{Value, json};

fn places() -> Places {
    Places {
        workspace: Some("/tmp/rec/project".to_owned()),
        homes: vec!["/tmp/rec/home".to_owned(), "/home/alice".to_owned()],
        user: Some("alice".to_owned()),
        host: Some("deskhost".to_owned()),
    }
}

fn claude() -> Masker {
    Masker::new(Agent::Claude, places())
}

fn grok() -> Masker {
    Masker::new(Agent::Grok, places())
}

fn text_of(value: &Value) -> String {
    serde_json::to_string(value).unwrap()
}

// --- Stripping ---------------------------------------------------------------------------------

#[test]
fn claude_account_fields_become_placeholders() {
    let mut frame = json!({"type": "control_response", "response": {"subtype": "success",
        "request_id": "r1", "response": {"account": {"email": "alice@corp.example",
        "organization": "Alice Corp", "subscriptionType": "max"}, "models": []}}});
    assert!(claude().frame(Side::Agent, &mut frame));
    let text = text_of(&frame);
    assert!(!text.contains("alice@corp"), "{text}");
    assert!(!text.contains("Alice Corp"), "{text}");
    assert!(frame["response"]["response"]["account"].is_object());
    assert!(frame["response"]["response"]["models"].is_array());
}

#[test]
fn claude_init_lists_are_emptied() {
    let mut frame = json!({"type": "system", "subtype": "init", "cwd": "/tmp/rec/project",
        "agents": ["reviewer"], "skills": ["delegate"], "slash_commands": ["/mine"],
        "plugins": [{"name": "p", "path": "/home/alice/.claude/plugins/p"}],
        "mcp_servers": [{"name": "personal", "status": "connected"}],
        "model": "claude-opus-5-5", "claude_code_version": "2.1.276"});
    assert!(claude().frame(Side::Agent, &mut frame));
    for list in [
        "agents",
        "skills",
        "slash_commands",
        "plugins",
        "mcp_servers",
    ] {
        assert_eq!(frame[list], json!([]), "{list}");
    }
    assert_eq!(frame["cwd"], "<workspace>");
    assert_eq!(frame["model"], "claude-opus-5-5");
    assert_eq!(frame["claude_code_version"], "2.1.276");
}

#[test]
fn claude_rate_limits_keep_only_their_status() {
    let mut frame = json!({"type": "rate_limit_event", "rate_limit_info": {"status": "allowed",
        "resetsAt": 1, "overageStatus": "x", "organizationId": "org-1"}, "session_id": "s"});
    claude().frame(Side::Agent, &mut frame);
    assert_eq!(frame["rate_limit_info"], json!({"status": "allowed"}));
}

#[test]
fn claude_memories_are_emptied() {
    let mut frame = json!({"type": "system", "subtype": "memory_recall",
        "memories": [{"path": "/home/alice/.claude/CLAUDE.md", "content": "my notes"}]});
    assert!(claude().frame(Side::Agent, &mut frame));
    assert!(!text_of(&frame).contains("my notes"));
}

#[test]
fn grok_machine_identity_and_user_commands_leave() {
    let mut frame = json!({"jsonrpc": "2.0", "id": 0, "result": {"protocolVersion": 1,
        "_meta": {"agentId": "a1b2", "agentInstanceId": "c3d4", "hostname": "deskhost",
        "availableCommands": [{"name": "mine", "_meta": {"scope": "user"}},
                              {"name": "help", "_meta": {"scope": "builtin"}}]}}});
    let mut masker = grok();
    masker.frame(
        Side::Client,
        &mut json!({"jsonrpc": "2.0", "id": 0, "method": "initialize",
        "params": {}}),
    );
    assert!(masker.frame(Side::Agent, &mut frame));
    let meta = &frame["result"]["_meta"];
    assert_eq!(meta["agentId"], "00000000-0000-4000-8000-000000000000");
    assert_eq!(
        meta["agentInstanceId"],
        "00000000-0000-4000-8000-000000000000"
    );
    assert_eq!(meta["hostname"], "<host>");
    assert_eq!(
        meta["availableCommands"],
        json!([{"name": "help", "_meta": {"scope": "builtin"}}])
    );
}

#[test]
fn grok_user_commands_leave_updates_too() {
    let mut frame = json!({"jsonrpc": "2.0", "method": "session/update", "params": {
        "sessionId": "s", "update": {"sessionUpdate": "available_commands_update",
        "availableCommands": [{"name": "mine", "_meta": {"scope": "user"}}, {"name": "help"}]}}});
    assert!(grok().frame(Side::Agent, &mut frame));
    assert_eq!(
        frame["params"]["update"]["availableCommands"],
        json!([{"name": "help"}])
    );
}

// --- Placeholders ------------------------------------------------------------------------------

#[test]
fn claude_session_id_from_the_arguments_is_a_placeholder_everywhere() {
    let mut masker = claude();
    let (argv, masked) = masker.argv(&[
        "/nix/store/abc-claude/bin/claude".to_owned(),
        "--output-format".to_owned(),
        "stream-json".to_owned(),
        "--session-id=3f1c2a9e-0000-4000-8000-123456789abc".to_owned(),
    ]);
    assert!(!masked);
    assert_eq!(argv[0], "claude");
    assert_eq!(argv[3], "--session-id=<id:1>");
    let mut frame = json!({"type": "system", "subtype": "init",
        "session_id": "3f1c2a9e-0000-4000-8000-123456789abc",
        "transcript": "/tmp/rec/home/.claude/projects/-tmp-rec-project/3f1c2a9e-0000-4000-8000-123456789abc.jsonl"});
    masker.frame(Side::Agent, &mut frame);
    assert_eq!(frame["session_id"], "<id:1>");
    assert_eq!(
        frame["transcript"], "<home>/.claude/projects/<workspace>/<id:1>.jsonl",
        "Claude's folder-name form of the workspace becomes <workspace> too"
    );
}

#[test]
fn claude_resume_takes_the_value_after_the_flag_too() {
    let mut masker = claude();
    let (argv, _) = masker.argv(&[
        "claude".to_owned(),
        "--resume".to_owned(),
        "abc-123-def-456".to_owned(),
    ]);
    assert_eq!(argv, ["claude", "--resume", "<id:1>"]);
}

#[test]
fn claude_client_ids_are_numbered_in_order_and_echoes_replaced() {
    let mut masker = claude();
    let mut init = json!({"type": "control_request", "request_id": "k2j3h4g5", "request": {"subtype": "initialize"}});
    masker.frame(Side::Client, &mut init);
    assert_eq!(init["request_id"], "<id:1>");
    let mut user = json!({"type": "user", "uuid": "u-778899", "session_id": "",
        "message": {"role": "user", "content": "hello"}});
    masker.frame(Side::Client, &mut user);
    assert_eq!(user["uuid"], "<id:2>");
    assert_eq!(user["session_id"], "", "an empty value is not an id");
    let mut answer = json!({"type": "control_response", "response": {"subtype": "success", "request_id": "k2j3h4g5"}});
    masker.frame(Side::Agent, &mut answer);
    assert_eq!(answer["response"]["request_id"], "<id:1>");
    let mut agent_request = json!({"type": "control_request", "request_id": "agent-req-1",
        "request": {"subtype": "can_use_tool", "tool_name": "Bash"}});
    masker.frame(Side::Agent, &mut agent_request);
    assert_eq!(
        agent_request["request_id"], "agent-req-1",
        "ids the agent chose stay"
    );
}

#[test]
fn acp_client_request_ids_become_placeholders_and_responses_follow() {
    let mut masker = grok();
    let mut request = json!({"jsonrpc": "2.0", "id": 7, "method": "session/new", "params": {"cwd": "/tmp/rec/project"}});
    masker.frame(Side::Client, &mut request);
    assert_eq!(request["id"], "<id:1>");
    assert_eq!(request["params"]["cwd"], "<workspace>");
    let mut response = json!({"jsonrpc": "2.0", "id": 7, "result": {"sessionId": "grok-s-1"}});
    masker.frame(Side::Agent, &mut response);
    assert_eq!(response["id"], "<id:1>");
    assert_eq!(
        response["result"]["sessionId"], "grok-s-1",
        "the agent's own ids stay"
    );
    let mut agent_request =
        json!({"jsonrpc": "2.0", "id": 7, "method": "session/request_permission", "params": {}});
    masker.frame(Side::Agent, &mut agent_request);
    assert_eq!(
        agent_request["id"], 7,
        "an agent request's id is the agent's, even if equal"
    );
    let mut count =
        json!({"jsonrpc": "2.0", "method": "session/update", "params": {"update": {"used": 7}}});
    masker.frame(Side::Agent, &mut count);
    assert_eq!(
        count["params"]["update"]["used"], 7,
        "numbers are replaced only as response ids"
    );
}

#[test]
fn folders_are_replaced_in_plain_and_encoded_forms_without_counting_as_masked() {
    let mut masker = claude();
    let mut frame = json!({"type": "assistant", "message": {"content": [{"type": "text",
        "text": "See /tmp/rec/project/src/main.rs and /home/alice/notes and file:///tmp/rec/home/x and %2Ftmp%2Frec%2Fproject%2Fa"}]}});
    let masked = masker.frame(Side::Agent, &mut frame);
    assert!(!masked, "placeholders are not masking");
    assert_eq!(
        frame["message"]["content"][0]["text"],
        "See <workspace>/src/main.rs and <home>/notes and file://<home>/x and <workspace>%2Fa"
    );
}

// --- Masking by shape --------------------------------------------------------------------------

#[test]
fn known_token_shapes_are_masked() {
    let samples = [
        "sk-ant-api03-AbCdEfGhIjKlMnOpQrStUvWxYz0123456789",
        "sk-proj-AbCdEfGhIjKlMnOpQrStUvWxYz0123456789",
        "xai-AbCdEfGhIjKlMnOpQrStUvWxYz0123456789",
        "ghp_AbCdEfGhIjKlMnOpQrStUvWxYz0123456789",
        "github_pat_11AbCdEfG0123456789_AbCdEfGhIjKlMnOpQrStUvWxYz",
        "glpat-AbCdEfGhIjKlMnOpQrSt",
        "AKIAIOSFODNN7EXAMPLE",
        "xoxb-1234567890-abcdefghij",
        "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U",
    ];
    for secret in samples {
        let mut frame = json!({"type": "user", "message": {"content": [{"type": "tool_result",
            "content": format!("KEY={secret} done")}]}});
        assert!(claude().frame(Side::Agent, &mut frame), "{secret}");
        let text = text_of(&frame);
        assert!(!text.contains(secret), "{secret} left in {text}");
        assert!(text.contains("<masked:"), "{text}");
        assert!(text.contains("done"), "the rest of the text stays: {text}");
    }
}

#[test]
fn a_pem_private_key_is_masked_whole() {
    let key = "-----BEGIN OPENSSH PRIVATE KEY-----\nb3BlbnNzaC1rZXktdjEAAAAABG5vbmU=\n-----END OPENSSH PRIVATE KEY-----";
    let (text, masked) = claude().line(&format!("cat id: {key} end"));
    assert!(masked);
    assert!(!text.contains("b3BlbnNzaC1rZXktdjEAAAAABG5vbmU"), "{text}");
    assert!(text.ends_with(" end"), "{text}");
}

#[test]
fn bearer_values_and_secret_keys_are_masked_but_counts_stay() {
    let mut masker = claude();
    let mut frame = json!({"type": "control_request", "request_id": "q1", "request": {
        "subtype": "mcp_set_servers", "servers": {"t3-code": {"type": "http",
        "url": "http://127.0.0.1:43123/mcp",
        "headers": {"Authorization": "Bearer dummy-mcp-credential-1234"}}}}});
    assert!(masker.frame(Side::Client, &mut frame));
    let text = text_of(&frame);
    assert!(!text.contains("dummy-mcp-credential-1234"), "{text}");
    let mut usage = json!({"type": "result", "usage": {"input_tokens": 12, "output_tokens": 3,
        "cache_read_input_tokens": 9}, "token": "s3cr3t-value", "api_key": "plainvalue",
        "password": 42});
    assert!(masker.frame(Side::Agent, &mut usage));
    assert_eq!(usage["usage"]["input_tokens"], 12);
    assert_eq!(usage["usage"]["output_tokens"], 3);
    assert_eq!(usage["usage"]["cache_read_input_tokens"], 9);
    assert!(usage["token"].as_str().unwrap().starts_with("<masked:"));
    assert!(usage["api_key"].as_str().unwrap().starts_with("<masked:"));
    assert_eq!(
        usage["password"], 42,
        "only string values of secret keys are masked"
    );
}

#[test]
fn a_value_masked_once_is_masked_everywhere_after() {
    let mut masker = claude();
    let mut set = json!({"type": "control_request", "request_id": "q1", "request": {
        "subtype": "mcp_set_servers", "servers": {"s": {"headers": {"Authorization": "Bearer echoed-secret-777"}}}}});
    masker.frame(Side::Client, &mut set);
    let mut echo = json!({"type": "system", "subtype": "status", "error": "header was echoed-secret-777, rejected"});
    assert!(masker.frame(Side::Agent, &mut echo));
    assert!(!text_of(&echo).contains("echoed-secret-777"));
    let (line, masked) = masker.line("stderr: echoed-secret-777");
    assert!(masked);
    assert!(!line.contains("echoed-secret-777"));
}

#[test]
fn user_and_host_are_masked_only_in_paths_urls_and_user_at_host() {
    let mut masker = claude();
    let (line, masked) = masker.line("alice@deskhost ssh://deskhost:22/x /srv/alice/data ok");
    assert!(masked);
    assert!(!line.contains("alice"), "{line}");
    assert!(!line.contains("deskhost"), "{line}");
    let (plain, masked) = masker.line("Alice met the deskhost team");
    assert!(!masked, "ordinary words stay: {plain}");
    assert_eq!(plain, "Alice met the deskhost team");
}

#[test]
fn secrets_inside_json_in_a_string_are_found() {
    let mut frame = json!({"type": "assistant", "message": {"content": [{"type": "tool_use",
        "input": {"command": "{\"api_key\":\"sk-ant-api03-NestedSecretValue0123456789\"}"}}]}});
    assert!(claude().frame(Side::Agent, &mut frame));
    assert!(!text_of(&frame).contains("NestedSecretValue"));
}

#[test]
fn a_document_is_masked_by_places_and_shapes() {
    let mut reference = json!({"messages": [{"text": "wrote /tmp/rec/project/a.rs with sk-ant-api03-AbCdEfGhIjKlMnOpQrStUvWxYz0123456789"}]});
    assert!(mask::document(&places(), &mut reference));
    assert_eq!(
        reference["messages"][0]["text"]
            .as_str()
            .unwrap()
            .split(" with ")
            .next()
            .unwrap(),
        "wrote <workspace>/a.rs"
    );
    assert!(!text_of(&reference).contains("AbCdEfGh"));
}

#[test]
fn a_clean_frame_is_untouched() {
    let original = json!({"type": "assistant", "message": {"id": "msg_01ABCdef", "content": [
        {"type": "text", "text": "Done: 3 files changed."}]}});
    let mut frame = original.clone();
    assert!(!claude().frame(Side::Agent, &mut frame));
    assert_eq!(frame, original);
}
