//! The check every committed recording and reference passes, in CI and before the owner commits
//! a new one ([ADR-0025]).
//!
//! Its detectors are its own, not the masker's, so that a pattern the masker lacks can still be
//! caught:
//!
//! - `email`: an e-mail address, outside the domains `example.invalid`, `example.com`,
//!   `example.org`, `anthropic.com` and `users.noreply.github.com`;
//! - `home-path`: a home folder written out: `/home/<name>`, `/Users/<name>`, `/root/`, their
//!   URL-encoded forms (`%2Fhome%2F`), and Claude's folder-name form (`-home-<name>`); the
//!   placeholder `<home>` is not one;
//! - `key-shape`: a token of a known form (the same list as the masker's, written again here);
//! - `random`: a run of 32 to 200 characters from `[A-Za-z0-9+/=_-]` mixing upper case, lower
//!   case and digits, with at least 4.3 bits of Shannon entropy per character; not a UUID, not
//!   hexadecimal, not an id of a known prefix (`msg_`, `toolu_`, `srvtoolu_`, `req_`, `call_`,
//!   `chatcmpl-`), and not the value of a key named `signature`, `encrypted_content` or `data`.
//!
//! Streamed text is also checked joined: the deltas of each content block of a Claude message
//! (`stream_event` / `content_block_delta`: `text`, `partial_json`, `thinking`), and the
//! consecutive `agent_message_chunk` and `agent_thought_chunk` texts of an ACP session, so that a
//! secret split across frames is still found.
//!
//! The details the list above leaves open:
//!
//! - Object keys are scanned too, the header's fields and an action's detail as well.
//! - A streamed piece is scanned only joined to its stream, never alone, so a secret inside one
//!   piece is found once; a finding there has the line of the frame it starts in.
//! - A home folder counts only at the start of a path: the byte before it is not a letter, a
//!   digit, `.`, `_`, `-` or a placeholder's `>`, so `/tmp/home/x` and `<workspace>/home/x` pass.
//! - A random run that overlaps a key shape is reported once, as the key shape.
//! - `key-shape` also covers the value after `Bearer ` when it has at least 8 characters, Basic
//!   credentials in base64, and the password of a URL (`scheme://user:password@host`). A token
//!   or a scheme also counts right after an escape written in text (`\n`) or a URL encoding
//!   (`%3D`), and a scheme's space may be written `%20`.
//!
//! [ADR-0025]: ../../../docs/adr/0025-agents-sessions-recorded-and-replayed.md

use std::collections::BTreeMap;
use std::ops::Range;

use serde_json::Value;

use crate::format::{Protocol, Record, Recording};

/// One thing the scan found. It never holds the value found, which may be a secret: only its
/// first four characters and its length.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    /// The line of the file, from 1, the header being line 1.
    pub line: usize,
    /// `email`, `home-path`, `key-shape` or `random`.
    pub detector: &'static str,
    /// The first four characters of what was found, then `…`, and its length.
    pub hint: String,
}

const ALLOWED_EMAIL_DOMAINS: &[&str] = &[
    "example.invalid",
    "example.com",
    "example.org",
    "anthropic.com",
    "users.noreply.github.com",
];

/// Home folders as written in a path; `true` when a user's name must follow.
const HOME_FOLDERS: &[(&str, bool)] = &[("/home/", true), ("/Users/", true), ("/root/", false)];
/// The same, URL-encoded, in lower case: matched without case.
const ENCODED_HOME_FOLDERS: &[(&str, bool)] = &[
    ("%2fhome%2f", true),
    ("%2fusers%2f", true),
    ("%2froot%2f", false),
];
/// The same, in Claude's folder-name form.
const CLAUDE_HOME_FOLDERS: &[(&str, bool)] =
    &[("-home-", true), ("-Users-", true), ("-root-", true)];

const RANDOM_MIN_CHARS: usize = 32;
const RANDOM_MAX_CHARS: usize = 200;
const RANDOM_MIN_BITS_PER_CHAR: f64 = 4.3;
const KNOWN_ID_PREFIXES: &[&str] = &["msg_", "toolu_", "srvtoolu_", "req_", "call_", "chatcmpl-"];
const RANDOM_EXEMPT_KEYS: &[&str] = &["signature", "encrypted_content", "data"];

