//! The checkout strip's ref picker (`BranchToolbarBranchSelector.tsx`,
//! `BranchPicker.tsx`, `BranchToolbar.logic.ts`, `state/queries.ts`'s
//! `usePaginatedBranches`): the branch control opens "Search refs..." over
//! the workspace's refs (`vcs.listRefs`, 100 a page, the next page loaded
//! near the list's end), each with its current, worktree, remote or default
//! badge. A draft in New worktree mode picks the base it branches off
//! (the repository's default branch until then) and whether it starts from
//! origin; otherwise a pick checks the ref out (`vcs.switchRef`), or reuses
//! the worktree that has it checked out, and "Create new ref" creates and
//! switches to the typed one (`vcs.createRef`). A server thread records its
//! new branch and worktree (`thread.metadata.update`), detaching its provider
//! sessions when the worktree changes. Right-clicking the control or a ref
//! copies its name.
//!
//! Not ported: the "Checkout pull request" item a typed pull request
//! reference offers, the pull request badge beside the control.

use std::collections::HashMap;

use eframe::egui::{self, CornerRadius, Key, Pos2, Rect, Sense, Ui, Vec2};
use serde_json::{Value, json};

use crate::app::App;
use crate::theme::{self, Weight, font};
use crate::ui::context_menu::{self, Item};
use crate::ui::toasts::{Kind, Toast};
use crate::ui::widgets;

pub(crate) const MENU: &str = "composer-branch";
/// `VCS_REF_LIST_LIMIT`.
const PAGE: u64 = 100;
const ROW: f32 = 28.0;
/// `max-h-56`.
const LIST_MAX: f32 = 224.0;
/// `DEFAULT_NEXT_PAGE_DISTANCE_PX`.
const NEXT_PAGE_DISTANCE: f32 = 96.0;

/// A draft's workspace beside its mode (`DraftThreadState`'s branch,
/// worktree path and start-from-origin), for one project.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct DraftWorkspace {
    pub(crate) project: String,
    pub(crate) branch: Option<String>,
    pub(crate) worktree: Option<String>,
    pub(crate) from_origin: Option<bool>,
}

#[derive(Default)]
pub(crate) struct BranchPicker {
    query: String,
    highlight: usize,
    /// `vcs.listRefs` pages by request key; `None` while asked.
    pages: HashMap<String, Option<Result<Value, String>>>,
    /// The further pages asked for each `cwd\0query`.
    cursors: HashMap<String, Vec<u64>>,
    /// The switch or create in flight, and the branch shown meanwhile
    /// (`useOptimistic`).
    action: Option<Pending>,
    /// "Copy branch name" for a ref.
    menu: Option<(context_menu::State, String)>,
    serial: u64,
}

struct Pending {
    key: String,
    owner: Owner,
    shown: String,
    remote: bool,
    create: bool,
    worktree: Option<String>,
}

/// Whose branch a pick changes.
#[derive(Clone)]
enum Owner {
    Draft(String),
    Thread(String),
}

/// One cwd and query's refs, its pages merged.
#[derive(Default)]
struct Listing {
    refs: Vec<Value>,
    next: Option<u64>,
    total: u64,
    loading: bool,
    loading_next: bool,
}

/// The picker's view of the strip's workspace.
struct Context {
    owner: Owner,
    cwd: String,
    project_cwd: String,
    worktree: Option<String>,
    /// A draft in New worktree mode picking its base.
    selecting_base: bool,
    value: Option<String>,
}

/// `sanitizeNewRefName`.
pub(crate) fn sanitize_ref_name(raw: &str) -> String {
    raw.split_whitespace().collect::<Vec<_>>().join("-")
}

/// `deriveLocalBranchNameFromRemoteRef`.
fn local_name(remote: &str) -> &str {
    match remote.find('/') {
        Some(index) if index > 0 && index + 1 < remote.len() => &remote[index + 1..],
        _ => remote,
    }
}

/// `shouldIncludeBranchPickerItem` for a ref name.
fn includes(name: &str, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    let name = name.to_lowercase();
    if name.contains(query) {
        return true;
    }
    let sanitized = sanitize_ref_name(query);
    !sanitized.is_empty() && sanitized != query && name.contains(&sanitized)
}

