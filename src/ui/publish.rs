//! "Publish repository" (`GitActionsControl.tsx`'s `PublishRepositoryDialog`
//! and its `PUBLISH_PROVIDER_OPTIONS` / `getPublishProviderReadiness`): for a
//! repository without a remote, a three-step wizard. Provider (the hosts the
//! server's source control discovery found signed in, the others asking for
//! setup), Repository (the path on the host, prefilled with the signed-in
//! account, visibility, and under Advanced the remote name and protocol),
//! then the result. Publishing runs `sourceControl.publishRepository`.
use eframe::egui::{self, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};
use serde_json::{Value, json};

use crate::app::App;
use crate::theme::{self, Weight, font};
use crate::ui::providers::add_instance::{
    Kind, dialog_button_enabled, step_strip_with, text_input,
};
use crate::ui::source_control::some;
use crate::ui::widgets;

const WIDTH: f32 = 576.0; // max-w-xl
const STEPS: [&str; 3] = ["Provider", "Repository", "Summary"];
const REQUEST: &str = "publish-repository";
const SUCCESS: Color32 = Color32::from_rgb(0, 188, 125); // --success

/// `PUBLISH_PROVIDER_OPTIONS`: kind, label, host, path placeholder, mark.
const PROVIDERS: [(&str, &str, &str, &str, &str); 3] = [
    (
        "forgejo",
        "Forgejo / Gitea",
        "your server",
        "owner/repo",
        "sc-forgejo",
    ),
    ("github", "GitHub", "github.com", "owner/repo", "sc-github"),
    (
        "gitlab",
        "GitLab",
        "gitlab.com",
        "group/project",
        "sc-gitlab",
    ),
];

#[derive(Default)]
pub(crate) struct PublishDialog {
    step: usize,
    selected: Option<&'static str>,
    repository: Option<String>,
    public: bool,
    remote: String,
    https: bool,
    advanced: bool,
    pending: bool,
    error: Option<String>,
    result: Option<Value>,
}

/// `getPublishProviderReadiness`: whether the host can publish, else why not.
fn readiness(discovered: &[Value], kind: &str) -> Result<(), String> {
    let Some(item) = discovered.iter().find(|p| p["kind"] == kind) else {
        return Err(
            "Provider status unavailable. Open Settings -> Source Control and rescan.".into(),
        );
    };
    if item["status"] != "available" {
        return Err(item["installHint"].as_str().unwrap_or_default().to_owned());
    }
    if item["auth"]["status"] == "unauthenticated" {
        let label = item["label"].as_str().unwrap_or(kind);
        return Err(some(&item["auth"]["detail"]).map(str::to_owned).unwrap_or_else(|| {
            format!("{label} is not authenticated. Open Settings -> Source Control for setup guidance.")
        }));
    }
    Ok(())
}

/// The providers, ready ones first, then by label.
fn sorted_providers(
    discovered: &[Value],
) -> Vec<(
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
)> {
    let mut providers = PROVIDERS.to_vec();
    providers.sort_by(|a, b| {
        let ready = |p: &(&str, &str, &str, &str, &str)| readiness(discovered, p.0).is_ok();
        ready(b).cmp(&ready(a)).then_with(|| a.1.cmp(b.1))
    });
    providers
}

/// `canSubmitPublishRepository`'s path check: "owner/name".
fn valid_repository(path: &str) -> bool {
    let mut parts = path.trim().split('/');
    let owner = parts.next().unwrap_or_default().trim();
    let rest: Vec<&str> = parts.collect();
    !owner.is_empty() && !rest.join("/").trim().is_empty()
}

impl App {
    pub(crate) fn open_publish_dialog(&mut self) {
        if self.discovery.result.is_none() && !self.discovery.pending {
            self.request_discovery();
        }
        self.publish = Some(PublishDialog {
            remote: "origin".into(),
            ..Default::default()
        });
    }

