//! Project badges (`ProjectFavicon.tsx`, `ProjectMonogram.tsx`,
//! `projectIdentity.ts`, the desktop's `project_icon.rs`): the chosen Lucide
//! icon in its color, emoji or monogram; without a chosen icon, the
//! project's favicon, else the title's two-letter monogram on its derived
//! color.

use base64::Engine;
use dioxus::prelude::*;
use pluto_core::model::Project;

use crate::session::Ctx;

/// Tailwind's 400 shades by name (`PROJECT_COLORS`).
const COLORS: [(&str, &str); 18] = [
    ("gray", "#99a1af"),
    ("red", "#ff6467"),
    ("orange", "#ff8904"),
    ("amber", "#ffb900"),
    ("yellow", "#fdc700"),
    ("lime", "#9ae600"),
    ("green", "#05df72"),
    ("emerald", "#00d492"),
    ("teal", "#00d5be"),
    ("cyan", "#00d3f2"),
    ("sky", "#00bcff"),
    ("blue", "#51a2ff"),
    ("indigo", "#7c86ff"),
    ("violet", "#a684ff"),
    ("purple", "#c27aff"),
    ("fuchsia", "#ed6aff"),
    ("pink", "#fb64b6"),
    ("rose", "#ff637e"),
];

fn words(name: &str) -> Vec<Vec<char>> {
    let mut words = Vec::new();
    let mut current = Vec::new();
    for c in name.trim().chars() {
        if c.is_alphanumeric() {
            current.push(c);
        } else if !current.is_empty() {
            words.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

/// `monogram`: the first letter, then a digit of the first word, the last
/// word's initial or the first word's last letter.
pub fn monogram(name: &str) -> String {
    let words = words(name);
    let Some(first_word) = words.first() else {
        return "PR".into();
    };
    let first = first_word[0];
    let second = first_word[1..]
        .iter()
        .copied()
        .find(|c| c.is_numeric())
        .or_else(|| {
            if words.len() > 1 {
                words.last().and_then(|w| w.first().copied())
            } else {
                first_word.last().copied()
            }
        })
        .unwrap_or(first);
    format!("{first}{second}")
        .to_uppercase()
        .chars()
        .take(2)
        .collect()
}

fn color_index(name: &str) -> usize {
    let seed = name.trim().to_lowercase();
    let seed = if seed.is_empty() {
        "project".into()
    } else {
        seed
    };
    let mut index = 0usize;
    for c in seed.chars() {
        index = (index * 31 + c as usize) % COLORS.len();
    }
    index
}

/// The badge's text and color.
pub fn identity(project: &Project) -> (String, &'static str) {
    let icon = project.project_icon.as_ref();
    let named = |value: &serde_json::Value| {
        value["color"]
            .as_str()
            .and_then(|name| COLORS.iter().find(|(n, _)| *n == name))
            .map(|(_, hex)| *hex)
    };
    let derived = COLORS[color_index(&project.title)].1;
    match icon {
        Some(icon) if icon["kind"] == "monogram" || icon["monogramText"].is_string() => {
            let text = icon["text"]
                .as_str()
                .or(icon["monogramText"].as_str())
                .map(str::to_owned)
                .unwrap_or_else(|| monogram(&project.title));
            (text, named(icon).unwrap_or(COLORS[11].1))
        }
        Some(icon) if icon["kind"] == "emoji" => (
            icon["emoji"].as_str().unwrap_or_default().to_owned(),
            named(icon).unwrap_or(derived),
        ),
        _ => (monogram(&project.title), derived),
    }
}

/// A favicon's bytes as a data URL, by their format; `None` for anything
/// that is not an image.
pub fn data_url(bytes: &[u8]) -> Option<String> {
    let head = &bytes[..bytes.len().min(512)];
    let text = String::from_utf8_lossy(head).to_lowercase();
    let mime = if bytes.starts_with(b"\x89PNG") {
        "image/png"
    } else if bytes.starts_with(&[0xff, 0xd8]) {
        "image/jpeg"
    } else if bytes.starts_with(b"GIF8") {
        "image/gif"
    } else if bytes.len() > 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        "image/webp"
    } else if bytes.starts_with(&[0, 0, 1, 0]) {
        "image/x-icon"
    } else if text.contains("<svg") {
        "image/svg+xml"
    } else {
        return None;
    };
    Some(format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

#[component]
pub fn Badge(project: Project, #[props(default)] class: &'static str) -> Element {
    let ctx = use_context::<Ctx>();
    let icon = project.project_icon.clone();
    let kind = icon
        .as_ref()
        .and_then(|i| i["kind"].as_str().map(str::to_owned));
    let monogram_icon = icon
        .as_ref()
        .is_some_and(|i| i["kind"] == "monogram" || i["monogramText"].is_string());
    let asked = project.clone();
    use_effect(move || {
        ctx.favicon(&asked);
    });
    let favicon = ctx
        .session
        .read()
        .assets
        .get(&format!("favicon:{}", project.id))
        .cloned()
        .flatten();
    let named = |value: &serde_json::Value| {
        value["color"]
            .as_str()
            .and_then(|name| COLORS.iter().find(|(n, _)| *n == name))
            .map(|(_, hex)| *hex)
    };
    match (kind.as_deref(), icon) {
        (Some("lucide"), Some(icon)) if !monogram_icon => {
            let color = named(&icon).unwrap_or(COLORS[0].1);
            let body = crate::lucide::body(icon["name"].as_str().unwrap_or_default());
            rsx! {
                span { class: "badge {class} glyph", style: "color:{color}",
                    svg { class: "i", view_box: "0 0 24 24", "aria-hidden": "true", dangerous_inner_html: body }
                }
            }
        }
        (Some("emoji"), Some(icon)) => rsx! {
            span { class: "badge {class} glyph emoji", "{icon[\"emoji\"].as_str().unwrap_or_default()}" }
        },
        (None, None) if favicon.is_some() => rsx! {
            span { class: "badge {class} glyph",
                img { src: favicon.unwrap_or_default(), alt: "" }
            }
        },
        _ => {
            let (text, color) = identity(&project);
            rsx! {
                span { class: "badge {class}", style: "background:{color}", "{text}" }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monograms_follow_the_desktop() {
        assert_eq!(monogram("pluto-server"), "PS");
        assert_eq!(monogram("NixOS"), "NS");
        assert_eq!(monogram("Project 2"), "P2");
        assert_eq!(monogram(""), "PR");
        assert!(
            data_url(b"\x89PNG\r\n")
                .unwrap()
                .starts_with("data:image/png;base64,")
        );
        assert!(
            data_url(b"<svg xmlns='x'/>")
                .unwrap()
                .starts_with("data:image/svg+xml")
        );
        assert!(data_url(b"not found").is_none());
    }
}
