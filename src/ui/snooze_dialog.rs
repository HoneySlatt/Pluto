//! The custom snooze dialog (`CustomSnoozeDialog.tsx`, the `Calendar` popup
//! from `ui/calendar.tsx`): a local date and time, or a duration, resolved by
//! `snooze::resolve_custom` and sent as `thread.snooze`.
//!
//! The time field edits its hour, minute and AM/PM segments like the native
//! input; the clock button's own picker is not ported.
use chrono::{Datelike, Duration, Local, NaiveDate, Timelike};
use eframe::egui::{
    self, Color32, CornerRadius, Key, Modifiers, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2,
};
use serde_json::json;

use crate::app::App;
use crate::snooze::CustomSnooze;
use crate::theme::{self, Weight, font};
use crate::ui::widgets;

const WIDTH: f32 = 384.0;
const UNITS: [&str; 3] = ["Minutes", "Hours", "Days"];

/// One open dialog, for the threads it snoozes.
pub struct SnoozeDialog {
    threads: Vec<String>,
    duration: bool,
    date: NaiveDate,
    hour: u32,
    minute: u32,
    /// The focused time segment (hour, minute, AM/PM) and its typed digits.
    segment: Option<usize>,
    typed: String,
    amount: String,
    unit: usize,
    calendar: Option<NaiveDate>,
    unit_open: bool,
    error: Option<&'static str>,
}

impl SnoozeDialog {
    pub fn new(threads: Vec<String>) -> Self {
        let initial = Local::now() + Duration::hours(1);
        Self {
            threads,
            duration: false,
            date: initial.date_naive(),
            hour: initial.hour(),
            minute: initial.minute(),
            segment: None,
            typed: String::new(),
            amount: "2".into(),
            unit: 1,
            calendar: None,
            unit_open: false,
            error: None,
        }
    }

    fn input(&self) -> CustomSnooze {
        if self.duration {
            CustomSnooze::For(self.amount.clone(), self.unit)
        } else {
            CustomSnooze::At(
                self.date
                    .and_hms_opt(self.hour, self.minute, 0)
                    .unwrap_or_default(),
            )
        }
    }

    /// Native time input keys: digits fill the segment, arrows step it or
    /// move between segments.
    fn time_keys(&mut self, ui: &Ui) {
        let Some(segment) = self.segment else {
            return;
        };
        let events = ui.input(|i| i.events.clone());
        for event in events {
            match event {
                egui::Event::Text(text) => {
                    for c in text.chars() {
                        if segment == 2 {
                            match c.to_ascii_lowercase() {
                                'a' if self.hour >= 12 => self.hour -= 12,
                                'p' if self.hour < 12 => self.hour += 12,
                                _ => {}
                            }
                        } else if let Some(digit) = c.to_digit(10) {
                            self.typed.push(c);
                            let value: u32 = self.typed.parse().unwrap_or(digit);
                            if segment == 0 {
                                let hour12 = value.clamp(1, 12) % 12;
                                self.hour = hour12 + if self.hour >= 12 { 12 } else { 0 };
                                if self.typed.len() >= 2 || value > 1 {
                                    self.typed.clear();
                                    self.segment = Some(1);
                                }
                            } else {
                                self.minute = value.min(59);
                                if self.typed.len() >= 2 || value > 5 {
                                    self.typed.clear();
                                    self.segment = Some(2);
                                }
                            }
                        }
                    }
                    self.error = None;
                }
                egui::Event::Key {
                    key, pressed: true, ..
                } => {
                    let step: i32 = match key {
                        Key::ArrowUp => 1,
                        Key::ArrowDown => -1,
                        _ => 0,
                    };
                    if step != 0 {
                        match segment {
                            0 => self.hour = (self.hour as i32 + step).rem_euclid(24) as u32,
                            1 => self.minute = (self.minute as i32 + step).rem_euclid(60) as u32,
                            _ => self.hour = (self.hour + 12) % 24,
                        }
                        self.typed.clear();
                        self.error = None;
                    }
                    match key {
                        Key::ArrowLeft => self.segment = Some(segment.saturating_sub(1)),
                        Key::ArrowRight | Key::Tab => self.segment = Some((segment + 1).min(2)),
                        _ => {}
                    }
                }
                _ => {}
            }
        }
    }
}