/// Scans a whole recording, joined deltas included.
pub fn recording(recording: &Recording) -> Vec<Finding> {
    let mut findings = Vec::new();
    let header = &recording.header;
    let header_texts = [&header.format, &header.recorded_on, &header.scenario]
        .into_iter()
        .chain(header.meta.iter().flat_map(|(key, value)| [key, value]));
    for text in header_texts {
        scan_text(text, 1, true, &mut findings);
    }
    let protocol = header.agent.protocol();
    let mut streams = Streams::default();
    for (index, record) in recording.records.iter().enumerate() {
        let line = index + 2;
        match record {
            Record::Spawn { argv, .. } => {
                for arg in argv {
                    scan_text(arg, line, true, &mut findings);
                }
            }
            Record::Frame { process, json, .. } => {
                let piece = streams.take(protocol, *process, json, line, &mut findings);
                scan_value(json, None, line, piece, &mut findings);
            }
            Record::Line { text, .. } => scan_text(text, line, true, &mut findings),
            Record::Action { name, detail } => {
                scan_text(name, line, true, &mut findings);
                scan_value(detail, None, line, None, &mut findings);
            }
            Record::StdinClosed { .. } | Record::Exit { .. } => {}
        }
    }
    streams.finish(&mut findings);
    findings.sort_by_key(|finding| finding.line);
    findings
}

/// Scans a JSON document, such as a reference; every finding has line 1.
pub fn document(value: &Value) -> Vec<Finding> {
    let mut findings = Vec::new();
    scan_value(value, None, 1, None, &mut findings);
    findings
}

/// Scans every key and string of a value, except the node `skip`, a streamed piece scanned with
/// its stream.
fn scan_value(
    value: &Value,
    key: Option<&str>,
    line: usize,
    skip: Option<&Value>,
    findings: &mut Vec<Finding>,
) {
    if skip.is_some_and(|skip| std::ptr::eq(skip, value)) {
        return;
    }
    match value {
        Value::String(text) => {
            let random_allowed = !key.is_some_and(|key| RANDOM_EXEMPT_KEYS.contains(&key));
            scan_text(text, line, random_allowed, findings);
        }
        Value::Array(items) => {
            for item in items {
                scan_value(item, key, line, skip, findings);
            }
        }
        Value::Object(fields) => {
            for (child_key, child) in fields {
                scan_text(child_key, line, true, findings);
                scan_value(child, Some(child_key), line, skip, findings);
            }
        }
        _ => {}
    }
}

fn scan_text(text: &str, line: usize, random_allowed: bool, findings: &mut Vec<Finding>) {
    for (range, detector) in detect(text, random_allowed) {
        findings.push(Finding {
            line,
            detector,
            hint: hint(&text[range]),
        });
    }
}

/// Everything the detectors find in one text, in order of position.
fn detect(text: &str, random_allowed: bool) -> Vec<(Range<usize>, &'static str)> {
    let mut found: Vec<(Range<usize>, &'static str)> = Vec::new();
    found.extend(emails(text).into_iter().map(|range| (range, "email")));
    found.extend(
        home_paths(text)
            .into_iter()
            .map(|range| (range, "home-path")),
    );
    let key_shapes = key_shapes(text);
    if random_allowed {
        let overlaps_a_key = |run: &Range<usize>| {
            key_shapes
                .iter()
                .any(|key| key.start < run.end && run.start < key.end)
        };
        found.extend(
            random_runs(text)
                .into_iter()
                .filter(|run| !overlaps_a_key(run))
                .map(|range| (range, "random")),
        );
    }
    found.extend(key_shapes.into_iter().map(|range| (range, "key-shape")));
    found.sort_by_key(|(range, _)| range.start);
    found
}

/// The first four characters of what was found, and its length; never the whole.
fn hint(found: &str) -> String {
    let head: String = found.chars().take(4).collect();
    format!("{head}…({} chars)", found.chars().count())
}

// --- Streams -----------------------------------------------------------------------------------

/// Streamed texts being joined, by process and stream.
#[derive(Default)]
struct Streams {
    open: BTreeMap<(u32, String), Joined>,
}

/// One stream's text so far, and where each piece of it came from.
#[derive(Default)]
struct Joined {
    /// ACP's chunk kind; a message and a thought are separate streams.
    kind: String,
    text: String,
    /// Each piece's offset in `text` and its line, in order.
    pieces: Vec<(usize, usize)>,
}

/// What a frame does to the streams.
enum StreamStep<'a> {
    /// A piece of streamed text, joined to its stream.
    Piece {
        stream: String,
        kind: &'a str,
        node: &'a Value,
    },
    /// The stream ends: a block starts or stops, or the session says something else.
    End {
        stream: String,
    },
    Unrelated,
}

