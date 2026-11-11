//! What is kept out of a recording, applied to each line before it is written ([ADR-0025],
//! [ADR-0009]: the patterns live here, in one place, with tests).
//!
//! Three layers, in this order, on decoded JSON values rather than raw text:
//!
//! 1. **Stripping**, frame type by frame type, of what is personal without looking secret:
//!    - Both: the string value of a key naming the account, anywhere in a frame (the keys of
//!      `ACCOUNT_KEYS`, such as `organization`, `subscription_tier_display` or
//!      `credential_expires_at`), becomes `<account>`, or `recorder@example.invalid` for an
//!      e-mail.
//!    - Claude: the `initialize` answer's `account` (e-mail, organisation, subscription) becomes
//!      fixed placeholders, and its `commands` and `agents`, which list the plugins and agents of
//!      the owner's account, become empty lists; `system/init`'s `agents`, `skills`,
//!      `slash_commands`, `terminal_slash_commands`, `plugins` and `mcp_servers`, and
//!      `system/commands_changed`'s `commands`, become empty lists; `system/memory_recall`'s
//!      memories are emptied; a `rate_limit_event` keeps only its `status`.
//!    - Grok (ACP): the `initialize` result's `_meta.agentId` and `_meta.agentInstanceId` become
//!      the nil-shaped id `00000000-0000-4000-8000-000000000000`, `_meta.hostname` becomes
//!      `<host>`, and commands whose `_meta.scope` is `"user"` leave every `availableCommands`
//!      list, in `initialize` and in `available_commands_update`.
//! 2. **Placeholders** for the ids the client chose, numbered per process from 1 in order of
//!    appearance: `<id:N>`. Claude: a client `control_request`'s `request_id`, a client
//!    `user` frame's `uuid` and `session_id`, and the values of `--session-id` and `--resume` in
//!    the arguments. ACP: the `id` of every request the client sends, and the `traceId` and
//!    `spanId` of every client frame. Every later occurrence of
//!    such a value, in either direction, whole or inside a longer string, becomes the same
//!    placeholder. The project's folder becomes `<workspace>` and every home folder `<home>`,
//!    also in their URL-encoded form and in Claude's folder-name form (`/home/u/p` written
//!    `-home-u-p`).
//! 3. **Masking by shape**, on every string value: tokens of known forms (`sk-ant-…`, `sk-…`,
//!    `xai-…`, `ghp_…` and GitHub's other prefixes, `github_pat_…`, `glpat-…`, `AKIA…`,
//!    `xox?-…`, JSON Web Tokens, PEM private keys), the value after `Bearer `, the whole value of
//!    a key named like a secret (`authorization`, `token`, `access_token`, `refresh_token`,
//!    `id_token`, `api_key`, `apikey`, `x-api-key`, `password`, `secret`, `client_secret`,
//!    `credential`, `credentials`, `cookie`, `set-cookie`, matched exactly and without case, on
//!    string values only), and the user's and host's names where they appear in a path, a URL or
//!    `user@host`. A masked value becomes `<masked:KIND>` and is learned: every later occurrence,
//!    whole or inside a longer string, is masked too.
//!
//! The details the list above leaves open:
//!
//! - Claude's account keeps its keys: a key naming an e-mail gets `recorder@example.invalid`,
//!   every other string `<account>`. Each string stripping replaces (the account's, Grok's host
//!   name and machine ids) is learned, so that a later quote of it is masked; a list stripping
//!   empties is not, its names being ordinary words.
//! - `KIND` is `token`, `bearer`, `secret-key`, `private-key`, `user`, `host` or `learned`.
//! - Shapes also cover, in any string: Basic credentials (`Basic` and base64 of 8 characters or
//!   more), a URL's password (`scheme://user:password@host`), and a key named like a secret
//!   written in text (`"api_key":"…"`, `PASSWORD=…`, `token: …`), whose value runs to the next
//!   quote, space, `;`, `,` or `\`; an `authorization`'s takes its scheme and its credentials.
//! - A token, a scheme, a key or a PEM key starts after a byte that is not a letter, a digit,
//!   `_` or `-`, or right after an escape written in text (`\n`, `\t`) or a URL encoding
//!   (`%3D`). A scheme's space may be written `%20`.
//! - A value is learned only when it has at least 8 characters, and a value that starts with
//!   `Bearer ` teaches the token after it too. The user's and host's names are never learned:
//!   outside a path they are ordinary words.
//! - An id the client chose is replaced inside a longer string only when it has at least 8
//!   characters; a shorter one only where it is the whole string. An ACP message's own `id` is
//!   replaced only in a response, never in a request the agent sends.
//! - Folders are replaced longest first, the workspace first between equals, and only absolute
//!   ones: a relative one is ignored. Claude's
//!   folder-name form is written both ways Claude has written it: `/` and `.` as `-`, and every
//!   character but a letter or a digit as `-`.
//! - Object keys are not masked; the scan still checks them.
//!
//! Every pass reads a string once, or once per value learned or id seen, so masking stays linear
//! in the size of what it reads.
//!
//! [ADR-0025]: ../../../docs/adr/0025-agents-sessions-recorded-and-replayed.md
//! [ADR-0009]: ../../../docs/adr/0009-secrets-privacy-and-the-sandbox.md

use std::ops::Range;
use std::path::Path;

use serde_json::{Map, Value};

use crate::format::{Agent, Protocol, Side};

/// The machine's places and names that must not reach a recording. Folders are absolute paths:
/// a relative one is ignored, since `.` would match all over the text.
#[derive(Clone, Debug, Default)]
pub struct Places {
    /// The project the agent works in; becomes `<workspace>`.
    pub workspace: Option<String>,
    /// Every home folder in play: the disposable one the agent runs with, and the owner's real
    /// one; each becomes `<home>`.
    pub homes: Vec<String>,
    /// The user's login name, masked where it appears in a path, a URL or `user@host`.
    pub user: Option<String>,
    /// The machine's host name, masked where it appears in a path, a URL or `user@host`.
    pub host: Option<String>,
}

/// What Grok's machine ids become: the shape of a UUID, the value of none.
const NIL_AGENT_ID: &str = "00000000-0000-4000-8000-000000000000";
/// What an e-mail address in Claude's account becomes: a domain reserved never to exist.
const ACCOUNT_EMAIL: &str = "recorder@example.invalid";
/// What every other string of Claude's account becomes.
const ACCOUNT_FIELD: &str = "<account>";

