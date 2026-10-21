//! The Source Control settings discovery (`SourceControlSettings.tsx`,
//! `RedactedSensitiveText.tsx`): the server's version control systems and
//! hosting integrations from `server.discoverSourceControl`, with their
//! status dot, version, authentication summary and availability switch.
//!
//! Not ported: the Git fetch interval and Bitbucket credentials panels behind
//! the row chevrons (the chevron shows a notice), and the scan button's
//! tooltip. Accounts stay redacted (blurred placeholder) until clicked.
use std::collections::HashSet;

use eframe::egui::{Color32, CornerRadius, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};
use serde_json::{Value, json};

use crate::app::App;
use crate::theme::{self, Weight, font};
use crate::ui::widgets;

const CARD_WIDTH: f32 = 848.0;
/// `bg-card/40` over the page.
fn card_fill() -> Color32 {
    theme::stock_or(Color32::from_rgb(13, 13, 13), |p| {
        theme::mix(p.background, p.card, 0.4)
    })
}
/// `border-border/60`.
fn card_border() -> Color32 {
    theme::stock_or(Color32::from_rgb(22, 22, 22), |p| {
        theme::mix(p.background, p.border, 0.6)
    })
}
/// `border-border/50` between rows.
fn row_divider() -> Color32 {
    theme::stock_or(Color32::from_rgb(21, 21, 21), |p| {
        theme::mix(p.background, p.border, 0.5)
    })
}
const SUCCESS: Color32 = Color32::from_rgb(0, 188, 125); // --success
/// The summary column: the row minus padding, the switch and its gap.
const SUMMARY_WIDTH: f32 = CARD_WIDTH - 2.0 - 32.0 - 30.0 - 12.0;

/// Discovery state shared by the settings page.
#[derive(Default)]
pub struct Discovery {
    pub result: Option<Result<Value, String>>,
    pub pending: bool,
    /// Accounts the user chose to reveal, by item kind.
    pub revealed: HashSet<String>,
}

/// `Option` fields arrive as `{"_tag": "Some", "value": …}`.
fn some(value: &Value) -> Option<&str> {
    (value["_tag"] == "Some")
        .then(|| value["value"].as_str())
        .flatten()
}

fn is_provider(item: &Value) -> bool {
    item.get("auth").is_some()
}

/// `isVcsNotReady`: a version control system the server does not drive yet.
fn not_ready(item: &Value) -> bool {
    !is_provider(item) && item["implemented"] != true
}

fn icon_name(item: &Value) -> Option<&'static str> {
    Some(match item["kind"].as_str()? {
        "git" => "vcs-git",
        "jj" => "vcs-jj",
        "github" => "sc-github",
        "gitlab" => "sc-gitlab",
        "azure-devops" => "sc-azure-devops",
        "bitbucket" => "sc-bitbucket",
        "forgejo" => "sc-forgejo",
        _ => return None,
    })
}

/// `itemStatusDot`.
fn status_dot(item: &Value) -> Color32 {
    if not_ready(item) {
        theme::alpha(theme::muted_foreground(), 0.35)
    } else if item["status"] != "available"
        || (is_provider(item) && item["auth"]["status"] != "authenticated")
    {
        theme::warning()
    } else {
        SUCCESS
    }
}

/// `redactedPlaceholder`: a stable stand-in of the same shape (FNV-1a seed,
/// then a murmur-style mix per character), with JavaScript's 32-bit math.
pub(crate) fn redacted_placeholder(value: &str) -> String {
    const ALPHABET: &[u8] = b"abcdefghjkmnpqrstuvwxyz23456789";
    let mut state: i32 = 0x811c9dc5u32 as i32;
    for unit in value.encode_utf16() {
        state ^= i32::from(unit);
        state = state.wrapping_mul(0x01000193);
    }
    let mut next = || {
        state = (state ^ ((state as u32) >> 13) as i32).wrapping_mul(0x85ebca6bu32 as i32);
        state = (state ^ ((state as u32) >> 16) as i32).wrapping_mul(0xc2b2ae35u32 as i32);
        let index = (i64::from(state).abs() % ALPHABET.len() as i64) as usize;
        ALPHABET[index] as char
    };
    value
        .chars()
        .map(|c| if "@.-_".contains(c) { c } else { next() })
        .collect()
}

