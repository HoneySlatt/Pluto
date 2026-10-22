//! Changed files card under an assistant message (`ChangedFilesTree.tsx`,
//! `lib/turnDiffTree.ts`, `DiffStatLabel.tsx`).
use std::collections::BTreeMap;

use eframe::egui::{CornerRadius, Pos2, Rect, Sense, Ui, Vec2};

use crate::app::App;
use crate::model::CheckpointFile;
use crate::theme::{self, Weight, font};
use crate::ui::widgets;

#[derive(Clone, Debug, PartialEq)]
pub enum Node {
    Directory {
        name: String,
        path: String,
        stat: (u64, u64),
        children: Vec<Node>,
    },
    File {
        name: String,
        path: String,
        stat: (u64, u64),
    },
}

#[derive(Default)]
struct Dir {
    stat: (u64, u64),
    dirs: BTreeMap<String, Dir>,
    files: Vec<(String, String, (u64, u64))>,
}

fn natural_key(name: &str) -> String {
    name.to_lowercase()
}

fn to_nodes(dir: &Dir, prefix: &str) -> Vec<Node> {
    let mut dirs: Vec<(&String, &Dir)> = dir.dirs.iter().collect();
    dirs.sort_by_key(|(name, _)| natural_key(name));
    let mut nodes: Vec<Node> = dirs
        .into_iter()
        .map(|(name, child)| {
            let path = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}/{name}")
            };
            compact(Node::Directory {
                name: name.clone(),
                children: to_nodes(child, &path),
                path,
                stat: child.stat,
            })
        })
        .collect();
    let mut files = dir.files.clone();
    files.sort_by_key(|(name, _, _)| natural_key(name));
    nodes.extend(
        files
            .into_iter()
            .map(|(name, path, stat)| Node::File { name, path, stat }),
    );
    nodes
}

/// Folds single-directory chains into one node (`src/ui` instead of `src` › `ui`).
fn compact(node: Node) -> Node {
    let Node::Directory {
        mut name,
        mut path,
        mut stat,
        mut children,
    } = node
    else {
        return node;
    };
    while children.len() == 1 && matches!(children[0], Node::Directory { .. }) {
        let Some(Node::Directory {
            name: child_name,
            path: child_path,
            stat: child_stat,
            children: grandchildren,
        }) = children.pop()
        else {
            break;
        };
        name = format!("{name}/{child_name}");
        path = child_path;
        stat = child_stat;
        children = grandchildren;
    }
    Node::Directory {
        name,
        path,
        stat,
        children,
    }
}

/// Port of `buildTurnDiffTree`.
pub fn build_tree(files: &[CheckpointFile]) -> Vec<Node> {
    let mut root = Dir::default();
    for file in files {
        let segments: Vec<&str> = file
            .path
            .split(['/', '\\'])
            .filter(|s| !s.is_empty())
            .collect();
        let Some((file_name, dirs)) = segments.split_last() else {
            continue;
        };
        let stat = (file.additions, file.deletions);
        root.stat.0 += stat.0;
        root.stat.1 += stat.1;
        let mut current = &mut root;
        for segment in dirs {
            current = current.dirs.entry((*segment).to_owned()).or_default();
            current.stat.0 += stat.0;
            current.stat.1 += stat.1;
        }
        current
            .files
            .push(((*file_name).to_owned(), file.path.clone(), stat));
    }
    to_nodes(&root, "")
}

/// `formatCompactDiffCount`.
fn compact_count(value: u64) -> String {
    let scaled = |value: f64, suffix: &str| {
        if value < 10.0 {
            let text = format!("{value:.1}");
            format!("{}{suffix}", text.trim_end_matches(".0"))
        } else {
            format!("{}{suffix}", value.round())
        }
    };
    match value {
        0..1_000 => value.to_string(),
        1_000..1_000_000 => scaled(value as f64 / 1e3, "k"),
        1_000_000..1_000_000_000 => scaled(value as f64 / 1e6, "m"),
        _ => scaled(value as f64 / 1e9, "b"),
    }
}