/// Keys whose string values are secrets whatever they look like; compared without case.
const SECRET_KEYS: &[&str] = &[
    "authorization",
    "token",
    "access_token",
    "refresh_token",
    "id_token",
    "api_key",
    "apikey",
    "x-api-key",
    "password",
    "secret",
    "client_secret",
    "credential",
    "credentials",
    "cookie",
    "set-cookie",
];

/// Whether a byte, in lower case, is the first of one of [`SECRET_KEYS`]: most words of prose
/// are not, and are passed over before any name is compared.
const STARTS_A_SECRET_KEY: [bool; 256] = {
    let mut table = [false; 256];
    let mut index = 0;
    while index < SECRET_KEYS.len() {
        table[SECRET_KEYS[index].as_bytes()[0] as usize] = true;
        index += 1;
    }
    table
};

/// Claude's flags whose value is an id the client chose.
const CLAUDE_ID_FLAGS: &[&str] = &["--session-id", "--resume"];

/// A masked value shorter than this is not learned: it would match ordinary text by chance.
const MIN_LEARNED_CHARS: usize = 8;
/// A client's id shorter than this is replaced only where it is a whole string, for the same
/// reason.
const MIN_ID_IN_TEXT_BYTES: usize = 8;

/// Token prefixes and the fewest characters a token has after its prefix. Slack's `xox?-` is
/// matched apart, its fourth letter being any.
const TOKEN_PREFIXES: &[(&str, usize)] = &[
    ("sk-", 20),
    ("xai-", 20),
    ("ghp_", 20),
    ("gho_", 20),
    ("ghu_", 20),
    ("ghs_", 20),
    ("ghr_", 20),
    ("github_pat_", 20),
    ("glpat-", 20),
];
const SLACK_MIN_CHARS: usize = 10;
/// A bearer value shorter than this is a word of prose ("Bearer token"), not a credential.
const BEARER_MIN_CHARS: usize = 8;
/// The shortest Basic credentials: `user:pw` in base64 is already 12.
const BASIC_MIN_CHARS: usize = 8;
/// A JSON Web Token's header and payload each have at least this many characters.
const JWT_PART_MIN_CHARS: usize = 10;
/// How far a PEM armour line's label may run before its closing dashes.
const PEM_LABEL_MAX_BYTES: usize = 64;

/// The masker of one process's traffic. It learns as it goes: ids the client chose, and values
/// it masked. A new process gets a new masker.
pub struct Masker {
    agent: Agent,
    scrubber: Scrubber,
    /// The ids the client chose, the longest string first, so that an id inside another is
    /// replaced after it.
    client_ids: Vec<ClientId>,
}

/// One id the client chose, and the placeholder it became.
struct ClientId {
    value: Value,
    placeholder: String,
}

impl Masker {
    pub fn new(agent: Agent, places: Places) -> Masker {
        Masker {
            agent,
            scrubber: Scrubber::new(&places),
            client_ids: Vec::new(),
        }
    }

    /// Masks the CLI's arguments; returns them and whether masking (not placeholders) changed
    /// any. The first argument is reduced to its file name.
    pub fn argv(&mut self, argv: &[String]) -> (Vec<String>, bool) {
        let mut masked_argv = Vec::with_capacity(argv.len());
        let mut previous: Option<&str> = None;
        for (index, arg) in argv.iter().enumerate() {
            let mut arg = if index == 0 {
                program_name(arg)
            } else {
                arg.clone()
            };
            if self.agent == Agent::Claude
                && let Some(start) = claude_id_start(previous, &arg)
                && start < arg.len()
            {
                let placeholder = self.placeholder_for(Value::String(arg[start..].to_owned()));
                arg.replace_range(start.., &placeholder);
            }
            previous = Some(&argv[index]);
            masked_argv.push(arg);
        }
        let mut masked = false;
        for arg in &mut masked_argv {
            masked |= self.scrubber.mask_shapes(arg);
        }
        for arg in &mut masked_argv {
            masked |= self.finish_text(arg);
        }
        (masked_argv, masked)
    }

    /// Masks one frame in place; returns whether masking (not placeholders) changed it.
    pub fn frame(&mut self, from: Side, frame: &mut Value) -> bool {
        let mut stripped = Vec::new();
        let mut masked = from == Side::Agent && strip(self.agent, frame, &mut stripped);
        // What stripping replaced is learned, so that a later quote of it is masked too.
        for value in &stripped {
            self.scrubber.learn(value);
        }
        if from == Side::Client {
            self.replace_client_ids(frame);
        }
        // An ACP message's own id is set aside while its strings are masked: an agent's request
        // may reuse a number or a string the client chose, and stays the agent's.
        let acp = self.agent.protocol() == Protocol::AcpJsonRpc;
        let message_id = if acp {
            frame.get_mut("id").map(Value::take)
        } else {
            None
        };
        masked |= self.scrubber.mask_secret_keys(frame);
        for_each_string(frame, None, &mut |_, text| {
            masked |= self.scrubber.mask_shapes(text);
        });
        for_each_string(frame, None, &mut |_, text| {
            masked |= self.finish_text(text);
        });
        if let Some(id) = message_id {
            let is_response = frame.get("method").is_none();
            let id = match self.known_id(&id) {
                Some(placeholder) if from == Side::Agent && is_response => {
                    Value::String(placeholder.to_owned())
                }
                _ => id,
            };
            if let Some(slot) = frame.get_mut("id") {
                *slot = id;
            }
        }
        masked
    }

    /// Masks a line that is not JSON; returns it and whether masking changed it.
    pub fn line(&mut self, text: &str) -> (String, bool) {
        let mut text = text.to_owned();
        let mut masked = self.scrubber.mask_shapes(&mut text);
        masked |= self.finish_text(&mut text);
        (text, masked)
    }

    /// Replaces the ids a client frame chose by their placeholders, numbering the new ones.
    fn replace_client_ids(&mut self, frame: &mut Value) {
        let fields: &[&str] = match self.agent.protocol() {
            Protocol::ClaudeStreamJson => match frame.get("type").and_then(Value::as_str) {
                Some("control_request") => &["request_id"],
                Some("user") => &["uuid", "session_id"],
                _ => &[],
            },
            // The fork adds a random `traceId` and `spanId` to every frame it sends.
            Protocol::AcpJsonRpc if frame.get("method").is_some() => &["id", "traceId", "spanId"],
            Protocol::AcpJsonRpc => &["traceId", "spanId"],
        };
        for field in fields {
            let Some(slot) = frame.get_mut(*field) else {
                continue;
            };
            let is_id = match &*slot {
                Value::String(text) => !text.is_empty(),
                Value::Number(_) => true,
                _ => false,
            };
            if is_id {
                let placeholder = self.placeholder_for(slot.take());
                *slot = Value::String(placeholder);
            }
        }
    }