/// `MiddleTruncate`: the name's start and end around an ellipsis when it
/// does not fit `max`.
fn middle_truncate(ui: &Ui, text: &str, font: egui::FontId, max: f32) -> String {
    if widgets::text_width(ui, text, font.clone()) <= max {
        return text.to_owned();
    }
    let chars: Vec<char> = text.chars().collect();
    let fits = |keep: usize| {
        let head = keep.div_ceil(2);
        let tail = keep / 2;
        let candidate: String = chars[..head]
            .iter()
            .chain(std::iter::once(&'…'))
            .chain(&chars[chars.len() - tail..])
            .collect();
        (widgets::text_width(ui, &candidate, font.clone()) <= max).then_some(candidate)
    };
    let (mut low, mut high) = (0, chars.len());
    let mut best = "…".to_owned();
    while low < high {
        let mid = (low + high).div_ceil(2);
        match fits(mid) {
            Some(candidate) => {
                best = candidate;
                low = mid;
            }
            None => high = mid - 1,
        }
    }
    best
}

/// `BranchPickerRefItem`'s badge.
fn badge(r: &Value, project_cwd: &str) -> Option<&'static str> {
    let secondary = r["worktreePath"]
        .as_str()
        .is_some_and(|path| path != project_cwd);
    if r["current"] == true {
        Some("current")
    } else if secondary {
        Some("worktree")
    } else if r["isRemote"] == true {
        Some("remote")
    } else if r["isDefault"] == true {
        Some("default")
    } else {
        None
    }
}

/// `resolveBranchSelectionTarget`: where a pick checks out, the worktree the
/// thread moves to, and whether that worktree already has the ref.
fn selection_target(
    project_cwd: &str,
    worktree: Option<&str>,
    r: &Value,
) -> (String, Option<String>, bool) {
    if let Some(path) = r["worktreePath"].as_str() {
        let next = (path != project_cwd).then(|| path.to_owned());
        return (path.to_owned(), next, true);
    }
    let next = worktree
        .filter(|_| r["isDefault"] != true)
        .map(str::to_owned);
    (
        next.clone().unwrap_or_else(|| project_cwd.to_owned()),
        next,
        false,
    )
}

/// `resolveBranchTriggerLabel`.
fn trigger_label(
    value: Option<&str>,
    selecting_base: bool,
    remote: Option<bool>,
    from_origin: bool,
) -> String {
    let Some(value) = value else {
        return "Select ref".into();
    };
    if selecting_base {
        return if from_origin && remote == Some(false) {
            format!("From origin/{value}")
        } else {
            format!("From {value}")
        };
    }
    value.to_owned()
}

/// `resolvePreviousWorktreeSeed`: the latest unarchived thread worktree of
/// the project other than `current`.
pub(crate) fn previous_worktree(
    threads: &[std::sync::Arc<crate::model::Thread>],
    project: &str,
    current: Option<&str>,
) -> Option<(Option<String>, String)> {
    threads
        .iter()
        .filter(|t| t.project_id == project && t.archived_at.is_none())
        .filter_map(|t| {
            let path = t.worktree_path.as_deref()?;
            let at = chrono::DateTime::parse_from_rfc3339(&t.updated_at).ok()?;
            (Some(path) != current).then_some((at, t))
        })
        .max_by_key(|(at, _)| *at)
        .map(|(_, t)| {
            (
                t.branch.clone(),
                t.worktree_path.clone().unwrap_or_default(),
            )
        })
}

impl App {
    /// The draft's workspace state when it is for the draft's project.
    pub(crate) fn draft_workspace(&self) -> Option<&DraftWorkspace> {
        let project = self.draft_project()?;
        self.draft_workspace
            .as_ref()
            .filter(|w| w.project == project.id)
    }

    fn draft_workspace_mut(&mut self) -> Option<&mut DraftWorkspace> {
        let project = self.draft_project()?.id;
        if self
            .draft_workspace
            .as_ref()
            .is_none_or(|w| w.project != project)
        {
            self.draft_workspace = Some(DraftWorkspace {
                project,
                ..Default::default()
            });
        }
        self.draft_workspace.as_mut()
    }

    /// `newWorktreesStartFromOrigin`, the project's override first.
    fn worktrees_start_from_origin(&self, project: &str) -> bool {
        let settings = &self.state.config["settings"];
        settings["projectSettingsOverrides"][project]["newWorktreesStartFromOrigin"]
            .as_bool()
            .or(settings["newWorktreesStartFromOrigin"].as_bool())
            == Some(true)
    }

