//! Code highlighting with the upstream `pierre-dark` and `pierre-light`
//! Shiki themes (light under a light environment theme), applied to syntect's
//! TextMate-derived grammars. Token boundaries can differ slightly from
//! Shiki's grammars.
//!
//! Highlighting runs on a worker thread: syntect spends ~0.1ms a line and
//! compiles a grammar on its first use (up to ~150ms), which would stall
//! frames. Code shows in the plain foreground until its spans arrive, as
//! upstream's Shiki highlights asynchronously too.
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::str::FromStr;
use std::sync::mpsc;

use eframe::egui::{self, Color32};
use syntect::highlighting::{
    Color, FontStyle, HighlightIterator, HighlightState, Highlighter as SyntectHighlighter,
    ScopeSelectors, StyleModifier, Theme, ThemeItem, ThemeSettings,
};
use syntect::parsing::{ParseState, ScopeStack, SyntaxSet};

#[derive(Default)]
pub struct Highlighter {
    /// Spans by the hash of (light theme, language, code).
    cache: HashMap<u64, Highlighted>,
    /// Hashes sent to the worker and not back yet.
    pending: HashSet<u64>,
    worker: Option<Worker>,
    /// Repainted when spans arrive.
    ctx: Option<egui::Context>,
}

struct Request {
    key: u64,
    light: bool,
    language: String,
    code: String,
}

struct Highlighted {
    request: Request,
    spans: Vec<(String, Color32)>,
}

struct Worker {
    requests: mpsc::Sender<Request>,
    results: mpsc::Receiver<Highlighted>,
}

impl Worker {
    fn start(ctx: Option<egui::Context>) -> Self {
        let (requests, inbox) = mpsc::channel::<Request>();
        let (outbox, results) = mpsc::channel();
        let _ = std::thread::Builder::new()
            .name("highlight".into())
            .spawn(move || {
                let engine = Engine::default();
                for request in inbox {
                    let spans = engine.compute(&request.code, &request.language, request.light);
                    if outbox.send(Highlighted { request, spans }).is_err() {
                        break;
                    }
                    if let Some(ctx) = &ctx {
                        ctx.request_repaint();
                    }
                }
            });
        Self { requests, results }
    }
}

/// syntect's grammars and the pierre themes.
struct Engine {
    syntaxes: SyntaxSet,
    dark: Theme,
    light: Theme,
}

fn color(hex: &str) -> Option<Color> {
    let hex = hex.trim_start_matches('#');
    let byte = |i: usize| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok();
    Some(Color {
        r: byte(0)?,
        g: byte(2)?,
        b: byte(4)?,
        a: if hex.len() >= 8 { byte(6)? } else { 255 },
    })
}

/// A pierre theme reduced to its foreground and token colors.
fn pierre(source: &str) -> Theme {
    let json: serde_json::Value = serde_json::from_str(source).expect("theme json");
    let mut scopes = Vec::new();
    for token in json["tokenColors"].as_array().into_iter().flatten() {
        let selectors = match &token["scope"] {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Array(list) => list
                .iter()
                .filter_map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", "),
            _ => continue,
        };
        let Ok(scope) = ScopeSelectors::from_str(&selectors) else {
            continue;
        };
        let style = &token["settings"];
        let font_style = style["fontStyle"].as_str().map(|s| {
            let mut f = FontStyle::empty();
            if s.contains("bold") {
                f |= FontStyle::BOLD;
            }
            if s.contains("italic") {
                f |= FontStyle::ITALIC;
            }
            f
        });
        scopes.push(ThemeItem {
            scope,
            style: StyleModifier {
                foreground: style["foreground"].as_str().and_then(color),
                background: None,
                font_style,
            },
        });
    }
    Theme {
        name: json["name"].as_str().map(str::to_owned),
        author: None,
        settings: ThemeSettings {
            foreground: json["foreground"].as_str().and_then(color),
            ..Default::default()
        },
        scopes,
    }
}

impl Default for Engine {
    fn default() -> Self {
        Self {
            syntaxes: SyntaxSet::load_defaults_newlines(),
            dark: pierre(include_str!("../assets/pierre-dark.json")),
            light: pierre(include_str!("../assets/pierre-light.json")),
        }
    }
}

impl Highlighter {
    pub fn new(ctx: egui::Context) -> Self {
        Self {
            ctx: Some(ctx),
            ..Default::default()
        }
    }

    /// Colored spans for `code` (plain foreground when the language is
    /// unknown), or None while the worker highlights it.
    pub fn spans(&mut self, code: &str, language: &str) -> Option<&[(String, Color32)]> {
        // Every code block on screen asks each frame: entries are kept and
        // spans are evicted all at once past this many.
        const MAX_ENTRIES: usize = 20_000;
        if let Some(worker) = &self.worker {
            while let Ok(done) = worker.results.try_recv() {
                self.pending.remove(&done.request.key);
                if self.cache.len() >= MAX_ENTRIES {
                    self.cache.clear();
                }
                self.cache.insert(done.request.key, done);
            }
        }
        let light = !crate::theme::palette().dark;
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        (light, language, code).hash(&mut hasher);
        let key = hasher.finish();
        let found = self.cache.get(&key).is_some_and(|done| {
            done.request.light == light
                && done.request.language == language
                && done.request.code == code
        });
        if found {
            return self.cache.get(&key).map(|done| done.spans.as_slice());
        }
        if self.pending.insert(key) {
            let worker = self
                .worker
                .get_or_insert_with(|| Worker::start(self.ctx.clone()));
            let _ = worker.requests.send(Request {
                key,
                light,
                language: language.to_owned(),
                code: code.to_owned(),
            });
        }
        None
    }

