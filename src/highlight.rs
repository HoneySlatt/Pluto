//! Code highlighting with the upstream `pierre-dark` Shiki theme, applied to
//! syntect's TextMate-derived grammars. Token boundaries can differ slightly
//! from Shiki's grammars.
use std::collections::HashMap;
use std::str::FromStr;

use eframe::egui::Color32;
use syntect::highlighting::{
    Color, FontStyle, HighlightIterator, HighlightState, Highlighter as SyntectHighlighter,
    ScopeSelectors, StyleModifier, Theme, ThemeItem, ThemeSettings,
};
use syntect::parsing::{ParseState, ScopeStack, SyntaxSet};

pub struct Highlighter {
    syntaxes: SyntaxSet,
    theme: Theme,
    cache: HashMap<(String, String), Vec<(String, Color32)>>,
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

fn pierre_dark() -> Theme {
    let json: serde_json::Value =
        serde_json::from_str(include_str!("../assets/pierre-dark.json")).expect("theme json");
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
        name: Some("pierre-dark".into()),
        author: None,
        settings: ThemeSettings {
            foreground: json["foreground"].as_str().and_then(color),
            ..Default::default()
        },
        scopes,
    }
}

impl Default for Highlighter {
    fn default() -> Self {
        Self {
            syntaxes: SyntaxSet::load_defaults_newlines(),
            theme: pierre_dark(),
            cache: HashMap::new(),
        }
    }
}

impl Highlighter {
    /// Colored spans for `code`; plain foreground when the language is unknown.
    pub fn spans(&mut self, code: &str, language: &str) -> &[(String, Color32)] {
        let key = (language.to_owned(), code.to_owned());
        if !self.cache.contains_key(&key) {
            let spans = self.compute(code, language);
            self.cache.insert(key.clone(), spans);
        }
        &self.cache[&key]
    }

    fn compute(&self, code: &str, language: &str) -> Vec<(String, Color32)> {
        let foreground = Color32::from_rgb(250, 250, 250);
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
        let highlighter = SyntectHighlighter::new(&self.theme);
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
            refine_rust_keywords(spans)
        } else {
            spans
        };
        refine_identifiers(spans, rust)
    }
}

const NAMESPACE: Color32 = Color32::from_rgb(0xff, 0xab, 0x16); // entity.name.namespace
const TYPE: Color32 = Color32::from_rgb(0xd5, 0x68, 0xea); // entity.name.type
const VARIABLE: Color32 = Color32::from_rgb(0xff, 0xa3, 0x59); // variable
const FUNCTION: Color32 = Color32::from_rgb(0x9d, 0x6a, 0xfb); // entity.name.function

const KEYWORD: Color32 = Color32::from_rgb(0xff, 0x67, 0x8d); // keyword, storage
const OPERATOR: Color32 = Color32::from_rgb(0x63, 0x63, 0x63); // keyword.operator, punctuation
const ASSIGNMENT: Color32 = Color32::from_rgb(0x08, 0xc0, 0xef); // keyword.operator.assignment

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
fn refine_rust_keywords(spans: Vec<(String, Color32)>) -> Vec<(String, Color32)> {
    let mut out: Vec<(String, Color32)> = Vec::new();
    let mut push = |text: &str, color: Color32| match out.last_mut() {
        Some((last, last_color)) if *last_color == color => last.push_str(text),
        _ => out.push((text.to_owned(), color)),
    };
    let operator = |c: char| "=!<>+-*/%^&|".contains(c);
    for (text, color) in &spans {
        if *color != KEYWORD && *color != OPERATOR {
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
            let refined = if *color == KEYWORD && RUST_TYPE_WORDS.contains(&token) {
                TYPE
            } else if *color == OPERATOR && RUST_OPERATORS.contains(&token) {
                ASSIGNMENT
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
fn refine_identifiers(spans: Vec<(String, Color32)>, rust: bool) -> Vec<(String, Color32)> {
    let foreground = Color32::from_rgb(250, 250, 250);
    let full: String = spans.iter().map(|(t, _)| t.as_str()).collect();
    let mut out: Vec<(String, Color32)> = Vec::new();
    let mut push = |text: &str, color: Color32| match out.last_mut() {
        Some((last, last_color)) if *last_color == color => last.push_str(text),
        _ => out.push((text.to_owned(), color)),
    };
    let mut offset = 0;
    for (text, color) in &spans {
        let refinable = *color == foreground || (rust && *color == TYPE);
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
                NAMESPACE
            } else if *color == TYPE || word.starts_with(|c: char| c.is_uppercase()) {
                TYPE
            } else if after.starts_with('(') || (rust && after.starts_with('!')) {
                FUNCTION
            } else {
                VARIABLE
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
        let mut highlighter = Highlighter::default();
        let spans = highlighter
            .spans("eframe::run(options, Box::new(App))", "rust")
            .to_vec();
        let color = |word: &str| spans.iter().find(|(t, _)| t == word).map(|(_, c)| *c);
        assert_eq!(color("eframe"), Some(NAMESPACE));
        assert_eq!(color("Box"), Some(NAMESPACE));
        assert_eq!(color("options"), Some(VARIABLE));
    }

    #[test]
    fn rust_keywords_and_operators_follow_shiki_scopes() {
        let mut highlighter = Highlighter::default();
        let spans = highlighter
            .spans("fn f() { let x: u8 = 1 + 2; }", "rust")
            .to_vec();
        let color = |word: &str| {
            spans
                .iter()
                .find(|(t, _)| t.trim() == word)
                .map(|(_, c)| *c)
        };
        assert_eq!(color("fn"), Some(KEYWORD));
        assert_eq!(color("let"), Some(TYPE));
        assert_eq!(color("u8"), Some(TYPE));
        assert_eq!(color("="), Some(ASSIGNMENT));
        assert_eq!(color("+"), Some(ASSIGNMENT));
        assert_eq!(color(";"), Some(OPERATOR));
    }
}
