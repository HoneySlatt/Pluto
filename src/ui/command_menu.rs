//! The composer's suggestion menu (`chat/ComposerCommandMenu.tsx`, with
//! `detectComposerTrigger` from `composer-logic.ts`, the items and
//! `onSelectComposerItem` of `ChatComposer.tsx`, `composerMenuHighlight.ts`,
//! `composerSlashCommandSearch.ts`, `providerSkillSearch.ts`,
//! `providerSkills.ts` and `shared/searchRanking.ts`).
//!
//! "/" at the start of a line lists `/model`, `/plan` and `/default` (plan
//! mode on), the provider's commands (at the prompt's start) and its skills;
//! "@" searches the workspace's files and folders (`projects.searchEntries`)
//! and inserts a file link; "$" lists the provider's skills. Arrows move the
//! highlight, Enter or Tab picks, Escape closes. Not ported: "#" pull
//! request references and `/usage-limits`.
use eframe::egui::{self, CornerRadius, Key, Modifiers, Pos2, Rect, Sense, Ui, Vec2};
use serde_json::Value;

use crate::app::App;
use crate::theme::{self, Weight, font};
use crate::ui::{cite, widgets};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum TriggerKind {
    Path,
    Slash,
    Skill,
}

/// An active trigger: its kind, query and character range in the
/// composer's display text.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Trigger {
    pub kind: TriggerKind,
    pub query: String,
    pub start: usize,
    pub end: usize,
}

fn is_space(c: char) -> bool {
    matches!(c, ' ' | '\n' | '\t' | '\r') || cite::placeholder_index(c).is_some()
}

/// `\p{Sc}`: currency symbols.
fn is_currency(c: char) -> bool {
    matches!(
        c,
        '$' | '¢'
            | '£'
            | '¤'
            | '¥'
            | '֏'
            | '؋'
            | '৲'
            | '৳'
            | '฿'
            | '៛'
            | '﷼'
            | '＄'
            | '￠'
            | '￡'
            | '￥'
            | '￦'
    ) || ('\u{20A0}'..='\u{20C0}').contains(&c)
}

/// `detectComposerTrigger` (without "#" pull requests), over characters.
pub(crate) fn detect_trigger(text: &str, cursor: usize) -> Option<Trigger> {
    let chars: Vec<char> = text.chars().collect();
    let cursor = cursor.min(chars.len());
    let line_start = chars[..cursor]
        .iter()
        .rposition(|c| *c == '\n')
        .map_or(0, |i| i + 1);
    let line: String = chars[line_start..cursor].iter().collect();
    if let Some(query) = line.strip_prefix('/')
        && !query.chars().any(char::is_whitespace)
    {
        return Some(Trigger {
            kind: TriggerKind::Slash,
            query: query.to_owned(),
            start: line_start,
            end: cursor,
        });
    }
    let token_start = chars[..cursor]
        .iter()
        .rposition(|c| is_space(*c))
        .map_or(0, |i| i + 1);
    let token: String = chars[token_start..cursor].iter().collect();
    let first = token.chars().next()?;
    let kind = if is_currency(first) {
        TriggerKind::Skill
    } else if first == '@' {
        TriggerKind::Path
    } else {
        return None;
    };
    Some(Trigger {
        kind,
        query: token.chars().skip(1).collect(),
        start: token_start,
        end: cursor,
    })
}

/// `normalizeSearchQuery` with a leading pattern of `strip` characters.
fn normalize_query(query: &str, strip: impl Fn(char) -> bool) -> String {
    query.trim().trim_start_matches(strip).to_lowercase()
}

/// `scoreSubsequenceMatch`.
fn subsequence_score(value: &str, query: &str) -> Option<i64> {
    let value: Vec<char> = value.chars().collect();
    let query: Vec<char> = query.chars().collect();
    if query.is_empty() {
        return Some(0);
    }
    let (mut q, mut first, mut previous, mut gaps) = (0, None, None, 0i64);
    for (i, c) in value.iter().enumerate() {
        if *c != query[q] {
            continue;
        }
        let start = *first.get_or_insert(i);
        if let Some(p) = previous {
            gaps += (i - p - 1) as i64;
        }
        previous = Some(i);
        q += 1;
        if q == query.len() {
            let span = (i - start + 1 - query.len()) as i64;
            let length = (value.len() - query.len()).min(64) as i64;
            return Some(start as i64 * 2 + gaps * 3 + span + length);
        }
    }
    None
}

