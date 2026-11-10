//! File references and external links in chat markdown
//! (`client-runtime/src/markdownLinks.ts`, `markdown-links.ts`, and the
//! `fileLinkChip` / `MarkdownLinkFavicon` parts of `ChatMarkdown.tsx`):
//! path-shaped inline code and file links become file chips labelled
//! "basename · parent · L12"; external links lead with their site's icon
//! (GitHub's mark, else the site favicon from Google's favicon service like
//! upstream, else a globe). Windows paths and `file:` URLs are not handled.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, OnceLock};

use eframe::egui::{self, Color32};

/// A file reference's path and position (`FilePathPosition`).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FileRef {
    pub(crate) path: String,
    pub(crate) line: Option<u32>,
    pub(crate) column: Option<u32>,
}

const POSIX_FILE_ROOTS: [&str; 24] = [
    "/Users/",
    "/home/",
    "/tmp/",
    "/var/",
    "/etc/",
    "/opt/",
    "/mnt/",
    "/Volumes/",
    "/private/",
    "/root/",
    "/usr/",
    "/bin/",
    "/sbin/",
    "/lib/",
    "/lib64/",
    "/srv/",
    "/dev/",
    "/proc/",
    "/sys/",
    "/run/",
    "/boot/",
    "/media/",
    "/workspace/",
    "/workspaces/",
];
const EXTENSIONLESS_FILE_NAMES: [&str; 27] = [
    "Makefile",
    "makefile",
    "GNUmakefile",
    "Dockerfile",
    "Containerfile",
    "Justfile",
    "justfile",
    "Rakefile",
    "Gemfile",
    "Procfile",
    "Brewfile",
    "Caddyfile",
    "Vagrantfile",
    "Jenkinsfile",
    "Podfile",
    "Fastfile",
    "BUILD",
    "WORKSPACE",
    "LICENSE",
    "LICENCE",
    "COPYING",
    "NOTICE",
    "AUTHORS",
    "CONTRIBUTORS",
    "CHANGELOG",
    "README",
    "CODEOWNERS",
];
const GENERIC_TLDS: [&str; 25] = [
    "com", "net", "org", "io", "dev", "app", "ai", "co", "edu", "gov", "mil", "info", "biz", "xyz",
    "me", "tv", "cc", "gg", "chat", "cloud", "site", "online", "tech", "store", "link",
];
const COUNTRY_TLDS: [&str; 31] = [
    "uk", "de", "fr", "nl", "se", "no", "fi", "dk", "pl", "ch", "at", "be", "es", "it", "pt", "eu",
    "us", "ca", "au", "nz", "jp", "kr", "cn", "br", "ru", "mx", "ie", "cz", "tr", "sg", "hk",
];

fn is_name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-')
}

/// `:12` or `:12:3` at the end (`POSITION_SUFFIX_PATTERN`): the split point.
fn position_suffix(path: &str) -> Option<(usize, u32, Option<u32>)> {
    let mut rest = path;
    let mut numbers = Vec::new();
    for _ in 0..2 {
        let colon = rest.rfind(':')?;
        let digits = &rest[colon + 1..];
        if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
            break;
        }
        numbers.push(digits.parse::<u32>().ok()?);
        rest = &rest[..colon];
        if rest.rfind(':').is_none_or(|c| {
            let d = &rest[c + 1..];
            d.is_empty() || !d.bytes().all(|b| b.is_ascii_digit())
        }) {
            break;
        }
    }
    match numbers.as_slice() {
        [line] => Some((rest.len(), *line, None)),
        [column, line] => Some((rest.len(), *line, Some(*column))),
        _ => None,
    }
}