/// The summary line(s) of a row (`itemSummary`), as plain text; `None` for
/// the authenticated account line, which is laid out separately.
fn summary(item: &Value) -> Option<String> {
    let label = item["label"].as_str().unwrap_or_default();
    let hint = item["installHint"].as_str().unwrap_or_default();
    if not_ready(item) {
        return Some(format!("Support for {label} is coming soon."));
    }
    if item["status"] != "available" {
        return Some(format!("Not available on this server: {hint}"));
    }
    if !is_provider(item) {
        return Some("Available".into());
    }
    let auth = &item["auth"];
    match auth["status"].as_str() {
        Some("authenticated") => None,
        Some("unauthenticated") => Some(match item["executable"].as_str() {
            None => format!("Available. {hint}"),
            Some(tool) => format!(
                "{label} is not authenticated on this server. Sign in or configure credentials using the {tool} tool on the server host to enable change request features."
            ),
        }),
        _ => Some(format!(
            "Could not verify {label}. {}",
            some(&auth["detail"]).unwrap_or(hint)
        )),
    }
}

fn summary_font() -> eframe::egui::FontId {
    font(12.0, Weight::Regular)
}

fn summary_color() -> Color32 {
    theme::alpha(theme::muted_foreground(), 0.8)
}

/// Rows with a details panel show a chevron before the switch.
fn has_details(item: &Value) -> bool {
    matches!(item["kind"].as_str(), Some("git" | "bitbucket"))
}

/// `GIT_FETCH_INTERVAL_STEP_SECONDS`.
const GIT_FETCH_STEP: i64 = 5;

/// The Git fetch interval of a background activity profile, in seconds
/// (`getBackgroundActivityPresetSettings`).
fn preset_git_fetch(profile: &str) -> i64 {
    match profile {
        "performance" => 15,
        "battery-saver" => 0,
        _ => 30,
    }
}

/// `getBackgroundActivityBaseProfile`.
fn base_profile(activity: &Value) -> String {
    let profile = activity["profile"].as_str().unwrap_or("balanced");
    if profile == "custom" {
        activity["baseProfile"]
            .as_str()
            .unwrap_or("balanced")
            .to_owned()
    } else {
        profile.to_owned()
    }
}

/// The resolved Git fetch interval and its profile default, in seconds
/// (`resolveServerBackgroundActivitySettings`, with the legacy fields read
/// while `backgroundActivity` is untouched).
fn git_fetch_seconds(settings: &Value) -> (i64, i64) {
    let activity = &settings["backgroundActivity"];
    let default = preset_git_fetch(&base_profile(activity));
    let untouched = activity["profile"].as_str().unwrap_or("balanced") == "balanced"
        && activity["baseProfile"].is_null()
        && activity["overrides"]
            .as_object()
            .is_none_or(|o| o.is_empty());
    let legacy_git = settings["automaticGitFetchInterval"]
        .as_i64()
        .unwrap_or(30_000);
    let legacy_profile = settings["backgroundActivityProfile"]
        .as_str()
        .unwrap_or("balanced");
    let legacy_health = settings["providerHealthRefreshInterval"]
        .as_i64()
        .unwrap_or(300_000);
    if untouched
        && (legacy_profile != "balanced" || legacy_git != 30_000 || legacy_health != 300_000)
    {
        return (legacy_git / 1000, default);
    }
    let overridden = (activity["profile"] == "custom")
        .then(|| activity["overrides"]["automaticGitFetchInterval"].as_i64())
        .flatten();
    (overridden.map_or(default, |ms| ms / 1000), default)
}