#[derive(Default)]
struct Bases {
    exact: i64,
    prefix: Option<i64>,
    boundary: Option<i64>,
    includes: Option<i64>,
    fuzzy: Option<i64>,
    markers: &'static [&'static str],
}

/// `scoreQueryMatch` (inputs already trimmed and lowercased).
fn score_match(value: &str, query: &str, bases: &Bases) -> Option<i64> {
    if value.is_empty() || query.is_empty() {
        return None;
    }
    let penalty = (value.chars().count().saturating_sub(query.chars().count())).min(64) as i64;
    if value == query {
        return Some(bases.exact);
    }
    if let Some(base) = bases.prefix
        && value.starts_with(query)
    {
        return Some(base + penalty);
    }
    if let Some(base) = bases.boundary {
        let markers: &[&str] = if bases.markers.is_empty() {
            &[" ", "-", "_", "/"]
        } else {
            bases.markers
        };
        let index = markers
            .iter()
            .filter_map(|m| {
                value
                    .find(&format!("{m}{query}"))
                    .map(|i| value[..i].chars().count() + m.chars().count())
            })
            .min();
        if let Some(index) = index {
            return Some(base + index as i64 * 2 + penalty);
        }
    }
    if let Some(base) = bases.includes
        && let Some(i) = value.find(query)
    {
        return Some(base + value[..i].chars().count() as i64 * 2 + penalty);
    }
    bases
        .fuzzy
        .and_then(|base| subsequence_score(value, query).map(|s| base + s))
}

/// `ProviderSkillSourceKind` (`resolveProviderSkillSourceKind`).
fn skill_source(skill: &Value) -> &'static str {
    let path = skill["path"]
        .as_str()
        .unwrap_or_default()
        .replace('\\', "/");
    if path.contains("/.codex/plugins/") || path.contains("/.agents/plugins/") {
        return "App";
    }
    match skill["scope"]
        .as_str()
        .map(|s| s.trim().to_lowercase())
        .as_deref()
    {
        Some("repo" | "repository") => "Repo",
        Some("project" | "workspace" | "local") => "Project",
        Some("user" | "personal") => "Personal",
        Some("system") => "System",
        _ => "Provider",
    }
}

fn source_icon(source: &str) -> &'static str {
    match source {
        "App" => "blocks",
        "Repo" | "Project" => "folder",
        "Personal" => "user-round",
        "System" => "settings",
        _ => "package",
    }
}