    /// The publish's answer: the summary step, or the error under the form.
    pub(crate) fn publish_result(&mut self, key: &str, result: &Result<Value, String>) -> bool {
        if key != REQUEST {
            return false;
        }
        if let Some(dialog) = &mut self.publish {
            dialog.pending = false;
            match result {
                Ok(value) => {
                    dialog.result = Some(value.clone());
                    dialog.step = 2;
                }
                Err(error) => dialog.error = Some(error.clone()),
            }
        }
        true
    }

    /// The wizard over `screen`.
    pub(crate) fn publish_dialog(&mut self, ui: &mut Ui, screen: Rect) {
        let Some(mut dialog) = self.publish.take() else {
            return;
        };
        let discovered: Vec<Value> = match &self.discovery.result {
            Some(Ok(result)) => result["sourceControlProviders"]
                .as_array()
                .cloned()
                .unwrap_or_default(),
            _ => Vec::new(),
        };
        let providers = sorted_providers(&discovered);
        let first_ready = providers
            .iter()
            .find(|p| readiness(&discovered, p.0).is_ok())
            .map(|p| p.0);
        // `publishProvider`: the pick while ready, else the first ready host.
        let provider_kind = dialog
            .selected
            .filter(|k| readiness(&discovered, k).is_ok())
            .or(first_ready)
            .or(dialog.selected)
            .unwrap_or("github");
        let provider = PROVIDERS
            .iter()
            .find(|p| p.0 == provider_kind)
            .copied()
            .unwrap_or(PROVIDERS[1]);
        let ready = readiness(&discovered, provider_kind).is_ok();
        let item = discovered.iter().find(|p| p["kind"] == provider_kind);
        let account = item.and_then(|i| some(&i["auth"]["account"]));
        let repository = dialog
            .repository
            .clone()
            .unwrap_or_else(|| account.map(|a| format!("{a}/")).unwrap_or_default());
        let host = if provider_kind == "forgejo" {
            item.and_then(|i| some(&i["auth"]["host"]))
                .unwrap_or(provider.2)
                .to_owned()
        } else {
            provider.2.to_owned()
        };
        let can_submit = ready && !dialog.pending && valid_repository(&repository);
        let summaries = [
            Some(provider.1.to_owned()),
            dialog
                .result
                .as_ref()
                .and_then(|r| r["repository"]["nameWithOwner"].as_str())
                .map(str::to_owned),
            None,
        ];

        let description = "Pick where to host it, then point us at a repo to push to.";
        let inner = WIDTH - 50.0;
        let panel_height = match dialog.step {
            0 => {
                16.0 + 8.0
                    + (providers.len().div_ceil(2) as f32) * 44.0
                    + (providers.len().div_ceil(2) as f32 - 1.0) * 12.0
            }
            1 => {
                let mut height = 16.0 + 8.0 + 36.0 + 20.0 + 16.0 + 8.0 + 56.0 + 20.0 + 16.0;
                if dialog.advanced {
                    height += 12.0 + 16.0 + 6.0 + 32.0;
                }
                if dialog.pending {
                    height += 20.0 + 34.0;
                } else if let Some(error) = &dialog.error {
                    let lines = widgets::galley(
                        ui,
                        error,
                        font(12.0, Weight::Regular),
                        Color32::WHITE,
                        16.0,
                        inner - 26.0,
                    )
                    .size()
                    .y;
                    height += 20.0 + 8.0 + 16.0 + 2.0 + lines + 8.0 + 2.0;
                }
                height
            }
            _ => 4.0 + 32.0 + 8.0 + 20.0 + 8.0 + 32.0 + 4.0 + 16.0 + 34.0 + 16.0 + 28.0,
        };
        let height = 137.0 + 20.0 + panel_height + 20.0 + 16.0 + 32.0 + 17.0;
        let d = Rect::from_center_size(screen.center(), Vec2::new(WIDTH, height));
        let mut close = false;
        let mut submit = false;
        let mut open_url = None;
        let mut open_settings = false;
        egui::Area::new(ui.id().with("publish-dialog"))
            .order(egui::Order::Foreground)
            .fixed_pos(screen.min)
            .show(ui.ctx(), |ui| {
                let (backdrop, response) = ui.allocate_exact_size(screen.size(), Sense::click());
                ui.painter().rect_filled(
                    backdrop,
                    0.0,
                    theme::alpha(theme::background(), 200.0 / 255.0),
                );
                if response.clicked() && !ui.rect_contains_pointer(d) {
                    close = true;
                }
                ui.painter().rect(
                    d,
                    CornerRadius::same(18),
                    theme::shade(12),
                    Stroke::new(1.0, theme::input()),
                    StrokeKind::Inside,
                );
                let close_rect = Rect::from_min_size(
                    Pos2::new(d.right() - 41.0, d.top() + 9.0),
                    Vec2::splat(32.0),
                );
                if widgets::icon_button(
                    ui,
                    close_rect,
                    "publish-close",
                    "x",
                    16.0,
                    theme::muted_foreground(),
                    "",
                )
                .clicked()
                {
                    close = true;
                }
                let left = d.left() + 25.0;
                let right = d.right() - 25.0;
                widgets::text(
                    ui,
                    Pos2::new(left, d.top() + 25.0),
                    "Publish repository",
                    font(20.0, Weight::SemiBold),
                    theme::foreground(),
                    20.0,
                );
                widgets::text(
                    ui,
                    Pos2::new(left, d.top() + 53.0),
                    description,
                    font(14.0, Weight::Regular),
                    theme::muted_foreground(),
                    20.0,
                );
                let step = dialog.step;
                if let Some(index) = step_strip_with(
                    ui,
                    Rect::from_min_size(
                        Pos2::new(left, d.top() + 81.0),
                        Vec2::new(right - left, 44.0),
                    ),
                    &STEPS,
                    &summaries,
                    step,
                    |index| step == 2 || index >= STEPS.len() - 1 || index > step,
                ) {
                    dialog.step = index;
                }
                let panel = Rect::from_min_size(
                    Pos2::new(d.left() + 1.0, d.top() + 137.0),
                    Vec2::new(WIDTH - 2.0, 20.0 + panel_height + 20.0),
                );
                ui.painter().rect_filled(panel, 0.0, theme::overlay(0.02));
                for y in [panel.top(), panel.bottom() - 1.0] {
                    ui.painter().rect_filled(
                        Rect::from_min_size(
                            Pos2::new(panel.left(), y),
                            Vec2::new(panel.width(), 1.0),
                        ),
                        0.0,
                        theme::overlay(0.05),
                    );
                }
                let top = panel.top() + 20.0;
                match dialog.step {
                    0 => {
                        open_settings = provider_step(
                            ui,
                            &mut dialog,
                            &discovered,
                            &providers,
                            provider_kind,
                            left,
                            right,
                            top,
                        )
                    }
                    1 => {
                        submit |= repository_step(
                            ui,
                            &mut dialog,
                            &repository,
                            provider,
                            &host,
                            left,
                            right,
                            top,
                        );
                    }
                    _ => open_url = summary_step(ui, &dialog, provider, left, right, top),
                }

                // Footer.
                let footer = panel.bottom() + 16.0;
                let mut x = right;
                let mut button = |ui: &mut Ui, label: &str, kind: Kind, enabled: bool| {
                    let width =
                        (widgets::text_width(ui, label, font(14.0, Weight::Medium)) + 24.0).ceil();
                    let rect =
                        Rect::from_min_size(Pos2::new(x - width, footer), Vec2::new(width, 32.0));
                    x = rect.left() - 8.0;
                    dialog_button_enabled(ui, rect, label, None, kind, enabled)
                };
                match dialog.step {
                    2 => close |= button(ui, "Done", Kind::Primary, true),
                    0 => {
                        if button(ui, "Next", Kind::Primary, first_ready.is_some() && ready) {
                            dialog.step = 1;
                        }
                        close |= button(ui, "Cancel", Kind::Outline, true);
                    }
                    _ => {
                        let label = if dialog.pending {
                            "Publishing..."
                        } else {
                            "Publish"
                        };
                        submit |= button(ui, label, Kind::Primary, can_submit);
                        if button(ui, "Back", Kind::Outline, !dialog.pending) {
                            dialog.step = 0;
                        }
                    }
                }
            });
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) && !dialog.pending {
            close = true;
        }
        if open_settings {
            self.publish = None;
            self.open_settings();
            self.open_settings_page(6);
            return;
        }
        if let Some(url) = open_url {
            ui.ctx().open_url(egui::OpenUrl::new_tab(url));
        }
        if submit
            && can_submit
            && let Some(cwd) = self.workspace_cwd()
        {
            dialog.error = None;
            dialog.pending = true;
            self.send_request(
                REQUEST.into(),
                "sourceControl.publishRepository",
                json!({
                    "cwd": cwd,
                    "provider": provider_kind,
                    "repository": repository.trim(),
                    "visibility": if dialog.public { "public" } else { "private" },
                    "remoteName": if dialog.remote.trim().is_empty() { "origin" } else { dialog.remote.trim() },
                    "protocol": if dialog.https { "https" } else { "ssh" },
                }),
            );
        }
        if !close {
            self.publish = Some(dialog);
        }
    }
}

