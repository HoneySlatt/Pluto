//! Where a markdown image's bytes come from (`client-runtime/markdownImages.ts`
//! `classifyMarkdownImageSource`), and whether a remote image loads without
//! a click (ADR-0022), shared by the desktop and the phone.

use std::collections::HashSet;

use serde_json::Value;

use crate::model::TurnItem;

/// Where an image's bytes must be loaded from.
#[derive(Debug, PartialEq)]
pub enum ImageSource {
    /// A URL the client loads itself (`https:`, `data:`, …).
    Direct(String),
    /// A file on the environment host, loaded through a signed asset URL.
    File(String),
    /// Nothing loadable (fragments, other schemes, `~` paths).
    Blocked,
}

/// `normalizeMarkdownLinkDestination`: trimmed, angle brackets dropped.
fn normalize_destination(value: &str) -> &str {
    let trimmed = value.trim();
    trimmed
        .strip_prefix('<')
        .and_then(|rest| rest.strip_suffix('>'))
        .unwrap_or(trimmed)
}

pub fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let Some(byte) = value
                .get(i + 1..i + 3)
                .and_then(|hex| u8::from_str_radix(hex, 16).ok())
        {
            out.push(byte);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| value.to_owned())
}
/// `classifyMarkdownImageSource` for POSIX paths: direct URLs load as they
/// are, absolute and `file:` paths and workspace-relative ones become host
/// files, anything else is blocked.
pub fn classify(value: &str, workspace_root: Option<&str>) -> ImageSource {
    let source = normalize_destination(value);
    if source.is_empty() || source.starts_with('#') || source.starts_with('?') {
        return ImageSource::Blocked;
    }
    let lower = source.to_ascii_lowercase();
    if ["http:", "https:", "data:", "blob:", "//"]
        .iter()
        .any(|p| lower.starts_with(p))
    {
        return ImageSource::Direct(source.to_owned());
    }
    if lower.starts_with("file:") {
        return url::Url::parse(source)
            .ok()
            .filter(|url| url.scheme() == "file")
            .map_or(ImageSource::Blocked, |url| {
                ImageSource::File(percent_decode(url.path()))
            });
    }
    let path = source.split(['?', '#']).next().unwrap_or_default();
    let path = percent_decode(path);
    if path.is_empty() {
        return ImageSource::Blocked;
    }
    if path.starts_with('/') {
        return ImageSource::File(path);
    }
    let has_scheme = path.split_once(':').is_some_and(|(scheme, _)| {
        scheme.starts_with(|c: char| c.is_ascii_alphabetic())
            && scheme
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '.' | '-'))
    });
    if has_scheme || path.starts_with("~/") {
        return ImageSource::Blocked;
    }
    let Some(root) = workspace_root else {
        return ImageSource::Blocked;
    };
    let joined = format!(
        "{}/{}",
        root.trim_end_matches('/'),
        path.trim_start_matches('/')
    );
    // A relative path stays in the workspace: `../` out of it is blocked.
    let inside = segments(&joined)
        .zip(segments(root))
        .is_some_and(|(target, workspace)| {
            target.len() > workspace.len() && target.starts_with(&workspace)
        });
    if !inside {
        return ImageSource::Blocked;
    }
    ImageSource::File(joined)
}

/// Lexical path segments of an absolute path, `.` and `..` resolved.
pub fn segments(path: &str) -> Option<Vec<&str>> {
    let rest = path.strip_prefix('/')?;
    let mut out: Vec<&str> = Vec::new();
    for segment in rest.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                out.pop();
            }
            segment => out.push(segment),
        }
    }
    Some(out)
}

/// The hosts of an answer's sources: those of the URLs its turn's tool calls
/// carry as whole values (a fetched page, a search result, an opened link),
/// in their input, output and page fields. Messages are not sources, nor a
/// URL quoted inside a longer text (a page's content), nor an icon's own
/// image URL: what an agent writes or reads cannot widen the set.
pub fn source_hosts<'a>(items: impl IntoIterator<Item = &'a TurnItem>) -> HashSet<String> {
    let mut hosts = HashSet::new();
    for item in items {
        if matches!(item.kind.as_str(), "user_message" | "assistant_message") {
            continue;
        }
        for (key, value) in &item.data {
            collect_hosts(key, value, &mut hosts);
        }
    }
    hosts
}