    /// The placeholder of an id, numbering it if it is new.
    fn placeholder_for(&mut self, value: Value) -> String {
        if let Some(placeholder) = self.known_id(&value) {
            return placeholder.to_owned();
        }
        let placeholder = format!("<id:{}>", self.client_ids.len() + 1);
        let length = id_text_len(&value);
        let position = self
            .client_ids
            .iter()
            .position(|id| id_text_len(&id.value) < length)
            .unwrap_or(self.client_ids.len());
        self.client_ids.insert(
            position,
            ClientId {
                value,
                placeholder: placeholder.clone(),
            },
        );
        placeholder
    }

    fn known_id(&self, value: &Value) -> Option<&str> {
        self.client_ids
            .iter()
            .find(|id| id.value == *value)
            .map(|id| id.placeholder.as_str())
    }

    /// Everything after the shapes, on one string: learned values, then ids and folders, then
    /// the user's and host's names. Returns whether masking (not placeholders) changed it.
    fn finish_text(&self, text: &mut String) -> bool {
        let mut masked = self.scrubber.mask_learned(text);
        self.replace_ids_in_text(text);
        self.scrubber.replace_folders(text);
        masked |= self.scrubber.mask_user_and_host(text);
        masked
    }

    fn replace_ids_in_text(&self, text: &mut String) {
        for id in &self.client_ids {
            let Value::String(value) = &id.value else {
                continue;
            };
            if text == value {
                *text = id.placeholder.clone();
                return;
            }
            if value.len() >= MIN_ID_IN_TEXT_BYTES {
                replace_all(text, value, &id.placeholder);
            }
        }
    }
}

/// Masks a JSON document that is not a frame, such as a scenario's reference: places and shapes
/// only, no stripping and no ids. Returns whether anything changed.
pub fn document(places: &Places, value: &mut Value) -> bool {
    let mut scrubber = Scrubber::new(places);
    let mut changed = scrubber.mask_secret_keys(value);
    for_each_string(value, None, &mut |_, text| {
        changed |= scrubber.mask_shapes(text);
    });
    for_each_string(value, None, &mut |_, text| {
        changed |= scrubber.mask_learned(text);
        changed |= scrubber.replace_folders(text);
        changed |= scrubber.mask_user_and_host(text);
    });
    changed
}

/// The masking that needs neither an agent nor ids: secret keys, shapes, learned values,
/// folders, and the user's and host's names. A [`Masker`] adds stripping and ids to it.
struct Scrubber {
    /// Longest first, the workspace first between equals.
    folders: Vec<Folder>,
    /// Lower case, matched without case.
    user: Option<String>,
    /// Lower case, matched without case.
    host: Option<String>,
    /// Values masked once, the longest first, masked wherever they appear again.
    learned: Vec<String>,
}

/// One folder, in each form it is written in.
struct Folder {
    plain: String,
    /// `/` written `%2F`, in lower case: matched without case.
    url_encoded: String,
    /// Claude's folder-name forms, one or two.
    claude_names: Vec<String>,
    placeholder: &'static str,
}

impl Scrubber {
    fn new(places: &Places) -> Scrubber {
        let named = places
            .workspace
            .iter()
            .map(|path| (path, "<workspace>"))
            .chain(places.homes.iter().map(|path| (path, "<home>")));
        let mut folders: Vec<Folder> = Vec::new();
        for (path, placeholder) in named {
            let plain = path.trim_end_matches('/');
            // A relative folder, such as `.`, would match all over the text.
            let absolute = plain.starts_with('/');
            if !absolute || folders.iter().any(|folder| folder.plain == plain) {
                continue;
            }
            folders.push(Folder::new(plain, placeholder));
        }
        // A stable sort keeps the workspace ahead of a home of the same length.
        folders.sort_by_key(|folder| std::cmp::Reverse(folder.plain.len()));
        let lower_name = |name: &Option<String>| {
            name.as_deref()
                .filter(|name| !name.is_empty())
                .map(str::to_ascii_lowercase)
        };
        Scrubber {
            folders,
            user: lower_name(&places.user),
            host: lower_name(&places.host),
            learned: Vec::new(),
        }
    }

    /// Masks the whole string value of every key named like a secret, and learns it.
    fn mask_secret_keys(&mut self, value: &mut Value) -> bool {
        let mut masked = false;
        for_each_string(value, None, &mut |key, text| {
            let secret_key = key.is_some_and(|key| {
                SECRET_KEYS
                    .iter()
                    .any(|secret| secret.eq_ignore_ascii_case(key))
            });
            if secret_key && !text.is_empty() && !is_masked(text) {
                self.learn(text);
                *text = "<masked:secret-key>".to_owned();
                masked = true;
            }
        });
        masked
    }

    /// Masks every secret of a known shape in one string, and learns them.
    fn mask_shapes(&mut self, text: &mut String) -> bool {
        let secrets = find_secrets(text);
        if secrets.is_empty() {
            return false;
        }
        let mut masked = String::with_capacity(text.len());
        let mut copied_up_to = 0;
        for (range, kind) in &secrets {
            masked.push_str(&text[copied_up_to..range.start]);
            masked.push_str("<masked:");
            masked.push_str(kind);
            masked.push('>');
            copied_up_to = range.end;
        }
        masked.push_str(&text[copied_up_to..]);
        for (range, _) in &secrets {
            self.learn(&text[range.clone()]);
        }
        *text = masked;
        true
    }

    fn mask_learned(&self, text: &mut String) -> bool {
        let mut masked = false;
        for value in &self.learned {
            masked |= replace_all(text, value, "<masked:learned>");
        }
        masked
    }

    fn replace_folders(&self, text: &mut String) -> bool {
        let mut changed = false;
        for folder in &self.folders {
            changed |= replace_all(text, &folder.plain, folder.placeholder);
            if text.contains('%') {
                changed |= replace_all_ignoring_case(text, &folder.url_encoded, folder.placeholder);
            }
            for name in &folder.claude_names {
                changed |= replace_all(text, name, folder.placeholder);
            }
        }
        changed
    }