fn file_icon(name: &str) -> &'static str {
    let extension = name.rsplit('.').next().unwrap_or_default();
    match extension {
        "rs" => "lang-rust",
        "ts" | "mts" | "cts" => "lang-typescript",
        "tsx" | "jsx" => "lang-react",
        "js" | "mjs" | "cjs" => "lang-javascript",
        "py" => "lang-python",
        "sh" | "bash" | "zsh" => "lang-bash",
        "json" => "lang-json",
        "md" => "lang-markdown",
        "css" => "lang-css",
        "html" => "lang-html",
        "go" => "lang-go",
        "c" | "h" => "lang-c",
        "cpp" | "hpp" | "cc" => "lang-cpp",
        "rb" => "lang-ruby",
        "swift" => "lang-swift",
        "zig" => "lang-zig",
        "yml" | "yaml" => "lang-yml",
        "sql" => "lang-database",
        "vue" => "lang-vue",
        "svelte" => "lang-svelte",
        "scss" | "sass" => "lang-sass",
        _ => "file-text",
    }
}

impl App {
    /// Paints the card at the cursor; returns nothing, allocates its height.
    pub(crate) fn changed_files_card(
        &mut self,
        ui: &mut Ui,
        turn_id: &str,
        files: &[CheckpointFile],
        width: f32,
    ) {
        let tree = build_tree(files);
        let has_dirs = files.iter().any(|f| f.path.contains(['/', '\\']));
        let expand_key = format!("changed-files:{turn_id}");
        let all_expanded = self.expansion.groups.contains(&expand_key);
        let rows = flatten(
            &tree,
            0,
            all_expanded,
            &self.expansion.groups,
            turn_id,
            has_dirs,
        );
        let height = 40.0 + 8.0 + rows.len() as f32 * 28.0 + 8.0;
        let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
        ui.painter()
            .rect_filled(rect, CornerRadius::same(10), theme::overlay(0.016));

        // Header.
        let header = Rect::from_min_size(rect.min, Vec2::new(width, 40.0));
        let count = files.len();
        let label = format!("{count} changed file{}", if count == 1 { "" } else { "s" });
        let mut x = header.left() + 12.0;
        x += widgets::text_in(
            ui,
            Rect::from_min_max(Pos2::new(x, header.top()), header.max),
            &label,
            font(12.0, Weight::Medium),
            theme::foreground(),
        ) + 12.0;
        let additions: u64 = files.iter().map(|f| f.additions).sum();
        let deletions: u64 = files.iter().map(|f| f.deletions).sum();
        if additions + deletions > 0 {
            let mono = theme::font(12.0, Weight::Medium);
            let mono = eframe::egui::FontId::new(mono.size, eframe::egui::FontFamily::Monospace);
            x += widgets::text_in(
                ui,
                Rect::from_min_max(Pos2::new(x, header.top()), header.max),
                &format!("+{}", compact_count(additions)),
                mono.clone(),
                theme::diff_addition(),
            ) + 4.0;
            widgets::text_in(
                ui,
                Rect::from_min_max(Pos2::new(x, header.top()), header.max),
                &format!("-{}", compact_count(deletions)),
                mono,
                theme::diff_deletion(),
            );
        }
        let open = Rect::from_min_size(
            Pos2::new(header.right() - 12.0 - 79.0, header.top() + 8.0),
            Vec2::new(79.0, 24.0),
        );
        let response = ui.interact(open, ui.id().with(("open-diff", turn_id)), Sense::click());
        let color = if response.hovered() {
            theme::foreground()
        } else {
            theme::muted_foreground()
        };
        if response.hovered() {
            ui.painter()
                .rect_filled(open, CornerRadius::same(8), theme::accent());
        }
        theme::paint_icon(
            ui,
            "file-diff",
            Rect::from_min_size(open.min + Vec2::new(6.0, 0.0), Vec2::new(12.0, 24.0)),
            12.0,
            color,
        );
        widgets::text_in(
            ui,
            Rect::from_min_max(open.min + Vec2::new(20.0, 0.0), open.max),
            "Open diff",
            font(12.0, Weight::Medium),
            color,
        );
        if response.clicked() {
            self.open_diff(turn_id, None);
            // `onOpenTurnDiff(turnId, files[0]?.path)`: the tree selects the first file.
            self.diff_tree_selected = files.first().map(|f| f.path.clone());
        }
        if has_dirs {
            let toggle = Rect::from_min_size(
                Pos2::new(open.left() - 4.0 - 24.0, open.top()),
                Vec2::splat(24.0),
            );
            let icon = if all_expanded {
                "chevrons-down-up"
            } else {
                "chevrons-up-down"
            };
            let tip = if all_expanded {
                "Collapse all folders"
            } else {
                "Expand all folders"
            };
            if widgets::icon_button(
                ui,
                toggle,
                ("toggle-folders", turn_id),
                icon,
                12.0,
                theme::muted_foreground(),
                tip,
            )
            .clicked()
            {
                if all_expanded {
                    self.expansion.groups.remove(&expand_key);
                } else {
                    self.expansion.groups.insert(expand_key.clone());
                }
                // Per-folder overrides reset when the global state flips.
                let prefix = format!("dir:{turn_id}:");
                self.expansion.groups.retain(|k| !k.starts_with(&prefix));
                self.keep_scroll = true;
            }
        }

        // Tree rows.
        let mut y = header.bottom() + 8.0;
        for row in rows {
            let line = Rect::from_min_size(
                Pos2::new(rect.left() + 8.0, y),
                Vec2::new(width - 16.0, 28.0),
            );
            y += 28.0;
            let response = ui.interact(
                line,
                ui.id().with(("tree-row", turn_id, &row.path)),
                Sense::click(),
            );
            if response.hovered() {
                ui.painter()
                    .rect_filled(line, CornerRadius::same(6), theme::overlay(0.024));
            }
            let mut x = line.left() + 8.0 + row.depth as f32 * 14.0;
            let icon_rect =
                |x: f32| Rect::from_min_size(Pos2::new(x, line.top()), Vec2::new(14.0, 28.0));
            if row.directory {
                theme::paint_icon(
                    ui,
                    if row.expanded {
                        "chevron-down"
                    } else {
                        "chevron-right"
                    },
                    icon_rect(x),
                    14.0,
                    theme::alpha(theme::muted_foreground(), 0.7),
                );
                x += 22.0;
                theme::paint_icon(
                    ui,
                    if row.expanded {
                        "folder"
                    } else {
                        "folder-closed"
                    },
                    icon_rect(x),
                    14.0,
                    theme::alpha(theme::muted_foreground(), 0.75),
                );
                x += 22.0;
                let color = if response.hovered() {
                    theme::alpha(theme::foreground(), 0.9)
                } else {
                    theme::alpha(theme::muted_foreground(), 0.9)
                };
                widgets::text_in(
                    ui,
                    Rect::from_min_max(
                        Pos2::new(x, line.top()),
                        Pos2::new(line.right() - 80.0, line.bottom()),
                    ),
                    &row.name,
                    theme::mono(11.0),
                    color,
                );
            } else {
                if row.spacer {
                    x += 22.0;
                }
                theme::paint_icon(
                    ui,
                    file_icon(&row.name),
                    icon_rect(x),
                    14.0,
                    theme::alpha(theme::muted_foreground(), 0.7),
                );
                x += 22.0;
                let color = if response.hovered() {
                    theme::foreground()
                } else {
                    theme::alpha(theme::foreground(), 0.85)
                };
                widgets::text_in(
                    ui,
                    Rect::from_min_max(
                        Pos2::new(x, line.top()),
                        Pos2::new(line.right() - 80.0, line.bottom()),
                    ),
                    &row.name,
                    theme::mono(12.0),
                    color,
                );
            }
            if row.stat.0 + row.stat.1 > 0 {
                // Two right-aligned 4ch columns, 8px apart.
                let right = line.right() - 8.0;
                let mono = theme::mono(10.0);
                let deletions = format!("-{}", compact_count(row.stat.1));
                let additions = format!("+{}", compact_count(row.stat.0));
                let del_width = widgets::text_width(ui, &deletions, mono.clone());
                widgets::text_in(
                    ui,
                    Rect::from_min_max(
                        Pos2::new(right - del_width, line.top()),
                        Pos2::new(right + 1.0, line.bottom()),
                    ),
                    &deletions,
                    mono.clone(),
                    theme::diff_deletion(),
                );
                let column = widgets::text_width(ui, "0000", mono.clone());
                let add_right = right - column - 8.0;
                let add_width = widgets::text_width(ui, &additions, mono.clone());
                widgets::text_in(
                    ui,
                    Rect::from_min_max(
                        Pos2::new(add_right - add_width, line.top()),
                        Pos2::new(add_right + 1.0, line.bottom()),
                    ),
                    &additions,
                    mono,
                    theme::diff_addition(),
                );
            }
            if response.clicked() {
                if row.directory {
                    let key = format!("dir:{turn_id}:{}", row.path);
                    // An override flips the folder relative to the global state.
                    if !self.expansion.groups.remove(&key) {
                        self.expansion.groups.insert(key);
                    }
                    self.keep_scroll = true;
                } else {
                    self.open_diff(turn_id, Some(row.path.clone()));
                }
            }
        }
    }
}

