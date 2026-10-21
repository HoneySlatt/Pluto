//! Bitbucket credentials for the environment, in the Bitbucket row's
//! details (`components/settings/BitbucketCredentialsSettings.tsx`).
//!
//! An access token, or an Atlassian account email with an API token, never
//! both. Tokens are write-only: the server keeps them in its secret store and
//! only returns a redacted marker, which is resent unchanged to keep a token.
//! A typed token is sent straight to the server (not merged into the local
//! settings) and the draft is cleared.

use eframe::egui::{
    self, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2, text::LayoutJob,
};
use serde_json::{Value, json};

use crate::app::App;
use crate::theme::{self, Weight, font};
use crate::ui::widgets;

#[derive(Clone, Copy, PartialEq)]
enum Method {
    AccessToken,
    ApiToken,
}

/// `METHODS`: label, description, link, link label.
fn method_info(method: Method) -> (&'static str, &'static str, &'static str, &'static str) {
    match method {
        Method::AccessToken => (
            "Access token",
            "Scoped to one repository, project, or workspace. Create it in that item's Bitbucket settings.",
            "https://support.atlassian.com/bitbucket-cloud/docs/access-tokens/",
            "Learn more",
        ),
        Method::ApiToken => (
            "API token",
            "Uses your Atlassian account, so it reaches every repository you can. Give it read and write access to repositories and pull requests, and read:user:bitbucket.",
            "https://id.atlassian.com/manage-profile/security/api-tokens",
            "Create an API token",
        ),
    }
}

/// The panel's drafts; tokens are cleared once sent.
#[derive(Default)]
pub(crate) struct BitbucketDraft {
    method: Option<Method>,
    access_token: String,
    email: Option<String>,
    api_token: String,
}

/// `max-w-2xl`.
const TEXT_WIDTH: f32 = 672.0;

fn text(value: &Value) -> &str {
    value.as_str().unwrap_or_default()
}

/// `savedMethod`.
fn saved_method(saved: &Value) -> Option<Method> {
    if !text(&saved["accessToken"]).is_empty() {
        Some(Method::AccessToken)
    } else if !text(&saved["email"]).is_empty() && !text(&saved["apiToken"]).is_empty() {
        Some(Method::ApiToken)
    } else {
        None
    }
}

/// The description with its inline link, wrapped at `max-w-2xl` (the link's
/// icon is painted after it).
fn description(ui: &Ui, method: Method) -> std::sync::Arc<egui::Galley> {
    let (_, description, _, link) = method_info(method);
    let mut job = LayoutJob::default();
    let format = |font_id: egui::FontId, color: Color32| egui::TextFormat {
        font_id,
        color,
        line_height: Some(19.5),
        ..Default::default()
    };
    job.append(
        &format!("{description} "),
        0.0,
        format(font(12.0, Weight::Regular), theme::muted_foreground()),
    );
    job.append(
        link,
        0.0,
        format(font(12.0, Weight::Medium), theme::foreground()),
    );
    job.wrap.max_width = TEXT_WIDTH;
    ui.fonts_mut(|f| f.layout_job(job))
}

impl App {
    /// The form's height (inside the details' `pt-1 pb-4`).
    pub(crate) fn bitbucket_height(&self, ui: &Ui) -> f32 {
        let saved = &self.state.config["settings"]["bitbucket"];
        let method = self
            .bitbucket
            .method
            .or(saved_method(saved))
            .unwrap_or(Method::AccessToken);
        let fields = if method == Method::AccessToken {
            50.0
        } else {
            50.0 + 16.0 + 50.0
        };
        28.0 + 16.0 + description(ui, method).size().y + 16.0 + fields + 16.0 + 24.0
    }