    /// Whether a draft's new worktree starts from origin
    /// (`resolveNewDraftStartFromOrigin` until the switch is flipped).
    pub(crate) fn draft_start_from_origin(&self) -> bool {
        let Some(project) = self.draft_project() else {
            return false;
        };
        self.draft_workspace()
            .and_then(|w| w.from_origin)
            .unwrap_or_else(|| {
                self.draft_env_mode() == "worktree" && self.worktrees_start_from_origin(&project.id)
            })
    }

    /// A draft's mode change resets what it picked for the other mode.
    pub(crate) fn set_draft_mode(&mut self, mode: &str) {
        let Some(project) = self.draft_project() else {
            return;
        };
        self.draft_env_mode = Some((project.id, mode.into()));
        if let Some(workspace) = self.draft_workspace_mut() {
            workspace.from_origin = None;
            if mode == "worktree" {
                workspace.worktree = None;
            }
        }
    }

    /// "Previous worktree": the draft opens in that worktree and branch.
    pub(crate) fn use_previous_worktree(&mut self, branch: Option<String>, path: String) {
        let Some(project) = self.draft_project() else {
            return;
        };
        self.draft_env_mode = Some((project.id, "local".into()));
        if let Some(workspace) = self.draft_workspace_mut() {
            workspace.branch = branch;
            workspace.worktree = Some(path);
        }
    }

    /// The draft's base for a new worktree: its pick, else the repository's
    /// default branch, else the checked-out one.
    pub(crate) fn draft_worktree_base(&mut self) -> Option<String> {
        if let Some(branch) = self.draft_workspace().and_then(|w| w.branch.clone()) {
            return Some(branch);
        }
        let cwd = self.workspace_cwd()?;
        let listing = self.ref_listing(&cwd, "");
        if listing.loading {
            return None;
        }
        listing
            .refs
            .iter()
            .find(|r| r["isDefault"] == true)
            .and_then(|r| r["name"].as_str().map(str::to_owned))
            .or_else(|| self.current_git_branch(&cwd, &listing))
    }

    fn current_git_branch(&self, cwd: &str, listing: &Listing) -> Option<String> {
        self.vcs
            .as_ref()
            .filter(|(c, _)| c == cwd)
            .and_then(|(_, status)| status["refName"].as_str().map(str::to_owned))
            .or_else(|| {
                listing
                    .refs
                    .iter()
                    .find(|r| r["current"] == true)
                    .and_then(|r| r["name"].as_str().map(str::to_owned))
            })
    }

    /// The refs of `cwd` matching `query`, asking for missing pages.
    fn ref_listing(&mut self, cwd: &str, query: &str) -> Listing {
        let target = format!("{cwd}\0{query}");
        let cursors: Vec<Option<u64>> = std::iter::once(None)
            .chain(
                self.branch_picker
                    .cursors
                    .get(&target)
                    .into_iter()
                    .flatten()
                    .map(|c| Some(*c)),
            )
            .collect();
        let mut listing = Listing::default();
        let mut seen = std::collections::HashSet::new();
        for (index, cursor) in cursors.iter().enumerate() {
            let key = format!(
                "branch-refs:{target}\0{}",
                cursor.map(|c| c.to_string()).unwrap_or_default()
            );
            if !self.branch_picker.pages.contains_key(&key) {
                self.branch_picker.pages.insert(key.clone(), None);
                let mut payload = json!({"cwd": cwd, "limit": PAGE});
                if !query.is_empty() {
                    payload["query"] = json!(query);
                }
                if let Some(cursor) = cursor {
                    payload["cursor"] = json!(cursor);
                }
                self.send_request(key.clone(), "vcs.listRefs", payload);
            }
            match self.branch_picker.pages.get(&key) {
                Some(Some(Ok(page))) => {
                    for r in page["refs"].as_array().into_iter().flatten() {
                        if let Some(name) = r["name"].as_str()
                            && seen.insert(name.to_owned())
                        {
                            listing.refs.push(r.clone());
                        }
                    }
                    listing.total = listing.total.max(page["totalCount"].as_u64().unwrap_or(0));
                    listing.next = page["nextCursor"].as_u64();
                }
                Some(None) if index == 0 => listing.loading = true,
                Some(None) => listing.loading_next = true,
                _ => {}
            }
        }
        if listing.loading_next {
            listing.next = None;
        }
        listing
    }