struct TreeRow {
    name: String,
    path: String,
    depth: usize,
    directory: bool,
    expanded: bool,
    spacer: bool,
    stat: (u64, u64),
}

fn flatten(
    nodes: &[Node],
    depth: usize,
    all_expanded: bool,
    overrides: &std::collections::BTreeSet<String>,
    turn_id: &str,
    has_dirs: bool,
) -> Vec<TreeRow> {
    let mut rows = Vec::new();
    for node in nodes {
        match node {
            Node::Directory {
                name,
                path,
                stat,
                children,
            } => {
                let flipped = overrides.contains(&format!("dir:{turn_id}:{path}"));
                let expanded = all_expanded != flipped;
                rows.push(TreeRow {
                    name: name.clone(),
                    path: path.clone(),
                    depth,
                    directory: true,
                    expanded,
                    spacer: false,
                    stat: *stat,
                });
                if expanded {
                    rows.extend(flatten(
                        children,
                        depth + 1,
                        all_expanded,
                        overrides,
                        turn_id,
                        has_dirs,
                    ));
                }
            }
            Node::File { name, path, stat } => rows.push(TreeRow {
                name: name.clone(),
                path: path.clone(),
                depth,
                directory: false,
                expanded: false,
                spacer: has_dirs || depth > 0,
                stat: *stat,
            }),
        }
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str, additions: u64, deletions: u64) -> CheckpointFile {
        CheckpointFile {
            path: path.into(),
            kind: "modified".into(),
            additions,
            deletions,
        }
    }

    #[test]
    fn single_directory_chains_are_compacted() {
        let tree = build_tree(&[file("src/ui/a.rs", 3, 1), file("src/ui/b.rs", 2, 0)]);
        let Node::Directory {
            name,
            stat,
            children,
            ..
        } = &tree[0]
        else {
            panic!("expected a directory");
        };
        assert_eq!(name, "src/ui");
        assert_eq!(*stat, (5, 1));
        assert_eq!(children.len(), 2);
    }

    #[test]
    fn counts_are_compacted_like_upstream() {
        assert_eq!(compact_count(999), "999");
        assert_eq!(compact_count(1_500), "1.5k");
        assert_eq!(compact_count(12_400), "12k");
        assert_eq!(compact_count(2_000_000), "2m");
    }
}