/// `splitFilePathPosition`: a `:line[:column]` suffix, else a `#L12C3` hash.
fn split_position(path: &str, hash: &str) -> FileRef {
    if let Some((end, line, column)) = position_suffix(path) {
        return FileRef {
            path: path[..end].to_owned(),
            line: (line > 0).then_some(line),
            column: column.filter(|c| *c > 0),
        };
    }
    let lower = hash.to_ascii_lowercase();
    if let Some(rest) = lower.strip_prefix("#l") {
        let (line, column) = match rest.split_once('c') {
            Some((l, c)) => (l, Some(c)),
            None => (rest, None),
        };
        if let Ok(line) = line.parse::<u32>()
            && column.is_none_or(|c| c.bytes().all(|b| b.is_ascii_digit()) && !c.is_empty())
        {
            return FileRef {
                path: path.to_owned(),
                line: (line > 0).then_some(line),
                column: column.and_then(|c| c.parse().ok()).filter(|c| *c > 0),
            };
        }
    }
    FileRef {
        path: path.to_owned(),
        line: None,
        column: None,
    }
}

fn has_extension(name: &str) -> bool {
    name.rsplit_once('.')
        .is_some_and(|(stem, ext)| !stem.is_empty() || name.starts_with('.') && !ext.is_empty())
        && name.rsplit_once('.').is_some_and(|(_, ext)| {
            !ext.is_empty()
                && ext
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        })
}

/// `VERSION_SUFFIX_PATTERN`: a final dot between digits (`glm-5.3`).
fn version_suffix(name: &str) -> bool {
    let bytes = name.as_bytes();
    (1..bytes.len().saturating_sub(1)).any(|i| {
        bytes[i] == b'.'
            && bytes[i - 1].is_ascii_digit()
            && bytes[i + 1].is_ascii_digit()
            && !name[i + 1..].contains('.')
    })
}

fn looks_like_hostname(segment: &str, has_position: bool) -> bool {
    if segment.starts_with('.') {
        return false;
    }
    let lowered = segment.to_ascii_lowercase();
    if lowered == "localhost" {
        return true;
    }
    if lowered.split('.').count() > 1
        && lowered
            .split('.')
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
    {
        return true;
    }
    let labels: Vec<&str> = lowered.split('.').collect();
    let Some(last) = labels.last() else {
        return false;
    };
    if labels.len() < 2 {
        return false;
    }
    GENERIC_TLDS.contains(last) || !has_position && COUNTRY_TLDS.contains(last)
}

/// `inlineCodeFilePathCandidate`: path-shaped inline code only.
pub(crate) fn inline_code_candidate(code: &str) -> Option<String> {
    let trimmed = code.trim();
    if trimmed.is_empty() || trimmed.contains(char::is_whitespace) || trimmed.contains('`') {
        return None;
    }
    let candidate = trimmed.replace('\\', "/");
    let has_position = position_suffix(&candidate).is_some();
    if !has_position && !candidate.contains('/') {
        return None;
    }
    let explicit = candidate.starts_with("~/")
        || candidate.starts_with("./")
        || candidate.starts_with("../")
        || candidate.starts_with('/');
    if !explicit {
        let without = match position_suffix(&candidate) {
            Some((end, ..)) => &candidate[..end],
            None => candidate.as_str(),
        };
        let first = without.split('/').next().unwrap_or(without);
        if looks_like_hostname(first, has_position) {
            return None;
        }
        let basename = without
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap_or("");
        if version_suffix(basename) {
            return None;
        }
        if !has_position && !has_extension(basename) {
            return None;
        }
    }
    Some(candidate)
}

fn relative_file_path(path: &str) -> bool {
    // `RELATIVE_FILE_PATH_PATTERN`: segments of name characters (single
    // spaces allowed inside), at least one `/`, an optional position.
    let without = match position_suffix(path) {
        Some((end, ..)) => &path[..end],
        None => path,
    };
    let segments: Vec<&str> = without.split('/').collect();
    segments.len() >= 2
        && segments.iter().all(|s| {
            !s.is_empty()
                && !s.starts_with(' ')
                && !s.ends_with(' ')
                && !s.contains("  ")
                && s.chars().all(|c| is_name_char(c) || c == ' ')
        })
}