    /// The form from `top`, between `left` and `right`.
    pub(crate) fn bitbucket_form(&mut self, ui: &mut Ui, left: f32, right: f32, top: f32) {
        let saved = self.state.config["settings"]["bitbucket"].clone();
        let current = saved_method(&saved);
        let method = self
            .bitbucket
            .method
            .or(current)
            .unwrap_or(Method::AccessToken);
        let method_is_saved = current == Some(method);
        let saved_email = text(&saved["email"]).to_owned();
        let saved_api_token = text(&saved["apiToken"]).to_owned();

        // The segmented method toggle.
        let labels = [Method::AccessToken, Method::ApiToken].map(|m| (m, method_info(m).0));
        let widths: Vec<f32> = labels
            .iter()
            .map(|(_, label)| widgets::text_width(ui, label, font(14.0, Weight::Medium)) + 24.0)
            .collect();
        let group = Rect::from_min_size(
            Pos2::new(left, top),
            Vec2::new(widths.iter().sum::<f32>() + 2.0 + 4.0, 28.0),
        );
        ui.painter()
            .rect_filled(group, CornerRadius::same(10), theme::overlay(0.032));
        let mut x = group.left() + 2.0;
        for ((value, label), width) in labels.iter().zip(&widths) {
            let rect = Rect::from_min_size(Pos2::new(x, top + 2.0), Vec2::new(*width, 24.0));
            let response = ui.interact(
                rect,
                ui.id().with(("bitbucket-method", label)),
                Sense::click(),
            );
            let active = *value == method;
            if active {
                ui.painter()
                    .rect_filled(rect, CornerRadius::same(8), theme::overlay(0.0576));
            }
            widgets::text_in(
                ui,
                Rect::from_min_max(Pos2::new(rect.left() + 12.0, rect.top()), rect.max),
                label,
                font(14.0, Weight::Medium),
                if active || response.hovered() {
                    theme::foreground()
                } else {
                    theme::muted_foreground()
                },
            );
            if response.clicked() {
                self.bitbucket.method = Some(*value);
            }
            x = rect.right() + 2.0;
        }

        // The description and its link.
        let mut y = top + 28.0 + 16.0;
        let galley = description(ui, method);
        let link_end = galley.rows.last().map_or(Pos2::new(left, y), |row| {
            Pos2::new(left + row.rect().right(), y + row.rect().top())
        });
        let (_, _, url, link_label) = method_info(method);
        let link_width = widgets::text_width(ui, link_label, font(12.0, Weight::Medium));
        let link_rect = Rect::from_min_max(
            Pos2::new(link_end.x - link_width, link_end.y),
            Pos2::new(link_end.x + 16.0, link_end.y + 19.5),
        );
        let height = galley.size().y;
        ui.painter()
            .galley(Pos2::new(left, y), galley, theme::muted_foreground());
        theme::paint_icon(
            ui,
            "external-link",
            Rect::from_min_size(
                Pos2::new(link_end.x + 4.0, link_end.y + 3.75),
                Vec2::splat(12.0),
            ),
            12.0,
            theme::foreground(),
        );
        let response = ui.interact(link_rect, ui.id().with("bitbucket-link"), Sense::click());
        if response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if response.clicked() {
            widgets::open_external(url);
        }
        y += height + 16.0;

        // The fields.
        let width = right - left;
        let placeholder = if method_is_saved {
            "Stored secret, enter a new value to replace"
        } else {
            "Not set"
        };
        if method == Method::AccessToken {
            label(ui, Pos2::new(left, y), "Access token");
            field(
                ui,
                Rect::from_min_size(Pos2::new(left, y + 22.0), Vec2::new(width, 28.0)),
                "bitbucket-access-token",
                &mut self.bitbucket.access_token,
                placeholder,
                true,
            );
            y += 50.0 + 16.0;
        } else {
            label(ui, Pos2::new(left, y), "Atlassian account email");
            let mut email = self
                .bitbucket
                .email
                .clone()
                .unwrap_or_else(|| saved_email.clone());
            if field(
                ui,
                Rect::from_min_size(Pos2::new(left, y + 22.0), Vec2::new(width, 28.0)),
                "bitbucket-email",
                &mut email,
                "you@example.com",
                false,
            ) {
                self.bitbucket.email = Some(email);
            }
            y += 50.0 + 16.0;
            label(ui, Pos2::new(left, y), "API token");
            field(
                ui,
                Rect::from_min_size(Pos2::new(left, y + 22.0), Vec2::new(width, 28.0)),
                "bitbucket-api-token",
                &mut self.bitbucket.api_token,
                placeholder,
                true,
            );
            y += 50.0 + 16.0;
        }

        // The note, then Remove and Save.
        let note = match current {
            None => Some("Without a saved token, the server falls back to its T3CODE_BITBUCKET_* environment variables.".to_owned()),
            Some(_) if method_is_saved => None,
            Some(saved) => Some(format!("Saving replaces your {}.", method_info(saved).0.to_lowercase())),
        };
        if let Some(note) = note {
            widgets::text(
                ui,
                Pos2::new(left, y + 4.0),
                &note,
                font(12.0, Weight::Regular),
                theme::muted_foreground(),
                16.0,
            );
        }
        // `patch` and `canSave`: saving one method clears the other.
        let email = self
            .bitbucket
            .email
            .clone()
            .unwrap_or_else(|| saved_email.clone())
            .trim()
            .to_owned();
        let access = self.bitbucket.access_token.trim().to_owned();
        let api = self.bitbucket.api_token.trim().to_owned();
        let patch = match method {
            Method::AccessToken => (!access.is_empty()).then(|| json!({"accessToken": access, "email": "", "apiToken": ""})),
            Method::ApiToken => (!email.is_empty() && (!api.is_empty() || !saved_api_token.is_empty())).then(|| {
                json!({"accessToken": "", "email": email, "apiToken": if api.is_empty() { saved_api_token.clone() } else { api.clone() }})
            }),
        };
        let can_save = patch.is_some()
            && (method == Method::AccessToken
                || !method_is_saved
                || !api.is_empty()
                || email != saved_email);
        let save_width =
            (widgets::text_width(ui, "Save", font(12.0, Weight::Medium)) + 16.0).ceil();
        let save = Rect::from_min_size(
            Pos2::new(right - save_width, y),
            Vec2::new(save_width, 24.0),
        );
        let mut send: Option<Value> = None;
        if xs_button(ui, save, "Save", true, can_save) {
            send = patch;
        }
        if current.is_some() {
            let remove_width =
                (widgets::text_width(ui, "Remove", font(12.0, Weight::Medium)) + 16.0).ceil();
            let remove = Rect::from_min_size(
                Pos2::new(save.left() - 8.0 - remove_width, y),
                Vec2::new(remove_width, 24.0),
            );
            if xs_button(ui, remove, "Remove", false, true) {
                send = Some(json!({"accessToken": "", "email": "", "apiToken": ""}));
            }
        }
        if let Some(next) = send {
            self.send_request(
                "settings:bitbucket".into(),
                "server.updateSettings",
                json!({"patch": {"bitbucket": next}}),
            );
            self.bitbucket.access_token.clear();
            self.bitbucket.api_token.clear();
            self.bitbucket.email = None;
            // `onSaved`: rescan so the row's status follows.
            self.request_discovery();
        }
    }
}

