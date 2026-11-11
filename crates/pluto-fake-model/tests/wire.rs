use pluto_fake_model::tools::MappedTool;
use pluto_fake_model::wire::{
    Reply, WireEvent, anthropic, ollama, output_tokens, pieces, responses, tokens,
};
use serde_json::{Value, json};

fn reply(tool: bool) -> Reply {
    Reply {
        model: "fake-model".into(),
        id: "r7".into(),
        text: "Héllo, wörld: a list\n- one\n- two".into(),
        tool: tool.then(|| MappedTool {
            call_id: "call_x".into(),
            name: "Bash".into(),
            arguments: json!({"command": "ls -la"}),
        }),
        input_tokens: 250,
        chunk_bytes: 5,
    }
}

/// Splits an SSE event into its type and its JSON.
fn sse(event: &WireEvent) -> (String, Value) {
    let rest = event.data.strip_prefix("event: ").expect("an event line");
    let (kind, rest) = rest.split_once('\n').unwrap();
    let json = rest
        .strip_prefix("data: ")
        .unwrap()
        .strip_suffix("\n\n")
        .expect("ends with a blank line");
    assert!(!json.contains('\n'), "one data line");
    (kind.to_string(), serde_json::from_str(json).unwrap())
}

#[test]
fn tokens_are_four_bytes_rounded_up() {
    assert_eq!(tokens(0), 0);
    assert_eq!(tokens(1), 1);
    assert_eq!(tokens(4), 1);
    assert_eq!(tokens(5), 2);
    let with_tool = reply(true);
    let arguments = serde_json::to_string(&with_tool.tool.as_ref().unwrap().arguments).unwrap();
    assert_eq!(
        output_tokens(&with_tool),
        tokens(with_tool.text.len() + arguments.len())
    );
    assert_eq!(
        output_tokens(&reply(false)),
        tokens(reply(false).text.len())
    );
}

#[test]
fn pieces_never_split_a_character() {
    let text = "Héllo, wörld";
    let cut = pieces(text, 2);
    assert_eq!(cut.concat(), text);
    assert!(cut.iter().all(|p| p.len() <= 2 || p.chars().count() == 1));
    assert_eq!(pieces("", 4), Vec::<&str>::new());
    assert_eq!(pieces("😀a", 1), vec!["😀", "a"]);
    assert_eq!(pieces("abcdefg", 3), vec!["abc", "def", "g"]);
}

