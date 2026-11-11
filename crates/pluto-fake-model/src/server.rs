//! The listener, the routes, the pacing and the journal.
//!
//! One thread accepts on each address; each connection gets a thread of its own, reads one request
//! ([`crate::http`]), answers it, and closes. Accepted sockets set `TCP_NODELAY`, so that each
//! streamed piece leaves at once, and a read and a write timeout of 30 seconds each: a client that
//! stops reading holds its thread for a minute at most.
//!
//! Routes, matched on the path without its query:
//! - `OPTIONS` on any path: 204, the CORS headers only (no `Content-Type`, no `Content-Length`).
//!   Kind `preflight`.
//! - `HEAD` on any path: 200, no body (Claude Code's `HEAD /api/hello`, Ollama's `HEAD /`). Kind
//!   `listing`.
//! - `GET /`: 200, `text/plain`, `Ollama is running`. Kind `listing`.
//! - `GET /api/version`: `{"version": OLLAMA_VERSION}`. Kind `listing`.
//! - `GET /api/tags`: `{"models": [...]}`, one per configured model, each with `name`, `model`,
//!   `modified_at`, `size`, `digest` and `details`. Kind `listing`.
//! - `POST /api/show`: `{"modelfile", "parameters", "template", "details", "model_info",
//!   "capabilities": ["completion", "tools"]}`; `model_info` holds `general.architecture` and a
//!   context length of 131072. Kind `listing`.
//! - `GET /v1/models` and `GET /models`: one list both OpenAI's and Anthropic's clients read:
//!   `{"object": "list", "data": [...], "has_more": false, "first_id", "last_id"}`, each model with
//!   `id`, `object: "model"`, `type: "model"`, `display_name`, `created`, `created_at` and
//!   `owned_by`. Kind `listing`.
//! - `POST /v1/messages/count_tokens`: `{"input_tokens"}` from the body's bytes. Kind `listing`.
//! - `POST /v1/messages`, `POST /v1/responses`, `POST /responses`, `POST /api/chat`: a model's
//!   answer in that API, streamed if asked. Kind `main` if the request offers at least one tool
//!   whose name is in [`tools::SHELL_NAMES`] or [`tools::READ_NAMES`], `side` otherwise (a title
//!   request that offers only a structured-output tool is a side request); a side request gets the
//!   script's `side_answer` and no tool.
//! - Anything else: 404, `{"error": {"type": "not_found", "message"}}`. Kind `unknown`.
//!
//! A request that cannot be read (malformed: 400; too large: 413; a `Content-Encoding`: 415), or a
//! body that is not JSON or not the API's shape (400), is answered with
//! `{"error": {"type": "invalid_request_error", "message"}}`. Kind `refused`.
//!
//! A main request's answer is the script's segment at the request's position. Its tool call's id
//! is `toolu_fake_<turn>_<segment>` on Anthropic's API and `call_fake_<turn>_<segment>` on the
//! others. From the turn's second segment on, the journal says whether the tool's result came
//! back: as many results as the segment's number, the last answering the previous segment's call
//! id (on Ollama, whose results carry no id, the count alone).
//!
//! Pacing: the answer's time is `first_token_ms` plus its bytes (the text's and the tool
//! arguments', whichever event carries them) at `tokens_per_second` four-byte tokens, all divided by the speed,
//! the same for every API. A stream's framing before its first piece of text leaves at once; the
//! first piece leaves after `first_token_ms`; each later event leaves once the bytes of the pieces
//! before it have been paid for, and the stream ends once all have. Deadlines are chained, so that
//! the time a write or a late wake-up takes is not added to the pace; a stream that fell behind by
//! more than 50 ms (a slow client) starts again from now. An answer that is not streamed (a
//! model's, not a listing) is written after the same total time. An infinite speed waits for
//! nothing; a time too long to count waits as long as can be.
//!
//! The journal has one entry per request, kept in memory and, if a path is given, appended to a
//! JSON Lines file, one line per request, flushed at once.

