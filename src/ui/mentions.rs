//! File mentions in the composer's prompt (`collectMentionTokens` from
//! `packages/shared/src/composerInlineTokens.ts`): a file link whose label
//! is its path's basename (`[app.rs](src/app.rs)`, what the "@" menu
//! inserts), or a bare `@path` / `@"quoted path"`; either followed by
//! whitespace. Bare `@scope/package` text stays a package name.
use std::ops::Range;

/// `MAX_FILE_LINK_LABEL_LENGTH`.
const MAX_LABEL: usize = 512;

fn starts_token(text: &str, at: usize) -> bool {
    text[..at]
        .chars()
        .next_back()
        .is_none_or(char::is_whitespace)
}

fn followed_by_space(text: &str, at: usize) -> bool {
    text[at..].chars().next().is_some_and(char::is_whitespace)
}

/// `URI_SCHEME_REGEX` without `WINDOWS_DRIVE_PATH_REGEX`.
fn has_external_scheme(path: &str) -> bool {
    let Some((scheme, rest)) = path.split_once(':') else {
        return false;
    };
    let is_scheme = scheme.starts_with(|c: char| c.is_ascii_alphabetic())
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '.' | '-'));
    let drive = scheme.len() == 1 && rest.starts_with(['/', '\\']);
    is_scheme && !drive
}

/// `SCOPED_PACKAGE_REFERENCE_REGEX`.
fn is_scoped_package(path: &str) -> bool {
    let segment = |s: &str| {
        s.starts_with(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit())
            && s.chars().all(|c| {
                c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-')
            })
    };
    let parts: Vec<&str> = path.split('/').collect();
    parts.len() >= 2
        && segment(parts[0])
        && segment(parts[1])
        && parts[2..].iter().all(|p| !p.is_empty())
}

/// A file link at `start` (its `[`): the path and the link's end.
fn file_link(text: &str, start: usize) -> Option<(String, usize)> {
    let mut label = String::new();
    let mut chars = text[start + 1..].char_indices();
    let mut close = None;
    let mut count = 0;
    while let Some((i, c)) = chars.next() {
        match c {
            ']' => {
                close = Some(start + 1 + i);
                break;
            }
            '\\' => {
                let (_, next) = chars.next()?;
                label.push(next);
            }
            c => label.push(c),
        }
        count += 1;
        if count > MAX_LABEL {
            return None;
        }
    }
    let after = close? + 1;
    let rest = text[after..].strip_prefix('(')?;
    let length = rest.find(|c: char| c == ')' || c.is_whitespace())?;
    if length == 0 || !rest[length..].starts_with(')') {
        return None;
    }
    let end = after + 1 + length + 1;
    let path = crate::ui::md_links::percent_decode(&rest[..length]);
    let basename = path.rsplit(['/', '\\']).next().unwrap_or(&path);
    (!path.is_empty()
        && !has_external_scheme(&path)
        && label == basename
        && followed_by_space(text, end))
    .then_some((path, end))
}

/// A bare mention at `start` (its `@`): the path and the token's end.
fn bare_mention(text: &str, start: usize) -> Option<(String, usize)> {
    let rest = &text[start + 1..];
    if let Some(quoted) = rest.strip_prefix('"') {
        let mut path = String::new();
        let mut chars = quoted.char_indices();
        while let Some((i, c)) = chars.next() {
            match c {
                '"' => {
                    let end = start + 2 + i + 1;
                    return (!path.is_empty() && followed_by_space(text, end))
                        .then_some((path, end));
                }
                '\\' => path.push(chars.next()?.1),
                c => path.push(c),
            }
        }
        return None;
    }
    let length = rest
        .find(|c: char| c.is_whitespace() || c == '@' || c == '"')
        .unwrap_or(rest.len());
    let path = &rest[..length];
    let end = start + 1 + length;
    (!path.is_empty() && !is_scoped_package(path) && followed_by_space(text, end))
        .then(|| (path.to_owned(), end))
}