#[test]
fn anthropic_stream_with_a_tool() {
    let reply = reply(true);
    let events = anthropic::events(&reply);
    let parsed: Vec<(String, Value)> = events.iter().map(sse).collect();
    let kinds: Vec<&str> = parsed.iter().map(|(k, _)| k.as_str()).collect();
    let text_pieces = pieces(&reply.text, 5).len();
    let json_pieces = pieces(r#"{"command":"ls -la"}"#, 5).len();
    let mut expected = vec!["message_start", "content_block_start"];
    expected.extend(std::iter::repeat_n("content_block_delta", text_pieces));
    expected.extend(["content_block_stop", "content_block_start"]);
    expected.extend(std::iter::repeat_n("content_block_delta", json_pieces));
    expected.extend(["content_block_stop", "message_delta", "message_stop"]);
    assert_eq!(kinds, expected);
    for (kind, data) in &parsed {
        assert_eq!(data["type"], kind.as_str());
    }
    let start = &parsed[0].1["message"];
    assert_eq!(start["id"], "r7");
    assert_eq!(start["role"], "assistant");
    assert_eq!(start["model"], "fake-model");
    assert_eq!(start["content"], json!([]));
    assert_eq!(start["usage"]["input_tokens"], 250);
    assert_eq!(parsed[1].1["index"], 0);
    assert_eq!(
        parsed[1].1["content_block"],
        json!({"type": "text", "text": ""})
    );
    let text: String = parsed
        .iter()
        .filter(|(k, d)| k == "content_block_delta" && d["delta"]["type"] == "text_delta")
        .map(|(_, d)| d["delta"]["text"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(text, reply.text);
    let tool_start = &parsed[2 + text_pieces + 1].1;
    assert_eq!(tool_start["index"], 1);
    assert_eq!(
        tool_start["content_block"],
        json!({"type": "tool_use", "id": "call_x", "name": "Bash", "input": {}})
    );
    let arguments: String = parsed
        .iter()
        .filter(|(k, d)| k == "content_block_delta" && d["delta"]["type"] == "input_json_delta")
        .map(|(_, d)| {
            assert_eq!(d["index"], 1);
            d["delta"]["partial_json"].as_str().unwrap().to_string()
        })
        .collect();
    assert_eq!(
        serde_json::from_str::<Value>(&arguments).unwrap(),
        json!({"command": "ls -la"})
    );
    let delta = &parsed[parsed.len() - 2].1;
    assert_eq!(delta["delta"]["stop_reason"], "tool_use");
    assert_eq!(delta["usage"]["output_tokens"], output_tokens(&reply));
    let carried: usize = events.iter().map(|e| e.text_bytes).sum();
    assert_eq!(carried, reply.text.len() + arguments.len());
    assert_eq!(events[0].text_bytes, 0);
}

#[test]
fn anthropic_stream_without_a_tool_and_without_text() {
    let parsed: Vec<(String, Value)> = anthropic::events(&reply(false)).iter().map(sse).collect();
    assert_eq!(
        parsed[parsed.len() - 2].1["delta"]["stop_reason"],
        "end_turn"
    );
    assert!(
        parsed
            .iter()
            .all(|(_, d)| d["content_block"]["type"] != "tool_use")
    );
    let mut silent = reply(true);
    silent.text.clear();
    let parsed: Vec<(String, Value)> = anthropic::events(&silent).iter().map(sse).collect();
    assert_eq!(parsed[1].1["content_block"]["type"], "tool_use");
    assert_eq!(parsed[1].1["index"], 0);
}

#[test]
fn anthropic_whole_message() {
    let message = anthropic::message(&reply(true));
    assert_eq!(message["type"], "message");
    assert_eq!(message["id"], "r7");
    assert_eq!(message["stop_reason"], "tool_use");
    assert_eq!(message["stop_sequence"], Value::Null);
    assert_eq!(
        message["content"][0],
        json!({"type": "text", "text": reply(true).text})
    );
    assert_eq!(
        message["content"][1],
        json!({"type": "tool_use", "id": "call_x", "name": "Bash", "input": {"command": "ls -la"}})
    );
    assert_eq!(
        message["usage"],
        json!({"input_tokens": 250, "output_tokens": output_tokens(&reply(true))})
    );
    assert_eq!(anthropic::message(&reply(false))["stop_reason"], "end_turn");
}

#[test]
fn responses_stream_with_a_tool() {
    let reply = reply(true);
    let events = responses::events(&reply);
    let parsed: Vec<(String, Value)> = events.iter().map(sse).collect();
    let kinds: Vec<&str> = parsed.iter().map(|(k, _)| k.as_str()).collect();
    let text_pieces = pieces(&reply.text, 5).len();
    let mut expected = vec![
        "response.created",
        "response.output_item.added",
        "response.content_part.added",
    ];
    expected.extend(std::iter::repeat_n(
        "response.output_text.delta",
        text_pieces,
    ));
    expected.extend([
        "response.output_text.done",
        "response.content_part.done",
        "response.output_item.done",
        "response.output_item.added",
        "response.output_item.done",
        "response.completed",
    ]);
    assert_eq!(kinds, expected);
    for (n, (kind, data)) in parsed.iter().enumerate() {
        assert_eq!(data["type"], kind.as_str());
        assert_eq!(data["sequence_number"], n);
    }
    assert_eq!(parsed[0].1["response"]["status"], "in_progress");
    assert_eq!(parsed[0].1["response"]["output"], json!([]));
    assert_eq!(parsed[1].1["output_index"], 0);
    assert_eq!(parsed[1].1["item"]["type"], "message");
    assert_eq!(parsed[1].1["item"]["id"], "msg_r7");
    let text: String = parsed
        .iter()
        .filter(|(k, _)| k == "response.output_text.delta")
        .map(|(_, d)| {
            assert_eq!(d["item_id"], "msg_r7");
            assert_eq!(d["content_index"], 0);
            d["delta"].as_str().unwrap().to_string()
        })
        .collect();
    assert_eq!(text, reply.text);
    let message_done = &parsed[5 + text_pieces].1["item"];
    assert_eq!(message_done["role"], "assistant");
    assert_eq!(
        message_done["content"],
        json!([{"type": "output_text", "text": reply.text, "annotations": []}])
    );
    let call_added = &parsed[6 + text_pieces].1;
    assert_eq!(call_added["output_index"], 1);
    assert_eq!(call_added["item"]["arguments"], "");
    let call = &parsed[7 + text_pieces].1["item"];
    assert_eq!(call["type"], "function_call");
    assert_eq!(call["id"], "fc_r7");
    assert_eq!(call["call_id"], "call_x");
    assert_eq!(call["name"], "Bash");
    assert_eq!(
        serde_json::from_str::<Value>(call["arguments"].as_str().unwrap()).unwrap(),
        json!({"command": "ls -la"})
    );
    let completed = &parsed.last().unwrap().1["response"];
    assert_eq!(completed["status"], "completed");
    assert_eq!(completed["output"].as_array().unwrap().len(), 2);
    assert_eq!(completed["usage"]["input_tokens"], 250);
    assert_eq!(completed["usage"]["output_tokens"], output_tokens(&reply));
    assert_eq!(
        completed["usage"]["total_tokens"],
        250 + output_tokens(&reply)
    );
    assert_eq!(*completed, responses::response(&reply));
    let carried: usize = events.iter().map(|e| e.text_bytes).sum();
    assert_eq!(carried, reply.text.len() + r#"{"command":"ls -la"}"#.len());
}

#[test]
fn responses_without_text_starts_with_the_call() {
    let mut silent = reply(true);
    silent.text.clear();
    let parsed: Vec<(String, Value)> = responses::events(&silent).iter().map(sse).collect();
    assert_eq!(parsed[1].1["item"]["type"], "function_call");
    assert_eq!(parsed[1].1["output_index"], 0);
    assert_eq!(parsed.len(), 4);
}

#[test]
fn ollama_lines_with_a_tool() {
    let reply = reply(true);
    let lines = ollama::lines(&reply);
    let parsed: Vec<Value> = lines
        .iter()
        .map(|l| {
            let json = l
                .data
                .strip_suffix('\n')
                .expect("a line ends with a newline");
            assert!(!json.contains('\n'));
            serde_json::from_str(json).unwrap()
        })
        .collect();
    let text_pieces = pieces(&reply.text, 5).len();
    assert_eq!(parsed.len(), text_pieces + 2);
    let text: String = parsed[..text_pieces]
        .iter()
        .map(|l| {
            assert_eq!(l["done"], false);
            assert_eq!(l["model"], "fake-model");
            assert!(l["created_at"].is_string());
            assert_eq!(l["message"]["role"], "assistant");
            l["message"]["content"].as_str().unwrap().to_string()
        })
        .collect();
    assert_eq!(text, reply.text);
    let call = &parsed[text_pieces];
    assert_eq!(call["done"], false);
    assert_eq!(call["message"]["content"], "");
    assert_eq!(
        call["message"]["tool_calls"],
        json!([{"function": {"name": "Bash", "arguments": {"command": "ls -la"}}}])
    );
    let last = parsed.last().unwrap();
    assert_eq!(last["done"], true);
    assert_eq!(last["done_reason"], "stop");
    assert_eq!(last["prompt_eval_count"], 250);
    assert_eq!(last["eval_count"], output_tokens(&reply));
    assert_eq!(last["total_duration"], 0);
    let whole = ollama::chat(&reply);
    assert_eq!(whole["done"], true);
    assert_eq!(whole["message"]["content"], reply.text);
    assert_eq!(
        whole["message"]["tool_calls"],
        call["message"]["tool_calls"]
    );
    assert!(
        ollama::chat(&self::reply(false))["message"]
            .get("tool_calls")
            .is_none()
    );
}