use crate::conversation::{self, Api, Conversation, Parts};
use crate::http::{self, ReadError, Request, Unreadable};
use crate::json;
use crate::script::Script;
use crate::tools;
use crate::wire::{self, Reply, WireEvent, anthropic, ollama, responses};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::VecDeque;
use std::fs::{File, OpenOptions};
use std::io::{self, BufReader, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// The connection timeout the binary uses: see [`Config::connection_timeout`].
pub const CONNECTION_TIMEOUT: Duration = Duration::from_secs(30);

/// How far behind its pace a stream may fall and still catch up: a loaded machine wakes a sleeping
/// thread a few milliseconds late, which the next pieces make up. Beyond this, the client is slow
/// to read, and the stream goes on at its pace from where it is.
const CATCH_UP: Duration = Duration::from_millis(50);

/// How much of a refused request's rest is read and dropped before the connection closes. Closing
/// with unread data would reset the connection and could cost the client our answer.
const DRAIN_LIMIT: u64 = 1024 * 1024;

/// How long the client may take to finish sending, and to close, after a refusal.
const DRAIN_TIMEOUT: Duration = Duration::from_secs(2);

/// The time the listings report for every model, so that every client receives the same bytes.
const MODEL_CREATED_AT: &str = "2026-01-01T00:00:00Z";
const MODEL_CREATED: u64 = 1_767_225_600;

/// The context length `POST /api/show` reports.
const CONTEXT_LENGTH: u64 = 131_072;

/// The version `GET /api/version` reports: Ollama's in nixpkgs on 2026-11-11.
pub const OLLAMA_VERSION: &str = "0.40.13";

/// How many earlier request bodies a new one is compared with, for
/// [`JournalEntry::repeated_prefix`].
pub const PREFIX_WINDOW: usize = 64;

pub struct Config {
    pub script: Script,
    /// How many times faster than the script's pace: a finite number above zero, or
    /// `f64::INFINITY` for no waiting.
    pub speed: f64,
    pub journal_path: Option<PathBuf>,
    /// The models the listings name. Answers echo whatever model a request names.
    pub models: Vec<String>,
    /// How long a connection may stay silent while its request is read, and how long writing one
    /// piece of an answer may take when the client does not read. The socket's own write timeout
    /// is not enough for the second: a write that moved a few bytes before it ran out succeeds, and
    /// `write_all` goes on, so that it could wait many times over (see [`TimedWriter`]).
    pub connection_timeout: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Main,
    Side,
    Listing,
    Preflight,
    Unknown,
    Refused,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct JournalEntry {
    /// The request's number, from 1, in the order requests were read.
    pub request: u64,
    /// Milliseconds since the server started, when the request was read.
    pub at_ms: u64,
    pub method: String,
    pub path: String,
    pub api: Option<Api>,
    pub kind: Kind,
    pub status: u16,
    pub turn: Option<usize>,
    pub segment: Option<usize>,
    /// The client's tool the answer called, if any.
    pub tool: Option<String>,
    /// The segment had a tool call that no offered tool fitted.
    pub tool_missing: bool,
    /// From a turn's second segment on: whether the previous call's result came back.
    pub tool_result_ok: Option<bool>,
    /// A model request's bytes by part.
    pub parts: Option<Parts>,
    /// The names of the tools a model request offers, in its order; empty for any other request.
    pub tools_offered: Vec<String>,
    /// The longest prefix, in bytes, this body shares with one of the [`PREFIX_WINDOW`] model
    /// request bodies read before it on the same API, whether or not their responses have ended; 0
    /// for the first.
    pub repeated_prefix: usize,
    /// The response's body bytes, as written.
    pub response_bytes: usize,
    /// Why a request was refused. A request that cannot be read has no method and no path, except
    /// one refused for its body (too large, encoded), which keeps the request line's.
    pub error: Option<String>,
}

/// A running server.
pub struct Handle {
    /// The addresses it listens on, the ports chosen if port 0 was asked.
    pub addrs: Vec<SocketAddr>,
    shared: Arc<Shared>,
    accept_threads: Vec<JoinHandle<()>>,
}

impl Handle {
    /// The journal so far.
    pub fn journal(&self) -> Vec<JournalEntry> {
        self.shared.lock_journal().entries.clone()
    }

    /// Stops accepting and returns once every listener's thread has ended. Connections already
    /// open finish on their own.
    pub fn stop(self) {
        self.shared.stopping.store(true, Ordering::SeqCst);
        // An `accept` only returns with a connection: make one for each listener.
        for addr in &self.addrs {
            let mut wake_up = *addr;
            if wake_up.ip().is_unspecified() {
                wake_up.set_ip(if wake_up.is_ipv4() {
                    [127, 0, 0, 1].into()
                } else {
                    [0, 0, 0, 0, 0, 0, 0, 1].into()
                });
            }
            // A listener that is already gone has nothing to wake.
            let _ = TcpStream::connect(wake_up);
        }
        for thread in self.accept_threads {
            // A listener thread only ends by a panic that it has already reported.
            let _ = thread.join();
        }
    }
}

/// Starts listening on every address and serving in the background.
pub fn spawn(config: Config, addrs: &[SocketAddr]) -> io::Result<Handle> {
    let journal_file = match &config.journal_path {
        Some(path) => Some(OpenOptions::new().create(true).append(true).open(path)?),
        None => None,
    };
    // Every address is bound before any thread starts, so that a failure to bind leaves nothing
    // running.
    let listeners = addrs
        .iter()
        .map(TcpListener::bind)
        .collect::<io::Result<Vec<_>>>()?;
    let bound = listeners
        .iter()
        .map(TcpListener::local_addr)
        .collect::<io::Result<Vec<_>>>()?;

    let shared = Arc::new(Shared {
        config,
        started: Instant::now(),
        next_request: AtomicU64::new(1),
        stopping: AtomicBool::new(false),
        journal: Mutex::new(Journal {
            entries: Vec::new(),
            file: journal_file,
            recent_bodies: VecDeque::new(),
        }),
    });
    let mut handle = Handle {
        addrs: bound,
        shared,
        accept_threads: Vec::new(),
    };
    for listener in listeners {
        let shared = Arc::clone(&handle.shared);
        let started = thread::Builder::new()
            .name("accept".into())
            .spawn(move || accept_loop(&listener, &shared));
        match started {
            Ok(thread) => handle.accept_threads.push(thread),
            Err(error) => {
                // A failure to start a thread leaves nothing running either.
                handle.stop();
                return Err(error);
            }
        }
    }
    Ok(handle)
}

/// What every thread shares.
struct Shared {
    config: Config,
    started: Instant,
    next_request: AtomicU64,
    stopping: AtomicBool,
    journal: Mutex<Journal>,
}

struct Journal {
    entries: Vec<JournalEntry>,
    file: Option<File>,
    /// The bodies of recent model requests, oldest first, at most [`PREFIX_WINDOW`] for each API.
    recent_bodies: VecDeque<(Api, Vec<u8>)>,
}

impl Shared {
    /// A poisoned lock is still a journal: a thread that panicked left whole entries behind.
    fn lock_journal(&self) -> MutexGuard<'_, Journal> {
        self.journal.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Numbers a request that was just read, and starts its entry.
    fn begin(&self, method: &str, path: &str) -> JournalEntry {
        JournalEntry {
            request: self.next_request.fetch_add(1, Ordering::SeqCst),
            at_ms: u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX),
            method: method.to_owned(),
            path: path.to_owned(),
            api: None,
            kind: Kind::Unknown,
            status: 0,
            turn: None,
            segment: None,
            tool: None,
            tool_missing: false,
            tool_result_ok: None,
            parts: None,
            tools_offered: Vec::new(),
            repeated_prefix: 0,
            response_bytes: 0,
            error: None,
        }
    }

    /// Adds a finished request to the journal, and to its file. Entries are added when the
    /// response ends, so that requests whose streams overlap can be out of order of their numbers.
    fn record(&self, entry: JournalEntry) {
        let mut journal = self.lock_journal();
        if let Some(file) = &mut journal.file {
            let line = format!("{}\n", json::compact(&entry));
            if let Err(error) = file.write_all(line.as_bytes()).and_then(|()| file.flush()) {
                eprintln!("pluto-fake-model: cannot write the journal: {error}");
            }
        }
        journal.entries.push(entry);
    }
}

impl Journal {
    /// The longest prefix a model request's body shares with the recent ones of its API; the body
    /// is then kept for the next requests to be measured against.
    fn measure_and_remember(&mut self, api: Api, body: &[u8]) -> usize {
        let shared = self.longest_shared_prefix(api, body);
        self.remember(api, body.to_vec());
        shared
    }

    fn longest_shared_prefix(&self, api: Api, body: &[u8]) -> usize {
        self.recent_bodies
            .iter()
            .rev()
            .filter(|(other_api, _)| *other_api == api)
            .take(PREFIX_WINDOW)
            .map(|(_, other)| other.iter().zip(body).take_while(|(a, b)| a == b).count())
            .max()
            .unwrap_or(0)
    }

    fn remember(&mut self, api: Api, body: Vec<u8>) {
        self.recent_bodies.push_back((api, body));
        let kept = self
            .recent_bodies
            .iter()
            .filter(|(other_api, _)| *other_api == api)
            .count();
        if kept > PREFIX_WINDOW
            && let Some(oldest) = self
                .recent_bodies
                .iter()
                .position(|(other_api, _)| *other_api == api)
        {
            self.recent_bodies.remove(oldest);
        }
    }
}

fn accept_loop(listener: &TcpListener, shared: &Arc<Shared>) {
    for connection in listener.incoming() {
        if shared.stopping.load(Ordering::SeqCst) {
            return;
        }
        match connection {
            Ok(stream) => {
                let shared = Arc::clone(shared);
                let started = thread::Builder::new()
                    .name("connection".into())
                    .spawn(move || serve_connection(&shared, stream));
                if let Err(error) = started {
                    eprintln!("pluto-fake-model: cannot start a connection's thread: {error}");
                }
            }
            Err(error) => {
                eprintln!("pluto-fake-model: cannot accept a connection: {error}");
                // A persistent failure (no file descriptor left) would otherwise spin.
                thread::sleep(Duration::from_millis(10));
            }
        }
    }
}

/// What the response will be, before anything is sent.
enum Answer {
    Whole {
        status: u16,
        content_type: &'static str,
        body: Vec<u8>,
        /// For a model's answer: the bytes the time to write it is paid for (see [`Pacer`]). A
        /// listing or a refusal is not paced.
        paced_bytes: Option<usize>,
    },
    /// Status 200, `body` pieces paced by their text.
    Stream {
        content_type: &'static str,
        events: Vec<WireEvent>,
    },
}

impl Answer {
    fn json(status: u16, value: &Value) -> Answer {
        Answer::Whole {
            status,
            content_type: "application/json",
            body: json::compact(value).into_bytes(),
            paced_bytes: None,
        }
    }

    fn text(status: u16, text: &str) -> Answer {
        Answer::Whole {
            status,
            content_type: "text/plain",
            body: text.as_bytes().to_vec(),
            paced_bytes: None,
        }
    }

    /// A model's answer written whole, after the time a stream of it would take.
    fn paced_json(value: &Value, paced_bytes: usize) -> Answer {
        Answer::Whole {
            status: 200,
            content_type: "application/json",
            body: json::compact(value).into_bytes(),
            paced_bytes: Some(paced_bytes),
        }
    }

    fn status(&self) -> u16 {
        match self {
            Answer::Whole { status, .. } => *status,
            Answer::Stream { .. } => 200,
        }
    }
}

fn serve_connection(shared: &Shared, stream: TcpStream) {
    if stream.set_nodelay(true).is_err()
        || stream
            .set_read_timeout(Some(shared.config.connection_timeout))
            .is_err()
        || stream
            .set_write_timeout(Some(shared.config.connection_timeout))
            .is_err()
    {
        return;
    }
    let Ok(read_half) = stream.try_clone() else {
        return;
    };
    let mut reader = BufReader::new(read_half);
    let mut writer = stream;

    let request = match http::read_request_keeping_request_line(&mut reader, &mut writer) {
        Ok(request) => request,
        // No request began, or the connection broke: nothing to answer or to journal.
        Err(Unreadable {
            error: ReadError::Closed | ReadError::Io(_),
            ..
        }) => return,
        Err(unreadable) => {
            refuse_unreadable(shared, &unreadable, &mut writer);
            return;
        }
    };

    let mut entry = shared.begin(&request.method, &request.path);
    let answer = answer(shared, &request, &mut entry);
    entry.status = answer.status();
    entry.response_bytes = send(shared, &mut writer, &answer);
    shared.record(entry);
}

/// Answers a request that could not be read, journals it, and closes the connection politely.
fn refuse_unreadable(shared: &Shared, unreadable: &Unreadable, writer: &mut TcpStream) {
    let (status, message) = match &unreadable.error {
        ReadError::Malformed(message) => (400, message.clone()),
        ReadError::TooLarge => (413, format!("the body is over {} bytes", http::MAX_BODY)),
        ReadError::Encoded(encoding) => {
            (415, format!("Content-Encoding {encoding} is not supported"))
        }
        ReadError::Closed | ReadError::Io(_) => return,
    };
    let mut entry = shared.begin(&unreadable.method, &unreadable.path);
    let answer = refusal(&mut entry, status, message);
    entry.status = status;
    entry.response_bytes = send(shared, writer, &answer);
    shared.record(entry);

    // The client may still be sending: read it out before closing, or the reset could cut our
    // answer.
    if writer.shutdown(Shutdown::Write).is_ok()
        && writer.set_read_timeout(Some(DRAIN_TIMEOUT)).is_ok()
    {
        let _ = io::copy(&mut writer.take(DRAIN_LIMIT), &mut io::sink());
    }
}

fn refusal(entry: &mut JournalEntry, status: u16, message: String) -> Answer {
    entry.kind = Kind::Refused;
    let body = json!({"error": {"type": "invalid_request_error", "message": message}});
    entry.error = Some(message);
    Answer::json(status, &body)
}

/// Chooses the answer to a request and fills the entry's account of it.
fn answer(shared: &Shared, request: &Request, entry: &mut JournalEntry) -> Answer {
    let config = &shared.config;
    match (request.method.as_str(), request.path.as_str()) {
        ("OPTIONS", _) => {
            entry.kind = Kind::Preflight;
            Answer::text(204, "")
        }
        ("HEAD", _) => listing(entry, Answer::text(200, "")),
        ("GET", "/") => listing(entry, Answer::text(200, "Ollama is running")),
        ("GET", "/api/version") => listing(
            entry,
            Answer::json(200, &json!({"version": OLLAMA_VERSION})),
        ),
        ("GET", "/api/tags") => listing(entry, Answer::json(200, &tags(&config.models))),
        ("POST", "/api/show") => listing(entry, Answer::json(200, &show())),
        ("GET", "/v1/models" | "/models") => {
            listing(entry, Answer::json(200, &models(&config.models)))
        }
        ("POST", "/v1/messages/count_tokens") => {
            let input_tokens = wire::tokens(request.body.len());
            listing(
                entry,
                Answer::json(200, &json!({"input_tokens": input_tokens})),
            )
        }
        ("POST", "/v1/messages") => model_answer(shared, Api::Anthropic, request, entry),
        ("POST", "/v1/responses" | "/responses") => {
            model_answer(shared, Api::Responses, request, entry)
        }
        ("POST", "/api/chat") => model_answer(shared, Api::Ollama, request, entry),
        _ => {
            entry.kind = Kind::Unknown;
            let message = format!("no route for {} {}", request.method, request.path);
            Answer::json(
                404,
                &json!({"error": {"type": "not_found", "message": message}}),
            )
        }
    }
}

fn listing(entry: &mut JournalEntry, answer: Answer) -> Answer {
    entry.kind = Kind::Listing;
    answer
}

fn model_details() -> Value {
    json!({
        "parent_model": "",
        "format": "gguf",
        "family": "fake",
        "families": ["fake"],
        "parameter_size": "1B",
        "quantization_level": "Q4_0"
    })
}

fn tags(models: &[String]) -> Value {
    let models: Vec<Value> = models
        .iter()
        .map(|name| {
            json!({
                "name": name,
                "model": name,
                "modified_at": MODEL_CREATED_AT,
                "size": 1,
                "digest": "0".repeat(64),
                "details": model_details()
            })
        })
        .collect();
    json!({"models": models})
}

fn show() -> Value {
    json!({
        "modelfile": "",
        "parameters": "",
        "template": "",
        "details": model_details(),
        "model_info": {"general.architecture": "fake", "fake.context_length": CONTEXT_LENGTH},
        "capabilities": ["completion", "tools"]
    })
}

/// The list both OpenAI's and Anthropic's clients read.
fn models(models: &[String]) -> Value {
    let data: Vec<Value> = models
        .iter()
        .map(|id| {
            json!({
                "id": id,
                "object": "model",
                "type": "model",
                "display_name": id,
                "created": MODEL_CREATED,
                "created_at": MODEL_CREATED_AT,
                "owned_by": "fake"
            })
        })
        .collect();
    json!({
        "object": "list",
        "data": data,
        "has_more": false,
        "first_id": models.first(),
        "last_id": models.last()
    })
}

/// A model's answer in one API: the script's segment at the request's position, or the side answer.
fn model_answer(shared: &Shared, api: Api, request: &Request, entry: &mut JournalEntry) -> Answer {
    entry.api = Some(api);
    let body: Value = match serde_json::from_slice(&request.body) {
        Ok(body) => body,
        Err(error) => return refusal(entry, 400, format!("the body is not JSON: {error}")),
    };
    let conversation = match conversation::read(api, &body, request.body.len()) {
        Ok(conversation) => conversation,
        Err(message) => return refusal(entry, 400, message),
    };
    entry.parts = Some(conversation.parts);
    entry.tools_offered = conversation
        .tools
        .iter()
        .map(|tool| tool.name.clone())
        .collect();
    // Measured now, not when the response ends: a request read while another's stream is still
    // flowing has that one before it.
    entry.repeated_prefix = shared
        .lock_journal()
        .measure_and_remember(api, &request.body);

    let script = &shared.config.script;
    let (text, tool) = if !tools::offers_a_callable_tool(&conversation.tools) {
        entry.kind = Kind::Side;
        (script.side_answer.clone(), None)
    } else {
        entry.kind = Kind::Main;
        let position = conversation.position;
        entry.turn = Some(position.turn);
        entry.segment = Some(position.segment);
        entry.tool_result_ok = tool_result_ok(api, &conversation);
        let segment = script.segment(position);
        let call_id = call_id(api, position.turn, position.segment);
        let tool = segment
            .tool
            .as_ref()
            .and_then(|call| tools::map(call, &conversation.tools, call_id));
        entry.tool_missing = segment.tool.is_some() && tool.is_none();
        (segment.text.clone(), tool)
    };
    entry.tool = tool.as_ref().map(|tool| tool.name.clone());

    let reply = Reply {
        model: conversation.model.clone(),
        id: format!("fake_{}", entry.request),
        text,
        tool,
        input_tokens: wire::tokens(request.body.len()),
        chunk_bytes: script.pace.chunk_bytes,
    };
    encode(api, &reply, conversation.stream)
}

fn encode(api: Api, reply: &Reply, stream: bool) -> Answer {
    let paced_bytes = wire::paced_bytes(reply);
    match (api, stream) {
        (Api::Anthropic, true) => Answer::Stream {
            content_type: "text/event-stream",
            events: anthropic::events(reply),
        },
        (Api::Anthropic, false) => Answer::paced_json(&anthropic::message(reply), paced_bytes),
        (Api::Responses, true) => Answer::Stream {
            content_type: "text/event-stream",
            events: responses::events(reply),
        },
        (Api::Responses, false) => Answer::paced_json(&responses::response(reply), paced_bytes),
        (Api::Ollama, true) => Answer::Stream {
            content_type: "application/x-ndjson",
            events: ollama::lines(reply),
        },
        (Api::Ollama, false) => Answer::paced_json(&ollama::chat(reply), paced_bytes),
    }
}

/// The id of the call a segment makes, which the client sends back with the call's result.
fn call_id(api: Api, turn: usize, segment: usize) -> String {
    let prefix = if api == Api::Anthropic {
        "toolu"
    } else {
        "call"
    };
    format!("{prefix}_fake_{turn}_{segment}")
}

/// From a turn's second segment on: whether the last result answers the previous segment's call.
/// Ollama's results carry no id, so there the count of results, which put the request here, is all.
fn tool_result_ok(api: Api, conversation: &Conversation) -> Option<bool> {
    let position = conversation.position;
    let previous_segment = position.segment.checked_sub(1)?;
    if api == Api::Ollama {
        return Some(true);
    }
    let expected = call_id(api, position.turn, previous_segment);
    Some(
        conversation
            .tool_results
            .last()
            .is_some_and(|result| result.call_id == expected),
    )
}

/// Writes the answer, paced; gives the body bytes written before the client left, if it did.
fn send(shared: &Shared, writer: &mut impl Write, answer: &Answer) -> usize {
    match answer {
        Answer::Whole {
            status,
            content_type,
            body,
            paced_bytes,
        } => {
            if let Some(bytes) = paced_bytes {
                let mut pacer = Pacer::new(&shared.config);
                pacer.pay_for(*bytes);
                pacer.wait_until_due();
            }
            let mut writer = TimedWriter::new(writer, shared.config.connection_timeout);
            match http::write_full(&mut writer, *status, content_type, body) {
                Ok(()) => body.len(),
                Err(_) => 0,
            }
        }
        Answer::Stream {
            content_type,
            events,
        } => send_stream(shared, writer, content_type, events),
    }
}

fn send_stream(
    shared: &Shared,
    writer: &mut impl Write,
    content_type: &str,
    events: &[WireEvent],
) -> usize {
    let mut written = 0;
    let timeout = shared.config.connection_timeout;
    if http::write_stream_head(&mut TimedWriter::new(writer, timeout), content_type).is_err() {
        return written;
    }
    let mut pacer = Pacer::new(&shared.config);
    for event in events {
        pacer.wait_for(event);
        let mut timed = TimedWriter::new(writer, timeout);
        if http::write_chunk(&mut timed, event.data.as_bytes()).is_err() {
            // The client left, or stopped reading: the request is journaled with what it was sent.
            return written;
        }
        written += event.data.len();
        pacer.pay_for(event.text_bytes);
    }
    // The last chunk only ends the framing; the body bytes are the events'.
    pacer.wait_until_due();
    let _ = http::finish_stream(&mut TimedWriter::new(writer, timeout));
    written
}

/// Writes to a connection, and gives up once it has been at one piece of the answer for longer
/// than the connection's timeout. The connection's own write timeout (set to the same time) ends
/// each wait; this ends the retries after it, so that a client that stops reading holds its thread
/// for at most twice that time.
struct TimedWriter<'a, W> {
    inner: &'a mut W,
    deadline: Instant,
}