/// Every mention in `text`: its path and byte range.
pub(crate) fn collect(text: &str) -> Vec<(String, Range<usize>)> {
    let mut out = Vec::new();
    for (start, c) in text.char_indices() {
        if !starts_token(text, start) {
            continue;
        }
        let found = match c {
            '[' => file_link(text, start),
            '@' => bare_mention(text, start),
            _ => None,
        };
        if let Some((path, end)) = found
            && out
                .last()
                .is_none_or(|(_, last): &(String, Range<usize>)| last.end <= start)
        {
            out.push((path, start..end));
        }
    }
    out
}

/// `SKILL_TOKEN_REGEX`: `$name` (any currency sign) followed by whitespace,
/// the name holding a letter; amounts like `$20`, `$20k` or `$1e6` stay
/// prose. Each skill's name and byte range.
pub(crate) fn collect_skills(text: &str) -> Vec<(String, Range<usize>)> {
    let name_char = |c: char| c.is_ascii_alphanumeric() || matches!(c, ':' | '_' | '-');
    let mut out = Vec::new();
    for (start, c) in text.char_indices() {
        if !starts_token(text, start) || !crate::ui::command_menu::is_currency(c) {
            continue;
        }
        let rest = &text[start + c.len_utf8()..];
        let length = rest.find(|c: char| !name_char(c)).unwrap_or(rest.len());
        let name = &rest[..length];
        let end = start + c.len_utf8() + length;
        // The amount guard: digits, an optional k/m/b/t or exponent.
        let digits = name.trim_start_matches(|c: char| c.is_ascii_digit() || c == '_');
        let amount = name.starts_with(|c: char| c.is_ascii_digit())
            && (digits.is_empty()
                || (digits.len() == 1 && "kKmMbBtT".contains(digits))
                || (digits.len() > 1
                    && digits.starts_with(['e', 'E'])
                    && digits[1..].chars().all(|c| c.is_ascii_digit())));
        if name.starts_with(|c: char| c.is_ascii_alphanumeric())
            && name.chars().any(|c| c.is_ascii_alphabetic())
            && !amount
            && followed_by_space(text, end)
        {
            out.push((name.to_owned(), start..end));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skills_leave_amounts_alone() {
        let names = |text: &str| -> Vec<String> {
            collect_skills(text).into_iter().map(|(n, _)| n).collect()
        };
        assert_eq!(
            names("lance $code-review puis €deploy:prod "),
            ["code-review", "deploy:prod"]
        );
        assert!(names("coûte $20 ou $20k ou $1e6 ").is_empty());
        assert_eq!(names("$2fa "), ["2fa"]);
        assert!(names("$review").is_empty());
        assert!(names("a$b ").is_empty());
    }

    #[test]
    fn mentions_follow_upstream() {
        let paths =
            |text: &str| -> Vec<String> { collect(text).into_iter().map(|(p, _)| p).collect() };
        assert_eq!(paths("voir [app.rs](src/app.rs) et"), ["src/app.rs"]);
        assert_eq!(paths("[a b.md](docs/a%20b.md) "), ["docs/a b.md"]);
        // The label must be the basename, and a space must follow.
        assert!(paths("[lien](src/app.rs) ").is_empty());
        assert!(paths("[app.rs](src/app.rs)").is_empty());
        assert!(paths("[x](https://x.y/x) ").is_empty());
        assert_eq!(
            paths("@README.md @\"a b/c d.rs\" "),
            ["README.md", "a b/c d.rs"]
        );
        // Like a scoped package, so text (the "@" menu inserts links).
        assert!(paths("@src/main.rs ").is_empty());
        assert!(paths("@scope/package ").is_empty());
        assert!(paths("mail@host.fr ").is_empty());
        let text = "x [app.rs](src/app.rs) y";
        assert_eq!(collect(text)[0].1, 2..22);
    }
}