    fn mask_user_and_host(&self, text: &mut String) -> bool {
        let mut masked = false;
        if let Some(user) = &self.user {
            masked |= mask_name(text, user, "<masked:user>", |before, after| {
                before == Some(b'/') || matches!(after, Some(b'/' | b'@'))
            });
        }
        if let Some(host) = &self.host {
            masked |= mask_name(text, host, "<masked:host>", |before, after| {
                matches!(before, Some(b'/' | b'@')) || after == Some(b'/')
            });
        }
        masked
    }

    /// Learns a masked value, and the token after it when it starts with `Bearer `.
    fn learn(&mut self, value: &str) {
        self.learn_one(value);
        if let Some(token) = after_bearer(value) {
            self.learn_one(token);
        }
    }

    fn learn_one(&mut self, value: &str) {
        if value.chars().count() < MIN_LEARNED_CHARS
            || self.learned.iter().any(|known| known == value)
        {
            return;
        }
        let position = self
            .learned
            .iter()
            .position(|known| known.len() < value.len())
            .unwrap_or(self.learned.len());
        self.learned.insert(position, value.to_owned());
    }
}

impl Folder {
    fn new(plain: &str, placeholder: &'static str) -> Folder {
        let slashes_and_dots: String = plain
            .chars()
            .map(|c| if c == '/' || c == '.' { '-' } else { c })
            .collect();
        let all_but_alphanumerics: String = plain
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect();
        let mut claude_names = vec![slashes_and_dots];
        if all_but_alphanumerics != claude_names[0] {
            claude_names.push(all_but_alphanumerics);
        }
        Folder {
            plain: plain.to_owned(),
            url_encoded: plain.replace('/', "%2F").to_ascii_lowercase(),
            claude_names,
            placeholder,
        }
    }
}

/// Removes the strings Claude and Grok write that are personal without looking secret. Each
/// string it replaces by a fixed one goes to `replaced`, to be learned.
fn strip(agent: Agent, frame: &mut Value, replaced: &mut Vec<String>) -> bool {
    let stripped = match agent {
        Agent::Claude => strip_claude(frame, replaced),
        Agent::Grok => strip_grok(frame, replaced),
    };
    stripped | replace_account_keys(frame, replaced)
}

/// Keys whose string value describes the owner's account, matched exactly and without case.
const ACCOUNT_KEYS: [&str; 12] = [
    "email",
    "emailAddress",
    "organization",
    "organization_name",
    "organizationName",
    "organization_uuid",
    "account_uuid",
    "subscriptionType",
    "subscription_type",
    "subscription_tier",
    "subscription_tier_display",
    "credential_expires_at",
];

/// Replaces the string value of every key of `ACCOUNT_KEYS`, wherever it is in a frame.
fn replace_account_keys(frame: &mut Value, replaced: &mut Vec<String>) -> bool {
    let mut changed = false;
    for_each_string(frame, None, &mut |key, text| {
        let Some(key) = key.filter(|key| {
            ACCOUNT_KEYS
                .iter()
                .any(|account_key| account_key.eq_ignore_ascii_case(key))
        }) else {
            return;
        };
        let fixed = if key.to_ascii_lowercase().contains("email") {
            ACCOUNT_EMAIL
        } else {
            ACCOUNT_FIELD
        };
        if text != fixed {
            replaced.push(std::mem::replace(text, fixed.to_owned()));
            changed = true;
        }
    });
    changed
}

fn strip_claude(frame: &mut Value, replaced: &mut Vec<String>) -> bool {
    let frame_type = string_field(frame, "type");
    let subtype = string_field(frame, "subtype");
    match (frame_type.as_str(), subtype.as_str()) {
        ("control_response", _) => {
            let account = frame
                .pointer_mut("/response/response/account")
                .is_some_and(|account| replace_account_fields(account, replaced));
            let lists = frame
                .pointer_mut("/response/response")
                .is_some_and(|answer| empty_lists(answer, &["commands", "agents"]));
            account | lists
        }
        ("system", "init") => empty_lists(
            frame,
            &[
                "agents",
                "skills",
                "slash_commands",
                "terminal_slash_commands",
                "plugins",
                "mcp_servers",
            ],
        ),
        ("system", "commands_changed") => empty_lists(frame, &["commands"]),
        ("system", "memory_recall") => empty_lists(frame, &["memories"]),
        ("rate_limit_event", _) => frame
            .get_mut("rate_limit_info")
            .is_some_and(keep_only_status),
        _ => false,
    }
}

fn strip_grok(frame: &mut Value, replaced: &mut Vec<String>) -> bool {
    let mut changed = false;
    if let Some(Value::Object(meta)) = frame.pointer_mut("/result/_meta") {
        changed |= set_if_present(meta, "agentId", NIL_AGENT_ID, replaced);
        changed |= set_if_present(meta, "agentInstanceId", NIL_AGENT_ID, replaced);
        changed |= set_if_present(meta, "hostname", "<host>", replaced);
    }
    changed | drop_user_commands(frame)
}