fn collect_hosts(key: &str, value: &Value, hosts: &mut HashSet<String>) {
    const IMAGE_KEYS: [&str; 4] = ["faviconUrl", "faviconUrlDark", "logoUrl", "logoUrlDark"];
    match value {
        Value::String(text) if !IMAGE_KEYS.contains(&key) => {
            if let Some(host) = url::Url::parse(text.trim())
                .ok()
                .filter(|url| matches!(url.scheme(), "http" | "https"))
                .and_then(|url| url.host_str().map(normalize_host))
            {
                hosts.insert(host);
            }
        }
        Value::Array(values) => values.iter().for_each(|v| collect_hosts(key, v, hosts)),
        Value::Object(map) => map.iter().for_each(|(k, v)| collect_hosts(k, v, hosts)),
        _ => {}
    }
}

fn normalize_host(host: &str) -> String {
    host.trim_end_matches('.').to_ascii_lowercase()
}

/// Whether a remote image or a link's icon loads without a click
/// (ADR-0022): over HTTPS, without credentials or a query string, from a
/// public host among the answer's `sources` (`source_hosts`). The fetch must
/// still go through `http::get_public`, which checks the address it
/// connects to and every redirect. Anything else loads on a click.
pub fn loads_by_itself(url: &url::Url, sources: &HashSet<String>) -> bool {
    url.scheme() == "https"
        && url.query().is_none()
        && url.username().is_empty()
        && url.password().is_none()
        && url.host_str().is_some_and(|host| {
            crate::http::is_public_host(host) && sources.contains(&normalize_host(host))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn item(kind: &str, data: Value) -> TurnItem {
        let mut item: TurnItem = serde_json::from_value(data).unwrap();
        item.kind = kind.to_owned();
        item
    }

    fn hosts(items: &[TurnItem]) -> Vec<String> {
        let mut hosts: Vec<String> = source_hosts(items).into_iter().collect();
        hosts.sort();
        hosts
    }

    #[test]
    fn sources_are_the_urls_tool_calls_carry_whole() {
        let items = [
            item(
                "dynamic_tool",
                json!({"input": {"url": "https://Docs.RS/serde"}, "output": {"results": [
                    {"title": "A", "url": "https://blog.example.org/a?ref=1"},
                    "Visit https://quoted.example.net/x for more",
                ]}}),
            ),
            item(
                "web_search",
                json!({"toolIcon": {"_tag": "website", "pageUrl": "https://news.example.com./",
                    "faviconUrl": "https://cdn.example.com/icon.png"}}),
            ),
            item(
                "assistant_message",
                json!({"text": "https://answer.example.com/"}),
            ),
            item("user_message", json!({"text": "https://user.example.com/"})),
            item(
                "command_execution",
                json!({"input": {"command": "ls ftp://files.example.com"}}),
            ),
        ];
        assert_eq!(
            hosts(&items),
            ["blog.example.org", "docs.rs", "news.example.com"]
        );
    }

    #[test]
    fn only_safe_urls_load_by_themselves() {
        let sources: HashSet<String> = ["docs.rs", "10.0.0.5", "intranet.local"]
            .map(str::to_owned)
            .into();
        let loads = |raw: &str| loads_by_itself(&url::Url::parse(raw).unwrap(), &sources);
        assert!(loads("https://docs.rs/logo.png"));
        assert!(loads("https://DOCS.rs./logo.png"));
        assert!(!loads("https://docs.rs/logo.png?secret=1"));
        assert!(!loads("https://docs.rs/logo.png?"));
        assert!(!loads("http://docs.rs/logo.png"));
        assert!(!loads("https://user:pass@docs.rs/logo.png"));
        assert!(!loads("https://evil.example.com/logo.png"));
        assert!(!loads("https://sub.docs.rs/logo.png"));
        assert!(!loads("https://10.0.0.5/logo.png"));
        assert!(!loads("https://intranet.local/logo.png"));
        assert!(!loads_by_itself(
            &url::Url::parse("https://docs.rs/logo.png").unwrap(),
            &HashSet::new()
        ));
    }
}