impl Streams {
    /// Takes a frame's streamed piece, if it has one, and returns it, so that the frame's own
    /// scan skips it; scans the streams the frame ends.
    fn take<'a>(
        &mut self,
        protocol: Protocol,
        process: u32,
        frame: &'a Value,
        line: usize,
        findings: &mut Vec<Finding>,
    ) -> Option<&'a Value> {
        let step = match protocol {
            Protocol::ClaudeStreamJson => claude_stream_step(frame),
            Protocol::AcpJsonRpc => acp_stream_step(frame),
        };
        match step {
            StreamStep::Piece { stream, kind, node } => {
                let key = (process, stream);
                let another_kind = self
                    .open
                    .get(&key)
                    .is_some_and(|joined| joined.kind != kind);
                if another_kind && let Some(joined) = self.open.remove(&key) {
                    joined.scan(findings);
                }
                let joined = self.open.entry(key).or_insert_with(|| Joined {
                    kind: kind.to_owned(),
                    ..Joined::default()
                });
                joined.pieces.push((joined.text.len(), line));
                joined.text.push_str(node.as_str().unwrap_or_default());
                Some(node)
            }
            StreamStep::End { stream } => {
                if let Some(joined) = self.open.remove(&(process, stream)) {
                    joined.scan(findings);
                }
                None
            }
            StreamStep::Unrelated => None,
        }
    }

    fn finish(self, findings: &mut Vec<Finding>) {
        for joined in self.open.into_values() {
            joined.scan(findings);
        }
    }
}

impl Joined {
    fn scan(&self, findings: &mut Vec<Finding>) {
        for (range, detector) in detect(&self.text, true) {
            let piece = self
                .pieces
                .partition_point(|(offset, _)| *offset <= range.start)
                .saturating_sub(1);
            findings.push(Finding {
                line: self.pieces.get(piece).map_or(1, |(_, line)| *line),
                detector,
                hint: hint(&self.text[range]),
            });
        }
    }
}

fn claude_stream_step(frame: &Value) -> StreamStep<'_> {
    if frame.get("type").and_then(Value::as_str) != Some("stream_event") {
        return StreamStep::Unrelated;
    }
    let event = &frame["event"];
    let parent = frame
        .get("parent_tool_use_id")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let stream = format!("{parent}/{}", event["index"]);
    match event.get("type").and_then(Value::as_str) {
        Some("content_block_delta") => ["text", "partial_json", "thinking"]
            .into_iter()
            .find_map(|field| event["delta"].get(field).filter(|node| node.is_string()))
            .map_or(StreamStep::Unrelated, |node| StreamStep::Piece {
                stream,
                kind: "",
                node,
            }),
        Some("content_block_start" | "content_block_stop") => StreamStep::End { stream },
        _ => StreamStep::Unrelated,
    }
}

fn acp_stream_step(frame: &Value) -> StreamStep<'_> {
    if frame.get("method").and_then(Value::as_str) != Some("session/update") {
        return StreamStep::Unrelated;
    }
    let params = &frame["params"];
    let stream = params
        .get("sessionId")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let update = &params["update"];
    let kind = update
        .get("sessionUpdate")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let text = update["content"]
        .get("text")
        .filter(|node| node.is_string());
    match (kind, text) {
        ("agent_message_chunk" | "agent_thought_chunk", Some(node)) => {
            StreamStep::Piece { stream, kind, node }
        }
        _ => StreamStep::End { stream },
    }
}