/// A radio card (`RadioPrimitive.Root`): selected, hovered or resting.
fn card(ui: &Ui, rect: Rect, selected: bool, hovered: bool) {
    let fill = if selected {
        theme::alpha(theme::primary(), 0.1)
    } else if hovered {
        theme::accent()
    } else {
        theme::overlay(0.035)
    };
    ui.painter().rect_filled(rect, CornerRadius::same(10), fill);
    if selected {
        ui.painter().rect_stroke(
            rect,
            CornerRadius::same(10),
            Stroke::new(1.0, theme::alpha(theme::primary(), 0.3)),
            StrokeKind::Inside,
        );
    }
}

fn label(ui: &Ui, pos: Pos2, text: &str) {
    widgets::text(
        ui,
        pos,
        text,
        font(12.0, Weight::Medium),
        theme::foreground(),
        16.0,
    );
}

/// Step 1: the provider cards; returns whether "Setup Required" was hit.
#[allow(clippy::too_many_arguments)]
fn provider_step(
    ui: &mut Ui,
    dialog: &mut PublishDialog,
    discovered: &[Value],
    providers: &[(
        &'static str,
        &'static str,
        &'static str,
        &'static str,
        &'static str,
    )],
    current: &str,
    left: f32,
    right: f32,
    top: f32,
) -> bool {
    label(ui, Pos2::new(left, top), "Provider");
    let width = (right - left - 12.0) / 2.0;
    let mut setup = false;
    for (index, (kind, name, _, _, icon)) in providers.iter().enumerate() {
        let rect = Rect::from_min_size(
            Pos2::new(
                left + (index % 2) as f32 * (width + 12.0),
                top + 24.0 + (index / 2) as f32 * 56.0,
            ),
            Vec2::new(width, 44.0),
        );
        let state = readiness(discovered, kind);
        let response = ui.interact(
            rect,
            ui.id().with(("publish-provider", *kind)),
            if state.is_ok() {
                Sense::click()
            } else {
                Sense::hover()
            },
        );
        let alpha = if state.is_ok() { 1.0 } else { 0.64 };
        card(
            ui,
            rect,
            state.is_ok() && *kind == current,
            response.hovered() && state.is_ok(),
        );
        let icon_color = if state.is_ok() {
            theme::foreground()
        } else {
            theme::alpha(theme::muted_foreground(), alpha)
        };
        theme::paint_icon(
            ui,
            icon,
            Rect::from_min_size(
                Pos2::new(rect.left() + 12.0, rect.center().y - 10.0),
                Vec2::splat(20.0),
            ),
            20.0,
            icon_color,
        );
        let mut text_right = rect.right() - 12.0;
        if let Err(hint) = &state {
            // `Button variant="warning-outline" size="micro"`.
            let badge_font = font(11.0, Weight::Medium);
            let text = "Setup Required";
            let w = (widgets::text_width(ui, text, badge_font.clone()) + 12.0).ceil();
            let badge = Rect::from_min_size(
                Pos2::new(text_right - w, rect.center().y - 10.0),
                Vec2::new(w, 20.0),
            );
            let hint = if hint.is_empty() {
                "Open Settings -> Source Control to configure this provider."
            } else {
                hint.as_str()
            };
            let clicked = ui
                .interact(
                    badge,
                    ui.id().with(("publish-setup", *kind)),
                    Sense::click(),
                )
                .on_hover_text(hint)
                .clicked();
            ui.painter().rect(
                badge,
                CornerRadius::same(6),
                theme::alpha(theme::warning(), 0.08),
                Stroke::new(1.0, theme::alpha(theme::warning(), 0.32)),
                StrokeKind::Inside,
            );
            widgets::text_in(
                ui,
                Rect::from_min_max(Pos2::new(badge.left() + 6.0, badge.top()), badge.max),
                text,
                badge_font,
                theme::warning_foreground(),
            );
            setup |= clicked;
            text_right = badge.left() - 8.0;
        }
        widgets::text_in(
            ui,
            Rect::from_min_max(
                Pos2::new(rect.left() + 44.0, rect.top()),
                Pos2::new(text_right, rect.bottom()),
            ),
            name,
            font(14.0, Weight::Medium),
            theme::alpha(theme::foreground(), alpha),
        );
        if response.clicked() {
            dialog.selected = Some(kind);
            dialog.repository = None;
        }
    }
    setup
}