/// `formatProviderSkillDisplayName`.
fn skill_display_name(skill: &Value) -> String {
    if let Some(name) = skill["displayName"]
        .as_str()
        .map(str::trim)
        .filter(|n| !n.is_empty())
    {
        return name.to_owned();
    }
    skill["name"]
        .as_str()
        .unwrap_or_default()
        .split([' ', ':', '_', '-', '\t', '\n'])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut chars = w.chars();
            chars
                .next()
                .map(|c| c.to_uppercase().collect::<String>() + chars.as_str())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// `scoreProviderSkill`.
fn skill_score(skill: &Value, query: &str) -> Option<i64> {
    let lower = |key: &str| skill[key].as_str().unwrap_or_default().to_lowercase();
    let name = lower("name");
    let label = skill_display_name(skill).to_lowercase();
    let tiers = |exact, prefix, boundary, includes| Bases {
        exact,
        prefix: Some(prefix),
        boundary,
        includes: Some(includes),
        ..Default::default()
    };
    [
        score_match(
            &name,
            query,
            &Bases {
                fuzzy: Some(100),
                markers: &["-", "_", "/"],
                ..tiers(0, 2, Some(4), 6)
            },
        ),
        score_match(
            &label,
            query,
            &Bases {
                fuzzy: Some(110),
                ..tiers(1, 3, Some(5), 7)
            },
        ),
        score_match(
            &lower("shortDescription"),
            query,
            &tiers(20, 22, Some(24), 26),
        ),
        score_match(&lower("description"), query, &tiers(30, 32, Some(34), 36)),
        score_match(&lower("scope"), query, &tiers(40, 42, None, 44)),
    ]
    .into_iter()
    .flatten()
    .min()
}

/// Skills a pick can start (`isProviderSkillUserInvocable`), one per name.
fn invocable_skills(skills: &[Value]) -> Vec<Value> {
    let mut seen = std::collections::HashSet::new();
    skills
        .iter()
        .filter(|s| s["enabled"] == true && s["userInvocable"] != false)
        .filter(|s| seen.insert(s["name"].as_str().unwrap_or_default().trim().to_lowercase()))
        .cloned()
        .collect()
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ItemKind {
    Path { path: String, directory: bool },
    Builtin(&'static str),
    ProviderCommand(String),
    Skill { name: String, source: &'static str },
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Item {
    pub id: String,
    pub kind: ItemKind,
    pub label: String,
    pub description: String,
}

/// The menu's highlight and dismissal, kept between frames.
#[derive(Default)]
pub(crate) struct MenuState {
    highlighted: Option<(String, String)>,
    dismissed: Option<String>,
    /// The trigger and items shown last frame, for the keys of this one.
    shown: Option<(Trigger, Vec<Item>, bool)>,
}

fn search_key(trigger: &Trigger) -> String {
    format!("{:?}:{}", trigger.kind, trigger.query.trim().to_lowercase())
}

/// `composerFileLinkBasename`, `serializeComposerFileLink`.
pub(crate) fn file_link(path: &str) -> String {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    let label = name
        .replace('\\', "\\\\")
        .replace('[', "\\[")
        .replace(']', "\\]");
    let mut destination = String::new();
    for c in path.chars() {
        match c {
            '(' => destination.push_str("%28"),
            ')' => destination.push_str("%29"),
            '#' => destination.push_str("%23"),
            '?' => destination.push_str("%3F"),
            '\\' => destination.push_str("%5C"),
            // `encodeURI` keeps these as they are.
            c if c.is_ascii_alphanumeric() || ";,/:@&=+$-_.!~*'".contains(c) => destination.push(c),
            c => {
                let mut buffer = [0; 4];
                for byte in c.encode_utf8(&mut buffer).bytes() {
                    destination.push_str(&format!("%{byte:02X}"));
                }
            }
        }
    }
    format!("[{label}]({destination})")
}

impl App {
    /// The selected provider's entry in the server config.
    fn composer_provider(&self, draft: bool) -> Option<Value> {
        let instance = self.composer_model_selection(draft).instance_id;
        self.providers()
            .iter()
            .find(|p| p["instanceId"] == instance.as_str())
            .cloned()
    }

    /// `resolveProviderSkillsForCwd` / `resolveProviderSlashCommandsForCwd`.
    fn provider_list(&self, draft: bool, key: &str) -> Vec<Value> {
        let Some(provider) = self.composer_provider(draft) else {
            return Vec::new();
        };
        let cwd = self.workspace_cwd();
        let snapshot = provider["workspaceSnapshots"]
            .as_array()
            .and_then(|s| s.iter().find(|s| cwd.as_deref() == s["cwd"].as_str()));
        snapshot
            .map_or(&provider[key], |s| &s[key])
            .as_array()
            .cloned()
            .unwrap_or_default()
    }

    /// The items for a trigger, and whether they are still loading.
    fn menu_items(&mut self, trigger: &Trigger, draft: bool) -> (Vec<Item>, bool) {
        match trigger.kind {
            TriggerKind::Path => {
                let query = trigger.query.trim();
                let Some(cwd) = self.workspace_cwd().filter(|_| !query.is_empty()) else {
                    return (Vec::new(), false);
                };
                match self.composer_path_search(&cwd, query) {
                    None => (Vec::new(), true),
                    Some(Err(_)) => (Vec::new(), false),
                    Some(Ok(entries)) => (
                        entries
                            .into_iter()
                            .map(|(path, directory)| {
                                let (folder, name) = path
                                    .rsplit_once('/')
                                    .map_or(("", path.as_str()), |(f, n)| (f, n));
                                Item {
                                    id: format!("path:{directory}:{path}"),
                                    label: name.to_owned(),
                                    description: folder.to_owned(),
                                    kind: ItemKind::Path {
                                        path: path.clone(),
                                        directory,
                                    },
                                }
                            })
                            .collect(),
                        false,
                    ),
                }
            }
            TriggerKind::Slash => (self.slash_items(trigger, draft), false),
            TriggerKind::Skill => {
                let skills = invocable_skills(&self.provider_list(draft, "skills"));
                let query = normalize_query(&trigger.query, is_currency);
                let mut ranked: Vec<(i64, String, Value)> = skills
                    .into_iter()
                    .filter_map(|skill| {
                        let score = if query.is_empty() {
                            0
                        } else {
                            skill_score(&skill, &query)?
                        };
                        let tie = format!(
                            "{}\0{}",
                            skill_display_name(&skill).to_lowercase(),
                            skill["name"].as_str().unwrap_or_default()
                        );
                        Some((score, tie, skill))
                    })
                    .collect();
                if !query.is_empty() {
                    ranked.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
                }
                let items = ranked
                    .into_iter()
                    .map(|(_, _, skill)| {
                        skill_item(&skill, skill_display_name(&skill), "Run provider skill")
                    })
                    .collect();
                (items, false)
            }
        }
    }

    /// `/` items: built-ins, provider commands (at the prompt's start) and
    /// skills, ranked by `searchSlashCommandItems`.
    fn slash_items(&self, trigger: &Trigger, draft: bool) -> Vec<Item> {
        let mut items = vec![Item {
            id: "slash:model".into(),
            kind: ItemKind::Builtin("model"),
            label: "/model".into(),
            description: "Switch response model for this thread".into(),
        }];
        if self.plan_mode_enabled(draft) {
            items.push(Item {
                id: "slash:plan".into(),
                kind: ItemKind::Builtin("plan"),
                label: "/plan".into(),
                description: "Switch this thread into plan mode".into(),
            });
            items.push(Item {
                id: "slash:default".into(),
                kind: ItemKind::Builtin("default"),
                label: "/default".into(),
                description: "Switch this thread back to normal build mode".into(),
            });
        }
        let skills = if self.client_setting("showSkillsInSlashMenu") == false {
            Vec::new()
        } else {
            invocable_skills(&self.provider_list(draft, "skills"))
        };
        let skill_names: std::collections::HashSet<String> = skills
            .iter()
            .map(|s| s["name"].as_str().unwrap_or_default().trim().to_lowercase())
            .collect();
        if trigger.start == 0 {
            for command in self.provider_list(draft, "slashCommands") {
                let name = command["name"].as_str().unwrap_or_default().to_owned();
                if name.is_empty() || skill_names.contains(&name.trim().to_lowercase()) {
                    continue;
                }
                // `/compact` and `/usage-limits` need state this client lacks.
                if name == "compact" || name == "usage-limits" {
                    continue;
                }
                let description = command["description"]
                    .as_str()
                    .or(command["input"]["hint"].as_str())
                    .unwrap_or("Run provider command")
                    .to_owned();
                items.push(Item {
                    id: format!("provider-slash-command:{name}"),
                    kind: ItemKind::ProviderCommand(name.clone()),
                    label: format!("/{name}"),
                    description,
                });
            }
        }
        for skill in &skills {
            let name = skill["name"].as_str().unwrap_or_default();
            items.push(skill_item(skill, format!("/skill:{name}"), ""));
        }
        let query = normalize_query(&trigger.query, |c| c == '/');
        if query.is_empty() {
            return items;
        }
        let mut ranked: Vec<(i64, String, Item)> = items
            .into_iter()
            .filter_map(|item| {
                let (score, tie) = match &item.kind {
                    ItemKind::Skill { name, .. } => {
                        let skill = skills.iter().find(|s| s["name"] == name.as_str())?;
                        let skill_query = query.strip_prefix("skill:").unwrap_or(&query);
                        let score = if query == "skill" || skill_query.is_empty() {
                            Some(0)
                        } else {
                            skill_score(skill, skill_query)
                        }
                        .or_else(|| "skill".starts_with(&query).then_some(i64::MAX))?;
                        (score, format!("2\0{name}"))
                    }
                    ItemKind::Builtin(command) => (
                        command_score(command, &item.description, &query)?,
                        format!("0\0{command}"),
                    ),
                    ItemKind::ProviderCommand(command) => (
                        command_score(command, &item.description, &query)?,
                        format!("1\0{command}"),
                    ),
                    ItemKind::Path { .. } => return None,
                };
                Some((score, tie, item))
            })
            .collect();
        ranked.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        ranked.into_iter().map(|(_, _, item)| item).collect()
    }

    /// The trigger at the composer's caret, and its items; None while the
    /// composer is unfocused or the menu was dismissed for this query.
    fn current_menu(
        &mut self,
        ui: &Ui,
        editor_id: egui::Id,
        draft: bool,
    ) -> Option<(Trigger, Vec<Item>, bool)> {
        if !ui.memory(|m| m.has_focus(editor_id)) {
            return None;
        }
        let caret = egui::TextEdit::load_state(ui.ctx(), editor_id)
            .and_then(|state| state.cursor.char_range())
            .filter(|range| range.primary == range.secondary)?
            .primary
            .index
            .0;
        let display = cite::collapse(&self.composer).display;
        let trigger = detect_trigger(&display, caret)?;
        if self.command_menu.dismissed.as_deref() == Some(search_key(&trigger).as_str()) {
            return None;
        }
        self.command_menu.dismissed = None;
        let (items, loading) = self.menu_items(&trigger, draft);
        Some((trigger, items, loading))
    }

    /// The highlighted item (`resolveComposerMenuActiveItemId`).
    fn active_item<'a>(&self, trigger: &Trigger, items: &'a [Item]) -> Option<&'a Item> {
        let key = search_key(trigger);
        self.command_menu
            .highlighted
            .as_ref()
            .filter(|(k, _)| *k == key)
            .and_then(|(_, id)| items.iter().find(|i| i.id == *id))
            .or(items.first())
    }

    /// Before the editor: the menu's keys. Returns whether it took Enter.
    pub(crate) fn command_menu_keys(
        &mut self,
        ui: &mut Ui,
        editor_id: egui::Id,
        draft: bool,
    ) -> bool {
        let Some((trigger, items, _)) = self.command_menu.shown.clone() else {
            return false;
        };
        if !ui.memory(|m| m.has_focus(editor_id)) {
            return false;
        }
        if ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
            self.command_menu.dismissed = Some(search_key(&trigger));
            self.command_menu.shown = None;
            return false;
        }
        if items.is_empty() {
            return false;
        }
        for (key, step) in [(Key::ArrowDown, 1isize), (Key::ArrowUp, -1)] {
            if ui.input_mut(|i| i.consume_key(Modifiers::NONE, key)) {
                let current = self
                    .active_item(&trigger, &items)
                    .and_then(|a| items.iter().position(|i| i.id == a.id));
                let len = items.len() as isize;
                let index = match current {
                    Some(i) => (i as isize + step).rem_euclid(len),
                    None if step > 0 => 0,
                    None => len - 1,
                };
                self.command_menu.highlighted =
                    Some((search_key(&trigger), items[index as usize].id.clone()));
            }
        }
        let pick = ui.input_mut(|i| {
            i.consume_key(Modifiers::NONE, Key::Enter) || i.consume_key(Modifiers::NONE, Key::Tab)
        });
        if pick && let Some(item) = self.active_item(&trigger, &items).cloned() {
            self.pick_command_item(ui.ctx(), editor_id, &trigger, &item, draft);
            return true;
        }
        false
    }

    /// `onSelectComposerItem`: the trigger replaced by the item's text, or
    /// a built-in run.
    fn pick_command_item(
        &mut self,
        ctx: &egui::Context,
        editor_id: egui::Id,
        trigger: &Trigger,
        item: &Item,
        draft: bool,
    ) {
        let replacement = match &item.kind {
            ItemKind::Path { path, .. } => format!("{} ", file_link(path)),
            ItemKind::ProviderCommand(name) => format!("/{name} "),
            ItemKind::Skill { name, .. } => format!("${name} "),
            ItemKind::Builtin(command) => {
                match *command {
                    "model" => self.open_menu = Some("model-picker".into()),
                    mode => {
                        let key = self.composer_key(draft);
                        self.composer_interaction.insert(key, mode.into());
                    }
                }
                String::new()
            }
        };
        let collapsed = cite::collapse(&self.composer);
        let chars: Vec<char> = collapsed.display.chars().collect();
        let mut end = trigger.end.min(chars.len());
        // `extendReplacementRangeForTrailingSpace`.
        if replacement.ends_with(' ') && chars.get(end) == Some(&' ') {
            end += 1;
        }
        let display: String = chars[..trigger.start]
            .iter()
            .chain(replacement.chars().collect::<Vec<_>>().iter())
            .chain(chars[end..].iter())
            .collect();
        self.composer = cite::expand(&display, &collapsed.tokens);
        let caret = trigger.start + replacement.chars().count();
        let mut state = egui::TextEdit::load_state(ctx, editor_id).unwrap_or_default();
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::one(
                egui::text::CCursor::new(caret),
            )));
        state.store(ctx, editor_id);
        self.command_menu.highlighted = None;
        self.command_menu.shown = None;
    }

    /// After the editor: the menu for this frame's text, drawn attached
    /// above the composer `surface` (`ComposerCommandMenuLayer`).
    pub(crate) fn composer_command_menu(
        &mut self,
        ui: &mut Ui,
        surface: Rect,
        editor_id: egui::Id,
        draft: bool,
    ) {
        let current = self.current_menu(ui, editor_id, draft);
        if current.as_ref().map(|(t, i, l)| (t, i, l))
            != self.command_menu.shown.as_ref().map(|(t, i, l)| (t, i, l))
        {
            ui.ctx().request_repaint();
        }
        self.command_menu.shown = current.clone();
        let Some((trigger, items, loading)) = current else {
            return;
        };
        const ROW: f32 = 28.0;
        const OVERLAP: f32 = crate::ui::requests_panel::OVERLAP;
        let content = if items.is_empty() {
            14.0 + 16.0 + 28.0
        } else {
            (items.len() as f32 * ROW + 16.0).min(288.0)
        };
        let rect = Rect::from_min_max(
            Pos2::new(
                surface.left() + 22.0,
                surface.top() + OVERLAP - content - OVERLAP,
            ),
            Pos2::new(surface.right() - 22.0, surface.top() + OVERLAP),
        );
        let active = self.active_item(&trigger, &items).map(|i| i.id.clone());
        let mut picked = None;
        let mut hovered = None;
        egui::Area::new(ui.id().with("composer-command-menu"))
            .order(egui::Order::Foreground)
            .fixed_pos(rect.min)
            .show(ui.ctx(), |ui| {
                let (_, _) = ui.allocate_exact_size(rect.size(), Sense::hover());
                crate::ui::requests_panel::plan_banner_surface(ui, rect);
                let body =
                    Rect::from_min_max(rect.min, Pos2::new(rect.right(), rect.bottom() - OVERLAP));
                if items.is_empty() {
                    let text = if loading {
                        match trigger.kind {
                            TriggerKind::Skill => "Searching workspace skills...",
                            _ => "Searching workspace files...",
                        }
                    } else {
                        match trigger.kind {
                            TriggerKind::Skill => {
                                "No skills found. Try / to browse provider commands."
                            }
                            TriggerKind::Path => "No matching files or folders.",
                            TriggerKind::Slash => "No matching command.",
                        }
                    };
                    widgets::text(
                        ui,
                        Pos2::new(body.left() + 20.0, body.top() + 14.0),
                        text,
                        font(12.0, Weight::Regular),
                        theme::secondary_label(),
                        16.0,
                    );
                    return;
                }
                let list = body.shrink(8.0);
                let mut child = ui.new_child(egui::UiBuilder::new().max_rect(list));
                child.set_clip_rect(list.intersect(ui.clip_rect()));
                egui::ScrollArea::vertical()
                    .id_salt(("composer-command-list", search_key(&trigger)))
                    .auto_shrink([false, false])
                    .show(&mut child, |ui| {
                        let top = ui.max_rect().top();
                        for (index, item) in items.iter().enumerate() {
                            let row = Rect::from_min_size(
                                Pos2::new(list.left(), top + index as f32 * ROW),
                                Vec2::new(list.width(), ROW),
                            );
                            let response = ui.interact(
                                row,
                                ui.id().with(("command-item", &item.id)),
                                Sense::click(),
                            );
                            if response.hovered() && ui.input(|i| i.pointer.delta() != Vec2::ZERO) {
                                hovered = Some(item.id.clone());
                            }
                            if response.clicked() {
                                picked = Some(item.clone());
                            }
                            let is_active = active.as_deref() == Some(item.id.as_str());
                            if is_active {
                                ui.painter().rect_filled(
                                    row,
                                    CornerRadius::same(6),
                                    theme::accent(),
                                );
                                if ui.input(|i| {
                                    i.key_pressed(Key::ArrowDown) || i.key_pressed(Key::ArrowUp)
                                }) {
                                    ui.scroll_to_rect(row, None);
                                }
                            }
                            command_row(ui, row, item, trigger.kind);
                        }
                        ui.allocate_rect(
                            Rect::from_min_size(
                                Pos2::new(list.left(), top),
                                Vec2::new(list.width(), items.len() as f32 * ROW),
                            ),
                            Sense::hover(),
                        );
                    });
            });
        if let Some(id) = hovered {
            self.command_menu.highlighted = Some((search_key(&trigger), id));
        }
        if let Some(item) = picked {
            self.pick_command_item(ui.ctx(), editor_id, &trigger, &item, draft);
            ui.memory_mut(|m| m.request_focus(editor_id));
        }
    }
}

