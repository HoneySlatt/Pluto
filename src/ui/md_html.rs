//! Raw HTML in answers (`markdownPipeline.ts`: `rehype-raw`, then
//! `rehype-sanitize` with GitHub's schema): the tags of the allowed set keep
//! their meaning (`<br>`, `<b>`, `<i>`, `<s>`, `<code>`, `<kbd>`, `<a href>`,
//! `<img>`, `<details>` and `<summary>`), any other tag is dropped and its
//! text kept, comments vanish. User messages show HTML as typed
//! (`parseRawHtml={false}`).

/// A piece of HTML: text (entities decoded) or a tag.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Token {
    Text(String),
    Open {
        name: String,
        attributes: Vec<(String, String)>,
    },
    Close(String),
}

impl Token {
    pub(crate) fn attribute(&self, key: &str) -> Option<&str> {
        match self {
            Token::Open { attributes, .. } => attributes
                .iter()
                .find(|(name, _)| name == key)
                .map(|(_, value)| value.as_str()),
            _ => None,
        }
    }
}

/// `&amp;`, `&lt;`, `&gt;`, `&quot;`, `&#39;`, `&nbsp;` and numeric references.
pub(crate) fn decode_entities(text: &str) -> String {
    if !text.contains('&') {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let Some(end) = rest[..rest.len().min(12)].find(';') else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let entity = &rest[1..end];
        let decoded = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some('\u{a0}'),
            _ => entity
                .strip_prefix("#x")
                .or_else(|| entity.strip_prefix("#X"))
                .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                .or_else(|| entity.strip_prefix('#').and_then(|n| n.parse().ok()))
                .and_then(char::from_u32),
        };
        match decoded {
            Some(c) => {
                out.push(c);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Splits HTML into text and tags; comments and doctype-like declarations
/// are dropped.
pub(crate) fn tokens(html: &str) -> Vec<Token> {
    let mut out = Vec::new();
    let mut rest = html;
    while !rest.is_empty() {
        let Some(at) = rest.find('<') else {
            out.push(Token::Text(decode_entities(rest)));
            break;
        };
        if at > 0 {
            out.push(Token::Text(decode_entities(&rest[..at])));
        }
        rest = &rest[at..];
        if let Some(comment) = rest.strip_prefix("<!--") {
            rest = comment.find("-->").map_or("", |end| &comment[end + 3..]);
            continue;
        }
        // A tag runs to the first `>` outside quotes.
        let mut quote = None;
        let mut end = None;
        for (index, c) in rest.char_indices().skip(1) {
            match (quote, c) {
                (Some(q), c) if c == q => quote = None,
                (None, '"' | '\'') => quote = Some(c),
                (None, '>') => {
                    end = Some(index);
                    break;
                }
                _ => {}
            }
        }
        let Some(end) = end else {
            out.push(Token::Text(decode_entities(rest)));
            break;
        };
        let inner = rest[1..end].trim();
        rest = &rest[end + 1..];
        if inner.starts_with('!') || inner.starts_with('?') {
            continue;
        }
        if let Some(name) = inner.strip_prefix('/') {
            out.push(Token::Close(name.trim().to_ascii_lowercase()));
            continue;
        }
        let inner = inner.trim_end_matches('/').trim_end();
        let name_end = inner
            .find(|c: char| c.is_whitespace())
            .unwrap_or(inner.len());
        let name = inner[..name_end].to_ascii_lowercase();
        if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            out.push(Token::Text(format!("<{inner}>")));
            continue;
        }
        out.push(Token::Open {
            name,
            attributes: attributes(&inner[name_end..]),
        });
    }
    out
}

fn attributes(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = text.trim_start();
    while !rest.is_empty() {
        let key_end = rest
            .find(|c: char| c == '=' || c.is_whitespace())
            .unwrap_or(rest.len());
        let key = rest[..key_end].to_ascii_lowercase();
        rest = rest[key_end..].trim_start();
        let mut value = String::new();
        if let Some(after) = rest.strip_prefix('=') {
            let after = after.trim_start();
            let (raw, next) = match after.chars().next() {
                Some(q @ ('"' | '\'')) => {
                    let body = &after[1..];
                    let close = body.find(q).unwrap_or(body.len());
                    (&body[..close], &body[(close + 1).min(body.len())..])
                }
                _ => {
                    let close = after.find(char::is_whitespace).unwrap_or(after.len());
                    (&after[..close], &after[close..])
                }
            };
            value = decode_entities(raw);
            rest = next.trim_start();
        }
        if !key.is_empty() {
            out.push((key, value));
        }
    }
    out
}

/// A link target the sanitizer keeps (`protocols.href`): http, https,
/// mailto and relative ones.
pub(crate) fn safe_href(href: &str) -> bool {
    let lower = href.trim().to_ascii_lowercase();
    match lower.split_once(':') {
        Some((scheme, _)) if !scheme.contains('/') => {
            matches!(
                scheme,
                "http" | "https" | "mailto" | "irc" | "ircs" | "xmpp"
            )
        }
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_splits_into_text_and_tags() {
        assert_eq!(
            tokens("a <b class=\"x > y\">bold</b><!-- gone -->&amp;<br/>"),
            vec![
                Token::Text("a ".into()),
                Token::Open {
                    name: "b".into(),
                    attributes: vec![("class".into(), "x > y".into())],
                },
                Token::Text("bold".into()),
                Token::Close("b".into()),
                Token::Text("&".into()),
                Token::Open {
                    name: "br".into(),
                    attributes: vec![],
                },
            ]
        );
        assert_eq!(decode_entities("&lt;&#65;&#x42;&nbsp;"), "<AB\u{a0}");
        assert!(safe_href("https://x.dev") && safe_href("docs/a.md"));
        assert!(!safe_href("javascript:alert(1)"));
    }
}