/// `backgroundActivityOverrideSettings` (Source Control's): the current
/// overrides with the Git interval set (or dropped), as a custom profile.
fn git_fetch_patch(activity: &Value, seconds: Option<i64>) -> Value {
    let mut overrides = activity["overrides"]
        .as_object()
        .cloned()
        .unwrap_or_default();
    match seconds {
        Some(seconds) => {
            overrides.insert(
                "automaticGitFetchInterval".into(),
                json!(seconds.max(0) * 1000),
            );
        }
        None => {
            overrides.remove("automaticGitFetchInterval");
        }
    }
    json!({"backgroundActivity": {
        "schemaVersion": 1,
        "profile": "custom",
        "baseProfile": base_profile(activity),
        "overrides": overrides,
    }})
}

/// A `Badge variant="warning" size="sm"`; returns its width.
fn warning_badge(ui: &Ui, pos: Pos2, text: &str, fade: f32) -> f32 {
    let badge_font = font(10.0, Weight::Medium);
    let width = widgets::text_width(ui, text, badge_font.clone()) + 8.0;
    let rect = Rect::from_min_size(pos, Vec2::new(width, 16.0));
    ui.painter().rect_filled(
        rect,
        CornerRadius::same(4),
        theme::alpha(theme::warning(), 0.16 * fade),
    );
    widgets::text(
        ui,
        Pos2::new(rect.left() + 4.0, rect.top() + 3.0),
        text,
        badge_font,
        theme::alpha(theme::warning_foreground(), fade),
        10.0,
    );
    width
}

impl App {
    pub(crate) fn request_discovery(&mut self) {
        self.discovery.pending = true;
        self.send_request(
            "source-control-discovery".into(),
            "server.discoverSourceControl",
            json!({}),
        );
    }

    fn row_height(&self, ui: &Ui, item: &Value, first: bool) -> f32 {
        let lines = summary(item).map_or(18.0, |text| {
            let width = SUMMARY_WIDTH - if has_details(item) { 32.0 } else { 0.0 };
            widgets::galley(ui, &text, summary_font(), summary_color(), 18.0, width)
                .size()
                .y
        });
        let kind = item["kind"].as_str().unwrap_or_default();
        let details = if has_details(item) && self.source_control_expanded.contains(kind) {
            self.details_height(ui, item)
        } else {
            0.0
        };
        12.0 + 20.0 + 4.0 + lines + 12.0 + if first { 0.0 } else { 1.0 } + details
    }