/// A `size="default"` button: 32px, 14px medium text, 11px padding.
pub(crate) fn dialog_button(ui: &mut Ui, rect: Rect, id: &str, label: &str, primary: bool) -> bool {
    let response = ui.interact(rect, ui.id().with(id), Sense::click());
    let (fill, stroke, color) = if primary {
        let fill = if response.hovered() {
            theme::primary_hover()
        } else {
            theme::primary()
        };
        (fill, fill, Color32::WHITE)
    } else {
        let fill = if response.hovered() {
            theme::overlay(0.06)
        } else {
            theme::control_surface()
        };
        (fill, theme::input(), theme::foreground())
    };
    ui.painter().rect(
        rect,
        CornerRadius::same(8),
        fill,
        Stroke::new(1.0, stroke),
        StrokeKind::Inside,
    );
    let width = widgets::text_width(ui, label, font(14.0, Weight::Medium));
    widgets::text_in(
        ui,
        Rect::from_min_max(
            Pos2::new(rect.center().x - width / 2.0, rect.top()),
            rect.max,
        ),
        label,
        font(14.0, Weight::Medium),
        color,
    );
    response.clicked()
}

/// The field surface of inputs and outline triggers.
pub(crate) fn field(ui: &Ui, rect: Rect, radius: u8) {
    ui.painter().rect(
        rect,
        CornerRadius::same(radius),
        theme::control_surface(),
        Stroke::new(1.0, theme::input()),
        StrokeKind::Inside,
    );
}

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

impl App {
    /// Opens the dialog for `threads` (`requestCustomSnooze`).
    pub(crate) fn request_custom_snooze(&mut self, threads: Vec<String>) {
        self.snooze_dialog = Some(SnoozeDialog::new(threads));
    }

