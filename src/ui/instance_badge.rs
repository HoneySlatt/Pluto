//! How a provider instance presents itself: its label, its accent color and
//! whether its icon carries the account badge. Ported from
//! `packages/client-runtime/src/state/providerInstanceDisplay.ts` and the badge
//! of `components/chat/ProviderInstanceIcon.tsx`.

use eframe::egui::{Color32, CornerRadius, Pos2, Rect, Stroke, StrokeKind, Ui};
use serde_json::Value;

use crate::app::App;
use crate::theme::{self, Weight, font};
use crate::ui::accent_picker;
use crate::ui::widgets;

/// What a badged instance icon shows.
pub(crate) struct Badge {
    pub(crate) initials: String,
    pub(crate) accent: Option<Color32>,
}

/// `PROVIDER_DISPLAY_NAMES`.
fn kind_label(driver: &str) -> Option<&'static str> {
    Some(match driver {
        "antigravity" => "Antigravity",
        "codex" => "Codex",
        "claudeAgent" => "Claude",
        "cursor" => "Cursor",
        "grok" => "Grok",
        "kimiCode" => "Kimi Code",
        "mistralVibe" => "Mistral Vibe",
        "zcode" => "Z Code",
        "opencode" => "OpenCode",
        _ => return None,
    })
}

/// `humanizeSlug`: `codex_personal` → "Codex Personal", `myInstance` → "My Instance".
fn humanize_slug(slug: &str) -> String {
    let mut spaced = String::new();
    let mut previous: Option<char> = None;
    for c in slug.chars() {
        if previous.is_some_and(|p| p.is_ascii_lowercase()) && c.is_ascii_uppercase() {
            spaced.push(' ');
        }
        spaced.push(if c == '_' || c == '-' { ' ' } else { c });
        previous = Some(c);
    }
    let collapsed = spaced
        .split(' ')
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>();
    collapsed
        .iter()
        .map(|word| {
            let mut chars = word.chars();
            chars
                .next()
                .map(|first| first.to_uppercase().chain(chars).collect::<String>())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// `resolveProviderInstanceDisplayName` for a server provider snapshot.
pub(crate) fn display_name(snapshot: &Value) -> String {
    let driver = snapshot["driver"].as_str().unwrap_or_default();
    let instance_id = snapshot["instanceId"].as_str().unwrap_or_default();
    let name = snapshot["displayName"]
        .as_str()
        .map(str::trim)
        .unwrap_or_default();
    let kind = kind_label(driver)
        .map(str::to_owned)
        .unwrap_or_else(|| humanize_slug(driver));
    if !name.is_empty() && name != kind {
        return name.to_owned();
    }
    // `defaultInstanceIdForDriver` is the driver itself.
    if instance_id != driver {
        let humanized = humanize_slug(instance_id);
        if !humanized.is_empty() {
            return humanized;
        }
    }
    if name.is_empty() {
        kind
    } else {
        name.to_owned()
    }
}

/// `providerInstanceInitials`: two letters of one word, or the first
/// letters of the first two words.
pub(crate) fn initials(label: &str) -> String {
    let cleaned = label.replace(['_', '-'], " ");
    let words: Vec<&str> = cleaned.split_whitespace().collect();
    match words.as_slice() {
        [] => String::new(),
        [word] => word.chars().take(2).collect::<String>().to_uppercase(),
        _ => words
            .iter()
            .take(2)
            .filter_map(|w| w.chars().next())
            .collect::<String>()
            .to_uppercase(),
    }
}

impl App {
    /// `shouldShowInstanceBadge`: an accent color, or several instances
    /// sharing the driver. None when the icon stays bare.
    pub(crate) fn instance_badge(&self, instance_id: &str) -> Option<Badge> {
        let providers = self.providers();
        let snapshot = providers.iter().find(|p| p["instanceId"] == instance_id)?;
        let accent = snapshot["accentColor"]
            .as_str()
            .and_then(accent_picker::parse);
        let shared = providers
            .iter()
            .filter(|p| p["driver"] == snapshot["driver"])
            .count()
            > 1;
        (accent.is_some() || shared).then(|| Badge {
            initials: initials(&display_name(snapshot)),
            accent,
        })
    }
}

/// The badge over an instance icon: its bottom-right corner sits `outset`
/// past the icon's, 12px tall (`h-3 min-w-3 px-0.5 text-5xs`), on the accent
/// with white initials or on the card with muted ones, ringed with `ring`.
pub(crate) fn paint(ui: &Ui, icon: Rect, outset: f32, badge: &Badge, ring: Color32, fade: f32) {
    let badge_font = font(7.0, Weight::SemiBold);
    let text_width = widgets::text_width(ui, &badge.initials, badge_font.clone());
    let width = (text_width + 6.0).max(12.0);
    let corner = icon.max + eframe::egui::Vec2::splat(outset);
    let rect = Rect::from_min_max(Pos2::new(corner.x - width, corner.y - 12.0), corner);
    let (fill, text) = match badge.accent {
        Some(accent) => (accent, Color32::WHITE),
        None => (theme::card(), theme::muted_foreground()),
    };
    // The background runs under the border, which matters for translucent rings.
    ui.painter()
        .rect_filled(rect, CornerRadius::same(6), theme::alpha(fill, fade));
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(6),
        Stroke::new(1.0, theme::alpha(ring, fade)),
        StrokeKind::Inside,
    );
    widgets::text(
        ui,
        Pos2::new(rect.center().x - text_width / 2.0, rect.center().y - 3.5),
        &badge.initials,
        badge_font,
        theme::alpha(text, fade),
        7.0,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn names_and_initials_follow_upstream() {
        assert_eq!(initials("Codex"), "CO");
        assert_eq!(initials("codex_personal work"), "CP");
        assert_eq!(initials("  "), "");
        assert_eq!(humanize_slug("codex_personal"), "Codex Personal");
        assert_eq!(humanize_slug("myCustomInstance"), "My Custom Instance");
        let default = json!({"instanceId": "codex", "driver": "codex", "displayName": "Codex"});
        assert_eq!(display_name(&default), "Codex");
        let second = json!({"instanceId": "codex_work", "driver": "codex", "displayName": "Codex"});
        assert_eq!(display_name(&second), "Codex Work");
        let named =
            json!({"instanceId": "codex_work", "driver": "codex", "displayName": "Day job"});
        assert_eq!(display_name(&named), "Day job");
    }
}