    /// `spans`, or the whole code in the plain foreground meanwhile.
    pub fn spans_or_plain(&mut self, code: &str, language: &str) -> Vec<(String, Color32)> {
        match self.spans(code, language) {
            Some(spans) => spans.to_vec(),
            None => vec![(code.to_owned(), plain())],
        }
    }
}

/// The foreground of code not highlighted (yet).
pub fn plain() -> Color32 {
    if crate::theme::palette().dark {
        DARK.foreground
    } else {
        LIGHT.foreground
    }
}

impl Engine {
    fn compute(&self, code: &str, language: &str, light: bool) -> Vec<(String, Color32)> {
        let (theme, scheme) = if light {
            (&self.light, &LIGHT)
        } else {
            (&self.dark, &DARK)
        };
        let foreground = scheme.foreground;
        let syntax = self.syntaxes.find_syntax_by_token(language).or_else(|| {
            let alias = match language {
                "ts" | "tsx" | "typescript" => "js",
                "sh" | "shell" | "zsh" | "console" => "bash",
                "nix" => "nix",
                _ => language,
            };
            self.syntaxes.find_syntax_by_token(alias)
        });
        let Some(syntax) = syntax else {
            return vec![(code.to_owned(), foreground)];
        };
        let highlighter = SyntectHighlighter::new(theme);
        let mut parse = ParseState::new(syntax);
        let mut state = HighlightState::new(&highlighter, ScopeStack::new());
        let mut spans: Vec<(String, Color32)> = Vec::new();
        for line in syntect::util::LinesWithEndings::from(code) {
            let Ok(ops) = parse.parse_line(line, &self.syntaxes) else {
                spans.push((line.to_owned(), foreground));
                continue;
            };
            for (style, text) in HighlightIterator::new(&mut state, &ops, line, &highlighter) {
                let c = style.foreground;
                let color = Color32::from_rgba_unmultiplied(c.r, c.g, c.b, c.a);
                match spans.last_mut() {
                    Some((last, last_color)) if *last_color == color => last.push_str(text),
                    _ => spans.push((text.to_owned(), color)),
                }
            }
        }
        let rust = language == "rust" || language == "rs";
        let spans = if rust {
            refine_rust_keywords(spans, scheme)
        } else {
            spans
        };
        refine_identifiers(spans, rust, scheme)
    }
}

/// The theme colors the refinements below read and write.
struct Scheme {
    foreground: Color32,
    namespace: Color32,  // entity.name.namespace
    type_: Color32,      // entity.name.type, storage.type
    variable: Color32,   // variable
    function: Color32,   // entity.name.function
    keyword: Color32,    // keyword, storage
    operator: Color32,   // keyword.operator, punctuation
    assignment: Color32, // keyword.operator.assignment
}

const DARK: Scheme = Scheme {
    foreground: Color32::from_rgb(0xfa, 0xfa, 0xfa),
    namespace: Color32::from_rgb(0xff, 0xab, 0x16),
    type_: Color32::from_rgb(0xd5, 0x68, 0xea),
    variable: Color32::from_rgb(0xff, 0xa3, 0x59),
    function: Color32::from_rgb(0x9d, 0x6a, 0xfb),
    keyword: Color32::from_rgb(0xff, 0x67, 0x8d),
    operator: Color32::from_rgb(0x63, 0x63, 0x63),
    assignment: Color32::from_rgb(0x08, 0xc0, 0xef),
};

const LIGHT: Scheme = Scheme {
    foreground: Color32::from_rgb(0x0a, 0x0a, 0x0a),
    namespace: Color32::from_rgb(0xd5, 0x90, 0x1c),
    type_: Color32::from_rgb(0xa6, 0x31, 0xbe),
    variable: Color32::from_rgb(0xd4, 0x76, 0x28),
    function: Color32::from_rgb(0x69, 0x3a, 0xcf),
    keyword: Color32::from_rgb(0xd3, 0x2a, 0x61),
    operator: Color32::from_rgb(0x63, 0x63, 0x63),
    assignment: Color32::from_rgb(0x08, 0xc0, 0xef),
};

/// Rust words Shiki scopes `storage.type` (the theme's later, purple rule)
/// or `entity.name.type.numeric`, where syntect's grammar says keyword.
const RUST_TYPE_WORDS: [&str; 22] = [
    "let", "struct", "enum", "trait", "type", "u8", "u16", "u32", "u64", "u128", "usize", "i8",
    "i16", "i32", "i64", "i128", "isize", "f32", "f64", "bool", "char", "str",
];

