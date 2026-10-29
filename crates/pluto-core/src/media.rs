//! Where a markdown image's bytes come from (`client-runtime/markdownImages.ts`
//! `classifyMarkdownImageSource`), shared by the desktop and the phone.

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