fn relative_file_name(path: &str) -> bool {
    let without = match position_suffix(path) {
        Some((end, ..)) => &path[..end],
        None => path,
    };
    !without.is_empty()
        && !without.contains('/')
        && without.chars().all(|c| is_name_char(c) || c == ' ')
        && has_extension(without)
}

fn looks_like_file_path(path: &str, authored: &str) -> bool {
    if path.starts_with("~/") || path.starts_with("./") || path.starts_with("../") {
        return true;
    }
    if path.starts_with('/') {
        if POSIX_FILE_ROOTS
            .iter()
            .any(|root| authored.starts_with(root))
        {
            return true;
        }
        if position_suffix(authored).is_some() {
            return true;
        }
        let basename = authored.rsplit('/').next().unwrap_or("");
        return EXTENSIONLESS_FILE_NAMES.contains(&basename) || has_extension(basename);
    }
    if EXTENSIONLESS_FILE_NAMES.contains(&path) {
        return true;
    }
    relative_file_path(authored) || relative_file_name(path)
}

pub(crate) fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(byte) = u8::from_str_radix(&value[i + 1..i + 3], 16)
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

/// `parseMarkdownFileLink`: a link destination naming a file, if it does.
pub(crate) fn parse_file_link(href: &str) -> Option<FileRef> {
    let normalized = href.trim().trim_start_matches('<').trim_end_matches('>');
    if normalized.is_empty() || normalized.starts_with('#') || normalized.starts_with("//") {
        return None;
    }
    let (path_with_search, hash) = match normalized.find('#') {
        Some(i) => (&normalized[..i], &normalized[i..]),
        None => (normalized, ""),
    };
    let path = path_with_search.split('?').next().unwrap_or("");
    let path = percent_decode(path.trim());
    let hash = percent_decode(hash.trim());
    if path.is_empty() {
        return None;
    }
    // An external scheme (`https:`, `mailto:`), but not `file.rs:12`.
    if let Some((scheme, rest)) = path.split_once(':')
        && !scheme.is_empty()
        && scheme.starts_with(|c: char| c.is_ascii_alphabetic())
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '.' | '-'))
        && (rest.starts_with("//")
            || !(rest
                .split(':')
                .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
                && rest.split(':').count() <= 2))
    {
        return None;
    }
    let position = split_position(&path, &hash);
    looks_like_file_path(&position.path, &path).then_some(position)
}

pub(crate) fn basename(path: &str) -> &str {
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() {
        return path;
    }
    trimmed.rsplit('/').next().unwrap_or(trimmed)
}

fn parent_segments(path: &str) -> Vec<&str> {
    let mut segments: Vec<&str> = path
        .split('/')
        .filter(|s| !s.is_empty() && *s != ".")
        .collect();
    segments.pop();
    segments
}

/// `buildFileLinkParentSuffixByPath`: paths sharing a basename get the
/// shortest distinct run of parent folders (two at least).
pub(crate) fn parent_suffixes(paths: &[String]) -> HashMap<String, String> {
    let mut groups: HashMap<&str, Vec<&str>> = HashMap::new();
    let mut seen = HashSet::new();
    for path in paths {
        if seen.insert(path.as_str()) {
            groups.entry(basename(path)).or_default().push(path);
        }
    }
    let mut suffixes = HashMap::new();
    for group in groups.values().filter(|g| g.len() >= 2) {
        for path in group {
            let segments = parent_segments(path);
            if segments.is_empty() {
                continue;
            }
            let tail = |segments: &[&str], depth: usize| {
                segments[segments.len().saturating_sub(depth)..].join("/")
            };
            let unique = (1..=segments.len())
                .find(|&depth| {
                    let candidate = tail(&segments, depth);
                    !group.iter().any(|other| {
                        other != path && tail(&parent_segments(other), depth) == candidate
                    })
                })
                .unwrap_or(segments.len());
            let depth = segments.len().min(unique.max(2));
            suffixes.insert((*path).to_owned(), tail(&segments, depth));
        }
    }
    suffixes
}

