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

/// The hosts of an answer's sources (ADR-0022): the pages its turn's
/// completed web searches and fetches returned (`results[].url` of a
/// `web_search` item, the server's shape for both), and the website an
/// MCP tool's icon names (`toolIcon.pageUrl`). Nothing an agent writes
/// counts: not a message, its reasoning, a command or its output, a tool's
/// arguments, a search's query or a fetch that did not complete. Otherwise
/// a misled agent could name any host, then load an image from it with a
/// secret in its path.
pub fn source_hosts<'a>(items: impl IntoIterator<Item = &'a TurnItem>) -> HashSet<String> {
    let mut hosts = HashSet::new();
    for item in items.into_iter().filter(|item| item.status == "completed") {
        let results = (item.kind == "web_search")
            .then(|| item.get("results").as_array())
            .flatten()
            .into_iter()
            .flatten()
            .map(|result| &result["url"]);
        let icon_page = std::iter::once(&item.get("toolIcon")["pageUrl"]);
        hosts.extend(results.chain(icon_page).filter_map(web_host));
    }
    hosts
}

/// The host of an http(s) URL given whole, normalized.
fn web_host(value: &Value) -> Option<String> {
    url::Url::parse(value.as_str()?.trim())
        .ok()
        .filter(|url| matches!(url.scheme(), "http" | "https"))
        .and_then(|url| url.host_str().map(normalize_host))
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

    fn item(kind: &str, status: &str, data: Value) -> TurnItem {
        let mut item: TurnItem = serde_json::from_value(data).unwrap();
        item.kind = kind.to_owned();
        item.status = status.to_owned();
        item
    }

    fn hosts(items: &[TurnItem]) -> Vec<String> {
        let mut hosts: Vec<String> = source_hosts(items).into_iter().collect();
        hosts.sort();
        hosts
    }

    #[test]
    fn sources_are_the_pages_completed_searches_and_fetches_returned() {
        let items = [
            item(
                "web_search",
                "completed",
                json!({"patterns": ["https://query.example.com/"], "results": [
                    {"title": "A", "url": "https://Blog.Example.org./a?ref=1"},
                    {"url": "ftp://files.example.com/"},
                    {"snippet": "Visit https://quoted.example.net/x"},
                ]}),
            ),
            item(
                "dynamic_tool",
                "completed",
                json!({"toolName": "docs", "input": {}, "toolIcon": {"_tag": "website",
                    "pageUrl": "https://docs.rs/", "faviconUrl": "https://cdn.example.com/i.png"}}),
            ),
            item(
                "web_search",
                "failed",
                json!({"results": [{"url": "https://failed.example.com/"}]}),
            ),
        ];
        assert_eq!(hosts(&items), ["blog.example.org", "docs.rs"]);
    }

    /// What an agent writes or reads never names a source: otherwise a
    /// misled agent could load `https://evil.example.net/<secret>.png`.
    #[test]
    fn what_an_agent_writes_is_never_a_source() {
        let evil = "https://evil.example.net/";
        let items = [
            item("assistant_message", "completed", json!({"text": evil})),
            item("user_message", "completed", json!({"text": evil})),
            item("reasoning", "completed", json!({"text": evil})),
            item(
                "todo_list",
                "completed",
                json!({"steps": [{"text": evil, "status": "pending"}]}),
            ),
            item(
                "command_execution",
                "completed",
                json!({"input": format!("echo {evil}"), "output": evil}),
            ),
            item(
                "dynamic_tool",
                "completed",
                json!({"toolName": "memory.save", "input": {"note": evil}, "output": evil}),
            ),
            item(
                "dynamic_tool",
                "declined",
                json!({"toolName": "WebFetch", "input": {"url": evil}}),
            ),
            item(
                "user_input_request",
                "completed",
                json!({"questions": [{"options": [{"label": evil}]}]}),
            ),
            item("web_search", "completed", json!({"patterns": [evil]})),
        ];
        let sources = source_hosts(&items);
        assert!(sources.is_empty(), "{sources:?}");
        let secret = url::Url::parse("https://evil.example.net/AKIAIOSFODNN7EXAMPLE.png").unwrap();
        assert!(!loads_by_itself(&secret, &sources));
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