// --- email -------------------------------------------------------------------------------------

fn emails(text: &str) -> Vec<Range<usize>> {
    let bytes = text.as_bytes();
    let mut found = Vec::new();
    for (at, _) in text.match_indices('@') {
        // `@` is in neither class, so each byte is read by at most the `@` on either side of it.
        let local_start = bytes[..at]
            .iter()
            .rposition(|&byte| !is_local_part_byte(byte))
            .map_or(0, |index| index + 1);
        let domain_end = bytes[at + 1..]
            .iter()
            .position(|&byte| !is_domain_byte(byte))
            .map_or(bytes.len(), |length| at + 1 + length);
        let domain = text[at + 1..domain_end].trim_end_matches(['.', '-']);
        if local_start < at && is_mail_domain(domain) && !is_allowed_domain(domain) {
            found.push(local_start..at + 1 + domain.len());
        }
    }
    found
}

fn is_local_part_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || b"._%+-".contains(&byte)
}

fn is_domain_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'.' || byte == b'-'
}

/// Labels joined by dots, the last one two letters or more: `corp.example`, not `co` nor
/// `4.17.21`.
fn is_mail_domain(domain: &str) -> bool {
    let labels: Vec<&str> = domain.split('.').collect();
    let top = labels.last().copied().unwrap_or_default();
    labels.len() >= 2
        && labels.iter().all(|label| !label.is_empty())
        && top.len() >= 2
        && top.bytes().all(|byte| byte.is_ascii_alphabetic())
}

fn is_allowed_domain(domain: &str) -> bool {
    let domain = domain.to_ascii_lowercase();
    ALLOWED_EMAIL_DOMAINS.iter().any(|allowed| {
        domain == *allowed
            || domain
                .strip_suffix(allowed)
                .is_some_and(|sub| sub.ends_with('.'))
    })
}

// --- home-path ---------------------------------------------------------------------------------

fn home_paths(text: &str) -> Vec<Range<usize>> {
    let mut found = Vec::new();
    find_home_folders(text, text, HOME_FOLDERS, &mut found);
    if text.contains('%') {
        let lower = text.to_ascii_lowercase();
        find_home_folders(text, &lower, ENCODED_HOME_FOLDERS, &mut found);
    }
    find_home_folders(text, text, CLAUDE_HOME_FOLDERS, &mut found);
    found
}

/// Finds each folder of `folders` in `haystack`, which is `text` or its ASCII lower case (same
/// offsets), where it starts a path; the range runs to the end of the name after it.
fn find_home_folders(
    text: &str,
    haystack: &str,
    folders: &[(&str, bool)],
    found: &mut Vec<Range<usize>>,
) {
    let bytes = text.as_bytes();
    for (folder, needs_a_name) in folders {
        for (start, _) in haystack.match_indices(folder) {
            let starts_a_path =
                start == 0 || (!is_path_name_byte(bytes[start - 1]) && bytes[start - 1] != b'>');
            let name_start = start + folder.len();
            let name_end = bytes[name_start..]
                .iter()
                .position(|&byte| !is_path_name_byte(byte))
                .map_or(bytes.len(), |length| name_start + length);
            if starts_a_path && (name_end > name_start || !needs_a_name) {
                found.push(start..name_end);
            }
        }
    }
}

fn is_path_name_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || b"._-".contains(&byte)
}

// --- key-shape ---------------------------------------------------------------------------------