    pub(crate) fn snooze_dialog(&mut self, ui: &mut Ui, screen: Rect) {
        let Some(mut dialog) = self.snooze_dialog.take() else {
            return;
        };
        let mut close = false;
        let mut submit = false;
        let error_height = if dialog.error.is_some() { 36.0 } else { 0.0 };
        let height = 279.0 + error_height;
        let d = Rect::from_min_size(
            Pos2::new(
                (screen.center().x - WIDTH / 2.0).round(),
                (screen.center().y - height / 2.0).round(),
            ),
            Vec2::new(WIDTH, height),
        );
        egui::Area::new(ui.id().with("snooze-dialog"))
            .order(egui::Order::Foreground)
            .fixed_pos(screen.min)
            .show(ui.ctx(), |ui| {
                let (backdrop, response) = ui.allocate_exact_size(screen.size(), Sense::click());
                ui.painter().rect_filled(
                    backdrop,
                    0.0,
                    theme::alpha(theme::background(), 200.0 / 255.0),
                );
                if response.clicked() && !ui.rect_contains_pointer(d) && dialog.calendar.is_none() {
                    close = true;
                }
                ui.painter().rect(
                    d,
                    CornerRadius::same(18),
                    theme::shade(12),
                    Stroke::new(1.0, theme::input()),
                    StrokeKind::Inside,
                );
                // Clicks on the dialog body blur the time segments.
                let body = ui.interact(d, ui.id().with("snooze-body"), Sense::click());
                if body.clicked() {
                    dialog.segment = None;
                }
                let close_rect = Rect::from_min_size(
                    Pos2::new(d.right() - 41.0, d.top() + 9.0),
                    Vec2::splat(32.0),
                );
                if widgets::icon_button(
                    ui,
                    close_rect,
                    "snooze-close",
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
                widgets::text(
                    ui,
                    Pos2::new(left, d.top() + 25.0),
                    "Custom snooze",
                    font(20.0, Weight::SemiBold),
                    theme::foreground(),
                    20.0,
                );
                widgets::text(
                    ui,
                    Pos2::new(left, d.top() + 53.0),
                    "Choose when snoozed threads return to your inbox.",
                    font(14.0, Weight::Regular),
                    theme::muted_foreground(),
                    20.0,
                );

                // Schedule type.
                let group =
                    Rect::from_min_size(Pos2::new(left, d.top() + 89.0), Vec2::new(334.0, 28.0));
                ui.painter()
                    .rect_filled(group, CornerRadius::same(10), theme::overlay(0.032));
                for (index, text) in ["Date and time", "Duration"].into_iter().enumerate() {
                    let button = Rect::from_min_size(
                        Pos2::new(group.left() + 2.0 + index as f32 * 166.0, group.top() + 2.0),
                        Vec2::new(164.0, 24.0),
                    );
                    let response =
                        ui.interact(button, ui.id().with(("snooze-mode", index)), Sense::click());
                    let active = dialog.duration == (index == 1);
                    if active {
                        ui.painter().rect_filled(
                            button,
                            CornerRadius::same(8),
                            theme::overlay(0.0576),
                        );
                    }
                    let color = if active || response.hovered() {
                        theme::foreground()
                    } else {
                        theme::muted_foreground()
                    };
                    let width = widgets::text_width(ui, text, font(14.0, Weight::Medium));
                    widgets::text_in(
                        ui,
                        Rect::from_min_max(
                            Pos2::new(button.center().x - width / 2.0, button.top()),
                            button.max,
                        ),
                        text,
                        font(14.0, Weight::Medium),
                        color,
                    );
                    if response.clicked() {
                        dialog.duration = index == 1;
                        dialog.error = None;
                        dialog.segment = None;
                    }
                }

                let right = d.left() + 198.0;
                let label_top = d.top() + 133.0;
                if dialog.duration {
                    label(ui, Pos2::new(left, label_top), "Snooze for");
                    let number = Rect::from_min_size(
                        Pos2::new(left, d.top() + 157.0),
                        Vec2::new(161.0, 32.0),
                    );
                    field(ui, number, 10);
                    for (icon, x, delta) in [
                        ("minus", number.left() + 4.0, -1.0),
                        ("plus", number.right() - 36.0, 1.0),
                    ] {
                        let button =
                            Rect::from_min_size(Pos2::new(x, number.top()), Vec2::new(32.0, 32.0));
                        let response = ui.interact(
                            button,
                            ui.id().with(("snooze-step", icon)),
                            Sense::click(),
                        );
                        theme::paint_icon(ui, icon, button, 16.0, theme::foreground());
                        if response.clicked() {
                            let value: f64 = dialog.amount.trim().parse().unwrap_or(0.0);
                            dialog.amount = format!("{}", (value + delta).max(0.0));
                            dialog.error = None;
                        }
                    }
                    let input = Rect::from_min_max(
                        Pos2::new(number.left() + 36.0, number.top() + 6.0),
                        Pos2::new(number.right() - 36.0, number.bottom() - 6.0),
                    );
                    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(input));
                    let before = dialog.amount.clone();
                    child.add(
                        egui::TextEdit::singleline(&mut dialog.amount)
                            .id(egui::Id::new("snooze-amount"))
                            .frame(egui::Frame::NONE)
                            .margin(egui::Margin::ZERO)
                            .font(font(14.0, Weight::Regular))
                            .text_color(theme::foreground())
                            .horizontal_align(egui::Align::Center)
                            .desired_width(input.width()),
                    );
                    if dialog.amount != before {
                        dialog.error = None;
                    }

                    label(ui, Pos2::new(right, label_top), "Unit");
                    let select = Rect::from_min_size(
                        Pos2::new(right, d.top() + 157.0),
                        Vec2::new(161.0, 32.0),
                    );
                    let response = ui.interact(select, ui.id().with("snooze-unit"), Sense::click());
                    field(ui, select, 10);
                    widgets::text_in(
                        ui,
                        Rect::from_min_max(
                            Pos2::new(select.left() + 12.0, select.top()),
                            select.max,
                        ),
                        UNITS[dialog.unit],
                        font(14.0, Weight::Medium),
                        theme::foreground(),
                    );
                    theme::paint_icon(
                        ui,
                        "chevron-down",
                        Rect::from_min_size(
                            Pos2::new(select.right() - 20.0, select.center().y - 6.0),
                            Vec2::splat(12.0),
                        ),
                        12.0,
                        theme::muted_foreground(),
                    );
                    if response.clicked() {
                        dialog.unit_open = !dialog.unit_open;
                    }
                    if dialog.unit_open {
                        let mut open = Some("snooze-unit".to_owned());
                        let entries: Vec<widgets::MenuEntry> = UNITS
                            .iter()
                            .map(|unit| widgets::MenuEntry::Item(widgets::MenuItem::new(unit)))
                            .collect();
                        let chosen = widgets::menu_aligned(
                            ui,
                            &mut open,
                            "snooze-unit",
                            select,
                            false,
                            false,
                            &entries,
                        );
                        if let Some(index) = chosen {
                            dialog.unit = index;
                            dialog.error = None;
                        }
                        dialog.unit_open = open.is_some() && chosen.is_none();
                    }
                } else {
                    label(ui, Pos2::new(left, label_top), "Date");
                    let date = Rect::from_min_size(
                        Pos2::new(left, d.top() + 155.0),
                        Vec2::new(161.0, 32.0),
                    );
                    let response = ui.interact(date, ui.id().with("snooze-date"), Sense::click());
                    let fill = if response.hovered() {
                        theme::overlay(0.06)
                    } else {
                        theme::control_surface()
                    };
                    ui.painter().rect(
                        date,
                        CornerRadius::same(8),
                        fill,
                        Stroke::new(1.0, theme::input()),
                        StrokeKind::Inside,
                    );
                    widgets::text_in(
                        ui,
                        Rect::from_min_max(Pos2::new(date.left() + 12.0, date.top()), date.max),
                        &dialog.date.format("%b %-d, %Y").to_string(),
                        font(14.0, Weight::Medium),
                        theme::foreground(),
                    );
                    theme::paint_icon(
                        ui,
                        "calendar",
                        Rect::from_min_size(
                            Pos2::new(date.right() - 26.0, date.center().y - 8.0),
                            Vec2::splat(16.0),
                        ),
                        16.0,
                        theme::muted_foreground(),
                    );
                    if response.clicked() {
                        dialog.calendar = match dialog.calendar {
                            Some(_) => None,
                            None => dialog.date.with_day(1),
                        };
                        dialog.segment = None;
                    }

                    label(ui, Pos2::new(right, label_top), "Time");
                    let time = Rect::from_min_size(
                        Pos2::new(right, d.top() + 157.0),
                        Vec2::new(161.0, 32.0),
                    );
                    field(ui, time, 10);
                    let hour12 = match dialog.hour % 12 {
                        0 => 12,
                        h => h,
                    };
                    let parts = [
                        format!("{hour12:02}"),
                        format!("{:02}", dialog.minute),
                        (if dialog.hour >= 12 { "PM" } else { "AM" }).to_owned(),
                    ];
                    let text_font = font(14.0, Weight::Regular);
                    let mut x = time.left() + 13.0;
                    for (index, part) in parts.iter().enumerate() {
                        let width = widgets::text_width(ui, part, text_font.clone());
                        let segment = Rect::from_min_size(
                            Pos2::new(x - 1.0, time.center().y - 10.0),
                            Vec2::new(width + 2.0, 20.0),
                        );
                        let response = ui.interact(
                            segment,
                            ui.id().with(("snooze-time", index)),
                            Sense::click(),
                        );
                        if dialog.segment == Some(index) {
                            ui.painter().rect_filled(
                                segment,
                                CornerRadius::same(2),
                                theme::alpha(theme::primary(), 0.5),
                            );
                        }
                        widgets::text_in(
                            ui,
                            Rect::from_min_size(
                                Pos2::new(x, time.top()),
                                Vec2::new(width + 1.0, time.height()),
                            ),
                            part,
                            text_font.clone(),
                            theme::foreground(),
                        );
                        if response.clicked() {
                            dialog.segment = Some(index);
                            dialog.typed.clear();
                            dialog.calendar = None;
                        }
                        x += width;
                        let separator = match index {
                            0 => ":",
                            1 => " ",
                            _ => "",
                        };
                        if !separator.is_empty() {
                            let sep_width = widgets::text_width(ui, separator, text_font.clone());
                            widgets::text_in(
                                ui,
                                Rect::from_min_size(
                                    Pos2::new(x, time.top()),
                                    Vec2::new(sep_width + 1.0, time.height()),
                                ),
                                separator,
                                text_font.clone(),
                                theme::foreground(),
                            );
                            x += sep_width;
                        }
                    }
                    theme::paint_icon(
                        ui,
                        "clock",
                        Rect::from_min_size(
                            Pos2::new(time.right() - 29.0, time.center().y - 8.0),
                            Vec2::splat(16.0),
                        ),
                        16.0,
                        theme::foreground(),
                    );
                    dialog.time_keys(ui);
                }

                if let Some(error) = dialog.error {
                    widgets::text(
                        ui,
                        Pos2::new(left, d.top() + 205.0),
                        error,
                        font(14.0, Weight::Regular),
                        theme::error_foreground(),
                        20.0,
                    );
                }

                // Footer.
                let footer = Rect::from_min_max(
                    Pos2::new(d.left() + 1.0, d.bottom() - 66.0),
                    Pos2::new(d.right() - 1.0, d.bottom() - 1.0),
                );
                ui.painter().rect_filled(
                    footer,
                    CornerRadius {
                        nw: 0,
                        ne: 0,
                        sw: 17,
                        se: 17,
                    },
                    theme::overlay(0.0216),
                );
                ui.painter().rect_filled(
                    Rect::from_min_size(footer.min, Vec2::new(footer.width(), 1.0)),
                    0.0,
                    theme::border(),
                );
                let snooze = Rect::from_min_size(
                    Pos2::new(d.right() - 25.0 - 73.0, footer.top() + 17.0),
                    Vec2::new(73.0, 32.0),
                );
                let cancel = Rect::from_min_size(
                    Pos2::new(snooze.left() - 7.0 - 70.0, snooze.top()),
                    Vec2::new(70.0, 32.0),
                );
                if dialog_button(ui, cancel, "snooze-cancel", "Cancel", false) {
                    close = true;
                }
                if dialog_button(ui, snooze, "snooze-submit", "Snooze", true) {
                    submit = true;
                }

                if let Some(month) = dialog.calendar {
                    let anchor = Rect::from_min_size(
                        Pos2::new(left, d.top() + 155.0),
                        Vec2::new(161.0, 32.0),
                    );
                    snooze_calendar(ui, &mut dialog, anchor, month);
                }
            });

        let (escape, enter) = ui.input_mut(|i| {
            (
                i.consume_key(Modifiers::NONE, Key::Escape),
                i.key_pressed(Key::Enter),
            )
        });
        if escape {
            if dialog.calendar.is_some() || dialog.unit_open {
                dialog.calendar = None;
                dialog.unit_open = false;
            } else {
                close = true;
            }
        }
        if submit || enter && dialog.calendar.is_none() {
            match crate::snooze::resolve_custom(&dialog.input(), chrono::Utc::now()) {
                Some(until) => {
                    for thread in &dialog.threads {
                        let mut cmd = crate::model::command("thread.snooze", thread);
                        cmd["snoozedUntil"] = json!(until);
                        self.park(thread, cmd, crate::ui::park::Park::Snooze, &dialog.threads);
                    }
                    return;
                }
                None => {
                    dialog.error = Some(if dialog.duration {
                        "Enter a positive duration."
                    } else {
                        "Choose a valid date and time in the future."
                    });
                }
            }
        }
        if !close {
            self.snooze_dialog = Some(dialog);
        }
    }
}