/// The chip's label: `basename · parent · L12:C3`.
pub(crate) fn chip_label(file: &FileRef, suffixes: &HashMap<String, String>) -> String {
    let mut parts = vec![basename(&file.path).to_owned()];
    if let Some(suffix) = suffixes.get(&file.path).filter(|s| !s.is_empty()) {
        parts.push(suffix.clone());
    }
    if let Some(line) = file.line {
        parts.push(match file.column {
            Some(column) => format!("L{line}:C{column}"),
            None => format!("L{line}"),
        });
    }
    parts.join(" · ")
}

/// `resolveExternalWebLinkHost`: an http(s) link's host name.
pub(crate) fn external_host(href: &str) -> Option<String> {
    let href = if href.starts_with("//") {
        format!("https:{href}")
    } else {
        href.to_owned()
    };
    let url = url::Url::parse(&href).ok()?;
    matches!(url.scheme(), "http" | "https")
        .then(|| {
            url.host_str()
                .map(|h| h.trim_matches(['[', ']']).to_owned())
        })
        .flatten()
        .filter(|h| !h.is_empty())
}

/// What leads a link: the GitHub mark, a fetched favicon, or the globe.
pub(crate) enum LinkIcon {
    Named(&'static str),
    Image(Arc<image::RgbaImage>),
}

#[derive(Clone)]
enum Favicon {
    Pending,
    Failed,
    Image(Arc<image::RgbaImage>),
}

fn favicons() -> &'static Mutex<HashMap<String, Favicon>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Favicon>>> = OnceLock::new();
    CACHE.get_or_init(Default::default)
}

/// `MarkdownLinkFavicon`: the icon for a link to `host`. GitHub has its
/// mark; a site among the answer's `sources` has its own favicon, fetched
/// from the site on a background thread once per host when the rule of
/// ADR-0022 allows it; any other link, or a failed fetch, has the globe.
pub(crate) fn link_icon(host: &str, sources: &HashSet<String>, ctx: &egui::Context) -> LinkIcon {
    let lower = host.to_ascii_lowercase();
    if lower == "github.com" || lower.ends_with(".github.com") {
        return LinkIcon::Named("github-mark");
    }
    let Some(url) = url::Url::parse(&format!("https://{host}/favicon.ico"))
        .ok()
        .filter(|url| pluto_core::media::loads_by_itself(url, sources))
    else {
        return LinkIcon::Named("globe");
    };
    let Ok(mut cache) = favicons().lock() else {
        return LinkIcon::Named("globe");
    };
    match cache.get(&lower).cloned() {
        Some(Favicon::Image(image)) => LinkIcon::Image(image),
        Some(_) => LinkIcon::Named("globe"),
        None => {
            cache.insert(lower.clone(), Favicon::Pending);
            let sources = sources.clone();
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                let state = crate::http::get_public(&url, |hop| {
                    pluto_core::media::loads_by_itself(hop, &sources)
                })
                .and_then(|bytes| crate::ui::attachments::decode(&bytes))
                .map_or(Favicon::Failed, |image| Favicon::Image(Arc::new(image)));
                if let Ok(mut cache) = favicons().lock() {
                    cache.insert(lower, state);
                }
                ctx.request_repaint();
            });
            LinkIcon::Named("globe")
        }
    }
}