/// Tokens of known forms. Unlike the masker, which tries each position, this reads the text a
/// word at a time: a word is a run of letters, digits, `_` and `-`.
fn key_shapes(text: &str) -> Vec<Range<usize>> {
    let bytes = text.as_bytes();
    let mut found = Vec::new();
    for word in words(bytes, is_word_byte) {
        for start in token_starts(bytes, &word) {
            let token = &bytes[start..word.end];
            if is_prefixed_token(token) || is_aws_access_key(token) {
                found.push(start..word.end);
                break;
            }
            if token.starts_with(b"eyJ")
                && let Some(end) = json_web_token_end(bytes, start..word.end)
            {
                found.push(start..end);
                break;
            }
        }
    }
    found.extend(private_keys(text));
    found.extend(values_after_scheme(
        text,
        "bearer",
        is_b64token_byte,
        |value| value.len() >= 8,
    ));
    found.extend(values_after_scheme(
        text,
        "basic",
        is_base64_byte,
        looks_like_base64,
    ));
    found.extend(url_passwords(text));
    found.sort_by_key(|range| range.start);
    found
}

fn is_word_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-'
}

/// Where a token may start in a word: at the word's start, and after the escape (`\n` written
/// as two characters) or the URL encoding (`%3D`) that the word starts with, whose letters and
/// digits join the word.
fn token_starts(bytes: &[u8], word: &Range<usize>) -> impl Iterator<Item = usize> {
    let before = word.start.checked_sub(1).map(|index| bytes[index]);
    let first = &bytes[word.clone()];
    let after_escape = before == Some(b'\\') && first.len() > 1 && b"nrtbf".contains(&first[0]);
    let after_encoding = before == Some(b'%')
        && first.len() > 2
        && first[0].is_ascii_hexdigit()
        && first[1].is_ascii_hexdigit();
    [
        Some(word.start),
        after_escape.then_some(word.start + 1),
        after_encoding.then_some(word.start + 2),
    ]
    .into_iter()
    .flatten()
}

/// Whether a word that a search found at `at` starts there: after a byte that is no word byte,
/// or right after an escape or a URL encoding, as in [`token_starts`].
fn starts_a_word(bytes: &[u8], at: usize) -> bool {
    let Some(before) = at.checked_sub(1).map(|index| bytes[index]) else {
        return true;
    };
    let escape = at >= 2 && bytes[at - 2] == b'\\' && b"nrtbf".contains(&before);
    let encoding = at >= 3
        && bytes[at - 3] == b'%'
        && bytes[at - 2].is_ascii_hexdigit()
        && before.is_ascii_hexdigit();
    !is_word_byte(before) || escape || encoding
}

/// Every maximal run of bytes that `belongs` accepts.
fn words(bytes: &[u8], belongs: fn(u8) -> bool) -> Vec<Range<usize>> {
    let mut runs = Vec::new();
    let mut start = None;
    for (index, &byte) in bytes.iter().enumerate() {
        match (belongs(byte), start) {
            (true, None) => start = Some(index),
            (false, Some(run_start)) => {
                runs.push(run_start..index);
                start = None;
            }
            _ => {}
        }
    }
    if let Some(run_start) = start {
        runs.push(run_start..bytes.len());
    }
    runs
}

fn is_prefixed_token(word: &[u8]) -> bool {
    const PREFIXES: &[&[u8]] = &[
        b"sk-",
        b"xai-",
        b"ghp_",
        b"gho_",
        b"ghu_",
        b"ghs_",
        b"ghr_",
        b"github_pat_",
        b"glpat-",
    ];
    let slack = word.len() >= 5 + 10
        && word.starts_with(b"xox")
        && word[3].is_ascii_lowercase()
        && word[4] == b'-';
    slack
        || PREFIXES
            .iter()
            .any(|prefix| word.starts_with(prefix) && word.len() >= prefix.len() + 20)
}

fn is_aws_access_key(word: &[u8]) -> bool {
    word.len() == 20
        && word.starts_with(b"AKIA")
        && word
            .iter()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
}

/// Given a word starting `eyJ`, the end of the JSON Web Token it begins: a `.`, a second word
/// starting `eyJ`, and an optional `.` and signature.
fn json_web_token_end(bytes: &[u8], header: Range<usize>) -> Option<usize> {
    let word_end = |from: usize| {
        bytes[from..]
            .iter()
            .position(|&byte| !is_word_byte(byte))
            .map_or(bytes.len(), |length| from + length)
    };
    if header.len() < 10 || bytes.get(header.end) != Some(&b'.') {
        return None;
    }
    let payload_start = header.end + 1;
    let payload_end = word_end(payload_start);
    if payload_end - payload_start < 10 || !bytes[payload_start..].starts_with(b"eyJ") {
        return None;
    }
    if bytes.get(payload_end) == Some(&b'.') {
        Some(word_end(payload_end + 1))
    } else {
        Some(payload_end)
    }
}

