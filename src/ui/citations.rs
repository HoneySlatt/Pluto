//! Assistant citations: a quote of rendered assistant text, with an
//! optional comment, sent inside the prompt as a markdown link
//! (`packages/shared/src/assistantCitations.ts`), its text selector
//! (`lib/assistantTextSelection.ts`) and its chip (`AssistantCitationChip`,
//! `ContextChip kind="citation"`).

use eframe::egui::{self, Color32, CornerRadius, Pos2, Rect, Stroke, StrokeKind, Ui, Vec2};

use crate::theme::{self, Weight, font};

/// `ASSISTANT_CITATION_MAX_TEXT_LENGTH` (UTF-16 units).
pub const MAX_TEXT_LENGTH: usize = 8_000;
/// `ASSISTANT_CITATION_MAX_COMMENT_LENGTH` (UTF-16 units).
pub const MAX_COMMENT_LENGTH: usize = 8_000;
/// `ASSISTANT_CITATION_CONTEXT_LENGTH` (UTF-16 units).
const CONTEXT_LENGTH: usize = 32;
const LINK_START: &str = "[Assistant quote](";
const HREF_PREFIX: &str = "t3-citation://v1/";
const MAX_HREF_LENGTH: usize = 9 * (MAX_TEXT_LENGTH + MAX_COMMENT_LENGTH) + 16_000;

/// `AssistantCitation` (version 1).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Citation {
    pub environment_id: String,
    pub thread_id: String,
    pub message_id: String,
    pub text: String,
    pub comment: Option<String>,
    pub start: u64,
    pub end: u64,
    pub prefix: String,
    pub suffix: String,
}

pub fn utf16_len(text: &str) -> usize {
    text.encode_utf16().count()
}

