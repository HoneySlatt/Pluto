//! The desktop app's text context menu (`DesktopWindow.ts`'s `context-menu`
//! handler, an Electron menu): Copy Link over a link, then Cut, Copy, Paste
//! and Select All, enabled like Chromium's edit flags for message text.
use eframe::egui::{self, Ui};

use crate::app::App;
use crate::ui::context_menu::{self, Item};
use crate::ui::widgets::{CopyCleanup, TEXT_MENU_REQUEST};

impl App {
    pub(crate) fn text_menu(&mut self, ui: &mut Ui) {
        let request: Option<(egui::Pos2, Option<String>)> = ui
            .ctx()
            .data_mut(|d| d.remove_temp(egui::Id::new(TEXT_MENU_REQUEST)));
        let plugin = ui.ctx().plugin::<CopyCleanup>();
        if let Some((at, link)) = request {
            self.open_menu = None;
            // Read the selection now, as Chromium's edit flags do, before a
            // press on the menu clears egui's.
            let mut plugin = plugin.lock();
            let selected = plugin.has_range;
            plugin.captured = None;
            // Only a real selection is read: egui copies a whole label for
            // an empty one.
            if !plugin.select_all && selected {
                plugin.capture = 3;
                self.pending_copy = true;
            }
            let can_copy = plugin.select_all || selected;
            self.text_menu = Some((context_menu::State::new(at), link, can_copy));
        }
        let Some((state, link, can_copy)) = self.text_menu.take() else {
            plugin.lock().menu_open = false;
            return;
        };
        let mut items = Vec::new();
        if link.is_some() {
            items.push(Item::new("copy-link", "Copy Link"));
        }
        let shortcut = |item: Item, keys: &str| Item {
            detail: Some(keys.to_owned()),
            ..item
        };
        items.extend([
            Item {
                disabled: true,
                separator_before: link.is_some(),
                ..shortcut(Item::new("cut", "Cut"), "Ctrl+X")
            },
            Item {
                disabled: !can_copy,
                ..shortcut(Item::new("copy", "Copy"), "Ctrl+C")
            },
            Item {
                disabled: true,
                ..shortcut(Item::new("paste", "Paste"), "Ctrl+V")
            },
            shortcut(Item::new("select-all", "Select All"), "Ctrl+A"),
        ]);
        let mut open = Some(state);
        let clicked = context_menu::show(ui, &mut open, &items);
        plugin.lock().menu_open = open.is_some();
        if let Some(state) = open {
            self.text_menu = Some((state, link.clone(), can_copy));
        }
        match clicked.as_deref() {
            Some("copy-link") => {
                if let Some(url) = link {
                    ui.ctx().copy_text(url);
                }
            }
            Some("copy") => {
                if plugin.lock().select_all {
                    self.pending_copy = true;
                } else if let Some(text) = plugin.lock().captured.take() {
                    ui.ctx().copy_text(text);
                }
            }
            Some("select-all") => {
                ui.ctx()
                    .plugin::<egui::text_selection::LabelSelectionState>()
                    .lock()
                    .clear_selection();
                plugin.lock().select_all = true;
            }
            _ => {}
        }
    }
}