/// `-----BEGIN … PRIVATE KEY-----`: the armour line is enough to report.
fn private_keys(text: &str) -> Vec<Range<usize>> {
    const BEGIN: &str = "-----BEGIN ";
    text.match_indices(BEGIN)
        .filter_map(|(start, _)| {
            let label_start = start + BEGIN.len();
            let rest = &text[label_start..];
            let label_end = rest.find("-----").filter(|&end| end <= 64)?;
            rest[..label_end]
                .ends_with("PRIVATE KEY")
                .then_some(start..label_start + label_end + 5)
        })
        .collect()
}

/// The values after an authentication scheme (any case) and its spaces, tabs or `%20`, made of
/// `is_value_byte` and accepted by `accepts`: Bearer's of at least 8 characters of RFC 6750's
/// `b64token`, and Basic's base64 credentials.
fn values_after_scheme(
    text: &str,
    scheme: &str,
    is_value_byte: fn(u8) -> bool,
    accepts: fn(&[u8]) -> bool,
) -> Vec<Range<usize>> {
    let lower = text.to_ascii_lowercase();
    let bytes = text.as_bytes();
    let mut found = Vec::new();
    for (start, _) in lower.match_indices(scheme) {
        if !starts_a_word(bytes, start) {
            continue;
        }
        let after = start + scheme.len();
        let mut value_start = after;
        loop {
            match &bytes[value_start..] {
                [b' ' | b'\t', ..] => value_start += 1,
                [b'%', b'2', b'0', ..] => value_start += 3,
                _ => break,
            }
        }
        let value_end = value_start
            + bytes[value_start..]
                .iter()
                .take_while(|&&byte| is_value_byte(byte))
                .count();
        if value_start > after && accepts(&bytes[value_start..value_end]) {
            found.push(value_start..value_end);
        }
    }
    found
}

fn is_b64token_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || b"-._~+/=".contains(&byte)
}

fn is_base64_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || b"+/=".contains(&byte)
}

/// Base64 rather than a word of prose: at least 8 characters, a multiple of 4, `=` only at the
/// end, and something other than a lower-case letter after the first character.
fn looks_like_base64(value: &[u8]) -> bool {
    let padding = value.iter().rev().take_while(|&&byte| byte == b'=').count();
    value.len() >= 8
        && value.len().is_multiple_of(4)
        && padding <= 2
        && !value[..value.len() - padding].contains(&b'=')
        && value[1..].iter().any(|byte| !byte.is_ascii_lowercase())
}

/// The passwords of URLs, `scheme://user:password@host`.
fn url_passwords(text: &str) -> Vec<Range<usize>> {
    let bytes = text.as_bytes();
    let mut found = Vec::new();
    for (scheme_end, _) in text.match_indices("://") {
        let authority = scheme_end + 3;
        // The user's part ends at `@`; anything that ends an authority first means there is none.
        let Some(length) = bytes[authority..].iter().position(|&byte| {
            byte == b'@' || byte.is_ascii_whitespace() || b"/?#\"'<>\\".contains(&byte)
        }) else {
            continue;
        };
        let at_sign = authority + length;
        if bytes[at_sign] != b'@' {
            continue;
        }
        if let Some(colon) = text[authority..at_sign].find(':')
            && authority + colon + 1 < at_sign
        {
            found.push(authority + colon + 1..at_sign);
        }
    }
    found
}

// --- random ------------------------------------------------------------------------------------

fn random_runs(text: &str) -> Vec<Range<usize>> {
    let bytes = text.as_bytes();
    words(bytes, is_random_byte)
        .into_iter()
        .filter(|run| looks_random(&bytes[run.clone()]))
        .collect()
}