/// `encodePathPart`: `encodeURIComponent`, with `!'()*` escaped too.
fn encode_path_part(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// `decodeURIComponent` (None on a malformed escape or invalid UTF-8).
fn decode_uri_component(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hex = value.get(index + 1..index + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out).ok()
}

impl Citation {
    /// `withAssistantCitationComment`: a blank comment removes it.
    pub fn with_comment(&self, comment: &str) -> Self {
        let trimmed = comment.trim();
        Self {
            comment: (!trimmed.is_empty()).then(|| trimmed.to_owned()),
            ..self.clone()
        }
    }

    /// `formatAssistantCitationHref`.
    pub fn href(&self) -> String {
        let path = [&self.environment_id, &self.thread_id, &self.message_id]
            .map(|part| encode_path_part(part))
            .join("/");
        let mut query = url::form_urlencoded::Serializer::new(String::new());
        query
            .append_pair("text", &self.text)
            .append_pair("start", &self.start.to_string())
            .append_pair("end", &self.end.to_string())
            .append_pair("prefix", &self.prefix)
            .append_pair("suffix", &self.suffix);
        if let Some(comment) = &self.comment {
            query.append_pair("comment", comment);
        }
        format!("{HREF_PREFIX}{path}?{}", query.finish())
    }

    /// `serializeAssistantCitation`.
    pub fn serialize(&self) -> String {
        format!("{LINK_START}{})", self.href())
    }

    /// The chip's label: the comment, else the quote, whitespace collapsed
    /// and cut at 64 characters.
    pub fn label(&self) -> String {
        let source = self
            .comment
            .as_deref()
            .map(str::trim)
            .filter(|c| !c.is_empty())
            .unwrap_or(&self.text);
        let preview = normalize_whitespace(source);
        let units: Vec<u16> = preview.encode_utf16().collect();
        if units.len() > 64 {
            format!("{}…", String::from_utf16_lossy(&units[..64]))
        } else {
            preview
        }
    }
}

/// `parseAssistantCitationHref`.
pub fn parse_href(href: &str) -> Option<Citation> {
    if !href.starts_with(HREF_PREFIX) || href.len() > MAX_HREF_LENGTH {
        return None;
    }
    let url = url::Url::parse(href).ok()?;
    let parts: Vec<&str> = url.path().trim_start_matches('/').split('/').collect();
    if url.scheme() != "t3-citation"
        || url.host_str() != Some("v1")
        || parts.len() != 3
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || url.fragment().is_some()
    {
        return None;
    }
    let pairs: Vec<(String, String)> = url.query_pairs().into_owned().collect();
    let get = |key: &str| -> Option<&String> {
        let mut values = pairs.iter().filter(|(k, _)| k == key);
        let value = values.next().map(|(_, v)| v);
        if values.next().is_some() { None } else { value }
    };
    let comment = get("comment").cloned();
    let required = ["text", "start", "end", "prefix", "suffix"];
    if pairs.len() != required.len() + usize::from(comment.is_some())
        || required.iter().any(|key| get(key).is_none())
    {
        return None;
    }
    let number = |key: &str| -> Option<u64> {
        let value = get(key)?;
        (!value.is_empty() && value.len() <= 16 && value.bytes().all(|b| b.is_ascii_digit()))
            .then(|| value.parse().ok())
            .flatten()
    };
    let citation = Citation {
        environment_id: decode_uri_component(parts[0])?,
        thread_id: decode_uri_component(parts[1])?,
        message_id: decode_uri_component(parts[2])?,
        text: get("text")?.clone(),
        comment,
        start: number("start")?,
        end: number("end")?,
        prefix: get("prefix")?.clone(),
        suffix: get("suffix")?.clone(),
    };
    // The `AssistantCitation` schema's checks.
    let ids_fit = [
        &citation.environment_id,
        &citation.thread_id,
        &citation.message_id,
    ]
    .iter()
    .all(|id| !id.is_empty() && utf16_len(id) <= 512);
    let valid = ids_fit
        && !citation.text.trim().is_empty()
        && utf16_len(&citation.text) <= MAX_TEXT_LENGTH
        && citation
            .comment
            .as_ref()
            .is_none_or(|c| utf16_len(c) <= MAX_COMMENT_LENGTH)
        && utf16_len(&citation.prefix) <= CONTEXT_LENGTH
        && utf16_len(&citation.suffix) <= CONTEXT_LENGTH
        && citation.end > citation.start;
    valid.then_some(citation)
}

/// `collectAssistantCitations`: each citation link with its byte range.
pub fn collect(text: &str) -> Vec<(Citation, std::ops::Range<usize>)> {
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(offset) = text[from..].find(LINK_START) {
        let start = from + offset;
        let href_start = start + LINK_START.len();
        // `[^\s)]{1,N}` then `)`.
        let href_len = text[href_start..]
            .find(|c: char| c.is_whitespace() || c == ')')
            .unwrap_or(text.len() - href_start);
        let href = &text[href_start..href_start + href_len];
        let closes = text[href_start + href_len..].starts_with(')');
        match parse_href(href).filter(|_| closes) {
            Some(citation) => {
                let end = href_start + href_len + 1;
                out.push((citation, start..end));
                from = end;
            }
            None => from = href_start,
        }
    }
    out
}

/// `assistantCitationsToPlainText`: quotes and comments in place of links.
pub fn to_plain_text(prompt: &str) -> String {
    let mut out = String::new();
    let mut cursor = 0;
    for (citation, range) in collect(prompt) {
        out.push_str(&prompt[cursor..range.start]);
        out.push_str(&citation.text);
        if let Some(comment) = &citation.comment {
            out.push_str("\nComment: ");
            out.push_str(comment);
        }
        cursor = range.end;
    }
    out.push_str(&prompt[cursor..]);
    out
}

/// JS `text.replace(/\s+/g, " ")`.
pub fn normalize_whitespace(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_space = false;
    for c in text.chars() {
        if c.is_whitespace() {
            if !in_space {
                out.push(' ');
            }
            in_space = true;
        } else {
            out.push(c);
            in_space = false;
        }
    }
    out
}

/// A quote's selector in its message's rendered text (`AssistantTextSelector`).
#[derive(Clone, Debug, PartialEq)]
pub struct Selector {
    pub text: String,
    pub start: u64,
    pub end: u64,
    pub prefix: String,
    pub suffix: String,
}

/// `createAssistantTextSelector`: the exact quote between byte offsets of
/// `text`, with UTF-16 positions and context in the whitespace-collapsed
/// stream.
pub fn create_selector(text: &str, raw_start: usize, raw_end: usize) -> Option<Selector> {
    let quote = &text[raw_start..raw_end];
    if quote.trim().is_empty() {
        return None;
    }
    let normalized: Vec<u16> = normalize_whitespace(text).encode_utf16().collect();
    let mut start = utf16_len(&normalize_whitespace(&text[..raw_start]));
    // A selection starting inside a whitespace run includes its normalized space.
    let before = text[..raw_start].chars().next_back();
    let at = text[raw_start..].chars().next();
    if before.is_some_and(char::is_whitespace) && at.is_some_and(char::is_whitespace) {
        start -= 1;
    }
    let end = utf16_len(&normalize_whitespace(&text[..raw_end]));
    let splits_pair = |offset: usize| {
        offset > 0
            && offset < normalized.len()
            && (0xd800..=0xdbff).contains(&normalized[offset - 1])
            && (0xdc00..=0xdfff).contains(&normalized[offset])
    };
    let mut prefix_start = start.saturating_sub(CONTEXT_LENGTH);
    let mut suffix_end = (end + CONTEXT_LENGTH).min(normalized.len());
    if splits_pair(prefix_start) {
        prefix_start += 1;
    }
    if splits_pair(suffix_end) {
        suffix_end -= 1;
    }
    Some(Selector {
        text: quote.to_owned(),
        start: start as u64,
        end: end as u64,
        prefix: String::from_utf16_lossy(&normalized[prefix_start..start]),
        suffix: String::from_utf16_lossy(&normalized[end..suffix_end]),
    })
}

/// The byte range of `quote` in `text`, both compared with whitespace
/// collapsed; the first match at or after `from` (a byte offset), else the
/// first anywhere.
pub fn find_quote(text: &str, quote: &str, from: usize) -> Option<(usize, usize)> {
    let wanted: Vec<char> = normalize_whitespace(quote).chars().collect();
    if wanted.iter().all(|c| *c == ' ') {
        return None;
    }
    // The collapsed stream, each character with the byte range it covers.
    let mut stream: Vec<(char, usize, usize)> = Vec::new();
    for (byte, c) in text.char_indices() {
        let end = byte + c.len_utf8();
        // Layout-only breaks are not text.
        if c == '\u{200B}' {
            continue;
        }
        if c.is_whitespace() {
            if let Some(last) = stream.last_mut()
                && last.0 == ' '
            {
                last.2 = end;
                continue;
            }
            stream.push((' ', byte, end));
        } else {
            stream.push((c, byte, end));
        }
    }
    let matches = (0..stream.len().saturating_sub(wanted.len() - 1)).filter(|&start| {
        stream[start..start + wanted.len()]
            .iter()
            .zip(&wanted)
            .all(|((c, ..), w)| c == w)
    });
    let mut first = None;
    for start in matches {
        let range = (stream[start].1, stream[start + wanted.len() - 1].2);
        if range.0 >= from {
            return Some(range);
        }
        first.get_or_insert(range);
    }
    first
}

/// The citation chip's colors (`kind="citation"`, accent
/// `oklch(0.62 0.16 259)`): (border, fill, text).
pub fn chip_colors() -> [Color32; 3] {
    crate::ui::md_links::chip_colors(0.16, 259.0, theme::border(), theme::foreground())
}

/// A chip's metrics at `size` (the surrounding text size; the chip's text
/// is 0.86em): its width for `label`, with the comment pencil when
/// `composer`.
pub fn chip_width(ui: &Ui, label: &str, size: f32, composer: bool) -> f32 {
    let chip = size * 0.86;
    let label_width =
        crate::ui::widgets::text_width(ui, label, font(chip, Weight::Medium)).min(16.0 * chip);
    let action = if composer {
        0.17 * chip + 0.33 * chip + 1.17 * chip
    } else {
        0.0
    };
    2.0 + 2.0 * 0.5 * chip + 1.17 * chip + 0.33 * chip + label_width + action
}

/// The chip's responses: the source link and the comment pencil.
pub struct ChipResponse {
    pub link: egui::Response,
    pub pencil: Option<egui::Response>,
}

/// Paints a chip whose box is `rect` (`h-[1.41em] rounded-[0.5em] border
/// px-[0.5em]`, the quote icon, the label truncated to 16em, and in the
/// composer the pencil): returns its source link and pencil.
pub fn paint_chip(
    ui: &mut Ui,
    rect: Rect,
    label: &str,
    size: f32,
    id: egui::Id,
    composer: bool,
) -> ChipResponse {
    let chip = size * 0.86;
    let [border, fill, text] = chip_colors();
    let radius = CornerRadius::same((0.5 * chip).round() as u8);
    let icon_size = (1.17 * chip).round();
    let action_size = 1.17 * chip;
    let pencil_rect = composer.then(|| {
        Rect::from_min_size(
            Pos2::new(
                rect.right() - 1.0 - 0.5 * chip - action_size,
                rect.center().y - action_size / 2.0,
            ),
            Vec2::splat(action_size),
        )
    });
    let link_rect = Rect::from_min_max(
        rect.min,
        Pos2::new(
            pencil_rect.map_or(rect.right(), |p| p.left() - 0.17 * chip),
            rect.bottom(),
        ),
    );
    let link = ui.interact(link_rect, id.with("link"), egui::Sense::click());
    let pencil = pencil_rect.map(|r| ui.interact(r, id.with("pencil"), egui::Sense::click()));
    let painter = ui.painter();
    painter.rect_filled(rect, radius, fill);
    // Chat chips tint on hover (`hover:bg-(--context-chip-accent)/17`).
    if !composer && link.hovered() {
        painter.rect_filled(
            link_rect.shrink(1.0),
            CornerRadius::same(4),
            chip_accent().gamma_multiply(0.17),
        );
    }
    painter.rect_stroke(rect, radius, Stroke::new(1.0, border), StrokeKind::Inside);
    let icon = Rect::from_min_size(
        Pos2::new(
            rect.left() + 1.0 + 0.5 * chip,
            rect.center().y - icon_size / 2.0,
        ),
        Vec2::splat(icon_size),
    );
    theme::paint_icon(ui, "quote", icon, icon_size, text);
    let label_left = icon.right() + 0.33 * chip;
    let label_right = pencil_rect.map_or(rect.right() - 1.0 - 0.5 * chip, |p| {
        p.left() - 0.17 * chip - 0.33 * chip
    });
    let galley = crate::ui::widgets::galley_truncated(
        ui,
        label,
        font(chip, Weight::Medium),
        text,
        (1.2 * chip).round(),
        (label_right - label_left).max(0.0),
    );
    ui.painter().galley(
        Pos2::new(label_left, rect.center().y - galley.size().y / 2.0),
        galley,
        text,
    );
    if let (Some(pencil_rect), Some(pencil)) = (pencil_rect, &pencil) {
        if pencil.hovered() {
            ui.painter().rect_filled(
                pencil_rect,
                CornerRadius::same(4),
                chip_accent().gamma_multiply(0.17),
            );
        }
        theme::paint_icon(ui, "pencil", pencil_rect, action_size.round(), text);
    }
    if link.hovered() || pencil.as_ref().is_some_and(|p| p.hovered()) {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    ChipResponse { link, pencil }
}

pub fn chip_accent() -> Color32 {
    Color32::from_rgb(70, 132, 229)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn citation() -> Citation {
        Citation {
            environment_id: "env".into(),
            thread_id: "thread 1".into(),
            message_id: "m(1)".into(),
            text: "pomme rouge & co".into(),
            comment: Some("Pourquoi ?".into()),
            start: 9,
            end: 25,
            prefix: "Voici ta ".into(),
            suffix: " 🍎".into(),
        }
    }

    #[test]
    fn links_round_trip_like_upstream() {
        let citation = citation();
        let href = citation.href();
        assert_eq!(
            href,
            "t3-citation://v1/env/thread%201/m%281%29?text=pomme+rouge+%26+co&start=9&end=25&prefix=Voici+ta+&suffix=+%F0%9F%8D%8E&comment=Pourquoi+%3F"
        );
        assert_eq!(parse_href(&href), Some(citation.clone()));
        let prompt = format!("Regarde {} ici", citation.serialize());
        let found = collect(&prompt);
        assert_eq!(found.len(), 1);
        assert_eq!(&prompt[found[0].1.clone()], citation.serialize());
        assert_eq!(
            to_plain_text(&prompt),
            "Regarde pomme rouge & co\nComment: Pourquoi ? ici"
        );
        assert!(parse_href("t3-citation://v1/a/b?text=x&start=0&end=1&prefix=&suffix=").is_none());
    }

    #[test]
    fn selectors_use_collapsed_utf16_positions() {
        let text = "Voici  ta\npomme rouge 🍎 fin";
        let start = text.find("pomme").unwrap();
        let end = start + "pomme rouge".len();
        let selector = create_selector(text, start, end).unwrap();
        assert_eq!((selector.start, selector.end), (9, 20));
        assert_eq!(selector.prefix, "Voici ta ");
        assert_eq!(selector.suffix, " 🍎 fin");
        assert_eq!(find_quote(text, "ta pomme", 0), Some((7, 15)));
        assert_eq!(find_quote("a b a b", "a b", 1), Some((4, 7)));
    }
}