/// Operators Shiki scopes as assignment, comparison, logical, arithmetic or
/// closure pipes (all cyan); `<`, `>`, `&`, `*`, `!`, `->` and `=>` stay
/// ambiguous or gray.
const RUST_OPERATORS: [&str; 21] = [
    "=", "==", "!=", "<=", ">=", "+=", "-=", "*=", "/=", "%=", "^=", "&=", "|=", "<<=", ">>=",
    "&&", "||", "+", "-", "/", "|",
];

/// Brings syntect's Rust scopes closer to Shiki's: the words above turn
/// purple and plain operators cyan.
fn refine_rust_keywords(spans: Vec<(String, Color32)>, scheme: &Scheme) -> Vec<(String, Color32)> {
    let mut out: Vec<(String, Color32)> = Vec::new();
    let mut push = |text: &str, color: Color32| match out.last_mut() {
        Some((last, last_color)) if *last_color == color => last.push_str(text),
        _ => out.push((text.to_owned(), color)),
    };
    let operator = |c: char| "=!<>+-*/%^&|".contains(c);
    for (text, color) in &spans {
        if *color != scheme.keyword && *color != scheme.operator {
            push(text, *color);
            continue;
        }
        let word_char = |c: char| c.is_alphanumeric() || c == '_';
        let mut rest = text.as_str();
        while let Some(first) = rest.chars().next() {
            let end = if word_char(first) {
                rest.find(|c: char| !word_char(c)).unwrap_or(rest.len())
            } else if operator(first) {
                rest.find(|c: char| !operator(c)).unwrap_or(rest.len())
            } else {
                first.len_utf8()
            };
            let token = &rest[..end];
            let refined = if *color == scheme.keyword && RUST_TYPE_WORDS.contains(&token) {
                scheme.type_
            } else if *color == scheme.operator && RUST_OPERATORS.contains(&token) {
                scheme.assignment
            } else {
                *color
            };
            push(token, refined);
            rest = &rest[end..];
        }
    }
    out
}

/// Shiki's TextMate grammars scope plain identifiers that syntect leaves
/// unscoped. Colors identifiers the way those grammars do: `x::` as a
/// namespace (Rust), `Capitalized` as a type, `call(` as a function, and
/// anything else as a variable.
fn refine_identifiers(
    spans: Vec<(String, Color32)>,
    rust: bool,
    scheme: &Scheme,
) -> Vec<(String, Color32)> {
    let foreground = scheme.foreground;
    let full: String = spans.iter().map(|(t, _)| t.as_str()).collect();
    let mut out: Vec<(String, Color32)> = Vec::new();
    let mut push = |text: &str, color: Color32| match out.last_mut() {
        Some((last, last_color)) if *last_color == color => last.push_str(text),
        _ => out.push((text.to_owned(), color)),
    };
    let mut offset = 0;
    for (text, color) in &spans {
        let refinable = *color == foreground || (rust && *color == scheme.type_);
        if !refinable {
            push(text, *color);
            offset += text.len();
            continue;
        }
        let mut rest = text.as_str();
        let mut local = 0;
        while !rest.is_empty() {
            let start = rest
                .find(|c: char| c.is_alphabetic() || c == '_')
                .unwrap_or(rest.len());
            if start > 0 {
                push(&rest[..start], *color);
            }
            rest = &rest[start..];
            local += start;
            if rest.is_empty() {
                break;
            }
            let end = rest
                .find(|c: char| !(c.is_alphanumeric() || c == '_'))
                .unwrap_or(rest.len());
            let word = &rest[..end];
            let after = full[offset + local + end..].trim_start_matches([' ', '\t']);
            let refined = if rust && after.starts_with("::") {
                scheme.namespace
            } else if *color == scheme.type_ || word.starts_with(|c: char| c.is_uppercase()) {
                scheme.type_
            } else if after.starts_with('(') || (rust && after.starts_with('!')) {
                scheme.function
            } else {
                scheme.variable
            };
            push(word, refined);
            rest = &rest[end..];
            local += end;
        }
        offset += text.len();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_identifiers_follow_shiki_scopes() {
        let spans = Engine::default().compute("eframe::run(options, Box::new(App))", "rust", false);
        let color = |word: &str| spans.iter().find(|(t, _)| t == word).map(|(_, c)| *c);
        assert_eq!(color("eframe"), Some(DARK.namespace));
        assert_eq!(color("Box"), Some(DARK.namespace));
        assert_eq!(color("options"), Some(DARK.variable));
    }

    #[test]
    fn rust_keywords_and_operators_follow_shiki_scopes() {
        let spans = Engine::default().compute("fn f() { let x: u8 = 1 + 2; }", "rust", false);
        let color = |word: &str| {
            spans
                .iter()
                .find(|(t, _)| t.trim() == word)
                .map(|(_, c)| *c)
        };
        assert_eq!(color("fn"), Some(DARK.keyword));
        assert_eq!(color("let"), Some(DARK.type_));
        assert_eq!(color("u8"), Some(DARK.type_));
        assert_eq!(color("="), Some(DARK.assignment));
        assert_eq!(color("+"), Some(DARK.assignment));
        assert_eq!(color(";"), Some(DARK.operator));
    }
}