    /// Asks for `cwd`'s first page again, showing the last one until it
    /// answers, and drops the rest (`branchRefState.refresh`), when the
    /// picker opens and after a switch.
    fn revalidate_refs(&mut self, cwd: &str) {
        let first = format!("branch-refs:{cwd}\0\0");
        let prefix = format!("branch-refs:{cwd}\0");
        self.branch_picker
            .pages
            .retain(|key, _| !key.starts_with(&prefix) || *key == first);
        let prefix = format!("{cwd}\0");
        self.branch_picker
            .cursors
            .retain(|key, _| !key.starts_with(&prefix));
        if self.branch_picker.pages.contains_key(&first) {
            self.send_request(first, "vcs.listRefs", json!({"cwd": cwd, "limit": PAGE}));
        }
    }

    fn branch_context(&mut self, draft: bool) -> Option<Context> {
        let cwd = self.workspace_cwd()?;
        let (owner, project_cwd, worktree, branch, mode) = if draft {
            let project = self.draft_project()?;
            let workspace = self.draft_workspace().cloned().unwrap_or_default();
            (
                Owner::Draft(project.id.clone()),
                project.workspace_root.clone(),
                workspace.worktree,
                workspace.branch,
                self.draft_env_mode(),
            )
        } else {
            let thread = self.current_thread()?;
            let project = self.state.project(&thread.project_id)?;
            (
                Owner::Thread(thread.id.clone()),
                project.workspace_root.clone(),
                thread.worktree_path.clone(),
                thread.branch.clone(),
                "local".to_owned(),
            )
        };
        let selecting_base = draft && mode == "worktree" && worktree.is_none();
        // `resolveBranchToolbarValue`.
        let value = if let Some(pending) = &self.branch_picker.action {
            Some(pending.shown.clone())
        } else if selecting_base {
            self.draft_worktree_base()
        } else {
            let listing = self.ref_listing(&cwd, "");
            self.current_git_branch(&cwd, &listing).or(branch)
        };
        Some(Context {
            owner,
            cwd,
            project_cwd,
            worktree,
            selecting_base,
            value,
        })
    }

    /// The branch control at the strip's right end (`ComposerControl xs`):
    /// its rect ends at `right`.
    pub(crate) fn branch_control(&mut self, ui: &mut Ui, top: f32, right: f32, draft: bool) {
        let Some(context) = self.branch_context(draft) else {
            return;
        };
        let remote = context.value.as_deref().and_then(|value| {
            let listing = self.ref_listing(&context.cwd, "");
            listing
                .refs
                .iter()
                .find(|r| r["name"] == value)
                .map(|r| r["isRemote"] == true)
        });
        let from_origin = draft && self.draft_start_from_origin();
        let label = trigger_label(
            context.value.as_deref(),
            context.selecting_base,
            remote,
            from_origin,
        );
        let label_font = font(12.0, Weight::Regular);
        let color = theme::alpha(theme::muted_foreground(), 0.7);
        let width = widgets::text_width(ui, &label, label_font.clone());
        let start = right - 7.0 - 12.0 - 2.0 - width - 2.0 - 12.0 - 7.0;
        let control = Rect::from_min_max(Pos2::new(start, top + 4.0), Pos2::new(right, top + 28.0));
        let loading = self.ref_listing(&context.cwd, "").loading;
        let enabled = !loading && self.branch_picker.action.is_none();
        let response = ui.interact(control, ui.id().with(MENU), Sense::click());
        let response = if enabled {
            response.on_hover_cursor(egui::CursorIcon::PointingHand)
        } else {
            response
        };
        if enabled && response.hovered() {
            ui.painter()
                .rect_filled(control, CornerRadius::same(6), theme::accent());
        }
        theme::paint_icon(
            ui,
            "git-branch",
            Rect::from_min_size(Pos2::new(start + 7.0, top + 10.0), Vec2::splat(12.0)),
            12.0,
            color,
        );
        widgets::text_in(
            ui,
            Rect::from_min_max(
                Pos2::new(start + 21.0, top + 4.0),
                Pos2::new(right, top + 28.0),
            ),
            &label,
            label_font,
            color,
        );
        theme::paint_icon(
            ui,
            "chevron-down",
            Rect::from_min_size(
                Pos2::new(start + 23.0 + width, top + 10.0),
                Vec2::splat(12.0),
            ),
            12.0,
            theme::alpha(theme::muted_foreground(), 0.5),
        );
        if enabled && response.clicked() {
            self.revalidate_refs(&context.cwd);
            self.branch_picker.query.clear();
            self.branch_picker.highlight = 0;
            self.open_menu = Some(MENU.into());
        }
        if response.secondary_clicked()
            && let (Some(value), Some(at)) =
                (context.value.clone(), response.interact_pointer_pos())
        {
            self.branch_picker.menu = Some((context_menu::State::new(at), value));
        }
        if self.open_menu.as_deref() == Some(MENU) {
            self.branch_popup(ui, control, &context, from_origin);
        }
        self.branch_copy_menu(ui);
    }

