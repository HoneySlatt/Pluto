//! Settings → Appearance, Typography only (`TypographySection` and the font
//! rows of `settings/SettingsPanels.tsx`, `SettingsFontPreviews.tsx`,
//! `FontFamilyPicker.tsx`, applied like `appearanceFonts.ts`): the simple
//! view (interface and monospace fonts) or, with Advanced, the interface,
//! prompt, code and terminal fonts, then word wrap. Each row picks an
//! installed family (`fc-list`; code surfaces list monospace families only)
//! and a size, with live previews: the prompt text and its chips, the diff
//! panel's rendering of a small patch, and the terminal's rendering of a dev
//! server transcript.
//!
//! The interface size zooms the whole client (upstream scales every rem);
//! prompt, code and terminal sizes stay absolute. Chosen families are loaded
//! through fontconfig by weight and put in front of Geist, which keeps
//! covering missing glyphs. The Interface rows are in `settings.rs`; the
//! theme library, contrast, glass and motion rows are not ported,
//! the family list draws every name in the interface font (upstream draws
//! each in its own face), the prompt preview is not editable, and the
//! Advanced switch lives in memory (upstream: local storage).

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use eframe::egui::{self, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};
use serde_json::{Value, json};

use crate::app::App;
use crate::theme::{self, CustomFonts, TypeSizes, Weight, font};
use crate::ui::md_links;
#[cfg(test)]
use crate::ui::settings::default_client_settings;
use crate::ui::settings::{
    Control, Row, RowAction, Store, card_border, card_fill, row_divider, switch,
};
use crate::ui::widgets::{self, MenuEntry, MenuItem};

pub(crate) const APPEARANCE_PAGE: usize = 1;
const CARD_WIDTH: f32 = 848.0;
const FAMILY_WIDTH: f32 = 176.0; // `sm:w-44`
const SIZE_WIDTH: f32 = 144.0;
const PICKER_MENU: &str = "typography-family";