impl<'a, W: Write> TimedWriter<'a, W> {
    fn new(inner: &'a mut W, timeout: Duration) -> Self {
        TimedWriter {
            inner,
            deadline: Instant::now() + timeout,
        }
    }
}

impl<W: Write> Write for TimedWriter<'_, W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if Instant::now() >= self.deadline {
            return Err(io::ErrorKind::TimedOut.into());
        }
        self.inner.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

/// Spaces an answer like a model's tokens: it is due after `first_token_ms`, and each piece written
/// puts the next one back by its own bytes' time. Time is kept as an offset from the start, so that
/// a duration too long to add up saturates instead of overflowing an instant.
struct Pacer {
    start: Instant,
    seconds_per_byte: f64,
    speed: f64,
    /// When the next event with text may leave, as an offset from `start`.
    due: Duration,
    /// Whether an event with text has been reached: framing before it leaves at once, framing
    /// after it keeps its place in the stream.
    reached_text: bool,
}

impl Pacer {
    fn new(config: &Config) -> Pacer {
        let pace = config.script.pace;
        Pacer {
            start: Instant::now(),
            // Four bytes make a token.
            seconds_per_byte: 1.0 / (4.0 * pace.tokens_per_second),
            speed: config.speed,
            due: seconds(pace.first_token_ms as f64 / 1000.0 / config.speed),
            reached_text: false,
        }
    }