    fn branch_copy_menu(&mut self, ui: &mut Ui) {
        let Some((state, name)) = self.branch_picker.menu.take() else {
            return;
        };
        let mut open = Some(state);
        let items = [Item::new("copy", "Copy branch name").icon("copy")];
        if context_menu::show(ui, &mut open, &items).is_some() {
            ui.ctx().copy_text(name.clone());
            self.toasts
                .add(Toast::new(Kind::Success, "Branch name copied").description(name.clone()));
        }
        self.branch_picker.menu = open.map(|state| (state, name));
    }

    /// `BranchPicker`'s popup (`w-80`, above the control, aligned to its end).
    fn branch_popup(&mut self, ui: &mut Ui, anchor: Rect, context: &Context, from_origin: bool) {
        let query = self.branch_picker.query.trim().to_owned();
        let ref_query = sanitize_ref_name(&query);
        let listing = self.ref_listing(&context.cwd, &ref_query);
        let lowered = query.to_lowercase();
        let new_name = sanitize_ref_name(&query);
        let create = (!context.selecting_base && !query.is_empty())
            .then_some(())
            .filter(|_| !listing.refs.iter().any(|r| r["name"] == new_name.as_str()));
        enum Row<'a> {
            Ref(&'a Value),
            Create,
        }
        let mut rows: Vec<Row> = listing
            .refs
            .iter()
            .filter(|r| includes(r["name"].as_str().unwrap_or_default(), &lowered))
            .map(Row::Ref)
            .collect();
        if create.is_some() {
            rows.push(Row::Create);
        }
        let status = if listing.loading {
            Some("Loading refs...".to_owned())
        } else if listing.loading_next {
            Some("Loading more refs...".to_owned())
        } else if listing.next.is_some() {
            Some(format!(
                "Showing {} of {} refs",
                listing.refs.len(),
                listing.total
            ))
        } else {
            None
        };
        let list_height = if rows.is_empty() {
            36.0
        } else {
            4.0 + (8.0 + rows.len() as f32 * ROW + 4.0).min(LIST_MAX) + 4.0
        };
        let origin_height = if context.selecting_base { 37.0 } else { 0.0 };
        let status_height = if status.is_some() { 32.0 } else { 0.0 };
        let height = 1.0 + 43.0 + list_height + origin_height + status_height + 1.0;
        let width = 320.0;
        let screen = ui.ctx().content_rect();
        let left = (anchor.right() - width).max(screen.left() + 8.0);
        let rect = Rect::from_min_size(
            Pos2::new(left, anchor.top() - 4.0 - height),
            Vec2::new(width, height),
        );
        // Keys: the arrows move the highlight, Enter picks it.
        let count = rows.len();
        if count > 0 {
            let (down, up) = ui.input_mut(|i| {
                (
                    i.consume_key(egui::Modifiers::NONE, Key::ArrowDown),
                    i.consume_key(egui::Modifiers::NONE, Key::ArrowUp),
                )
            });
            let highlight = &mut self.branch_picker.highlight;
            if down {
                *highlight = (*highlight + 1) % count;
            }
            if up {
                *highlight = (*highlight + count - 1) % count;
            }
        }
        let highlight = self.branch_picker.highlight.min(count.saturating_sub(1));
        let enter = count > 0 && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Enter));
        let mut open = self.open_menu.take();
        let mut query_text = self.branch_picker.query.clone();
        let mut picked: Option<usize> = enter.then_some(highlight);
        let mut hovered: Option<usize> = None;
        let mut copy: Option<(Pos2, String)> = None;
        let mut flip_origin = false;
        let mut load_next = false;
        let value = context.value.clone();
        let project_cwd = context.project_cwd.clone();
        widgets::popup(ui, &mut open, MENU, rect, anchor, |ui, rect| {
            let inner = rect.shrink(1.0);
            let search = Rect::from_min_size(
                Pos2::new(inner.left() + 12.0, inner.top() + 9.0),
                Vec2::new(inner.width() - 24.0, 33.0),
            );
            widgets::combobox_search(ui, search, &mut query_text, "Search refs...");
            let list = Rect::from_min_size(
                Pos2::new(inner.left(), inner.top() + 43.0),
                Vec2::new(inner.width(), list_height),
            );
            if rows.is_empty() {
                let galley = widgets::galley(
                    ui,
                    "No refs found.",
                    font(14.0, Weight::Regular),
                    theme::muted_foreground(),
                    20.0,
                    f32::INFINITY,
                );
                ui.painter().galley(
                    list.center() - galley.size() / 2.0,
                    galley,
                    theme::muted_foreground(),
                );
            } else {
                let scroll = list.shrink2(Vec2::new(0.0, 4.0));
                let mut list_ui = ui.new_child(egui::UiBuilder::new().max_rect(scroll));
                list_ui.set_clip_rect(scroll);
                let output = egui::ScrollArea::vertical()
                    .id_salt("composer-branch-list")
                    .auto_shrink([false, false])
                    .show(&mut list_ui, |ui| {
                        let origin = ui.max_rect().min + Vec2::new(0.0, 8.0);
                        for (index, row) in rows.iter().enumerate() {
                            let item = Rect::from_min_size(
                                Pos2::new(inner.left() + 8.0, origin.y + index as f32 * ROW),
                                Vec2::new(inner.width() - 16.0, ROW),
                            );
                            let response = ui.interact(
                                item,
                                ui.id().with(("branch-ref", index)),
                                Sense::click(),
                            );
                            if response.hovered() {
                                hovered = Some(index);
                            }
                            let (name, tag) = match row {
                                Row::Ref(r) => (
                                    r["name"].as_str().unwrap_or_default().to_owned(),
                                    badge(r, &project_cwd),
                                ),
                                Row::Create => (format!("Create new ref \"{new_name}\""), None),
                            };
                            let selected = matches!(row, Row::Ref(_))
                                && value.as_deref() == Some(name.as_str());
                            if index == highlight {
                                ui.painter().rect_filled(
                                    item,
                                    CornerRadius::same(4),
                                    theme::accent(),
                                );
                            } else if selected {
                                ui.painter().rect_filled(
                                    item,
                                    CornerRadius::same(4),
                                    theme::alpha(theme::foreground(), 0.08),
                                );
                            }
                            let mut text_right = item.right() - 8.0;
                            if let Some(tag) = tag {
                                let small = font(10.0, Weight::Regular);
                                let tag_width = widgets::text_width(ui, tag, small.clone());
                                widgets::text_in(
                                    ui,
                                    Rect::from_min_max(
                                        Pos2::new(text_right - tag_width, item.top()),
                                        Pos2::new(text_right, item.bottom()),
                                    ),
                                    tag,
                                    small,
                                    theme::alpha(theme::muted_foreground(), 0.45),
                                );
                                text_right -= tag_width + 8.0;
                            }
                            widgets::text_in(
                                ui,
                                Rect::from_min_max(
                                    Pos2::new(item.left() + 8.0, item.top()),
                                    Pos2::new(text_right, item.bottom()),
                                ),
                                &middle_truncate(
                                    ui,
                                    &name,
                                    font(14.0, Weight::Regular),
                                    text_right - item.left() - 8.0,
                                ),
                                font(14.0, Weight::Regular),
                                theme::foreground(),
                            );
                            if response.clicked() {
                                picked = Some(index);
                            }
                            if response.secondary_clicked()
                                && matches!(row, Row::Ref(_))
                                && let Some(at) = response.interact_pointer_pos()
                            {
                                copy = Some((at, name.clone()));
                            }
                        }
                        ui.allocate_rect(
                            Rect::from_min_size(
                                ui.max_rect().min,
                                Vec2::new(inner.width(), 8.0 + rows.len() as f32 * ROW + 4.0),
                            ),
                            Sense::hover(),
                        );
                    });
                let offset = output.state.offset.y;
                let content = output.content_size.y;
                if content - (offset + output.inner_rect.height()) <= NEXT_PAGE_DISTANCE {
                    load_next = true;
                }
            }
            let mut bottom = list.bottom();
            if context.selecting_base {
                // The "Start from origin" row (`border-t px-3 py-2 text-xs`).
                let row = Rect::from_min_size(
                    Pos2::new(inner.left(), bottom),
                    Vec2::new(inner.width(), origin_height),
                );
                ui.painter().hline(
                    row.x_range(),
                    row.top() + 0.5,
                    egui::Stroke::new(1.0, theme::alpha(theme::border(), 0.6)),
                );
                let muted = theme::muted_foreground();
                theme::paint_icon(
                    ui,
                    "refresh-cw",
                    Rect::from_min_size(
                        Pos2::new(row.left() + 12.0, row.center().y - 6.0),
                        Vec2::splat(12.0),
                    ),
                    12.0,
                    muted,
                );
                widgets::text_in(
                    ui,
                    Rect::from_min_max(
                        Pos2::new(row.left() + 30.0, row.top()),
                        Pos2::new(row.right() - 48.0, row.bottom()),
                    ),
                    "Start from origin",
                    font(12.0, Weight::Medium),
                    muted,
                );
                let response = ui
                    .interact(row, ui.id().with("branch-origin"), Sense::click())
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .on_hover_text(
                        "Creates the worktree from the latest matching branch on origin \
                         instead of your local branch.",
                    );
                let switched = crate::ui::settings::switch(
                    ui,
                    row.right() - 12.0,
                    row.center().y,
                    from_origin,
                    "branch-origin",
                );
                flip_origin = switched || response.clicked();
                bottom = row.bottom();
            }
            if let Some(status) = &status {
                widgets::text_in(
                    ui,
                    Rect::from_min_max(
                        Pos2::new(inner.left() + 12.0, bottom),
                        Pos2::new(inner.right() - 12.0, bottom + status_height),
                    ),
                    status,
                    font(12.0, Weight::Medium),
                    theme::muted_foreground(),
                );
            }
        });
        if query_text != self.branch_picker.query {
            self.branch_picker.highlight = 0;
        } else if let Some(index) = hovered
            && ui.input(|i| i.pointer.delta() != Vec2::ZERO)
        {
            self.branch_picker.highlight = index;
        }
        self.branch_picker.query = query_text;
        self.open_menu = open;
        if let Some((at, name)) = copy {
            self.branch_picker.menu = Some((context_menu::State::new(at), name));
        }
        if flip_origin && let Some(workspace) = self.draft_workspace_mut() {
            workspace.from_origin = Some(!from_origin);
        }
        if load_next
            && let Some(next) = listing.next
            && !listing.loading_next
        {
            let cursors = self
                .branch_picker
                .cursors
                .entry(format!("{}\0{ref_query}", context.cwd))
                .or_default();
            if !cursors.contains(&next) {
                cursors.push(next);
            }
        }
        let Some(index) = picked else {
            return;
        };
        match rows.get(index) {
            Some(Row::Ref(r)) => {
                let r = (*r).clone();
                self.select_ref(context, &r);
            }
            Some(Row::Create) => self.create_ref(context, &new_name),
            None => {}
        }
    }

    /// `selectBranch`.
    fn select_ref(&mut self, context: &Context, r: &Value) {
        let Some(name) = r["name"].as_str().map(str::to_owned) else {
            return;
        };
        self.open_menu = None;
        if context.selecting_base {
            if let Some(workspace) = self.draft_workspace_mut() {
                workspace.branch = Some(name);
            }
            return;
        }
        let (cwd, worktree, reuse) =
            selection_target(&context.project_cwd, context.worktree.as_deref(), r);
        if reuse {
            self.set_owner_branch(&context.owner, Some(name), worktree);
            return;
        }
        let remote = r["isRemote"] == true;
        let shown = if remote { local_name(&name) } else { &name }.to_owned();
        self.branch_picker.serial += 1;
        let key = format!("branch-action:{}", self.branch_picker.serial);
        self.send_request(
            key.clone(),
            "vcs.switchRef",
            json!({"cwd": cwd, "refName": name}),
        );
        self.branch_picker.action = Some(Pending {
            key,
            owner: context.owner.clone(),
            shown,
            remote,
            create: false,
            worktree,
        });
    }

    /// `createRef`.
    fn create_ref(&mut self, context: &Context, name: &str) {
        if name.is_empty() {
            return;
        }
        self.open_menu = None;
        self.branch_picker.serial += 1;
        let key = format!("branch-action:{}", self.branch_picker.serial);
        self.send_request(
            key.clone(),
            "vcs.createRef",
            json!({"cwd": context.cwd, "refName": name, "switchRef": true}),
        );
        self.branch_picker.action = Some(Pending {
            key,
            owner: context.owner.clone(),
            shown: name.to_owned(),
            remote: false,
            create: true,
            worktree: context.worktree.clone(),
        });
    }

    /// `setThreadBranch`.
    fn set_owner_branch(
        &mut self,
        owner: &Owner,
        branch: Option<String>,
        worktree: Option<String>,
    ) {
        match owner {
            Owner::Draft(project) => {
                if let Some(workspace) = self.draft_workspace_mut()
                    && workspace.project == *project
                {
                    workspace.branch = branch;
                    workspace.worktree = worktree;
                }
            }
            Owner::Thread(id) => {
                let current = self
                    .state
                    .threads
                    .iter()
                    .find(|t| t.id == *id)
                    .map(|t| t.worktree_path.clone());
                // `stopSession`: the provider sessions leave with the old worktree.
                if current.is_some_and(|path| path != worktree)
                    && let Some(detail) = self
                        .current_detail()
                        .filter(|d| d.thread["id"] == id.as_str())
                {
                    let base = uuid::Uuid::new_v4().to_string();
                    for session in &detail.provider_sessions {
                        let Some(session_id) = session["id"].as_str() else {
                            continue;
                        };
                        self.dispatch(json!({
                            "type": "provider-session.detach",
                            "commandId": format!("{base}:detach:{session_id}"),
                            "threadId": id,
                            "providerSessionId": session_id,
                            "reason": "client-requested",
                        }));
                    }
                }
                let mut command = crate::model::command("thread.metadata.update", id);
                command["branch"] = json!(branch);
                command["worktreePath"] = json!(worktree);
                self.dispatch(command);
            }
        }
    }

    /// `vcs.listRefs`, `vcs.switchRef` and `vcs.createRef` answers; true when
    /// `key` was one.
    pub(crate) fn branch_result(&mut self, key: &str, result: &Result<Value, String>) -> bool {
        if key.starts_with("branch-refs:") {
            if let Some(page) = self.branch_picker.pages.get_mut(key) {
                *page = Some(result.clone());
            }
            return true;
        }
        if !key.starts_with("branch-action:") {
            return false;
        }
        let Some(action) = self
            .branch_picker
            .action
            .take_if(|action| action.key == key)
        else {
            return true;
        };
        match result {
            Ok(value) => {
                let returned = value["refName"].as_str().map(str::to_owned);
                let branch = if action.create || action.remote {
                    returned.unwrap_or(action.shown)
                } else {
                    action.shown
                };
                self.set_owner_branch(&action.owner, Some(branch), action.worktree);
            }
            Err(error) => {
                let title = if action.create {
                    "Failed to create and switch ref."
                } else {
                    "Failed to switch ref."
                };
                self.toasts
                    .add(Toast::new(Kind::Error, title).description(error.clone()));
            }
        }
        if let Some(cwd) = self.workspace_cwd() {
            self.revalidate_refs(&cwd);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ref_names_lose_their_whitespace() {
        assert_eq!(sanitize_ref_name("  new  branch\tname "), "new-branch-name");
        assert_eq!(local_name("origin/feature/x"), "feature/x");
        assert_eq!(local_name("main"), "main");
        assert!(includes("feature/new-branch", "new branch"));
        assert!(!includes("main", "dev"));
    }

    #[test]
    fn a_worktree_base_reads_from_origin_for_local_refs() {
        assert_eq!(
            trigger_label(Some("main"), true, Some(false), true),
            "From origin/main"
        );
        assert_eq!(
            trigger_label(Some("origin/dev"), true, Some(true), true),
            "From origin/dev"
        );
        assert_eq!(trigger_label(Some("main"), false, None, true), "main");
        assert_eq!(trigger_label(None, true, None, false), "Select ref");
    }

    #[test]
    fn a_ref_checked_out_elsewhere_reuses_its_worktree() {
        let elsewhere = json!({"name": "dev", "worktreePath": "/repo-dev"});
        assert_eq!(
            selection_target("/repo", None, &elsewhere),
            ("/repo-dev".into(), Some("/repo-dev".into()), true)
        );
        let default = json!({"name": "main", "isDefault": true});
        assert_eq!(
            selection_target("/repo", Some("/wt"), &default),
            ("/repo".into(), None, false)
        );
        let plain = json!({"name": "x"});
        assert_eq!(
            selection_target("/repo", Some("/wt"), &plain),
            ("/wt".into(), Some("/wt".into()), false)
        );
        assert_eq!(badge(&elsewhere, "/repo"), Some("worktree"));
        assert_eq!(badge(&default, "/repo"), Some("default"));
    }
}