/// The defaults of the typography client settings.
pub(crate) fn defaults() -> [(&'static str, Value); 9] {
    [
        ("fontFamilySans", json!("")),
        ("fontSizeInterface", json!(16)),
        ("fontFamilyComposer", json!("")),
        ("fontSizePrompt", json!(14)),
        ("fontFamilyCode", json!("")),
        ("fontSizeCode", json!(13)),
        ("fontFamilyTerminal", json!("")),
        ("fontSizeTerminal", json!(12)),
        ("wordWrap", json!(true)),
    ]
}

/// One font row (`FontFamilySettingsRow`).
struct FontRow {
    id: &'static str,
    title: &'static str,
    description: &'static str,
    family_key: &'static str,
    size_key: &'static str,
    range: (i64, i64),
    monospace: bool,
    preview: Preview,
}

#[derive(Clone, Copy, PartialEq)]
enum Preview {
    None,
    Prompt,
    Code,
    Terminal,
    CodeAndTerminal,
}

/// The rows of the simple or the Advanced view.
fn font_rows(advanced: bool) -> Vec<FontRow> {
    let interface = |preview| FontRow {
        id: "interface-font",
        title: "Interface font",
        description: "Everything outside code blocks and the terminal.",
        family_key: "fontFamilySans",
        size_key: "fontSizeInterface",
        range: (12, 20),
        monospace: false,
        preview,
    };
    let code = |title, description, preview| FontRow {
        id: "code-font",
        title,
        description,
        family_key: "fontFamilyCode",
        size_key: "fontSizeCode",
        range: (10, 18),
        monospace: true,
        preview,
    };
    if !advanced {
        return vec![
            interface(Preview::Prompt),
            code(
                "Monospace font",
                "Code blocks, diffs, file previews, and the terminal.",
                Preview::CodeAndTerminal,
            ),
        ];
    }
    vec![
        interface(Preview::None),
        FontRow {
            id: "prompt-font",
            title: "Prompt font",
            description: "Only the box you write prompts in. Mono works well here.",
            family_key: "fontFamilyComposer",
            size_key: "fontSizePrompt",
            range: (12, 20),
            monospace: false,
            preview: Preview::Prompt,
        },
        code(
            "Code font",
            "Code blocks, diffs, and file previews.",
            Preview::Code,
        ),
        FontRow {
            id: "terminal-font",
            title: "Terminal font",
            description: "Terminal output, independent from code blocks and diffs.",
            family_key: "fontFamilyTerminal",
            size_key: "fontSizeTerminal",
            range: (8, 20),
            monospace: true,
            preview: Preview::Terminal,
        },
    ]
}

/// A face fontconfig resolves: its family names, file and spacing.
struct FontMatch {
    families: Vec<String>,
    file: PathBuf,
    monospace: bool,
}

/// `fc-match` for `family` at a fontconfig weight (80 regular, 100 medium,
/// 180 semibold, 200 bold).
fn fc_match(family: &str, weight: u32) -> Option<FontMatch> {
    let escaped: String = family
        .chars()
        .flat_map(|c| match c {
            '-' | ':' | ',' | '\\' => vec!['\\', c],
            c => vec![c],
        })
        .collect();
    let output = std::process::Command::new("fc-match")
        .args([
            "-f",
            "%{family}\u{1f}%{file}\u{1f}%{spacing}",
            &format!("{escaped}:weight={weight}"),
        ])
        .output()
        .ok()?;
    let text = String::from_utf8(output.stdout).ok()?;
    let mut parts = text.split('\u{1f}');
    let families = parts
        .next()?
        .split(',')
        .map(|f| f.trim().to_owned())
        .collect();
    let file = PathBuf::from(parts.next()?);
    let monospace = parts
        .next()
        .is_some_and(|s| s.trim() == "100" || s.trim() == "90");
    Some(FontMatch {
        families,
        file,
        monospace,
    })
}

/// The installed face of a family preference (a name or a comma list, the
/// first installed one winning) at `weight`; None for the default font.
/// fontconfig falls back to another family for unknown names, so the match
/// must name the family asked for.
fn resolve_font(preference: &str, weight: u32, monospace: bool) -> Option<Arc<Vec<u8>>> {
    preference
        .split(',')
        .map(|name| name.trim().trim_matches(['"', '\'']))
        .filter(|name| !name.is_empty())
        .find_map(|name| {
            let found = fc_match(name, weight)?;
            let named = found.families.iter().any(|f| f.eq_ignore_ascii_case(name));
            (named && (!monospace || found.monospace))
                .then(|| std::fs::read(&found.file).ok())
                .flatten()
                .map(Arc::new)
        })
}

/// Lists the installed families off the UI thread, so the Appearance
/// page opens without waiting on `fc-list`.
pub(crate) fn prefetch_families() {
    static STARTED: std::sync::Once = std::sync::Once::new();
    STARTED.call_once(|| {
        std::thread::spawn(|| {
            installed_families(false);
            installed_families(true);
        });
    });
}

/// Installed families (`fc-list`), sorted; monospace ones only for code.
fn installed_families(monospace: bool) -> Arc<Vec<String>> {
    static ALL: OnceLock<Arc<Vec<String>>> = OnceLock::new();
    static MONO: OnceLock<Arc<Vec<String>>> = OnceLock::new();
    let cell = if monospace { &MONO } else { &ALL };
    cell.get_or_init(|| {
        let mut args = vec!["--format", "%{family[0]}\n"];
        if monospace {
            args.insert(0, ":spacing=100");
        }
        let output = std::process::Command::new("fc-list").args(&args).output();
        let mut families: Vec<String> = output
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .unwrap_or_default()
            .lines()
            .map(str::trim)
            .filter(|f| !f.is_empty() && !f.starts_with('.'))
            .map(str::to_owned)
            .collect();
        families.sort_by_key(|f| f.to_lowercase());
        families.dedup();
        Arc::new(families)
    })
    .clone()
}

/// What the typography settings resolve to, to notice changes.
#[derive(Clone, PartialEq)]
struct Applied {
    families: [String; 4],
    sizes: TypeSizes,
}

/// Typography state: the Advanced switch, the applied settings, the
/// family picker's search.
#[derive(Default)]
pub(crate) struct Typography {
    pub(crate) advanced: bool,
    applied: Option<Applied>,
    query: String,
}

impl App {
    fn type_setting(&self, key: &str) -> Value {
        self.client_setting(key)
    }

    fn type_size(&self, key: &str, range: (i64, i64), default: i64) -> f32 {
        self.type_setting(key)
            .as_i64()
            .unwrap_or(default)
            .clamp(range.0, range.1) as f32
    }

    /// `applyAppearanceFontVariables`: the zoom, the sizes and the fonts,
    /// applied when the settings change. Outside Advanced the terminal
    /// follows the code font (`resolveTerminalFontPreference`).
    pub(crate) fn apply_typography(&mut self, ctx: &egui::Context) {
        let advanced = self.typography.advanced;
        let text = |key: &str| {
            self.type_setting(key)
                .as_str()
                .unwrap_or_default()
                .trim()
                .to_owned()
        };
        let code_size = self.type_size("fontSizeCode", (10, 18), 13);
        let applied = Applied {
            families: [
                text("fontFamilySans"),
                text("fontFamilyComposer"),
                text("fontFamilyCode"),
                if advanced {
                    text("fontFamilyTerminal")
                } else {
                    text("fontFamilyCode")
                },
            ],
            sizes: TypeSizes {
                code: code_size,
                prompt: self.type_size("fontSizePrompt", (12, 20), 14),
                terminal: if advanced {
                    self.type_size("fontSizeTerminal", (8, 20), 12)
                } else {
                    code_size
                },
                zoom: self.type_size("fontSizeInterface", (12, 20), 16) / 16.0,
            },
        };
        if self.typography.applied.as_ref() == Some(&applied) {
            return;
        }
        let previous = self.typography.applied.replace(applied.clone());
        theme::set_type_sizes(applied.sizes);
        if previous.as_ref().map(|p| p.sizes.zoom) != Some(applied.sizes.zoom) {
            ctx.set_zoom_factor(applied.sizes.zoom);
        }
        if previous.as_ref().map(|p| &p.families) != Some(&applied.families) {
            let [sans, composer, code, terminal] = &applied.families;
            let custom = CustomFonts {
                sans: [80, 100, 180, 200].map(|w| resolve_font(sans, w, false)),
                composer: resolve_font(composer, 80, false),
                code: [80, 200].map(|w| resolve_font(code, w, true)),
                terminal: [80, 200].map(|w| resolve_font(terminal, w, true)),
            };
            theme::install_fonts(ctx, &custom);
        }
    }

    /// The Appearance page: the Typography section; returns its bottom.
    pub(crate) fn appearance_page(&mut self, ui: &mut Ui, left: f32, top: f32) -> f32 {
        let advanced = self.typography.advanced;
        // `SettingsSection` header with its Advanced switch.
        widgets::text_in(
            ui,
            Rect::from_min_size(Pos2::new(left + 16.0, top), Vec2::new(CARD_WIDTH, 28.0)),
            "Typography",
            font(14.0, Weight::Regular),
            theme::alpha(theme::foreground(), 0.7),
        );
        let card_right = left + CARD_WIDTH;
        if switch(
            ui,
            card_right - 16.0,
            top + 14.0,
            advanced,
            "typography-advanced",
        ) {
            self.typography.advanced = !advanced;
        }
        let label_width = widgets::text_width(ui, "Advanced", font(12.0, Weight::Medium));
        widgets::text_in(
            ui,
            Rect::from_min_size(
                Pos2::new(card_right - 16.0 - 32.0 - 8.0 - label_width, top),
                Vec2::new(label_width + 1.0, 28.0),
            ),
            "Advanced",
            font(12.0, Weight::Medium),
            theme::muted_foreground(),
        );
        let card_top = top + 28.0 + 10.0;

        // Row heights: the row itself, then its preview (`pt-3 pb-1`).
        let rows = font_rows(advanced);
        let width = CARD_WIDTH - 2.0 * 16.0;
        let previews: Vec<f32> = rows
            .iter()
            .map(|row| self.preview_height(ui, row, width))
            .collect();
        let base = 12.0 + 20.0 + 4.0 + 18.0 + 12.0;
        let heights: Vec<f32> = rows
            .iter()
            .zip(&previews)
            .map(|(_, preview)| {
                if *preview > 0.0 {
                    base - 8.0 + 4.0 + preview + 8.0 + 4.0
                } else {
                    base
                }
            })
            .chain(std::iter::once(base))
            .collect();
        let card = Rect::from_min_size(
            Pos2::new(left, card_top),
            Vec2::new(CARD_WIDTH, heights.iter().sum::<f32>() + 2.0),
        );
        ui.painter().rect(
            card,
            CornerRadius::same(14),
            card_fill(),
            Stroke::new(1.0, card_border()),
            StrokeKind::Inside,
        );
        let mut y = card.top() + 1.0;
        let mut writes: Vec<(String, Value)> = Vec::new();
        for (index, (row, height)) in rows.iter().zip(&heights).enumerate() {
            if index > 0 {
                ui.painter().rect_filled(
                    Rect::from_min_size(
                        Pos2::new(card.left() + 1.0, y),
                        Vec2::new(CARD_WIDTH - 2.0, 1.0),
                    ),
                    0.0,
                    row_divider(),
                );
            }
            self.font_row(
                ui,
                Rect::from_min_size(
                    Pos2::new(card.left() + 1.0, y),
                    Vec2::new(
                        CARD_WIDTH - 2.0,
                        base - if previews[index] > 0.0 { 8.0 } else { 0.0 },
                    ),
                ),
                row,
                index > 0,
                &mut writes,
            );
            if previews[index] > 0.0 {
                let preview = Rect::from_min_size(
                    Pos2::new(card.left() + 16.0, y + base - 8.0 + 4.0),
                    Vec2::new(width, previews[index]),
                );
                self.paint_preview(ui, preview, row);
            }
            y += height;
        }

        // `WordWrapRow`.
        let wrap = self.type_setting("wordWrap") != false;
        let wrap_row = Row {
            id: "word-wrap".into(),
            title: "Word wrap".into(),
            description:
                "Wrap long lines in code blocks, tables, diffs, and file previews by default."
                    .into(),
            status: None,
            layers: false,
            reset: !wrap,
            control: Control::Switch(wrap),
        };
        ui.painter().rect_filled(
            Rect::from_min_size(
                Pos2::new(card.left() + 1.0, y),
                Vec2::new(CARD_WIDTH - 2.0, 1.0),
            ),
            0.0,
            row_divider(),
        );
        let wrap_rect = Rect::from_min_size(
            Pos2::new(card.left() + 1.0, y),
            Vec2::new(CARD_WIDTH - 2.0, base),
        );
        match self.settings_row(ui, wrap_rect, &wrap_row, true) {
            Some(RowAction::Toggle(on)) => writes.push(("wordWrap".into(), json!(on))),
            Some(RowAction::Reset) => writes.push(("wordWrap".into(), json!(true))),
            _ => {}
        }
        for (key, value) in writes {
            self.update_setting(Store::Client, &key, value);
        }
        card.bottom()
    }

    /// A font row: title, description and reset, then the family picker and
    /// the size select on the right.
    fn font_row(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        row: &FontRow,
        divided: bool,
        writes: &mut Vec<(String, Value)>,
    ) {
        let default_size = defaults()
            .into_iter()
            .find(|(k, _)| *k == row.size_key)
            .and_then(|(_, v)| v.as_i64())
            .unwrap_or(14);
        let family = self
            .type_setting(row.family_key)
            .as_str()
            .unwrap_or_default()
            .to_owned();
        let size = self.type_size(row.size_key, row.range, default_size) as i64;
        let settings_row = Row {
            id: row.id.into(),
            title: row.title.into(),
            description: row.description.into(),
            status: None,
            layers: false,
            reset: !family.is_empty() || size != default_size,
            control: Control::None,
        };
        if let Some(RowAction::Reset) = self.settings_row(ui, rect, &settings_row, divided) {
            writes.push((row.family_key.into(), json!("")));
            writes.push((row.size_key.into(), json!(default_size)));
        }
        // The controls sit centered on the title and description.
        let center_y = rect.top() + if divided { 1.0 } else { 0.0 } + 12.0 + 21.0;
        let right = rect.right() - 16.0;
        let size_rect = Rect::from_min_size(
            Pos2::new(right - SIZE_WIDTH, center_y - 14.0),
            Vec2::new(SIZE_WIDTH, 28.0),
        );
        let family_rect = Rect::from_min_size(
            Pos2::new(size_rect.left() - 8.0 - FAMILY_WIDTH, center_y - 14.0),
            Vec2::new(FAMILY_WIDTH, 28.0),
        );
        let default_label = if row.monospace { "Geist Mono" } else { "Geist" };
        // The prompt defaults to whatever the interface font resolves to.
        let default_label = if row.family_key == "fontFamilyComposer" {
            let sans = self
                .type_setting("fontFamilySans")
                .as_str()
                .unwrap_or_default()
                .trim()
                .to_owned();
            if sans.is_empty() {
                default_label.to_owned()
            } else {
                sans
            }
        } else {
            default_label.to_owned()
        };
        let label = if family.trim().is_empty() {
            default_label.clone()
        } else {
            family.clone()
        };
        let menu_id = format!("{PICKER_MENU}:{}", row.id);
        if select_trigger(
            ui,
            family_rect,
            ("family", row.id),
            &label,
            self.open_menu.as_deref() == Some(menu_id.as_str()),
        ) {
            self.typography.query.clear();
            self.open_menu = if self.open_menu.as_deref() == Some(menu_id.as_str()) {
                None
            } else {
                Some(menu_id.clone())
            };
        }
        if let Some(choice) = self.family_picker(
            ui,
            family_rect,
            &menu_id,
            row.monospace,
            &family,
            &default_label,
        ) {
            writes.push((row.family_key.into(), json!(choice)));
        }
        // The size select (`SelectTrigger size="sm"`).
        let size_menu = format!("typography-size:{}", row.id);
        if select_trigger(
            ui,
            size_rect,
            ("size", row.id),
            &format!("{size} px"),
            self.open_menu.as_deref() == Some(size_menu.as_str()),
        ) {
            self.open_menu = if self.open_menu.as_deref() == Some(size_menu.as_str()) {
                None
            } else {
                Some(size_menu.clone())
            };
        }
        let labels: Vec<String> = (row.range.0..=row.range.1)
            .map(|px| format!("{px} px"))
            .collect();
        let entries: Vec<MenuEntry> = labels
            .iter()
            .enumerate()
            .map(|(i, label)| {
                MenuEntry::Item(MenuItem {
                    selected: row.range.0 + i as i64 == size,
                    ..MenuItem::new(label)
                })
            })
            .collect();
        if let Some(index) = widgets::menu_aligned(
            ui,
            &mut self.open_menu,
            &size_menu,
            size_rect,
            false,
            true,
            &entries,
        ) {
            writes.push((row.size_key.into(), json!(row.range.0 + index as i64)));
        }
    }

    /// `FontFamilyPicker`: a searchable list of the installed families, the
    /// default first; returns the chosen family ("" for the default).
    fn family_picker(
        &mut self,
        ui: &mut Ui,
        anchor: Rect,
        menu_id: &str,
        monospace: bool,
        selected: &str,
        default_label: &str,
    ) -> Option<String> {
        if self.open_menu.as_deref() != Some(menu_id) {
            return None;
        }
        let families = installed_families(monospace);
        let query = self.typography.query.trim().to_lowercase();
        let mut items: Vec<Option<&String>> = Vec::new();
        if query.is_empty() {
            items.push(None);
        }
        items.extend(
            families
                .iter()
                .filter(|f| query.is_empty() || f.to_lowercase().contains(&query))
                .map(Some),
        );
        let width = 260.0;
        let list_height = (items.len().max(1) as f32 * 32.0).min(288.0);
        let rect = Rect::from_min_size(
            Pos2::new(anchor.right() - width, anchor.bottom() + 4.0),
            Vec2::new(width, 8.0 + 36.0 + list_height + 8.0),
        );
        let mut picked = None;
        let mut open = self.open_menu.clone();
        let mut query_text = std::mem::take(&mut self.typography.query);
        widgets::popup(ui, &mut open, menu_id, rect, anchor, |ui, inner| {
            let search = Rect::from_min_size(
                inner.min + Vec2::new(12.0, 4.0),
                Vec2::new(inner.width() - 24.0, 36.0),
            );
            widgets::combobox_search(ui, search, &mut query_text, "Search fonts…").request_focus();
            let list = Rect::from_min_max(
                Pos2::new(inner.left() + 4.0, search.bottom() + 4.0),
                Pos2::new(inner.right() - 4.0, inner.bottom() - 4.0),
            );
            let mut list_ui = ui.new_child(egui::UiBuilder::new().max_rect(list));
            egui::ScrollArea::vertical()
                .id_salt(("font-list", menu_id))
                .show(&mut list_ui, |ui| {
                    if items.is_empty() {
                        widgets::text_in(
                            ui,
                            Rect::from_min_size(
                                list.min + Vec2::new(8.0, 0.0),
                                Vec2::new(list.width(), 32.0),
                            ),
                            "No fonts found.",
                            font(14.0, Weight::Regular),
                            theme::muted_foreground(),
                        );
                    }
                    for item in &items {
                        let (row, response) =
                            ui.allocate_exact_size(Vec2::new(list.width(), 32.0), Sense::click());
                        if response.hovered() {
                            ui.painter()
                                .rect_filled(row, CornerRadius::same(6), theme::accent());
                        }
                        let name = item.map_or(default_label, |f| f.as_str());
                        widgets::text_in(
                            ui,
                            Rect::from_min_max(
                                Pos2::new(row.left() + 8.0, row.top()),
                                Pos2::new(row.right() - 64.0, row.bottom()),
                            ),
                            name,
                            font(14.0, Weight::Regular),
                            theme::foreground(),
                        );
                        let mut x = row.right() - 8.0;
                        let is_selected = match item {
                            None => selected.trim().is_empty(),
                            Some(f) => f.eq_ignore_ascii_case(selected.trim()),
                        };
                        if is_selected {
                            theme::paint_icon(
                                ui,
                                "check",
                                Rect::from_min_size(
                                    Pos2::new(x - 14.0, row.center().y - 7.0),
                                    Vec2::splat(14.0),
                                ),
                                14.0,
                                theme::muted_foreground(),
                            );
                            x -= 14.0 + 6.0;
                        }
                        if item.is_none() {
                            let w = widgets::text_width(ui, "default", font(10.0, Weight::Regular));
                            widgets::text_in(
                                ui,
                                Rect::from_min_size(
                                    Pos2::new(x - w, row.top()),
                                    Vec2::new(w + 1.0, 32.0),
                                ),
                                "default",
                                font(10.0, Weight::Regular),
                                theme::alpha(theme::muted_foreground(), 0.6),
                            );
                        }
                        if response.clicked() {
                            picked = Some(item.map_or(String::new(), |f| f.clone()));
                        }
                    }
                });
        });
        self.typography.query = query_text;
        self.open_menu = open;
        if picked.is_some() {
            self.open_menu = None;
        }
        picked
    }

    fn preview_height(&self, ui: &Ui, row: &FontRow, width: f32) -> f32 {
        match row.preview {
            Preview::None => 0.0,
            Preview::Prompt => prompt_preview_height(ui, width),
            Preview::Code => code_preview_height(),
            Preview::Terminal => 208.0,
            Preview::CodeAndTerminal => code_preview_height() + 8.0 + 208.0,
        }
    }

    fn paint_preview(&mut self, ui: &mut Ui, rect: Rect, row: &FontRow) {
        match row.preview {
            Preview::None => {}
            Preview::Prompt => paint_prompt_preview(ui, rect),
            Preview::Code => self.paint_code_preview(ui, rect),
            Preview::Terminal => paint_terminal_preview(ui, rect),
            Preview::CodeAndTerminal => {
                let code =
                    Rect::from_min_size(rect.min, Vec2::new(rect.width(), code_preview_height()));
                self.paint_code_preview(ui, code);
                paint_terminal_preview(
                    ui,
                    Rect::from_min_max(Pos2::new(rect.left(), code.bottom() + 8.0), rect.max),
                );
            }
        }
    }

    /// `CodeFontPreview`: the diff panel's rendering of a small patch,
    /// unified like upstream's preview.
    fn paint_code_preview(&mut self, ui: &mut Ui, rect: Rect) {
        let files = crate::diff::parse(DIFF_PREVIEW_PATCH);
        let Some(file) = files.first() else {
            return;
        };
        ui.painter()
            .rect_filled(rect, CornerRadius::same(8), theme::control_surface());
        let split = std::mem::replace(&mut self.diff_split, false);
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(rect)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        child.set_clip_rect(rect.intersect(ui.clip_rect()));
        self.diff_file_header(&mut child, file, false, rect.width());
        self.diff_file_body(&mut child, file, rect.width(), None);
        self.diff_split = split;
    }
}

const DIFF_PREVIEW_PATCH: &str = "diff --git a/src/formatUser.ts b/src/formatUser.ts\n--- a/src/formatUser.ts\n+++ b/src/formatUser.ts\n@@ -1,3 +1,3 @@\n export function formatUser(user: User) {\n-  return user.name.toUpperCase();\n+  return `${user.name} <${user.email}>`; // 0O 1lI\n }\n";

fn code_preview_height() -> f32 {
    // The file header, then the four rows.
    32.0 + 4.0 * theme::code_line(20.0) + 8.0
}

/// A select-style trigger (`SelectButton size="sm"`); returns whether it
/// was clicked.
fn select_trigger(
    ui: &mut Ui,
    rect: Rect,
    id: impl egui::AsIdSalt,
    label: &str,
    open: bool,
) -> bool {
    let response = ui.interact(rect, ui.id().with(id), Sense::click());
    ui.painter().rect(
        rect,
        CornerRadius::same(10),
        if response.hovered() || open {
            theme::overlay(0.05)
        } else {
            theme::control_surface()
        },
        Stroke::new(1.0, theme::input()),
        StrokeKind::Inside,
    );
    let galley = widgets::galley_truncated(
        ui,
        label,
        font(14.0, Weight::Regular),
        theme::foreground(),
        20.0,
        rect.width() - 34.0,
    );
    ui.painter().galley(
        Pos2::new(rect.left() + 10.0, rect.center().y - 10.0),
        galley,
        theme::foreground(),
    );
    theme::paint_icon(
        ui,
        "chevron-down",
        Rect::from_min_size(
            Pos2::new(rect.right() - 18.0, rect.top()),
            Vec2::new(12.0, rect.height()),
        ),
        12.0,
        theme::muted_foreground(),
    );
    response.clicked()
}

/// The prompt preview's segments: text and chips (`PROMPT_PREVIEW_TEXT`).
enum Segment {
    Text(&'static str),
    /// A chip: its label, icon and accent (chroma, hue).
    Chip(&'static str, (&'static str, Color32), (f32, f32)),
}

fn prompt_segments() -> Vec<Segment> {
    let skill = (0.16, 322.0);
    let mention = (0.11, 215.0);
    vec![
        Segment::Text("Use "),
        Segment::Chip("Frontend Design", ("box", Color32::PLACEHOLDER), skill),
        Segment::Text(" to fix the flaky test in "),
        Segment::Chip(
            "surface.test.ts",
            crate::ui::files::file_icon("surface.test.ts"),
            mention,
        ),
        Segment::Text(" and align the header with "),
        Segment::Chip(
            "SettingsPanels.tsx",
            crate::ui::files::file_icon("SettingsPanels.tsx"),
            mention,
        ),
        Segment::Text(" before shipping."),
    ]
}

/// The preview text laid out in the prompt font, chips holding room before
/// their labels; with each chip's label character range.
fn layout_prompt(ui: &Ui, width: f32) -> (Arc<egui::Galley>, Vec<(usize, usize)>) {
    let prompt = theme::composer_font();
    let chip_size = prompt.size * 0.86;
    let chip_font = egui::FontId::new(chip_size, prompt.family.clone());
    let line = theme::composer_line();
    let mut job = egui::text::LayoutJob::default();
    let mut ranges = Vec::new();
    let mut chars = 0usize;
    for segment in prompt_segments() {
        match segment {
            Segment::Text(text) => {
                job.append(
                    text,
                    0.0,
                    widgets::format(prompt.clone(), theme::foreground(), line),
                );
                chars += text.chars().count();
            }
            Segment::Chip(label, _, _) => {
                let label = label.replace(' ', "\u{a0}");
                let lead = 1.0 + (0.5 + 1.17 + 0.33) * chip_size;
                job.append(
                    &label,
                    lead,
                    widgets::format(chip_font.clone(), theme::foreground(), line),
                );
                let start = chars;
                chars += label.chars().count();
                ranges.push((start, chars));
                // The chip's right padding and border.
                let mut pad = widgets::format(chip_font.clone(), Color32::TRANSPARENT, line);
                let nbsp = ui.fonts_mut(|f| f.glyph_width(&chip_font, '\u{a0}'));
                pad.extra_letter_spacing = 0.5 * chip_size + 1.0 - nbsp;
                job.append("\u{a0}", 0.0, pad);
                chars += 1;
            }
        }
    }
    job.wrap.max_width = width - 24.0 - 2.0;
    (
        widgets::center_rows(ui.fonts_mut(|f| f.layout_job(job))),
        ranges,
    )
}

fn prompt_preview_height(ui: &Ui, width: f32) -> f32 {
    let (galley, _) = layout_prompt(ui, width);
    // `px-3 py-2`, a 1px border, `min-h-14` for the editor.
    galley.size().y.max(56.0) + 16.0 + 2.0
}

/// `PromptFontPreview`: the prompt text and its chips in the prompt font.
fn paint_prompt_preview(ui: &Ui, rect: Rect) {
    ui.painter().rect(
        rect,
        CornerRadius::same(8),
        theme::background(),
        Stroke::new(1.0, theme::border()),
        StrokeKind::Inside,
    );
    let (galley, ranges) = layout_prompt(ui, rect.width());
    let origin = rect.min + Vec2::new(13.0, 9.0);
    let chip_size = theme::composer_font().size * 0.86;
    let lead = 1.0 + (0.5 + 1.17 + 0.33) * chip_size;
    let chip_font = egui::FontId::new(chip_size, theme::composer_font().family);
    let nbsp = ui.fonts_mut(|f| f.glyph_width(&chip_font, '\u{a0}'));
    let chips: Vec<_> = prompt_segments()
        .into_iter()
        .filter_map(|s| match s {
            Segment::Chip(_, icon, accent) => Some((icon, accent)),
            Segment::Text(_) => None,
        })
        .collect();
    for ((start, end), (icon, (chroma, hue))) in ranges.iter().zip(chips) {
        let first = galley.pos_from_cursor(egui::text::CCursor::new(*start));
        let last = galley.pos_from_cursor(egui::text::CCursor::new(*end));
        let height = (1.41 * chip_size).round();
        let center = origin.y + (first.top() + first.bottom()) / 2.0;
        let chip = Rect::from_min_max(
            Pos2::new(
                (origin.x + first.left() - lead).round(),
                (center - height / 2.0).round(),
            ),
            Pos2::new(
                // The right padding precedes the spacer glyph (letter
                // spacing); the glyph itself closes the chip.
                (origin.x + last.left() + nbsp).round(),
                (center - height / 2.0).round() + height,
            ),
        );
        let [border, fill, text] =
            md_links::chip_colors(chroma, hue, theme::border(), theme::foreground());
        let radius = CornerRadius::same((0.5 * chip_size).round() as u8);
        ui.painter().rect_filled(chip, radius, fill);
        ui.painter()
            .rect_stroke(chip, radius, Stroke::new(1.0, border), StrokeKind::Inside);
        let icon_size = (1.17 * chip_size).round();
        let tint = if icon.1 == Color32::PLACEHOLDER {
            text
        } else {
            icon.1
        };
        theme::paint_icon(
            ui,
            icon.0,
            Rect::from_min_size(
                Pos2::new(
                    chip.left() + 1.0 + (0.5 * chip_size).round(),
                    chip.center().y - icon_size / 2.0,
                ),
                Vec2::splat(icon_size),
            ),
            icon_size,
            tint,
        );
    }
    // Chip labels take their chip's text color.
    let mut colored = (*galley.job).clone();
    let mut chip_index = 0;
    for section in colored.sections.iter_mut() {
        if section.leading_space > 0.0 {
            let chip = prompt_segments()
                .into_iter()
                .filter_map(|s| match s {
                    Segment::Chip(_, _, accent) => Some(accent),
                    Segment::Text(_) => None,
                })
                .nth(chip_index);
            if let Some((chroma, hue)) = chip {
                section.format.color =
                    md_links::chip_colors(chroma, hue, theme::border(), theme::foreground())[2];
            }
            chip_index += 1;
        }
    }
    let galley = widgets::center_rows(ui.fonts_mut(|f| f.layout_job(colored)));
    ui.painter().galley(origin, galley, theme::foreground());
}

// `TERMINAL_PREVIEW_TRANSCRIPT`: a prompt, a dev server's startup, a test
// summary and a READY badge.
const TERMINAL_PROMPT: &str = "\x1b[1;32m→\x1b[0m \x1b[1;36mt3code\x1b[0m \x1b[1;34mgit:(\x1b[1;31mmain\x1b[1;34m)\x1b[0m \x1b[1;33m✗\x1b[0m ";

fn terminal_transcript() -> String {
    format!(
        "{TERMINAL_PROMPT}vpr dev\r\n\r\n  \x1b[1;32mVITE\x1b[0m \x1b[32mv7.1.1\x1b[0m  \x1b[2mready in\x1b[0m \x1b[1m1.24s\x1b[0m\r\n\r\n  \x1b[32m→\x1b[0m  \x1b[2mLocal:\x1b[0m    \x1b[4;36mhttp://127.0.0.1:5173/\x1b[0m\r\n  \x1b[32m→\x1b[0m  \x1b[2mNetwork:\x1b[0m  \x1b[4;36mhttp://192.168.1.24:5173/\x1b[0m\r\n\r\n  \x1b[32m✓ 85 passed\x1b[0m   \x1b[33m△ 2 warnings\x1b[0m   \x1b[31m✗ 0 failed\x1b[0m\r\n\r\n  \x1b[42;30m READY \x1b[0m \x1b[2mwatching for changes — press\x1b[0m \x1b[1mq\x1b[0m \x1b[2mto quit\x1b[0m\r\n\r\n{TERMINAL_PROMPT}"
    )
}

/// `TerminalFontPreview`: the terminal's rendering of the transcript, its
/// cursor outlined (unfocused).
fn paint_terminal_preview(ui: &Ui, rect: Rect) {
    ui.painter().rect(
        rect,
        CornerRadius::same(8),
        theme::background(),
        Stroke::new(1.0, theme::border()),
        StrokeKind::Inside,
    );
    let mono = theme::terminal_font(false);
    let cell_width = widgets::text_width(ui, "M", mono).max(1.0);
    let cell_height = theme::terminal_cell_height();
    let padding = 4.0;
    let inner = rect.shrink(1.0 + padding);
    let cols = ((inner.width() / cell_width).floor() as usize).max(1);
    let rows = ((inner.height() / cell_height).floor() as usize).max(1);
    let mut terminal = crate::terminal::Terminal::new(cols, rows);
    terminal.feed(&terminal_transcript());
    let screen = &terminal.screen;
    let painter = ui
        .painter()
        .with_clip_rect(rect.shrink(1.0).intersect(ui.clip_rect()));
    let lines = screen.view(0);
    crate::ui::terminal_surface::paint_terminal_lines(
        ui,
        &painter,
        inner.min,
        &lines,
        cell_width,
        |_, _| false,
    );
    let cursor = Rect::from_min_size(
        Pos2::new(
            inner.left() + screen.cursor().1 as f32 * cell_width,
            inner.top() + screen.cursor().0 as f32 * cell_height,
        ),
        Vec2::new(cell_width, cell_height),
    );
    painter.rect_stroke(
        cursor.shrink(0.5),
        0.0,
        Stroke::new(1.0, theme::foreground()),
        StrokeKind::Inside,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_follow_the_advanced_switch() {
        let simple: Vec<&str> = font_rows(false).iter().map(|r| r.title).collect();
        assert_eq!(simple, ["Interface font", "Monospace font"]);
        let advanced: Vec<&str> = font_rows(true).iter().map(|r| r.title).collect();
        assert_eq!(
            advanced,
            [
                "Interface font",
                "Prompt font",
                "Code font",
                "Terminal font"
            ]
        );
    }

    #[test]
    fn defaults_follow_upstream() {
        let settings = default_client_settings();
        for (key, value) in defaults() {
            assert_eq!(settings[key], value, "{key}");
        }
        assert_eq!(settings["fontSizeInterface"], 16);
        assert_eq!(settings["fontSizeCode"], 13);
        assert_eq!(settings["wordWrap"], true);
    }
}