/// `Label`: 14px medium, `leading-none`.
fn label(ui: &Ui, pos: Pos2, text: &str) {
    widgets::text(
        ui,
        pos,
        text,
        font(14.0, Weight::Medium),
        theme::foreground(),
        16.0,
    );
}

/// `Input size="sm"`; a token field is masked. Returns whether it changed.
fn field(
    ui: &mut Ui,
    rect: Rect,
    id: &str,
    text: &mut String,
    placeholder: &str,
    password: bool,
) -> bool {
    ui.painter().rect(
        rect,
        CornerRadius::same(10),
        theme::control_surface(),
        Stroke::new(1.0, theme::input()),
        StrokeKind::Inside,
    );
    let inner = Rect::from_min_max(
        Pos2::new(rect.left() + 10.0, rect.top() + 1.0),
        Pos2::new(rect.right() - 10.0, rect.bottom() - 1.0),
    );
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner));
    let response = child.add(
        egui::TextEdit::singleline(text)
            .id(egui::Id::new(id))
            .password(password)
            .frame(egui::Frame::NONE)
            .margin(egui::Margin::ZERO)
            .font(font(14.0, Weight::Regular))
            .text_color(theme::foreground())
            .vertical_align(egui::Align::Center)
            .desired_width(inner.width())
            .min_size(Vec2::new(0.0, inner.height())),
    );
    // Painted like the settings fields: egui's hint text renders lighter.
    if text.is_empty() {
        widgets::text_in(
            ui,
            inner,
            placeholder,
            font(14.0, Weight::Regular),
            theme::muted_foreground(),
        );
    }
    response.changed()
}

/// `Button size="xs"`: primary (Save) or outline (Remove); dimmed when disabled.
fn xs_button(ui: &Ui, rect: Rect, label: &str, primary: bool, enabled: bool) -> bool {
    let response = ui.interact(
        rect,
        ui.id().with(("bitbucket-button", label)),
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    let alpha = if enabled { 1.0 } else { 0.64 };
    let hovered = enabled && response.hovered();
    let color = if primary {
        let fill = if hovered {
            theme::primary_hover()
        } else {
            theme::primary()
        };
        ui.painter()
            .rect_filled(rect, CornerRadius::same(8), theme::alpha(fill, alpha));
        Color32::WHITE
    } else {
        ui.painter().rect(
            rect,
            CornerRadius::same(8),
            if hovered {
                theme::overlay(0.06)
            } else {
                theme::control_surface()
            },
            Stroke::new(1.0, theme::input()),
            StrokeKind::Inside,
        );
        theme::foreground()
    };
    let width = widgets::text_width(ui, label, font(12.0, Weight::Medium));
    widgets::text_in(
        ui,
        Rect::from_min_max(
            Pos2::new(rect.center().x - width / 2.0, rect.top()),
            rect.max,
        ),
        label,
        font(12.0, Weight::Medium),
        theme::alpha(color, alpha),
    );
    response.clicked()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_method_follows_upstream() {
        assert!(saved_method(&json!({"accessToken": "", "email": "", "apiToken": ""})).is_none());
        assert!(saved_method(&json!({"accessToken": "<redacted>"})) == Some(Method::AccessToken));
        assert!(
            saved_method(&json!({"email": "a@b.c", "apiToken": "<redacted>"}))
                == Some(Method::ApiToken)
        );
        assert!(saved_method(&json!({"email": "a@b.c", "apiToken": ""})).is_none());
    }
}