/// Step 2: path, visibility, advanced options, status; returns whether
/// Enter asked to publish.
#[allow(clippy::too_many_arguments)]
fn repository_step(
    ui: &mut Ui,
    dialog: &mut PublishDialog,
    repository: &str,
    provider: (&str, &str, &str, &str, &str),
    host: &str,
    left: f32,
    right: f32,
    top: f32,
) -> bool {
    let mut y = top;
    label(ui, Pos2::new(left, y), "Repository");
    y += 24.0;
    // The input group: the host addon, then the path.
    let group = Rect::from_min_size(Pos2::new(left, y), Vec2::new(right - left, 36.0));
    ui.painter().rect(
        group,
        CornerRadius::same(8),
        theme::background(),
        Stroke::new(1.0, theme::input()),
        StrokeKind::Inside,
    );
    let addon_text = format!("{host}/");
    let addon_width =
        10.0 + 14.0 + 6.0 + widgets::text_width(ui, &addon_text, theme::mono(12.0)) + 10.0;
    let addon = Rect::from_min_size(group.min, Vec2::new(addon_width.ceil(), 36.0));
    ui.painter().rect_filled(
        addon.shrink(1.0),
        CornerRadius {
            nw: 7,
            sw: 7,
            ne: 0,
            se: 0,
        },
        theme::alpha(theme::muted(), 0.5),
    );
    ui.painter().rect_filled(
        Rect::from_min_size(
            Pos2::new(addon.right() - 1.0, addon.top()),
            Vec2::new(1.0, 36.0),
        ),
        0.0,
        theme::input(),
    );
    theme::paint_icon(
        ui,
        provider.4,
        Rect::from_min_size(
            Pos2::new(addon.left() + 10.0, addon.center().y - 7.0),
            Vec2::splat(14.0),
        ),
        14.0,
        theme::muted_foreground(),
    );
    widgets::text_in(
        ui,
        Rect::from_min_max(Pos2::new(addon.left() + 30.0, addon.top()), addon.max),
        &addon_text,
        theme::mono(12.0),
        theme::muted_foreground(),
    );
    let field = Rect::from_min_max(
        Pos2::new(addon.right() + 12.0, group.top() + 1.0),
        Pos2::new(group.right() - 12.0, group.bottom() - 1.0),
    );
    let mut text = repository.to_owned();
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(field));
    let response = child.add_enabled(
        !dialog.pending,
        egui::TextEdit::singleline(&mut text)
            .id(egui::Id::new("publish-repository-path"))
            .frame(egui::Frame::NONE)
            .margin(egui::Margin::ZERO)
            .font(theme::mono(14.0))
            .text_color(theme::foreground())
            .vertical_align(egui::Align::Center)
            .desired_width(field.width())
            .min_size(Vec2::new(0.0, field.height())),
    );
    if text.is_empty() {
        widgets::text_in(
            ui,
            field,
            provider.3,
            theme::mono(14.0),
            theme::alpha(theme::muted_foreground(), 0.6),
        );
    }
    if text != repository {
        dialog.repository = Some(text);
    }
    let enter = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
    y += 36.0 + 20.0;

    label(ui, Pos2::new(left, y), "Visibility");
    y += 24.0;
    let width = (right - left - 12.0) / 2.0;
    for (index, (public, name, detail, icon)) in [
        (false, "Private", "Only invited people", "lock"),
        (true, "Public", "Anyone on the web", "globe"),
    ]
    .into_iter()
    .enumerate()
    {
        let rect = Rect::from_min_size(
            Pos2::new(left + index as f32 * (width + 12.0), y),
            Vec2::new(width, 56.0),
        );
        let response = ui.interact(
            rect,
            ui.id().with(("publish-visibility", index)),
            Sense::click(),
        );
        card(ui, rect, dialog.public == public, response.hovered());
        theme::paint_icon(
            ui,
            icon,
            Rect::from_min_size(
                Pos2::new(rect.left() + 12.0, rect.center().y - 8.0),
                Vec2::splat(16.0),
            ),
            16.0,
            theme::muted_foreground(),
        );
        widgets::text(
            ui,
            Pos2::new(rect.left() + 40.0, rect.top() + 10.0),
            name,
            font(14.0, Weight::Medium),
            theme::foreground(),
            20.0,
        );
        widgets::text(
            ui,
            Pos2::new(rect.left() + 40.0, rect.top() + 30.0),
            detail,
            font(12.0, Weight::Regular),
            theme::muted_foreground(),
            16.0,
        );
        if response.clicked() && !dialog.pending {
            dialog.public = public;
        }
    }
    y += 56.0 + 20.0;

    // "Advanced".
    let toggle_font = font(12.0, Weight::Medium);
    let toggle = Rect::from_min_size(
        Pos2::new(left, y),
        Vec2::new(
            18.0 + widgets::text_width(ui, "Advanced", toggle_font.clone()),
            16.0,
        ),
    );
    let response = ui.interact(toggle, ui.id().with("publish-advanced"), Sense::click());
    let color = if response.hovered() {
        theme::foreground()
    } else {
        theme::muted_foreground()
    };
    theme::paint_icon(
        ui,
        if dialog.advanced {
            "chevron-down"
        } else {
            "chevron-right"
        },
        Rect::from_min_size(Pos2::new(left, y + 1.0), Vec2::splat(14.0)),
        14.0,
        color,
    );
    widgets::text(
        ui,
        Pos2::new(left + 18.0, y),
        "Advanced",
        toggle_font,
        color,
        16.0,
    );
    if response.clicked() {
        dialog.advanced = !dialog.advanced;
    }
    y += 16.0;
    if dialog.advanced {
        y += 12.0;
        label(ui, Pos2::new(left, y), "Remote");
        label(ui, Pos2::new(left + width + 12.0, y), "Protocol");
        y += 22.0;
        let remote = Rect::from_min_size(Pos2::new(left, y), Vec2::new(width, 32.0));
        text_input(ui, remote, "publish-remote", &mut dialog.remote, "origin");
        let mut x = left + width + 12.0;
        for (https, name) in [(false, "SSH"), (true, "HTTPS")] {
            let w = (widgets::text_width(ui, name, font(14.0, Weight::Medium)) + 20.0).ceil();
            let rect = Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, 32.0));
            let response = ui.interact(
                rect,
                ui.id().with(("publish-protocol", name)),
                Sense::click(),
            );
            let pressed = dialog.https == https;
            if pressed || response.hovered() {
                ui.painter()
                    .rect_filled(rect, CornerRadius::same(8), theme::accent());
            }
            widgets::text_in(
                ui,
                Rect::from_min_max(Pos2::new(rect.left() + 10.0, rect.top()), rect.max),
                name,
                font(14.0, Weight::Medium),
                if pressed {
                    theme::foreground()
                } else {
                    theme::muted_foreground()
                },
            );
            if response.clicked() && !dialog.pending {
                dialog.https = https;
            }
            x = rect.right() + 4.0;
        }
        y += 32.0;
    }
    let box_rect = |height: f32| {
        Rect::from_min_size(Pos2::new(left, y + 20.0), Vec2::new(right - left, height))
    };
    if dialog.pending {
        let rect = box_rect(34.0);
        ui.painter()
            .rect_filled(rect, CornerRadius::same(8), theme::overlay(0.035));
        crate::ui::content_search::paint_spinner(
            ui,
            Pos2::new(rect.left() + 19.0, rect.center().y),
            14.0,
        );
        widgets::text_in(
            ui,
            Rect::from_min_max(Pos2::new(rect.left() + 34.0, rect.top()), rect.max),
            &format!("Publishing repository to {}...", provider.1),
            font(12.0, Weight::Regular),
            theme::muted_foreground(),
        );
    } else if let Some(error) = &dialog.error {
        let galley = widgets::galley(
            ui,
            error,
            font(12.0, Weight::Regular),
            theme::alpha(theme::error_foreground(), 0.9),
            16.0,
            right - left - 26.0,
        );
        let rect = box_rect(8.0 + 16.0 + 2.0 + galley.size().y + 8.0 + 2.0);
        ui.painter().rect(
            rect,
            CornerRadius::same(8),
            theme::alpha(theme::error_foreground(), 0.1),
            Stroke::new(1.0, theme::alpha(theme::error_foreground(), 0.4)),
            StrokeKind::Inside,
        );
        widgets::text(
            ui,
            Pos2::new(rect.left() + 13.0, rect.top() + 9.0),
            "Publish failed",
            font(12.0, Weight::Medium),
            theme::error_foreground(),
            16.0,
        );
        ui.painter().galley(
            Pos2::new(rect.left() + 13.0, rect.top() + 27.0),
            galley,
            theme::error_foreground(),
        );
    }
    enter
}

