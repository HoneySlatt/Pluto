//! The whole Lucide catalog for project icons (`lucide-react/dynamic`'s
//! `iconNames` and `DynamicIcon`), from `assets/lucide/catalog.txt`
//! (`scripts/gen-lucide-catalog.mjs`). Icons are built as white-stroked SVGs on
//! first use and tinted when painted.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use eframe::egui::{self, Color32, Rect, Ui};

struct Catalog {
    /// Every name, aliases included, in `iconNames` order.
    names: Vec<&'static str>,
    /// Name to its icon's SVG elements.
    bodies: HashMap<&'static str, &'static str>,
}

fn catalog() -> &'static Catalog {
    static CATALOG: OnceLock<Catalog> = OnceLock::new();
    CATALOG.get_or_init(|| {
        let text: &'static str = include_str!("../../assets/lucide/catalog.txt");
        let (index, icons) = text.split_once("\n\n").unwrap_or((text, ""));
        let files: HashMap<&str, &str> = icons
            .lines()
            .filter_map(|line| line.split_once('\t'))
            .collect();
        let mut names = Vec::new();
        let mut bodies = HashMap::new();
        for (name, file) in index.lines().filter_map(|line| line.split_once('\t')) {
            if let Some(body) = files.get(file) {
                names.push(name);
                bodies.insert(name, *body);
            }
        }
        Catalog { names, bodies }
    })
}

/// `POPULAR_PROJECT_ICONS`.
const POPULAR: [&str; 24] = [
    "folder-code",
    "code-2",
    "terminal",
    "globe-2",
    "server",
    "database",
    "bot",
    "sparkles",
    "smartphone",
    "monitor",
    "cloud-cog",
    "package",
    "book-open",
    "flask-conical",
    "shield-check",
    "rocket",
    "gamepad-2",
    "music",
    "image",
    "shopping-bag",
    "git-branch",
    "workflow",
    "wrench",
    "layers-3",
];

/// `filterProjectIconNames`: the popular icons, or up to 60 names containing
/// the query (spaces read as dashes).
pub fn filter(query: &str) -> Vec<&'static str> {
    let normalized = query
        .trim()
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-");
    if normalized.is_empty() {
        return POPULAR.to_vec();
    }
    catalog()
        .names
        .iter()
        .copied()
        .filter(|name| name.contains(&normalized))
        .take(60)
        .collect()
}

pub fn exists(name: &str) -> bool {
    catalog().bodies.contains_key(name)
}

/// The icon as an image source (`bytes://lucide/<name>.svg`), built once.
fn source(name: &str) -> Option<egui::ImageSource<'static>> {
    static BUILT: OnceLock<Mutex<HashMap<String, Arc<[u8]>>>> = OnceLock::new();
    let body = catalog().bodies.get(name)?;
    let mut built = BUILT.get_or_init(Default::default).lock().ok()?;
    let bytes = built
        .entry(name.to_owned())
        .or_insert_with(|| {
            format!(
                r##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="#ffffff" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">{body}</svg>"##
            )
            .into_bytes()
            .into()
        })
        .clone();
    Some(egui::ImageSource::Bytes {
        uri: format!("bytes://lucide/{name}.svg").into(),
        bytes: egui::load::Bytes::Shared(bytes),
    })
}

/// Paints the icon (or `folder-code`, `DynamicProjectIconFallback`) in `rect`.
pub fn paint(ui: &Ui, rect: Rect, name: &str, tint: Color32) {
    let Some(source) = source(name).or_else(|| source("folder-code")) else {
        return;
    };
    egui::Image::new(source)
        .fit_to_exact_size(rect.size())
        .tint(tint)
        .paint_at(ui, rect);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_catalog_filters_like_upstream() {
        assert!(exists("folder-code") && exists("code-2") && exists("layers-3"));
        assert_eq!(filter("").len(), 24);
        assert!(POPULAR.iter().all(|name| exists(name)));
        let found = filter("git branch");
        assert!(found.contains(&"git-branch") && found.iter().all(|n| n.contains("git-branch")));
        assert_eq!(filter("a").len(), 60);
    }
}