fn string_field(frame: &Value, key: &str) -> String {
    frame
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn replace_account_fields(account: &mut Value, replaced: &mut Vec<String>) -> bool {
    let mut changed = false;
    for_each_string(account, None, &mut |key, text| {
        let names_an_email = key.is_some_and(|key| key.to_ascii_lowercase().contains("email"));
        let fixed = if names_an_email {
            ACCOUNT_EMAIL
        } else {
            ACCOUNT_FIELD
        };
        if text != fixed {
            replaced.push(std::mem::replace(text, fixed.to_owned()));
            changed = true;
        }
    });
    changed
}

fn empty_lists(frame: &mut Value, keys: &[&str]) -> bool {
    let mut changed = false;
    for key in keys {
        if let Some(list) = frame.get_mut(*key)
            && !matches!(list, Value::Array(items) if items.is_empty())
        {
            *list = Value::Array(Vec::new());
            changed = true;
        }
    }
    changed
}

fn keep_only_status(info: &mut Value) -> bool {
    let Value::Object(fields) = info else {
        return false;
    };
    let before = fields.len();
    fields.retain(|key, _| key == "status");
    fields.len() != before
}

fn set_if_present(
    fields: &mut Map<String, Value>,
    key: &str,
    fixed: &str,
    replaced: &mut Vec<String>,
) -> bool {
    match fields.get_mut(key) {
        Some(value) if value.as_str() != Some(fixed) => {
            if let Value::String(original) =
                std::mem::replace(value, Value::String(fixed.to_owned()))
            {
                replaced.push(original);
            }
            true
        }
        _ => false,
    }
}

/// Removes the user's own commands from every `availableCommands` list in a frame.
fn drop_user_commands(value: &mut Value) -> bool {
    match value {
        Value::Object(fields) => {
            let mut changed = false;
            for (key, child) in fields.iter_mut() {
                if key == "availableCommands"
                    && let Value::Array(commands) = child
                {
                    let before = commands.len();
                    commands.retain(|command| {
                        command.pointer("/_meta/scope").and_then(Value::as_str) != Some("user")
                    });
                    changed |= commands.len() != before;
                }
                changed |= drop_user_commands(child);
            }
            changed
        }
        Value::Array(items) => items
            .iter_mut()
            .fold(false, |changed, item| drop_user_commands(item) | changed),
        _ => false,
    }
}

/// Calls `visit` on every string value with the key it sits under; an array's items sit under
/// the array's key.
fn for_each_string<F: FnMut(Option<&str>, &mut String)>(
    value: &mut Value,
    key: Option<&str>,
    visit: &mut F,
) {
    match value {
        Value::String(text) => visit(key, text),
        Value::Array(items) => {
            for item in items {
                for_each_string(item, key, visit);
            }
        }
        Value::Object(fields) => {
            for (child_key, child) in fields.iter_mut() {
                for_each_string(child, Some(child_key), visit);
            }
        }
        _ => {}
    }
}

/// Whether a string is already a value masked away, as when a document is masked twice.
fn is_masked(text: &str) -> bool {
    text.starts_with("<masked:") && text.ends_with('>')
}

/// The program's file name, without its folder.
fn program_name(program: &str) -> String {
    Path::new(program).file_name().map_or_else(
        || program.to_owned(),
        |name| name.to_string_lossy().into_owned(),
    )
}

/// Where the id starts in a Claude argument: the whole argument after an id flag, or the part
/// after `=` of `--flag=value`.
fn claude_id_start(previous: Option<&str>, arg: &str) -> Option<usize> {
    if previous.is_some_and(|flag| CLAUDE_ID_FLAGS.contains(&flag)) {
        return Some(0);
    }
    CLAUDE_ID_FLAGS.iter().find_map(|flag| {
        let value = arg.strip_prefix(flag)?.strip_prefix('=')?;
        Some(arg.len() - value.len())
    })
}

/// How long an id is as text; a number has no text to be found in.
fn id_text_len(value: &Value) -> usize {
    value.as_str().map_or(0, str::len)
}

/// The token after `Bearer ` and its spaces, when `text` starts with it, in any case.
fn after_bearer(text: &str) -> Option<&str> {
    let bytes = text.as_bytes();
    let has_scheme = bytes.len() > 7
        && bytes[..6].eq_ignore_ascii_case(b"bearer")
        && matches!(bytes[6], b' ' | b'\t');
    has_scheme.then(|| text[7..].trim_start_matches([' ', '\t']))
}

/// Replaces every `from` in `text`; returns whether there was one.
fn replace_all(text: &mut String, from: &str, to: &str) -> bool {
    if from.is_empty() || !text.contains(from) {
        return false;
    }
    *text = text.replace(from, to);
    true
}

/// Replaces every `lower_from`, matched without ASCII case, in `text`.
fn replace_all_ignoring_case(text: &mut String, lower_from: &str, to: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    if !lower.contains(lower_from) {
        return false;
    }
    // ASCII lower-casing keeps every byte where it was, so offsets in `lower` hold in `text`.
    let mut replaced = String::with_capacity(text.len());
    let mut copied_up_to = 0;
    for (start, found) in lower.match_indices(lower_from) {
        replaced.push_str(&text[copied_up_to..start]);
        replaced.push_str(to);
        copied_up_to = start + found.len();
    }
    replaced.push_str(&text[copied_up_to..]);
    *text = replaced;
    true
}

/// Masks `lower_name`, matched without case, where it stands as a whole name and `in_context`
/// accepts the bytes around it.
fn mask_name(
    text: &mut String,
    lower_name: &str,
    replacement: &str,
    in_context: fn(Option<u8>, Option<u8>) -> bool,
) -> bool {
    // Every context has a `/` or an `@` next to the name.
    if !text.contains(['/', '@']) {
        return false;
    }
    let lower = text.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let mut masked = String::new();
    let mut copied_up_to = 0;
    let mut changed = false;
    for (start, found) in lower.match_indices(lower_name) {
        let end = start + found.len();
        let before = start.checked_sub(1).map(|index| bytes[index]);
        let after = bytes.get(end).copied();
        let whole_name = !before.is_some_and(is_word_byte) && !after.is_some_and(is_word_byte);
        if !whole_name || !in_context(before, after) {
            continue;
        }
        masked.push_str(&text[copied_up_to..start]);
        masked.push_str(replacement);
        copied_up_to = end;
        changed = true;
    }
    if !changed {
        return false;
    }
    masked.push_str(&text[copied_up_to..]);
    *text = masked;
    true
}

/// Bytes of a word: a token, a key's name, a user's or host's name. Letters, digits, `_` and
/// `-`.
fn is_word_byte(byte: u8) -> bool {
    WORD_BYTES[usize::from(byte)]
}

/// [`is_word_byte`] as a table: the scan asks it of every byte, and a table read costs no call
/// even in an unoptimised build (see `masking_five_megabytes_stays_linear`).
const WORD_BYTES: [bool; 256] = {
    let mut table = [false; 256];
    let mut byte = 0;
    while byte < 256 {
        let value = byte as u8;
        table[byte] = value.is_ascii_alphanumeric() || value == b'_' || value == b'-';
        byte += 1;
    }
    table
};

/// Whether a token or a key may start at `at`: after a byte that is not a word byte, or right
/// after a two-byte escape (`\n`, `\r`, `\t`, `\b`, `\f`) or a URL encoding (`%3D`), whose last
/// bytes are word bytes. A run of word bytes is so entered at its start and at most once more,
/// after the escape or encoding that begins it, which keeps the scan linear.
fn may_start_at(bytes: &[u8], at: usize) -> bool {
    if at == 0 || !is_word_byte(bytes[at - 1]) {
        return true;
    }
    let after_escape = at >= 2
        && bytes[at - 2] == b'\\'
        && matches!(bytes[at - 1], b'n' | b'r' | b't' | b'b' | b'f');
    let after_encoding = at >= 3
        && bytes[at - 3] == b'%'
        && bytes[at - 2].is_ascii_hexdigit()
        && bytes[at - 1].is_ascii_hexdigit();
    after_escape || after_encoding
}

/// Every secret of a known shape in `text`, in order and without overlaps, with its kind.
fn find_secrets(text: &str) -> Vec<(Range<usize>, &'static str)> {
    let bytes = text.as_bytes();
    let mut secrets = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        if !WORD_BYTES[usize::from(bytes[at])] {
            // Outside a word, only the `:` of `://` starts a secret: a URL's password.
            match url_password(bytes, at) {
                Some(range) => {
                    at = range.end;
                    secrets.push((range, "token"));
                }
                None => at += 1,
            }
            continue;
        }
        // Each run of word bytes is read once to its end. A secret starts at the run's start,
        // or at its second or third byte, after the escape or the encoding that begins it (see
        // `may_start_at`); nowhere else in it.
        let run_end = token_run_end(bytes, at);
        let found = [at, at + 1, at + 2]
            .into_iter()
            .filter(|&start| start < run_end && may_start_at(bytes, start))
            .find_map(|start| secret_in_run(text, start, run_end));
        match found {
            Some((range, kind)) => {
                at = range.end;
                secrets.push((range, kind));
            }
            None => at = run_end,
        }
    }
    secrets
}