/// Step 3: published or created, the repository and its link; returns the
/// URL to open.
fn summary_step(
    ui: &mut Ui,
    dialog: &PublishDialog,
    provider: (&str, &str, &str, &str, &str),
    left: f32,
    right: f32,
    top: f32,
) -> Option<String> {
    let Some(result) = &dialog.result else {
        widgets::text(
            ui,
            Pos2::new(left, top),
            "Publish result unavailable.",
            font(12.0, Weight::Regular),
            theme::muted_foreground(),
            16.0,
        );
        return None;
    };
    let center = (left + right) / 2.0;
    let pushed = result["status"] == "pushed";
    let mut y = top + 4.0;
    ui.painter().circle_filled(
        Pos2::new(center, y + 16.0),
        16.0,
        theme::alpha(SUCCESS, 0.15),
    );
    theme::paint_icon(
        ui,
        "check",
        Rect::from_center_size(Pos2::new(center, y + 16.0), Vec2::splat(16.0)),
        16.0,
        SUCCESS,
    );
    y += 32.0 + 8.0;
    let title = if pushed {
        "Repository published"
    } else {
        "Repository created"
    };
    let title_font = font(14.0, Weight::SemiBold);
    let w = widgets::text_width(ui, title, title_font.clone());
    widgets::text(
        ui,
        Pos2::new(center - w / 2.0, y),
        title,
        title_font,
        theme::foreground(),
        20.0,
    );
    y += 20.0 + 8.0;
    let detail = if pushed {
        format!(
            "{} is now live on {}.",
            result["branch"].as_str().unwrap_or_default(),
            provider.1
        )
    } else {
        format!(
            "Remote \"{}\" is set up. Make a commit and push it to share your code.",
            result["remoteName"].as_str().unwrap_or_default()
        )
    };
    let galley = widgets::galley(
        ui,
        &detail,
        font(12.0, Weight::Regular),
        theme::muted_foreground(),
        16.0,
        320.0,
    );
    let galley_width = galley.size().x;
    ui.painter().galley(
        Pos2::new(center - galley_width / 2.0, y),
        galley,
        theme::muted_foreground(),
    );
    y += 32.0 + 4.0 + 16.0;
    let row = Rect::from_min_size(Pos2::new(left, y), Vec2::new(right - left, 34.0));
    ui.painter()
        .rect_filled(row, CornerRadius::same(10), theme::overlay(0.035));
    theme::paint_icon(
        ui,
        provider.4,
        Rect::from_min_size(
            Pos2::new(row.left() + 12.0, row.center().y - 7.0),
            Vec2::splat(14.0),
        ),
        14.0,
        theme::muted_foreground(),
    );
    widgets::text_in(
        ui,
        Rect::from_min_max(
            Pos2::new(row.left() + 34.0, row.top()),
            Pos2::new(row.right() - 12.0, row.bottom()),
        ),
        result["repository"]["nameWithOwner"]
            .as_str()
            .unwrap_or_default(),
        theme::mono(12.0),
        theme::foreground(),
    );
    y += 34.0 + 16.0;
    let button = Rect::from_min_size(Pos2::new(left, y), Vec2::new(right - left, 28.0));
    let label = format!("Open on {}", provider.1);
    let response =
        widgets::outline_button(ui, button, "publish-open", None, "", CornerRadius::same(8));
    let w = widgets::text_width(ui, &label, font(14.0, Weight::Medium));
    widgets::text_in(
        ui,
        Rect::from_min_max(
            Pos2::new(button.center().x - w / 2.0, button.top()),
            button.max,
        ),
        &label,
        font(14.0, Weight::Medium),
        theme::foreground(),
    );
    response.clicked().then(|| {
        result["repository"]["url"]
            .as_str()
            .unwrap_or_default()
            .to_owned()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn providers_are_ready_only_when_signed_in() {
        let discovered = vec![
            json!({"kind": "github", "label": "GitHub", "status": "available", "auth": {"status": "authenticated"}}),
            json!({"kind": "gitlab", "label": "GitLab", "status": "available", "auth": {"status": "unauthenticated", "detail": {"_tag": "None"}}}),
            json!({"kind": "forgejo", "status": "missing", "installHint": "Install it", "auth": {}}),
        ];
        assert!(readiness(&discovered, "github").is_ok());
        assert_eq!(
            readiness(&discovered, "gitlab").unwrap_err(),
            "GitLab is not authenticated. Open Settings -> Source Control for setup guidance."
        );
        assert_eq!(readiness(&discovered, "forgejo").unwrap_err(), "Install it");
        assert!(
            readiness(&discovered, "gitea")
                .unwrap_err()
                .starts_with("Provider status unavailable")
        );
        let order: Vec<&str> = sorted_providers(&discovered).iter().map(|p| p.0).collect();
        assert_eq!(order, ["github", "forgejo", "gitlab"]);
        assert!(valid_repository("me/app"));
        assert!(!valid_repository("me/"));
        assert!(!valid_repository("/app"));
    }
}
