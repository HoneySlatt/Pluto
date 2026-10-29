//! Project badges (`ProjectFavicon.tsx`, `ProjectMonogram.tsx`,
//! `projectIdentity.ts`, the desktop's `project_icon.rs`): the chosen Lucide
//! icon in its color, emoji or monogram; without a chosen icon, the
//! project's favicon, else the title's two-letter monogram on its derived
//! color.

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

/// A project's badge as Compose draws it.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct Badge {
    pub project_id: String,
    pub title: String,
    /// The monogram, or the emoji.
    pub text: String,
    /// `0xRRGGBB`: the monogram's tile, or the Lucide icon's stroke.
    pub color: u32,
    /// A chosen Lucide icon's name.
    pub lucide: Option<String>,
    pub emoji: bool,
    /// No icon was chosen: the favicon shows once the server sent it.
    pub favicon: bool,
}

fn rgb(hex: &str) -> u32 {
    u32::from_str_radix(hex.trim_start_matches('#'), 16).unwrap_or(0x99a1af)
}

pub fn badge(project: &Project) -> Badge {
    let icon = project.project_icon.as_ref();
    let kind = icon.and_then(|i| i["kind"].as_str());
    let monogram_icon =
        icon.is_some_and(|i| i["kind"] == "monogram" || i["monogramText"].is_string());
    let named = |value: &serde_json::Value| {
        value["color"]
            .as_str()
            .and_then(|name| COLORS.iter().find(|(n, _)| *n == name))
            .map(|(_, hex)| *hex)
    };
    let (text, color) = identity(project);
    let mut badge = Badge {
        project_id: project.id.clone(),
        title: project.title.clone(),
        text,
        color: rgb(color),
        lucide: None,
        emoji: false,
        favicon: icon.is_none(),
    };
    match (kind, icon) {
        (Some("lucide"), Some(icon)) if !monogram_icon => {
            badge.lucide = Some(icon["name"].as_str().unwrap_or_default().to_owned());
            badge.color = rgb(named(icon).unwrap_or(COLORS[0].1));
        }
        (Some("emoji"), Some(icon)) => {
            badge.text = icon["emoji"].as_str().unwrap_or_default().to_owned();
            badge.emoji = true;
        }
        _ => {}
    }
    badge
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