// Oklab, for the chip colors (`color-mix(in oklab, …)`).
fn to_linear(c: u8) -> f32 {
    let c = c as f32 / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn from_linear(c: f32) -> u8 {
    let c = if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    };
    (c.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn oklab(color: Color32) -> [f32; 3] {
    let (r, g, b) = (
        to_linear(color.r()),
        to_linear(color.g()),
        to_linear(color.b()),
    );
    let l = (0.412_221_47 * r + 0.536_332_55 * g + 0.051_445_99 * b).cbrt();
    let m = (0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b).cbrt();
    let s = (0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b).cbrt();
    [
        0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s,
        1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s,
        0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s,
    ]
}

fn from_oklab([l, a, b]: [f32; 3]) -> Color32 {
    let l_ = (l + 0.396_337_78 * a + 0.215_803_76 * b).powi(3);
    let m_ = (l - 0.105_561_346 * a - 0.063_854_17 * b).powi(3);
    let s_ = (l - 0.089_484_18 * a - 1.291_485_5 * b).powi(3);
    Color32::from_rgb(
        from_linear(4.076_741_7 * l_ - 3.307_711_6 * m_ + 0.230_969_94 * s_),
        from_linear(-1.268_438 * l_ + 2.609_757_4 * m_ - 0.341_319_38 * s_),
        from_linear(-0.004_196_086 * l_ - 0.703_418_6 * m_ + 1.707_614_7 * s_),
    )
}

fn oklch(l: f32, c: f32, h: f32) -> Color32 {
    let h = h.to_radians();
    from_oklab([l, c * h.cos(), c * h.sin()])
}

/// `color-mix(in oklab, a t, b)` with premultiplied alpha, as CSS mixes a
/// translucent color: the result keeps the combined alpha (unmultiplied).
fn mix_oklab(a: Color32, b: Color32, t: f32) -> Color32 {
    let alpha = |c: Color32| c.a() as f32 / 255.0;
    let unmultiplied = |c: Color32| {
        let [r, g, b, _] = c.to_srgba_unmultiplied();
        Color32::from_rgb(r, g, b)
    };
    let (wa, wb) = (t * alpha(a), (1.0 - t) * alpha(b));
    let total = wa + wb;
    if total <= 0.0 {
        return Color32::TRANSPARENT;
    }
    let (la, lb) = (oklab(unmultiplied(a)), oklab(unmultiplied(b)));
    let lab = [0, 1, 2].map(|i| (la[i] * wa + lb[i] * wb) / total);
    let [r, g, b, _] = from_oklab(lab).to_srgba_unmultiplied();
    Color32::from_rgba_unmultiplied(r, g, b, (total * 255.0).round() as u8)
}

/// A mention chip's (border, fill, text) (`ContextChip kind="mention"`:
/// accent `oklch(0.62 0.11 215)`; the border 34% toward it from the
/// border color, the fill the accent at 11%, the text 22% toward it). Fill
/// and border may be translucent: paint the fill, then the border over it.
pub(crate) fn mention_colors(border: Color32, foreground: Color32) -> [Color32; 3] {
    chip_colors(0.11, 215.0, border, foreground)
}

/// A context chip's (border, fill, text) for its kind's accent
/// `oklch(0.62 chroma hue)` (`contextChipVariants`).
pub(crate) fn chip_colors(
    chroma: f32,
    hue: f32,
    border: Color32,
    foreground: Color32,
) -> [Color32; 3] {
    // Every paragraph asks for them; the oklab mixes are kept per theme.
    type Key = (u32, u32, Color32, Color32);
    thread_local! {
        static COLORS: std::cell::RefCell<Vec<(Key, [Color32; 3])>> =
            const { std::cell::RefCell::new(Vec::new()) };
    }
    let key = (chroma.to_bits(), hue.to_bits(), border, foreground);
    COLORS.with_borrow_mut(|colors| {
        if let Some((_, value)) = colors.iter().find(|(k, _)| *k == key) {
            return *value;
        }
        let accent = oklch(0.62, chroma, hue);
        let value = [
            mix_oklab(accent, border, 0.34),
            accent.gamma_multiply(0.11),
            mix_oklab(accent, foreground, 0.22),
        ];
        if colors.len() >= 16 {
            colors.clear();
        }
        colors.push((key, value));
        value
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inline_code_needs_path_evidence() {
        assert_eq!(
            inline_code_candidate("crates/core/src/overview.rs:58").as_deref(),
            Some("crates/core/src/overview.rs:58")
        );
        assert_eq!(
            inline_code_candidate("src/app.rs").as_deref(),
            Some("src/app.rs")
        );
        assert_eq!(
            inline_code_candidate("app.rs:12").as_deref(),
            Some("app.rs:12")
        );
        assert_eq!(inline_code_candidate("README.md"), None);
        assert_eq!(inline_code_candidate("cargo test"), None);
        assert_eq!(inline_code_candidate("origin/main"), None);
        assert_eq!(inline_code_candidate("github.com/foo/bar.rs"), None);
        assert_eq!(inline_code_candidate("models/glm-5.3"), None);
        assert_eq!(inline_code_candidate("~/notes"), Some("~/notes".into()));
    }

    #[test]
    fn file_links_parse_positions() {
        assert_eq!(
            parse_file_link("src/main.rs#L10"),
            Some(FileRef {
                path: "src/main.rs".into(),
                line: Some(10),
                column: None
            })
        );
        assert_eq!(
            parse_file_link("crates/a.rs:3:7"),
            Some(FileRef {
                path: "crates/a.rs".into(),
                line: Some(3),
                column: Some(7)
            })
        );
        assert_eq!(parse_file_link("https://github.com/x"), None);
        assert_eq!(parse_file_link("#section"), None);
        assert_eq!(parse_file_link("/chat/settings"), None);
        assert!(parse_file_link("/home/me/notes").is_some());
    }

    #[test]
    fn labels_disambiguate_basenames() {
        let paths = vec![
            "src/ui/mod.rs".to_owned(),
            "src/model/mod.rs".to_owned(),
            "src/app.rs".to_owned(),
        ];
        let suffixes = parent_suffixes(&paths);
        assert_eq!(
            suffixes.get("src/ui/mod.rs").map(String::as_str),
            Some("src/ui")
        );
        assert!(!suffixes.contains_key("src/app.rs"));
        let file = FileRef {
            path: "src/app.rs".into(),
            line: Some(58),
            column: None,
        };
        assert_eq!(chip_label(&file, &suffixes), "app.rs · L58");
    }

    #[test]
    fn links_name_their_host() {
        assert_eq!(
            external_host("https://docs.gog.com/x").as_deref(),
            Some("docs.gog.com")
        );
        assert_eq!(external_host("mailto:a@b.c"), None);
    }

    #[test]
    fn chip_colors_match_the_reference() {
        // Measured on the reference over the stock dark palette.
        let [border, fill, text] = mention_colors(
            Color32::WHITE.gamma_multiply(0.06),
            Color32::from_rgb(0xf5, 0xf5, 0xf5),
        );
        // Composited like the page: the fill over the background, the
        // border over the fill.
        let over = |top: Color32, below: Color32| {
            let a = top.a() as f32 / 255.0;
            let c = |t: u8, b: u8| (t as f32 + b as f32 * (1.0 - a)).round() as u8;
            Color32::from_rgb(
                c(top.r(), below.r()),
                c(top.g(), below.g()),
                c(top.b(), below.b()),
            )
        };
        let fill = over(fill, Color32::from_rgb(0x0a, 0x0a, 0x0a));
        let border = over(border, fill);
        let close = |a: Color32, b: Color32| {
            (a.r() as i32 - b.r() as i32).abs() <= 3
                && (a.g() as i32 - b.g() as i32).abs() <= 3
                && (a.b() as i32 - b.b() as i32).abs() <= 3
        };
        assert!(
            close(border, Color32::from_rgb(0x1c, 0x4e, 0x57)),
            "{border:?}"
        );
        assert!(close(fill, Color32::from_rgb(0x09, 0x1a, 0x1c)), "{fill:?}");
        assert!(close(text, Color32::from_rgb(0xcb, 0xe0, 0xe6)), "{text:?}");
    }
}
