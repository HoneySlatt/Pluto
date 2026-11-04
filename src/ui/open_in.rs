//! "Open in editor" (`chat/OpenInPicker.tsx`, `editorPreferences.ts`): the
//! header's Open split button and the Files surface's compact one open the
//! workspace, or the shown file, with `shell.openInEditor` in the editor
//! last used (else the first installed one in `EDITORS` order); the chevron
//! lists the editors the server found on its PATH (`availableEditors`).
//! Upstream's remote-over-SSH links are not ported: this client opens on
//! the server's machine. Editors without a bundled mark show none.
use eframe::egui::{Pos2, Ui};
use serde_json::json;

use crate::app::App;
use crate::ui::context_menu::{self, Item};

/// `EDITORS`: id, label and bundled mark, in upstream's order.
const EDITORS: [(&str, &str, Option<&str>); 21] = [
    ("cursor", "Cursor", Some("provider-cursor")),
    ("trae", "Trae", None),
    ("kiro", "Kiro", None),
    ("vscode", "VS Code", None),
    ("vscode-insiders", "VS Code Insiders", None),
    ("vscodium", "VSCodium", None),
    ("zed", "Zed", Some("zed")),
    ("antigravity", "Antigravity", Some("provider-antigravity")),
    ("idea", "IntelliJ IDEA", None),
    ("aqua", "Aqua", None),
    ("clion", "CLion", None),
    ("datagrip", "DataGrip", None),
    ("dataspell", "DataSpell", None),
    ("goland", "GoLand", None),
    ("phpstorm", "PhpStorm", None),
    ("pycharm", "PyCharm", None),
    ("rider", "Rider", None),
    ("rubymine", "RubyMine", None),
    ("rustrover", "RustRover", None),
    ("webstorm", "WebStorm", None),
    // `getLocalFileManagerName` on Linux, with the generic folder.
    ("file-manager", "Files", Some("folder-closed")),
];

const REQUEST: &str = "open-in-editor";

/// The editor's label (`EDITORS`).
pub(crate) fn editor_label(id: &str) -> &str {
    EDITORS
        .iter()
        .find(|(editor, ..)| *editor == id)
        .map_or(id, |(_, label, _)| label)
}

/// The editor's mark, and whether it is a brand (drawn in the text color)
/// rather than the generic folder (muted).
pub(crate) fn editor_icon(id: &str) -> Option<(&'static str, bool)> {
    EDITORS
        .iter()
        .find(|(editor, ..)| *editor == id)
        .and_then(|(_, _, icon)| *icon)
        .map(|icon| (icon, icon != "folder-closed"))
}

impl App {
    /// The installed editors, in `EDITORS` order.
    pub(crate) fn available_editors(&self) -> Vec<&'static str> {
        let listed = &self.state.config["availableEditors"];
        EDITORS
            .iter()
            .map(|(id, ..)| *id)
            .filter(|id| {
                listed
                    .as_array()
                    .is_some_and(|ids| ids.iter().any(|v| v == id))
            })
            .collect()
    }

    /// `usePreferredEditor`: the last used editor while installed, else
    /// the first installed one.
    pub(crate) fn preferred_editor(&self) -> Option<&'static str> {
        let available = self.available_editors();
        self.last_editor
            .as_deref()
            .and_then(|last| available.iter().copied().find(|id| *id == last))
            .or_else(|| available.first().copied())
    }

    /// Opens `path` (a folder or file on the server's machine) in `editor`,
    /// which becomes the preferred one.
    pub(crate) fn open_in_editor(&mut self, path: &str, editor: &str) {
        self.send_request(
            REQUEST.into(),
            "shell.openInEditor",
            json!({"cwd": path, "editor": editor}),
        );
        if self.last_editor.as_deref() != Some(editor) {
            self.last_editor = Some(editor.to_owned());
            crate::app::save_ui_state(self);
        }
    }

    /// The request's failure as the status toast; `false` for other keys.
    pub(crate) fn open_in_result(
        &mut self,
        key: &str,
        result: &Result<serde_json::Value, String>,
    ) -> bool {
        if key != REQUEST {
            return false;
        }
        if let Err(error) = result {
            self.notify(
                crate::ui::toasts::Kind::Error,
                format!("Could not open in editor: {error}"),
            );
        }
        true
    }

    /// Opens the editor list for `path`, its right edge at `at`.
    pub(crate) fn toggle_open_in_menu(&mut self, path: Option<String>, at: Pos2) {
        self.open_in_menu = match (&self.open_in_menu, path) {
            (Some(_), _) | (None, None) => None,
            (None, Some(path)) => Some((path, context_menu::State::dropdown(at).aligned_end())),
        };
    }

    /// The editor list (`MenuPopup align="end"`): "No installed editors
    /// found", or each editor with the favorite shortcut on the preferred.
    pub(crate) fn open_in_menu(&mut self, ui: &mut Ui) {
        let Some((path, state)) = self.open_in_menu.take() else {
            return;
        };
        let preferred = self.preferred_editor();
        let shortcut = crate::ui::shortcuts::effective_shortcut_label(
            &self.state.config["keybindings"],
            "editor.openFavorite",
            &[("isDesktop", true)],
        );
        let available = self.available_editors();
        let items: Vec<Item> = if available.is_empty() {
            let mut none = Item::new("none", "No installed editors found");
            none.disabled = true;
            vec![none]
        } else {
            available
                .iter()
                .map(|id| {
                    let label = EDITORS
                        .iter()
                        .find(|(editor, ..)| editor == id)
                        .map_or("Editor", |(_, label, _)| label);
                    let mut item = Item::new(*id, label);
                    if let Some((icon, brand)) = editor_icon(id) {
                        item.icon = Some(icon);
                        item.icon_tint = brand.then(crate::theme::foreground);
                    }
                    if Some(*id) == preferred {
                        item.detail = shortcut.clone();
                    }
                    item
                })
                .collect()
        };
        let mut open = Some(state);
        let clicked = context_menu::show(ui, &mut open, &items);
        if let Some(state) = open {
            self.open_in_menu = Some((path.clone(), state));
        }
        if let Some(editor) = clicked.filter(|id| id != "none") {
            self.open_in_editor(&path, &editor);
        }
    }

    /// `editor.openFavorite`: the workspace in the preferred editor.
    pub(crate) fn open_in_favorite(&mut self) -> bool {
        match (self.workspace_cwd(), self.preferred_editor()) {
            (Some(cwd), Some(editor)) => {
                self.open_in_editor(&cwd, editor);
                true
            }
            _ => false,
        }
    }
}