/// `scoreSlashCommandItem` for a built-in or provider command.
fn command_score(command: &str, description: &str, query: &str) -> Option<i64> {
    let primary = score_match(
        &command.to_lowercase(),
        query,
        &Bases {
            exact: 0,
            prefix: Some(2),
            boundary: Some(4),
            includes: Some(6),
            fuzzy: Some(100),
            markers: &["-", "_", "/"],
        },
    );
    let description = score_match(
        &description.to_lowercase(),
        query,
        &Bases {
            exact: 20,
            prefix: Some(22),
            boundary: Some(24),
            includes: Some(26),
            ..Default::default()
        },
    );
    primary.into_iter().chain(description).min()
}

fn skill_item(skill: &Value, label: String, fallback: &str) -> Item {
    let name = skill["name"].as_str().unwrap_or_default().to_owned();
    let description = skill["shortDescription"]
        .as_str()
        .or(skill["description"].as_str())
        .map(str::to_owned)
        .or_else(|| skill["scope"].as_str().map(|s| format!("{s} skill")))
        .unwrap_or_else(|| fallback.to_owned());
    Item {
        id: format!("skill:{name}"),
        kind: ItemKind::Skill {
            name,
            source: skill_source(skill),
        },
        label,
        description,
    }
}

/// `ComposerCommandMenuItem`: the entry's icon, its label (at most 45%) and
/// description, and a skill's source badge.
fn command_row(ui: &Ui, row: Rect, item: &Item, trigger: TriggerKind) {
    let mut x = row.left() + 8.0;
    if let ItemKind::Path { path, directory } = &item.kind {
        let (icon, color) = if *directory {
            ("folder", theme::muted_foreground())
        } else {
            crate::ui::files::file_icon(path)
        };
        theme::paint_icon(
            ui,
            icon,
            Rect::from_min_size(Pos2::new(x, row.center().y - 8.0), Vec2::splat(16.0)),
            16.0,
            color,
        );
        x += 16.0 + 8.0;
    }
    let mut right = row.right() - 8.0;
    if let ItemKind::Skill { source, .. } = &item.kind {
        let label = if trigger == TriggerKind::Skill {
            format!("{source} Skill")
        } else {
            (*source).to_owned()
        };
        let badge_font = font(12.0, Weight::Medium);
        let width = widgets::text_width(ui, &label, badge_font.clone()) + 6.0 + 12.0 + 4.0;
        let badge = Rect::from_min_size(
            Pos2::new(right - width, row.center().y - 9.0),
            Vec2::new(width, 18.0),
        );
        ui.painter()
            .rect_filled(badge, CornerRadius::same(6), theme::overlay(0.08));
        theme::paint_icon(
            ui,
            source_icon(source),
            Rect::from_min_size(
                Pos2::new(badge.left() + 3.0, badge.center().y - 6.0),
                Vec2::splat(12.0),
            ),
            12.0,
            theme::alpha(theme::foreground(), 0.8),
        );
        widgets::text_in(
            ui,
            Rect::from_min_max(Pos2::new(badge.left() + 19.0, badge.top()), badge.max),
            &label,
            badge_font,
            theme::foreground(),
        );
        right = badge.left() - 8.0;
    }
    let label_font = font(12.0, Weight::Medium);
    let max_label = ((right - x) * 0.45).max(20.0);
    let slash_skill = trigger == TriggerKind::Slash && matches!(item.kind, ItemKind::Skill { .. });
    let label_width = if slash_skill {
        // "/skill:" in the secondary color, then the skill's name.
        let name = item.label.trim_start_matches("/skill:");
        let prefix = widgets::text_in(
            ui,
            Rect::from_min_max(
                Pos2::new(x, row.top()),
                Pos2::new(x + max_label, row.bottom()),
            ),
            "/skill:",
            label_font.clone(),
            theme::secondary_label(),
        );
        prefix
            + widgets::text_in(
                ui,
                Rect::from_min_max(
                    Pos2::new(x + prefix, row.top()),
                    Pos2::new(x + max_label, row.bottom()),
                ),
                name,
                label_font,
                theme::foreground(),
            )
    } else {
        widgets::text_in(
            ui,
            Rect::from_min_max(
                Pos2::new(x, row.top()),
                Pos2::new(x + max_label, row.bottom()),
            ),
            &item.label,
            label_font,
            theme::foreground(),
        )
    };
    let description_left = x + label_width + 8.0;
    if description_left < right {
        widgets::text_in(
            ui,
            Rect::from_min_max(
                Pos2::new(description_left, row.top()),
                Pos2::new(right, row.bottom()),
            ),
            &item.description,
            font(12.0, Weight::Regular),
            theme::secondary_label(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn triggers_follow_the_caret() {
        let t = |text: &str| detect_trigger(text, text.chars().count());
        assert_eq!(
            t("/mo"),
            Some(Trigger {
                kind: TriggerKind::Slash,
                query: "mo".into(),
                start: 0,
                end: 3
            })
        );
        assert_eq!(t("/model x"), None);
        assert_eq!(
            t("voir @src/ap").map(|t| (t.kind, t.query, t.start)),
            Some((TriggerKind::Path, "src/ap".into(), 5))
        );
        assert_eq!(
            t("lance $rev").map(|t| (t.kind, t.query)),
            Some((TriggerKind::Skill, "rev".into()))
        );
        assert_eq!(t("ligne\n/pl").map(|t| t.start), Some(6));
        assert_eq!(t("mail@x"), None);
        assert_eq!(t("plain"), None);
    }

    #[test]
    fn matches_rank_like_upstream() {
        let bases = Bases {
            exact: 0,
            prefix: Some(2),
            boundary: Some(4),
            includes: Some(6),
            fuzzy: Some(100),
            markers: &["-", "_", "/"],
        };
        assert_eq!(score_match("model", "model", &bases), Some(0));
        assert_eq!(score_match("model", "mo", &bases), Some(5));
        assert_eq!(score_match("code-review", "rev", &bases), Some(4 + 10 + 8));
        assert_eq!(score_match("compact", "mpa", &bases), Some(6 + 4 + 4));
        assert!(score_match("model", "mdl", &bases).is_some_and(|s| s >= 100));
        assert_eq!(score_match("model", "xyz", &bases), None);
    }

    #[test]
    fn file_links_escape_like_upstream() {
        assert_eq!(file_link("src/app.rs"), "[app.rs](src/app.rs)");
        assert_eq!(
            file_link("docs/a b(1).md"),
            "[a b(1).md](docs/a%20b%281%29.md)"
        );
        assert_eq!(
            file_link("x/[tag]#1.md"),
            "[\\[tag\\]#1.md](x/%5Btag%5D%231.md)"
        );
    }
}