fn is_random_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || b"+/=_-".contains(&byte)
}

fn looks_random(run: &[u8]) -> bool {
    (RANDOM_MIN_CHARS..=RANDOM_MAX_CHARS).contains(&run.len())
        && run.iter().any(u8::is_ascii_uppercase)
        && run.iter().any(u8::is_ascii_lowercase)
        && run.iter().any(u8::is_ascii_digit)
        && !is_uuid(run)
        && !run.iter().all(u8::is_ascii_hexdigit)
        && !KNOWN_ID_PREFIXES
            .iter()
            .any(|prefix| run.starts_with(prefix.as_bytes()))
        && bits_per_char(run) >= RANDOM_MIN_BITS_PER_CHAR
}

fn is_uuid(run: &[u8]) -> bool {
    run.len() == 36
        && run.iter().enumerate().all(|(index, byte)| match index {
            8 | 13 | 18 | 23 => *byte == b'-',
            _ => byte.is_ascii_hexdigit(),
        })
}

/// The Shannon entropy of a run of ASCII bytes, in bits per character.
fn bits_per_char(run: &[u8]) -> f64 {
    let mut counts = [0usize; 128];
    for &byte in run {
        counts[usize::from(byte & 0x7f)] += 1;
    }
    let length = run.len() as f64;
    counts
        .iter()
        .filter(|&&count| count > 0)
        .map(|&count| {
            let share = count as f64 / length;
            -share * share.log2()
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn detectors(value: Value) -> Vec<&'static str> {
        document(&value)
            .into_iter()
            .map(|finding| finding.detector)
            .collect()
    }

    #[test]
    fn a_hint_never_holds_the_value() {
        let found = document(&json!("mail alice@corp.example"));
        assert_eq!(found[0].hint, "alic…(18 chars)");
    }

    #[test]
    fn versions_folders_in_other_places_and_placeholders_pass() {
        let clean = json!([
            "lodash@4.17.21",
            "@types/node",
            "<masked:user>@<masked:host>",
            "/tmp/home/x",
            "<workspace>/home/x",
            "<home>/.config",
            "work-from-home-office",
            "Bearer token",
            "-----BEGIN CERTIFICATE-----",
            "sk-learn",
            "noreply@users.noreply.github.com"
        ]);
        assert_eq!(detectors(clean), Vec::<&str>::new());
    }

    #[test]
    fn keys_and_bearer_values_are_scanned() {
        assert_eq!(detectors(json!({"/home/alice/x": 1})), ["home-path"]);
        assert_eq!(
            detectors(json!("Authorization: Bearer abcdefgh12")),
            ["key-shape"]
        );
        assert_eq!(
            detectors(json!("-----BEGIN EC PRIVATE KEY-----\nMHcCAQEE")),
            ["key-shape"]
        );
    }

    #[test]
    fn basic_credentials_and_url_passwords_are_key_shapes_until_masked() {
        assert_eq!(
            detectors(json!("Basic dXNlcjpwYXNzd29yZA==")),
            ["key-shape"]
        );
        assert_eq!(
            detectors(json!("https://bob:pw12@git.example.com/")),
            ["key-shape"]
        );
        let masked = json!([
            "Basic <masked:token>",
            "https://<masked:user>:<masked:token>@git.example.com/",
            "Basic configuration",
            "https://git.example.com/a:b@c"
        ]);
        assert_eq!(detectors(masked), Vec::<&str>::new());
    }

    #[test]
    fn a_token_after_an_escape_is_a_key_shape() {
        let text = "a\\nsk-ant-api03-AbCdEfGhIjKlMnOpQrStUvWxYz0123456789";
        assert_eq!(detectors(json!(text)), ["key-shape"]);
    }

    #[test]
    fn a_random_run_under_an_exempt_key_passes() {
        let run = "Zx9Qw3Er7Ty1Ui5Op2As8Df4Gh6Jk0Lz3Xc";
        assert_eq!(detectors(json!({"data": run})), Vec::<&str>::new());
        assert_eq!(detectors(json!({"text": run})), ["random"]);
    }
}