/// The secret that starts at `at`, in a run of word bytes that ends at `run_end`, if one does.
/// A value after a scheme or a key's name starts after `at`.
fn secret_in_run(text: &str, at: usize, run_end: usize) -> Option<(Range<usize>, &'static str)> {
    let bytes = text.as_bytes();
    let first = bytes[at];
    if first == b'-' {
        return private_key_end(text, at).map(|end| (at..end, "private-key"));
    }
    if STARTS_A_SECRET_KEY[usize::from(first.to_ascii_lowercase())]
        && let Some(range) = secret_key_value(text, at, run_end)
    {
        return Some((range, "secret-key"));
    }
    let token_end = match first {
        b'B' | b'b' => {
            if let Some(range) = bearer_value(bytes, at) {
                return Some((range, "bearer"));
            }
            return basic_value(bytes, at).map(|range| (range, "token"));
        }
        b'A' => aws_key_end(bytes, at),
        b'e' => json_web_token_end(bytes, at),
        b'g' | b's' | b'x' => prefixed_token_end(bytes, at),
        _ => None,
    };
    token_end.map(|end| (at..end, "token"))
}

/// Where the run of word bytes that starts at `from` ends.
fn token_run_end(bytes: &[u8], from: usize) -> usize {
    let mut end = from;
    while end < bytes.len() && WORD_BYTES[usize::from(bytes[end])] {
        end += 1;
    }
    end
}

fn prefixed_token_end(bytes: &[u8], at: usize) -> Option<usize> {
    let rest = &bytes[at..];
    for (prefix, min_chars) in TOKEN_PREFIXES {
        if rest.starts_with(prefix.as_bytes()) {
            let body = at + prefix.len();
            let end = token_run_end(bytes, body);
            return (end - body >= *min_chars).then_some(end);
        }
    }
    let slack = rest.len() >= 5
        && rest.starts_with(b"xox")
        && rest[3].is_ascii_lowercase()
        && rest[4] == b'-';
    if slack {
        let end = token_run_end(bytes, at + 5);
        return (end - (at + 5) >= SLACK_MIN_CHARS).then_some(end);
    }
    None
}

/// An AWS access key id: `AKIA` and 16 upper-case letters or digits, alone.
fn aws_key_end(bytes: &[u8], at: usize) -> Option<usize> {
    let end = at + 20;
    let key = bytes.get(at..end)?;
    let shaped = key.starts_with(b"AKIA")
        && key[4..]
            .iter()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit());
    let alone = !bytes.get(end).is_some_and(u8::is_ascii_alphanumeric);
    (shaped && alone).then_some(end)
}

/// A JSON Web Token: a header and a payload, both JSON in base64url (so both start `eyJ`), and
/// a signature that may be missing.
fn json_web_token_end(bytes: &[u8], at: usize) -> Option<usize> {
    if !bytes[at..].starts_with(b"eyJ") {
        return None;
    }
    let header_end = token_run_end(bytes, at);
    if header_end - at < JWT_PART_MIN_CHARS || bytes.get(header_end) != Some(&b'.') {
        return None;
    }
    let payload = header_end + 1;
    if !bytes[payload..].starts_with(b"eyJ") {
        return None;
    }
    let payload_end = token_run_end(bytes, payload);
    if payload_end - payload < JWT_PART_MIN_CHARS {
        return None;
    }
    if bytes.get(payload_end) == Some(&b'.') {
        Some(token_run_end(bytes, payload_end + 1))
    } else {
        Some(payload_end)
    }
}

/// The value after `Bearer ` (any case): the characters of RFC 6750's `b64token`.
fn bearer_value(bytes: &[u8], at: usize) -> Option<Range<usize>> {
    let start = after_scheme(bytes, at, b"bearer")?;
    let length = bytes[start..]
        .iter()
        .take_while(|&&byte| byte.is_ascii_alphanumeric() || b"-._~+/=".contains(&byte))
        .count();
    (length >= BEARER_MIN_CHARS).then_some(start..start + length)
}

/// The credentials after `Basic ` (any case): `user:password` in base64.
fn basic_value(bytes: &[u8], at: usize) -> Option<Range<usize>> {
    let start = after_scheme(bytes, at, b"basic")?;
    let length = bytes[start..]
        .iter()
        .take_while(|&&byte| byte.is_ascii_alphanumeric() || b"+/=".contains(&byte))
        .count();
    looks_like_base64(&bytes[start..start + length]).then_some(start..start + length)
}

/// Where the value after an authentication scheme starts: the scheme in any case, then
/// spaces, tabs or URL-encoded spaces (`%20`).
fn after_scheme(bytes: &[u8], at: usize, scheme: &[u8]) -> Option<usize> {
    let scheme_end = at + scheme.len();
    if !bytes.get(at..scheme_end)?.eq_ignore_ascii_case(scheme) {
        return None;
    }
    let mut start = scheme_end;
    loop {
        match &bytes[start..] {
            [b' ' | b'\t', ..] => start += 1,
            [b'%', b'2', b'0', ..] => start += 3,
            _ => break,
        }
    }
    (start > scheme_end).then_some(start)
}