    /// The Version Control and Source Control Providers sections from
    /// `top`; returns the top of the next section.
    pub(crate) fn discovery_sections(&mut self, ui: &mut Ui, left: f32, top: f32) -> f32 {
        let result = self.discovery.result.clone();
        let Some(Ok(result)) = result else {
            let (title, text) = match &self.discovery.result {
                Some(Err(error)) => ("Could not scan the server environment", error.clone()),
                _ if self.discovery.pending => ("Version Control", "Scanning…".to_owned()),
                _ => ("Version Control", String::new()),
            };
            self.discovery_header(ui, left, top, title, true);
            let card =
                Rect::from_min_size(Pos2::new(left, top + 38.0), Vec2::new(CARD_WIDTH, 68.0));
            paint_card(ui, card);
            widgets::text(
                ui,
                Pos2::new(card.left() + 16.0, card.top() + 24.0),
                &text,
                font(14.0, Weight::Regular),
                theme::muted_foreground(),
                20.0,
            );
            return card.bottom() + 32.0;
        };
        let systems: Vec<Value> = result["versionControlSystems"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let providers: Vec<Value> = result["sourceControlProviders"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let mut y = top;
        if !systems.is_empty() {
            y = self.discovery_section(ui, left, y, "Version Control", true, &systems);
        }
        if !providers.is_empty() {
            y = self.discovery_section(
                ui,
                left,
                y,
                "Source Control Providers",
                systems.is_empty(),
                &providers,
            );
        }
        y
    }

    /// A section title with the optional rescan button (`ghost-muted`
    /// `icon-xs`).
    fn discovery_header(&mut self, ui: &mut Ui, left: f32, top: f32, title: &str, scan: bool) {
        widgets::text_in(
            ui,
            Rect::from_min_size(Pos2::new(left + 16.0, top), Vec2::new(CARD_WIDTH, 28.0)),
            title,
            font(14.0, Weight::Regular),
            theme::alpha(theme::foreground(), 0.7),
        );
        if scan {
            let button = Rect::from_min_size(
                Pos2::new(left + CARD_WIDTH - 40.0, top + 2.0),
                Vec2::splat(24.0),
            );
            if widgets::icon_button(
                ui,
                button,
                "rescan-source-control",
                "refresh-cw",
                14.0,
                theme::muted_foreground(),
                "Rescan Git and hosting integrations",
            )
            .clicked()
                && !self.discovery.pending
            {
                self.request_discovery();
            }
        }
    }

    fn discovery_section(
        &mut self,
        ui: &mut Ui,
        left: f32,
        top: f32,
        title: &str,
        scan: bool,
        items: &[Value],
    ) -> f32 {
        self.discovery_header(ui, left, top, title, scan);
        let heights: Vec<f32> = items
            .iter()
            .enumerate()
            .map(|(i, item)| self.row_height(ui, item, i == 0))
            .collect();
        let card = Rect::from_min_size(
            Pos2::new(left, top + 38.0),
            Vec2::new(CARD_WIDTH, heights.iter().sum::<f32>() + 2.0),
        );
        paint_card(ui, card);
        let mut row_top = card.top() + 1.0;
        for (index, (item, height)) in items.iter().zip(&heights).enumerate() {
            let rect = Rect::from_min_size(
                Pos2::new(card.left() + 1.0, row_top),
                Vec2::new(CARD_WIDTH - 2.0, *height),
            );
            self.discovery_row(ui, rect, item, index, items.len());
            row_top += height;
        }
        card.bottom() + 32.0
    }

    /// `DiscoveryItemRow`.
    fn discovery_row(&mut self, ui: &mut Ui, rect: Rect, item: &Value, index: usize, count: usize) {
        let kind = item["kind"].as_str().unwrap_or_default().to_owned();
        let painter = ui.painter().clone();
        // Hover tint, rounded with the card at its ends.
        if ui.rect_contains_pointer(rect) {
            let radius = CornerRadius {
                nw: if index == 0 { 13 } else { 0 },
                ne: if index == 0 { 13 } else { 0 },
                sw: if index + 1 == count { 13 } else { 0 },
                se: if index + 1 == count { 13 } else { 0 },
            };
            painter.rect_filled(rect, radius, theme::alpha(theme::muted(), 0.2));
        }
        let mut top = rect.top();
        if index > 0 {
            painter.rect_filled(
                Rect::from_min_size(rect.min, Vec2::new(rect.width(), 1.0)),
                0.0,
                row_divider(),
            );
            top += 1.0;
        }
        // Rows not yet supported fade to 80%.
        let fade = if not_ready(item) { 0.8 } else { 1.0 };
        let left = rect.left() + 16.0;
        let line = top + 12.0;
        if let Some(icon) = icon_name(item) {
            let tint = if kind == "github" {
                theme::alpha(theme::foreground(), 0.8 * fade)
            } else {
                theme::alpha(Color32::WHITE, fade)
            };
            theme::paint_icon(
                ui,
                icon,
                Rect::from_min_size(Pos2::new(left + 1.0, line + 1.0), Vec2::splat(18.0)),
                18.0,
                tint,
            );
        }
        // Status dot with a 2px background ring, at the mark's top-left.
        let dot = Pos2::new(left + 2.0, line + 2.0);
        painter.circle_filled(dot, 6.0, theme::alpha(theme::background(), fade));
        painter.circle_filled(dot, 4.0, theme::alpha(status_dot(item), fade));
        let label = item["label"].as_str().unwrap_or_default();
        let label_font = font(14.0, Weight::Medium);
        widgets::text(
            ui,
            Pos2::new(left + 28.0, line),
            label,
            label_font.clone(),
            theme::alpha(theme::foreground(), fade),
            20.0,
        );
        let mut x = left + 28.0 + widgets::text_width(ui, label, label_font) + 8.0;
        if let Some(version) = some(&item["version"]) {
            widgets::text(
                ui,
                Pos2::new(x, line + 2.0),
                version,
                theme::mono(12.0),
                theme::alpha(theme::muted_foreground(), fade),
                16.0,
            );
            x += widgets::text_width(ui, version, theme::mono(12.0)) + 8.0;
        }
        if not_ready(item) {
            x += warning_badge(ui, Pos2::new(x, line + 2.0), "Coming Soon", fade) + 8.0;
        }
        if is_provider(item) && item["auth"]["status"] == "unauthenticated" {
            warning_badge(ui, Pos2::new(x, line + 2.0), "Not authenticated", fade);
        }

        let summary_top = line + 24.0;
        match summary(item) {
            Some(text) => {
                let width = SUMMARY_WIDTH - if has_details(item) { 32.0 } else { 0.0 };
                let galley = widgets::galley(
                    ui,
                    &text,
                    summary_font(),
                    theme::alpha(summary_color(), fade),
                    18.0,
                    width,
                );
                painter.galley(Pos2::new(left, summary_top), galley, summary_color());
            }
            None => self.account_line(ui, Pos2::new(left, summary_top), item, &kind),
        }

        // Controls: the details chevron and the read-only availability switch.
        let expanded = has_details(item) && self.source_control_expanded.contains(&kind);
        let details = if expanded {
            self.details_height(ui, item)
        } else {
            0.0
        };
        let center_y = rect.center().y - details / 2.0 + if index > 0 { 0.5 } else { 0.0 };
        let right = rect.right() - 16.0;
        if has_details(item) {
            let button = Rect::from_min_size(
                Pos2::new(right - 30.0 - 8.0 - 24.0, center_y - 12.0),
                Vec2::splat(24.0),
            );
            if widgets::icon_button(
                ui,
                button,
                ("source-control-details", &kind),
                if expanded {
                    "chevron-up"
                } else {
                    "chevron-down"
                },
                14.0,
                theme::muted_foreground(),
                "",
            )
            .clicked()
                && !self.source_control_expanded.remove(&kind)
            {
                self.source_control_expanded.insert(kind.clone());
            }
        }
        if !not_ready(item) {
            let enabled = item["status"] == "available"
                && if is_provider(item) {
                    item["auth"]["status"] == "authenticated"
                } else {
                    item["implemented"] == true
                };
            let track = Rect::from_min_size(
                Pos2::new(right - 30.0, center_y - 9.0),
                Vec2::new(30.0, 18.0),
            );
            // Disabled: 64%.
            let track_color = if enabled {
                theme::primary()
            } else {
                theme::overlay(0.08)
            };
            painter.rect_filled(
                track,
                CornerRadius::same(9),
                theme::alpha(track_color, 0.64),
            );
            let thumb_x = if enabled {
                track.left() + 14.0
            } else {
                track.left() + 2.0
            };
            painter.rect_filled(
                Rect::from_min_size(Pos2::new(thumb_x, track.top() + 2.0), Vec2::splat(14.0)),
                CornerRadius::same(7),
                theme::background(),
            );
        }
        let top = rect.bottom() - details + 4.0;
        match kind.as_str() {
            "git" if expanded => {
                self.git_fetch_interval(ui, rect.left() + 16.0, rect.right() - 16.0, top);
            }
            "bitbucket" if expanded => {
                self.bitbucket_form(ui, rect.left() + 16.0, rect.right() - 16.0, top);
            }
            _ => {}
        }
    }

    /// The height of a row's expanded details (`px-4 pb-4 pt-1`).
    fn details_height(&self, ui: &Ui, item: &Value) -> f32 {
        match item["kind"].as_str() {
            Some("git") => 4.0 + 43.5 + 16.0,
            Some("bitbucket") => 4.0 + self.bitbucket_height(ui) + 16.0,
            _ => 0.0,
        }
    }

    /// `GitFetchIntervalSettings`: title, policy tooltip, reset, the
    /// description, and the seconds field.
    fn git_fetch_interval(&mut self, ui: &mut Ui, left: f32, right: f32, top: f32) {
        let settings = self.state.config["settings"].clone();
        let (seconds, default) = git_fetch_seconds(&settings);
        let title = "Git fetch interval";
        let title_font = font(12.0, Weight::Medium);
        widgets::text(
            ui,
            Pos2::new(left, top + 2.0),
            title,
            title_font.clone(),
            theme::foreground(),
            16.0,
        );
        let info = Rect::from_min_size(
            Pos2::new(left + widgets::text_width(ui, title, title_font) + 4.0, top),
            Vec2::splat(20.0),
        );
        widgets::icon_button(
            ui,
            info,
            "git-fetch-policy",
            "info",
            14.0,
            theme::muted_foreground(),
            "This interval is configured for Git only. The shared Background activity policy still decides whether Git refreshes may run when the timer fires. Custom intervals appear as Advanced in General settings.",
        );
        let mut next: Option<Option<i64>> = None;
        if seconds != default {
            let reset = Rect::from_min_size(Pos2::new(info.right() + 4.0, top), Vec2::splat(20.0));
            if widgets::icon_button(
                ui,
                reset,
                "git-fetch-reset",
                "undo-2",
                12.0,
                theme::muted_foreground(),
                "Reset fetch interval",
            )
            .clicked()
            {
                next = Some(None);
            }
        }
        widgets::text(
            ui,
            Pos2::new(left, top + 24.0),
            "Refresh remote branches in the background. Set to 0 to avoid automatic Git prompts.",
            font(12.0, Weight::Regular),
            theme::muted_foreground(),
            19.5,
        );
        let unit_width = widgets::text_width(ui, "seconds", font(12.0, Weight::Regular));
        let field = Rect::from_min_size(
            Pos2::new(right - unit_width - 8.0 - 128.0, top),
            Vec2::new(128.0, 28.0),
        );
        if let Some(value) =
            self.interval_field(ui, field, seconds, "git-fetch-interval", GIT_FETCH_STEP)
        {
            next = Some(Some(value));
        }
        widgets::text(
            ui,
            Pos2::new(field.right() + 8.0, top + 6.0),
            "seconds",
            font(12.0, Weight::Regular),
            theme::muted_foreground(),
            16.0,
        );
        if let Some(value) = next {
            let patch = git_fetch_patch(&settings["backgroundActivity"], value);
            // Replace locally: the settings patch merges and would keep a
            // dropped override until the server's echo.
            self.state.config["settings"]["backgroundActivity"] =
                patch["backgroundActivity"].clone();
            self.patch_server_settings(patch);
        }
    }

    /// "Authenticated as <account>", the account redacted until clicked.
    fn account_line(&mut self, ui: &mut Ui, pos: Pos2, item: &Value, kind: &str) {
        let small = summary_font();
        let mut x = pos.x;
        for word in ["Authenticated", "as"] {
            widgets::text(
                ui,
                Pos2::new(x, pos.y),
                word,
                small.clone(),
                summary_color(),
                18.0,
            );
            x += widgets::text_width(ui, word, small.clone()) + 4.0;
            if some(&item["auth"]["account"]).is_none() {
                return;
            }
        }
        let Some(account) = some(&item["auth"]["account"]) else {
            return;
        };
        self.redacted_text(
            ui,
            Pos2::new(x, pos.y + 1.0),
            account,
            kind,
            ("Click to reveal account", "Click to hide account"),
        );
    }

    /// `RedactedSensitiveText`: a stable placeholder of `value`'s shape,
    /// blurred, until clicked; returns its width. `key` names the reveal
    /// state, `tips` the (reveal, hide) tooltips.
    pub(crate) fn redacted_text(
        &mut self,
        ui: &mut Ui,
        pos: Pos2,
        value: &str,
        key: &str,
        tips: (&str, &str),
    ) -> f32 {
        let revealed = self.discovery.revealed.contains(key);
        let shown = if revealed {
            value.to_owned()
        } else {
            redacted_placeholder(value)
        };
        let mono = theme::mono(11.0);
        let width = widgets::text_width(ui, &shown, mono.clone());
        let button = Rect::from_min_size(pos, Vec2::new(width, 17.0));
        let response = ui
            .interact(button, ui.id().with(("reveal", key)), Sense::click())
            .on_hover_text(if revealed { tips.1 } else { tips.0 });
        let color = if response.hovered() {
            theme::foreground()
        } else {
            theme::muted_foreground()
        };
        if revealed {
            widgets::text(ui, pos, &shown, mono, color, 16.5);
        } else {
            // `blur-xs` (4px): the text smeared over two rings of offsets.
            let mut offsets = vec![(0.0f32, 0.0f32)];
            for (radius, count) in [(1.3f32, 6), (2.8, 12)] {
                for step in 0..count {
                    let angle = std::f32::consts::TAU * step as f32 / count as f32;
                    offsets.push((angle.cos() * radius, angle.sin() * radius));
                }
            }
            let alpha = 1.4 / offsets.len() as f32;
            for (dx, dy) in offsets {
                widgets::text(
                    ui,
                    Pos2::new(pos.x + dx, pos.y + dy),
                    &shown,
                    mono.clone(),
                    theme::alpha(color, alpha),
                    16.5,
                );
            }
        }
        if response.clicked() {
            if revealed {
                self.discovery.revealed.remove(key);
            } else {
                self.discovery.revealed.insert(key.to_owned());
            }
        }
        width
    }
}

fn paint_card(ui: &Ui, card: Rect) {
    ui.painter().rect(
        card,
        CornerRadius::same(14),
        card_fill(),
        Stroke::new(1.0, card_border()),
        StrokeKind::Inside,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn git_fetch_interval_resolves_like_upstream() {
        assert_eq!(git_fetch_seconds(&json!({})), (30, 30));
        assert_eq!(
            git_fetch_seconds(&json!({"automaticGitFetchInterval": 60_000})),
            (60, 30)
        );
        let custom = json!({"backgroundActivity": {"profile": "custom", "baseProfile": "performance",
            "overrides": {"automaticGitFetchInterval": 45_000}}});
        assert_eq!(git_fetch_seconds(&custom), (45, 15));
        assert_eq!(
            git_fetch_seconds(&json!({"backgroundActivity": {"profile": "battery-saver"}})),
            (0, 0)
        );
        let patch = git_fetch_patch(&custom["backgroundActivity"], None);
        assert_eq!(
            patch,
            json!({"backgroundActivity": {"schemaVersion": 1, "profile": "custom",
                "baseProfile": "performance", "overrides": {}}})
        );
    }

    #[test]
    fn placeholders_keep_separators_and_length() {
        let placeholder = redacted_placeholder("jane.doe@example.com");
        assert_eq!(placeholder.len(), "jane.doe@example.com".len());
        assert_eq!(placeholder.chars().nth(4), Some('.'));
        assert_eq!(placeholder.chars().nth(8), Some('@'));
        assert_eq!(placeholder, redacted_placeholder("jane.doe@example.com"));
        assert_ne!(placeholder, "jane.doe@example.com");
    }

    #[test]
    fn summaries_follow_status_and_auth() {
        let missing = json!({"label": "GitLab", "status": "missing", "installHint": "Install glab.", "auth": {"status": "unknown"}});
        assert_eq!(
            summary(&missing).as_deref(),
            Some("Not available on this server: Install glab.")
        );
        let api = json!({"label": "Bitbucket", "status": "available", "installHint": "Add a token.", "auth": {"status": "unauthenticated"}});
        assert_eq!(summary(&api).as_deref(), Some("Available. Add a token."));
        let jj = json!({"label": "Jujutsu", "kind": "jj", "implemented": false, "status": "missing", "installHint": ""});
        assert_eq!(
            summary(&jj).as_deref(),
            Some("Support for Jujutsu is coming soon.")
        );
        let signed_in = json!({"status": "available", "auth": {"status": "authenticated"}});
        assert_eq!(summary(&signed_in), None);
    }
}
