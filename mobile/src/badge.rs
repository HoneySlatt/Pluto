//! Project badges (`ProjectMonogram.tsx`, `projectIdentity.ts`, the
//! desktop's `project_icon.rs`): a chosen monogram or emoji, else the
//! title's two-letter monogram on its derived color.

use dioxus::prelude::*;
use pluto_core::model::Project;

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

#[component]
pub fn Badge(project: Project, #[props(default)] class: &'static str) -> Element {
    let (text, color) = identity(&project);
    rsx! {
        span { class: "badge {class}", style: "background:{color}", "{text}" }
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
    }
}