/// Whether a run reads as base64 rather than as a word of prose ("Basic configuration"): a
/// multiple of 4 characters, `=` only as the padding at its end, and a digit, `+`, `/`, `=` or
/// a capital after its first character.
fn looks_like_base64(run: &[u8]) -> bool {
    let body = run
        .strip_suffix(b"==")
        .or_else(|| run.strip_suffix(b"="))
        .unwrap_or(run);
    run.len() >= BASIC_MIN_CHARS
        && run.len().is_multiple_of(4)
        && !body.contains(&b'=')
        && run[1..].iter().any(|byte| !byte.is_ascii_lowercase())
}

/// A URL's password, `scheme://user:password@host`, found from the `:` of `://`. The authority
/// ends at the first `/`, so each byte is read for one URL only.
fn url_password(bytes: &[u8], at: usize) -> Option<Range<usize>> {
    if bytes[at] != b':' || !bytes[at..].starts_with(b"://") {
        return None;
    }
    let authority = at + 3;
    let user_info_length = bytes[authority..]
        .iter()
        .position(|&byte| byte == b'@' || ends_an_authority(byte))?;
    let at_sign = authority + user_info_length;
    if bytes[at_sign] != b'@' {
        return None;
    }
    let colon = bytes[authority..at_sign]
        .iter()
        .position(|&byte| byte == b':')?;
    let start = authority + colon + 1;
    (start < at_sign).then_some(start..at_sign)
}

fn ends_an_authority(byte: u8) -> bool {
    byte.is_ascii_whitespace() || b"/?#\"'<>\\".contains(&byte)
}

/// The value of a key named like a secret, written in text: `"api_key":"…"`, `PASSWORD=…`,
/// `token: …`. The name is one of [`SECRET_KEYS`], whole and in any case; the value runs to the
/// next quote, space, `;`, `,` or `\`, and an `authorization`'s takes the word after one space
/// too, its credentials after its scheme.
fn secret_key_value(text: &str, at: usize, name_end: usize) -> Option<Range<usize>> {
    let bytes = text.as_bytes();
    // A cheap test first: most words are not followed by `=` or `:`.
    let value_start = value_after_separator(bytes, name_end)?;
    let name = &bytes[at..name_end];
    let secret_name = SECRET_KEYS
        .iter()
        .find(|secret| secret.as_bytes().eq_ignore_ascii_case(name))?;
    let mut value_end = value_word_end(bytes, value_start);
    if value_end == value_start || text[value_start..].starts_with("<masked:") {
        return None;
    }
    if *secret_name == "authorization" && bytes.get(value_end) == Some(&b' ') {
        value_end = value_word_end(bytes, value_end + 1);
    }
    Some(value_start..value_end)
}

/// After a key's name: an optional closing quote, spaces, `=` or `:`, spaces and an optional
/// opening quote; returns where the value starts.
fn value_after_separator(bytes: &[u8], name_end: usize) -> Option<usize> {
    let mut at = skip_quote(bytes, name_end);
    at = skip_spaces(bytes, at);
    if !matches!(bytes.get(at), Some(b'=' | b':')) {
        return None;
    }
    at = skip_spaces(bytes, at + 1);
    Some(skip_quote(bytes, at))
}

/// Skips one quote: `"`, `'`, or `\"` as JSON text inside a string writes it.
fn skip_quote(bytes: &[u8], at: usize) -> usize {
    match &bytes[at..] {
        [b'"' | b'\'', ..] => at + 1,
        [b'\\', b'"', ..] => at + 2,
        _ => at,
    }
}

fn skip_spaces(bytes: &[u8], at: usize) -> usize {
    at + bytes[at..]
        .iter()
        .take_while(|&&byte| byte == b' ' || byte == b'\t')
        .count()
}

fn value_word_end(bytes: &[u8], from: usize) -> usize {
    from + bytes[from..]
        .iter()
        .take_while(|&&byte| !byte.is_ascii_whitespace() && !b"\"';,\\".contains(&byte))
        .count()
}

/// A PEM private key, from its `-----BEGIN … PRIVATE KEY-----` line through its `-----END …-----`
/// line, or to the end of the text when the end line is missing.
fn private_key_end(text: &str, at: usize) -> Option<usize> {
    const BEGIN: &str = "-----BEGIN ";
    const END: &str = "-----END ";
    let bytes = text.as_bytes();
    if !bytes[at..].starts_with(BEGIN.as_bytes()) {
        return None;
    }
    let label_start = at + BEGIN.len();
    let label_length = armour_label_length(&bytes[label_start..])?;
    if !bytes[label_start..label_start + label_length].ends_with(b"PRIVATE KEY") {
        return None;
    }
    let body = label_start + label_length + 5;
    let Some(offset) = text[body..].find(END) else {
        return Some(text.len());
    };
    let end_label = body + offset + END.len();
    Some(
        armour_label_length(&bytes[end_label..])
            .map_or(text.len(), |length| end_label + length + 5),
    )
}

