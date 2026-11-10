//! `MessagesTimeline.tsx`'s `ToolActivityIconView` and shared `favicon.ts`:
//! website favicons, theme-aware integration logos and signed native-app
//! icons. Keep the normal tool glyph while the image loads, if it fails, or
//! when it may not load by itself (ADR-0022).

use eframe::egui::{self, Rect, Ui, Vec2};
use serde_json::{Value, json};

use crate::{
    app::App,
    model::TurnItem,
    theme,
    ui::attachments::{self, Fetch},
};

fn icon(item: &TurnItem) -> &Value {
    if item.get("toolIcon").is_object() {
        item.get("toolIcon")
    } else {
        &item.get("toolSource")["icon"]
    }
}

fn image_url(value: &Value) -> Option<String> {
    let raw = value.as_str().filter(|s| s.len() <= 4096)?;
    let url = url::Url::parse(raw).ok()?;
    matches!(url.scheme(), "http" | "https" | "data").then(|| url.into())
}

fn direct_url(icon: &Value, dark: bool) -> Option<String> {
    match icon["_tag"].as_str()? {
        "themed-logo" => {
            let chosen = if dark && icon["logoUrlDark"].is_string() {
                &icon["logoUrlDark"]
            } else {
                &icon["logoUrl"]
            };
            image_url(chosen)
        }
        "website" => {
            let page = icon["pageUrl"]
                .as_str()
                .filter(|s| s.len() <= 4096)
                .and_then(|s| url::Url::parse(s).ok())
                .filter(|u| matches!(u.scheme(), "http" | "https"));
            let themed = page
                .as_ref()
                .filter(|p| p.host_str() == Some("github.com"))
                .map(|_| {
                    format!(
                        "https://github.githubassets.com/favicons/favicon{}.svg",
                        if dark { "-dark" } else { "" }
                    )
                });
            let fallback = || page.as_ref()?.join("/favicon.ico").ok().map(Into::into);
            if dark {
                image_url(&icon["faviconUrlDark"])
                    .or(themed)
                    .or_else(|| image_url(&icon["faviconUrl"]))
                    .or_else(fallback)
            } else {
                image_url(&icon["faviconUrl"]).or(themed).or_else(fallback)
            }
        }
        _ => None,
    }
}

impl App {
    /// True once the supplied icon is painted. Requests reuse the image
    /// loader's byte/dimension budgets and the server's signed-URL path.
    pub(crate) fn tool_image_icon(
        &mut self,
        ui: &Ui,
        rect: Rect,
        item: &TurnItem,
        muted: bool,
    ) -> bool {
        let icon = icon(item);
        let dark = theme::background().r() < 128;
        let native = icon["_tag"] == "native-app";
        let (key, direct) = if native && icon["app"].is_object() {
            // Native app identities belong to the connected environment.
            (
                format!("tool-app:{}:{}", self.connect.server, icon["app"]),
                None,
            )
        } else if let Some(url) = direct_url(icon, dark) {
            (format!("tool-icon:{url}"), Some(url))
        } else {
            return false;
        };
        match attachments::media_state(&key) {
            Some(attachments::MediaState::Ready(image)) => {
                let size = if native { 20.0 } else { 16.0 };
                let rect = Rect::from_center_size(rect.center(), Vec2::splat(size));
                ui.painter().rect_filled(rect, 2.0, theme::background());
                let brightness = if !dark && muted { 0.6 } else { 1.0 };
                let tint = theme::alpha(
                    egui::Color32::from_gray((255.0 * brightness) as u8),
                    if muted { 0.7 } else { 1.0 },
                );
                attachments::paint_contain(ui, rect, &key, &image, tint);
                true
            }
            Some(_) => false,
            None => {
                if let Some(url) = direct {
                    // An icon outside the answer's sources keeps the tool's
                    // glyph (ADR-0022).
                    let sources = self.answer_sources(item.run_id.as_deref());
                    if attachments::loads_by_itself(&url, &sources) {
                        attachments::load_direct(&key, &url, Fetch::BySelf(sources), ui.ctx());
                    }
                } else {
                    attachments::mark_loading(&key);
                    self.send_request(
                        format!("attachment:{key}"),
                        "assets.createUrl",
                        json!({"resource": {"_tag": "native-app-icon", "app": icon["app"]}}),
                    );
                }
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn website_variants_follow_upstream_precedence() {
        let icon = json!({"_tag":"website", "pageUrl":"https://github.com/example/repo", "faviconUrl":"https://example.test/light.png"});
        assert_eq!(
            direct_url(&icon, false).as_deref(),
            Some("https://example.test/light.png")
        );
        assert_eq!(
            direct_url(&icon, true).as_deref(),
            Some("https://github.githubassets.com/favicons/favicon-dark.svg")
        );
        let page = json!({"_tag":"website", "pageUrl":"https://example.test/path?q=1", "faviconUrlDark":"file:///private/icon.png"});
        assert_eq!(
            direct_url(&page, true).as_deref(),
            Some("https://example.test/favicon.ico")
        );
        assert!(
            direct_url(
                &json!({"_tag":"website", "pageUrl":"file:///tmp/icon"}),
                true
            )
            .is_none()
        );
    }

    #[test]
    fn themed_logos_and_source_icons_are_preserved() {
        let source = json!({"_tag":"themed-logo", "logoUrl":"https://example.test/light.svg", "logoUrlDark":"https://example.test/dark.svg"});
        let mut item = TurnItem::default();
        item.data
            .insert("toolSource".into(), json!({"icon":source}));
        assert_eq!(
            direct_url(icon(&item), true).as_deref(),
            Some("https://example.test/dark.svg")
        );
        item.data.insert(
            "toolIcon".into(),
            json!({"_tag":"native-app", "app":{"_tag":"app-id", "appId":"org.example.App"}}),
        );
        assert_eq!(icon(&item)["_tag"], "native-app");
        assert!(direct_url(icon(&item), false).is_none());
    }
}