/// The month grid under the date button; days before today are struck out.
fn snooze_calendar(ui: &mut Ui, dialog: &mut SnoozeDialog, anchor: Rect, month: NaiveDate) {
    let today = Local::now().date_naive();
    let first_weekday = month.weekday().num_days_from_sunday() as i64;
    let grid_start = month - Duration::days(first_weekday);
    let next_month = if month.month() == 12 {
        NaiveDate::from_ymd_opt(month.year() + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(month.year(), month.month() + 1, 1)
    }
    .unwrap_or(month);
    let days_shown = (next_month - grid_start).num_days();
    let rows = ((days_shown + 6) / 7) as f32;
    let popup = Rect::from_min_size(
        Pos2::new(anchor.left(), anchor.bottom() + 4.0),
        Vec2::new(
            270.0,
            9.0 + 36.0 + 4.0 + 36.0 + 1.0 + rows * 38.0 - 2.0 + 10.0,
        ),
    );
    egui::Area::new(ui.id().with("snooze-calendar"))
        .order(egui::Order::Tooltip)
        .fixed_pos(popup.min)
        .show(ui.ctx(), |ui| {
            ui.painter().rect(
                popup,
                CornerRadius::same(14),
                theme::popover(),
                Stroke::new(1.0, theme::alpha(theme::foreground(), 0.1)),
                StrokeKind::Inside,
            );
            let header = Rect::from_min_size(
                Pos2::new(popup.left() + 9.0, popup.top() + 9.0),
                Vec2::new(252.0, 36.0),
            );
            let title = month.format("%B %Y").to_string();
            let width = widgets::text_width(ui, &title, font(14.0, Weight::Medium));
            widgets::text_in(
                ui,
                Rect::from_min_max(
                    Pos2::new(header.center().x - width / 2.0, header.top()),
                    header.max,
                ),
                &title,
                font(14.0, Weight::Medium),
                theme::foreground(),
            );
            for (icon, x, forward) in [
                ("chevron-left", popup.left() + 11.0, false),
                ("chevron-right", popup.right() - 50.0, true),
            ] {
                let button = Rect::from_min_size(Pos2::new(x, header.top()), Vec2::new(36.0, 36.0));
                let response =
                    ui.interact(button, ui.id().with(("calendar-nav", icon)), Sense::click());
                if response.hovered() {
                    ui.painter()
                        .rect_filled(button, CornerRadius::same(10), theme::accent());
                }
                theme::paint_icon(ui, icon, button, 16.0, theme::foreground());
                if response.clicked() {
                    dialog.calendar = Some(if forward {
                        next_month
                    } else {
                        (month - Duration::days(1)).with_day(1).unwrap_or(month)
                    });
                }
            }
            let grid_left = popup.left() + 9.0;
            let weekday_top = header.bottom() + 4.0;
            for (index, day) in ["Su", "Mo", "Tu", "We", "Th", "Fr", "Sa"]
                .iter()
                .enumerate()
            {
                let cell = Rect::from_min_size(
                    Pos2::new(grid_left + index as f32 * 36.0, weekday_top),
                    Vec2::splat(36.0),
                );
                let width = widgets::text_width(ui, day, font(12.0, Weight::Medium));
                widgets::text_in(
                    ui,
                    Rect::from_min_max(
                        Pos2::new(cell.center().x - width / 2.0, cell.top()),
                        cell.max,
                    ),
                    day,
                    font(12.0, Weight::Medium),
                    theme::alpha(theme::muted_foreground(), 0.72),
                );
            }
            let days_top = weekday_top + 37.0;
            for offset in 0..days_shown {
                let day = grid_start + Duration::days(offset);
                let row = (offset / 7) as f32;
                let column = (offset % 7) as f32;
                let cell = Rect::from_min_size(
                    Pos2::new(grid_left + column * 36.0, days_top + row * 38.0),
                    Vec2::splat(36.0),
                );
                let disabled = day < today;
                let outside = day.month() != month.month();
                let selected = day == dialog.date;
                let response = ui.interact(
                    cell,
                    ui.id().with(("calendar-day", offset)),
                    if disabled {
                        Sense::hover()
                    } else {
                        Sense::click()
                    },
                );
                if selected {
                    ui.painter()
                        .rect_filled(cell, CornerRadius::same(10), theme::primary());
                } else if response.hovered() && !disabled {
                    ui.painter()
                        .rect_filled(cell, CornerRadius::same(10), theme::accent());
                }
                let color = if selected {
                    Color32::WHITE
                } else if disabled || outside {
                    theme::alpha(theme::muted_foreground(), 0.72)
                } else {
                    theme::foreground()
                };
                let text = day.day().to_string();
                let width = widgets::text_width(ui, &text, font(14.0, Weight::Regular));
                let text_left = cell.center().x - width / 2.0;
                widgets::text_in(
                    ui,
                    Rect::from_min_max(Pos2::new(text_left, cell.top()), cell.max),
                    &text,
                    font(14.0, Weight::Regular),
                    color,
                );
                if day == today {
                    // Today's marker under the number.
                    ui.painter().circle_filled(
                        Pos2::new(cell.center().x, cell.top() + 29.0),
                        1.5,
                        color,
                    );
                }
                if disabled {
                    let y = cell.center().y + 1.0;
                    ui.painter().line_segment(
                        [Pos2::new(text_left, y), Pos2::new(text_left + width, y)],
                        Stroke::new(1.0, color),
                    );
                }
                if response.clicked() {
                    dialog.date = day;
                    dialog.calendar = None;
                    dialog.error = None;
                }
            }
            // Clicks outside the popup close it.
            let outside = ui
                .input(|i| i.pointer.interact_pos())
                .is_some_and(|pos| !popup.contains(pos) && !anchor.contains(pos));
            if ui.input(|i| i.pointer.any_click()) && outside {
                dialog.calendar = None;
            }
        });
}