/// The length of an armour line's label, up to its closing `-----`.
fn armour_label_length(bytes: &[u8]) -> Option<usize> {
    bytes
        .windows(5)
        .take(PEM_LABEL_MAX_BYTES)
        .position(|window| window == b"-----")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// The processor time this thread has used. Wall time would count the other tests running
    /// beside it, which made these limits fail under the workspace's parallel tests.
    fn thread_time() -> Duration {
        let mut now = libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        // SAFETY: `now` is a valid, writable timespec, and CLOCK_THREAD_CPUTIME_ID exists on
        // every Linux kernel this crate builds for.
        let result = unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut now) };
        assert_eq!(result, 0, "clock_gettime failed");
        Duration::new(now.tv_sec as u64, now.tv_nsec as u32)
    }

    fn places() -> Places {
        Places {
            workspace: Some("/tmp/rec/project".to_owned()),
            homes: vec!["/tmp/rec/home".to_owned(), "/home/alice".to_owned()],
            user: Some("alice".to_owned()),
            host: Some("deskhost".to_owned()),
        }
    }

    /// The processor time `work` takes on this thread.
    fn time_of(work: impl FnOnce()) -> Duration {
        let started = thread_time();
        work();
        thread_time() - started
    }

    /// Masking must stay linear: ten times the text may cost at most this many times the time
    /// (a pass that rescans from every position would cost about a hundred). A ratio holds on
    /// any machine, where an absolute limit depends on the processor.
    const MOST_COST_OF_TEN_TIMES_THE_TEXT: u32 = 30;

    /// An absolute ceiling against a catastrophe only, far above the 0.3 to 0.6 s measured for
    /// 5 MB on the owner's desktop in a debug build.
    const CEILING: Duration = Duration::from_secs(5);

    fn assert_linear(name: &str, mask: impl Fn(&str), small: &str, large: &str) {
        let small_time = time_of(|| mask(small));
        let large_time = time_of(|| mask(large));
        assert!(large_time < CEILING, "{name}: {large_time:?}");
        assert!(
            large_time < small_time * MOST_COST_OF_TEN_TIMES_THE_TEXT,
            "{name}: {small_time:?} for {} bytes, {large_time:?} for {}",
            small.len(),
            large.len()
        );
    }

    #[test]
    fn masking_five_megabytes_stays_linear() {
        let chunk = "See /tmp/rec/project/src/main.rs, ask sk-learn and eyJ.e then -----BEGIN \
                     CERTIFICATE----- by alice@deskhost: Bearer x and xoxb-1 é. ";
        let text = |bytes: usize| {
            let mut text = chunk.repeat(bytes / chunk.len());
            text.push_str("sk-ant-api03-AbCdEfGhIjKlMnOpQrStUvWxYz0123456789");
            text
        };
        let (small, large) = (text(500_000), text(5_000_000));

        let (masked, changed) = Masker::new(Agent::Claude, places()).line(&large);
        assert!(changed);
        assert!(masked.ends_with("<masked:token>"));
        assert_linear(
            "a line",
            |text| drop(Masker::new(Agent::Claude, places()).line(text)),
            &small,
            &large,
        );
        // A frame's strings are walked twice: shapes first, the rest after.
        let frame = |text: &str| serde_json::json!({"type": "user", "message": {"content": text}});
        assert_linear(
            "a frame",
            |text| {
                let mut frame = frame(text);
                assert!(Masker::new(Agent::Claude, places()).frame(Side::Agent, &mut frame));
            },
            &small,
            &large,
        );
    }

    #[test]
    fn a_shape_that_cannot_end_does_not_rescan() {
        // Each run is read once from its start: a string of candidates stays linear.
        assert!(find_secrets(&"eyJ".repeat(1_000_000)).is_empty());
        assert_linear(
            "eyJ repeated",
            |text| assert!(find_secrets(text).is_empty()),
            &"eyJ".repeat(100_000),
            &"eyJ".repeat(1_000_000),
        );
    }

    #[test]
    fn a_pem_key_without_its_end_line_is_masked_to_the_end() {
        let (text, masked) = Masker::new(Agent::Claude, places())
            .line("key: -----BEGIN RSA PRIVATE KEY-----\nMIIEow cut here");
        assert!(masked);
        assert_eq!(text, "key: <masked:private-key>");
    }

    #[test]
    fn prose_after_a_scheme_or_a_key_like_word_stays() {
        let text = "Basic configuration and Basic Settings; the token is fine; tokens: 12";
        let (masked_text, masked) = Masker::new(Agent::Claude, places()).line(text);
        assert!(!masked, "{masked_text}");
    }

    #[test]
    fn an_authorization_in_text_loses_its_scheme_and_credentials() {
        let (text, masked) =
            Masker::new(Agent::Claude, places()).line("Authorization: Token abc123xyz789, next");
        assert!(masked);
        assert_eq!(text, "Authorization: <masked:secret-key>, next");
    }

    #[test]
    fn a_pem_key_after_an_escaped_newline_is_masked() {
        let (text, masked) = Masker::new(Agent::Claude, places())
            .line("{\"k\":\"a\\n-----BEGIN RSA PRIVATE KEY-----\\nMIIE\\n-----END RSA PRIVATE KEY-----\"}");
        assert!(masked);
        assert!(!text.contains("MIIE"), "{text}");
    }

    #[test]
    fn a_relative_folder_is_ignored() {
        let relative = Places {
            workspace: Some(".".to_owned()),
            homes: vec!["home".to_owned()],
            ..Places::default()
        };
        let (text, _) = Masker::new(Agent::Claude, relative).line("run cargo.test in home/x");
        assert_eq!(text, "run cargo.test in home/x");
    }

    #[test]
    fn a_certificate_and_short_lookalikes_stay() {
        let text = "-----BEGIN CERTIFICATE----- sk-short AKIA123 Bearer token risk-assessment";
        let (masked_text, masked) = Masker::new(Agent::Claude, places()).line(text);
        assert!(!masked, "{masked_text}");
        assert_eq!(masked_text, text);
    }

    #[test]
    fn a_short_client_id_is_replaced_only_whole() {
        let mut masker = Masker::new(Agent::Claude, places());
        let mut request = serde_json::json!({"type": "control_request", "request_id": "r1",
            "request": {"subtype": "initialize"}});
        masker.frame(Side::Client, &mut request);
        let mut answer = serde_json::json!({"type": "control_response",
            "response": {"request_id": "r1", "note": "r1 and r12"}});
        masker.frame(Side::Agent, &mut answer);
        assert_eq!(answer["response"]["request_id"], "<id:1>");
        assert_eq!(answer["response"]["note"], "r1 and r12");
    }

    #[test]
    fn a_home_inside_the_workspace_is_replaced_before_it() {
        let nested = Places {
            workspace: Some("/tmp/rec".to_owned()),
            homes: vec!["/tmp/rec/home/".to_owned()],
            ..Places::default()
        };
        let (text, _) = Masker::new(Agent::Claude, nested).line("/tmp/rec/home/x /tmp/rec/y");
        assert_eq!(text, "<home>/x <workspace>/y");
    }

    #[test]
    fn claude_folder_names_of_both_kinds_are_replaced() {
        let underscored = Places {
            workspace: Some("/tmp/my_project".to_owned()),
            ..Places::default()
        };
        let (text, _) =
            Masker::new(Agent::Claude, underscored).line("-tmp-my_project/a -tmp-my-project/b");
        assert_eq!(text, "<workspace>/a <workspace>/b");
    }
}