    /// Waits until the event is due. Framing before the first text leaves at once.
    fn wait_for(&mut self, event: &WireEvent) {
        self.reached_text |= event.text_bytes > 0;
        if self.reached_text {
            self.wait_until_due();
        }
    }

    fn wait_until_due(&self) {
        if let Some(rest) = self.due.checked_sub(self.start.elapsed())
            && !rest.is_zero()
        {
            thread::sleep(rest);
        }
    }

    /// Charges the time a piece of that many bytes takes to the pieces after it. The deadline is
    /// chained from the last one, not from the moment the piece was written, so that the time a
    /// write takes, or a sleep that overshoots, does not add up over thousands of pieces. A stream
    /// that fell behind by more than [`CATCH_UP`] starts again from now, rather than catch up in a
    /// burst; a smaller lag, a late wake-up of the machine, is made up by the pieces after it.
    fn pay_for(&mut self, bytes: usize) {
        if bytes == 0 {
            return;
        }
        // An infinite speed makes every delay zero, and a zero delay never sleeps.
        let delay = seconds(bytes as f64 * self.seconds_per_byte / self.speed);
        let now = self.start.elapsed();
        let behind = now > self.due.saturating_add(CATCH_UP);
        let base = if behind { now } else { self.due };
        self.due = base.saturating_add(delay);
    }
}

/// A duration of that many seconds. A number that cannot be a wait (negative, not a number) is no
/// wait; one too large to count is the longest wait there is.
fn seconds(value: f64) -> Duration {
    if value.is_nan() || value <= 0.0 {
        return Duration::ZERO;
    }
    Duration::try_from_secs_f64(value).unwrap_or(Duration::MAX)
}
