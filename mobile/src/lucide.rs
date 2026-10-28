//! The whole Lucide catalog for project icons, from the desktop's
//! `assets/lucide/catalog.txt` (`lucide-react/dynamic`'s `iconNames`, built by
//! `scripts/gen-lucide-catalog.mjs`): an index of names (aliases included) to
//! icon files, a blank line, then each file's SVG elements.

use std::collections::HashMap;
use std::sync::OnceLock;

fn catalog() -> &'static HashMap<&'static str, &'static str> {
    static CATALOG: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    CATALOG.get_or_init(|| {
        let text: &'static str = include_str!("../../assets/lucide/catalog.txt");
        let (index, icons) = text.split_once("\n\n").unwrap_or((text, ""));
        let files: HashMap<&str, &str> = icons
            .lines()
            .filter_map(|line| line.split_once('\t'))
            .collect();
        index
            .lines()
            .filter_map(|line| line.split_once('\t'))
            .filter_map(|(name, file)| Some((name, *files.get(file)?)))
            .collect()
    })
}

/// The icon's SVG elements, or `folder-code`'s (`DynamicProjectIconFallback`).
pub fn body(name: &str) -> &'static str {
    let catalog = catalog();
    catalog
        .get(name)
        .or_else(|| catalog.get("folder-code"))
        .copied()
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icons_and_aliases_resolve() {
        assert!(body("git-branch").contains("<"));
        assert_eq!(body("alarm-check"), body("alarm-clock-check"));
        assert_eq!(body("no-such-icon"), body("folder-code"));
    }
}
