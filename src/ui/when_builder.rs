//! The keybinding when-clause editor (`WhenExpressionBuilder` and
//! `WhenExpressionNodeEditor` in `settings/KeybindingsSettings.tsx`, the
//! parser from `shared/keybindings.ts`): an expression field kept in sync
//! with a tree of conditions and and/or groups.
//!
//! Not ported: the unknown-variable warnings.
use eframe::egui::{self, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};
use serde_json::Value;

use crate::theme::{self, Weight, font};
use crate::ui::widgets;

const MAX_DEPTH: usize = 64;
pub(crate) const WIDTH: f32 = 578.0;
const ROW: f32 = 28.0;
const GAP: f32 = 8.0;
const INDENT: f32 = 16.0;

/// `KeybindingWhenNode`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum When {
    Id(String),
    Not(Box<When>),
    And(Box<When>, Box<When>),
    Or(Box<When>, Box<When>),
}

impl When {
    pub(crate) fn from_value(value: &Value) -> Option<Self> {
        Some(match value["type"].as_str()? {
            "identifier" => When::Id(value["name"].as_str()?.to_owned()),
            "not" => When::Not(Box::new(Self::from_value(&value["node"])?)),
            "and" => When::And(
                Box::new(Self::from_value(&value["left"])?),
                Box::new(Self::from_value(&value["right"])?),
            ),
            "or" => When::Or(
                Box::new(Self::from_value(&value["left"])?),
                Box::new(Self::from_value(&value["right"])?),
            ),
            _ => return None,
        })
    }

    /// `whenAstToExpression`.
    pub(crate) fn expression(&self) -> String {
        let wrap = |node: &When| match node {
            When::Id(_) | When::Not(_) => node.expression(),
            _ => format!("({})", node.expression()),
        };
        match self {
            When::Id(name) => name.clone(),
            When::Not(node) => format!("!{}", wrap(node)),
            When::And(left, right) => format!("{} && {}", wrap(left), wrap(right)),
            When::Or(left, right) => format!("{} || {}", wrap(left), wrap(right)),
        }
    }

    fn operator(&self) -> Option<bool> {
        match self {
            When::And(..) => Some(false),
            When::Or(..) => Some(true),
            _ => None,
        }
    }

    /// `flattenWhenChildren`: the operands of a run of one operator.
    fn children(&self, or: bool) -> Vec<When> {
        match (self, or) {
            (When::And(left, right), false) | (When::Or(left, right), true) => {
                let mut children = left.children(or);
                children.extend(right.children(or));
                children
            }
            _ => vec![self.clone()],
        }
    }

    /// `conditionParts`: an identifier, possibly negated.
    fn condition(&self) -> Option<(&str, bool)> {
        match self {
            When::Id(name) => Some((name, false)),
            When::Not(node) => match node.as_ref() {
                When::Id(name) => Some((name, true)),
                _ => None,
            },
            _ => None,
        }
    }
}

/// `buildWhenExpressionGroup`: operands folded left with one operator.
fn group(children: Vec<When>, or: bool) -> Option<When> {
    let mut iter = children.into_iter();
    let first = iter.next()?;
    Some(iter.fold(first, |left, right| {
        if or {
            When::Or(Box::new(left), Box::new(right))
        } else {
            When::And(Box::new(left), Box::new(right))
        }
    }))
}

fn default_condition() -> When {
    When::Id(DEFAULT_VARIABLE.into())
}

/// `defaultWhenGroup`: the default condition and its negation.
fn default_group(or: bool) -> When {
    group(
        vec![
            default_condition(),
            When::Not(Box::new(default_condition())),
        ],
        or,
    )
    .expect("two operands")
}

/// `parseKeybindingWhenExpression`; `None` for an invalid expression.
pub(crate) fn parse(expression: &str) -> Option<When> {
    #[derive(PartialEq)]
    enum Token {
        And,
        Or,
        Not,
        Open,
        Close,
        Id(String),
    }
    let mut tokens = Vec::new();
    let chars: Vec<char> = expression.chars().collect();
    let mut index = 0;
    while index < chars.len() {
        let c = chars[index];
        if c.is_whitespace() {
            index += 1;
        } else if c == '&' && chars.get(index + 1) == Some(&'&') {
            tokens.push(Token::And);
            index += 2;
        } else if c == '|' && chars.get(index + 1) == Some(&'|') {
            tokens.push(Token::Or);
            index += 2;
        } else if c == '!' {
            tokens.push(Token::Not);
            index += 1;
        } else if c == '(' {
            tokens.push(Token::Open);
            index += 1;
        } else if c == ')' {
            tokens.push(Token::Close);
            index += 1;
        } else if c.is_ascii_alphabetic() || c == '_' {
            let start = index;
            while index < chars.len()
                && (chars[index].is_ascii_alphanumeric() || matches!(chars[index], '_' | '.' | '-'))
            {
                index += 1;
            }
            tokens.push(Token::Id(chars[start..index].iter().collect()));
        } else {
            return None;
        }
    }
    if tokens.is_empty() {
        return None;
    }
    struct Parser {
        tokens: Vec<Token>,
        index: usize,
    }
    impl Parser {
        fn primary(&mut self, depth: usize) -> Option<When> {
            if depth > MAX_DEPTH {
                return None;
            }
            match self.tokens.get(self.index)? {
                Token::Id(name) => {
                    let name = name.clone();
                    self.index += 1;
                    Some(When::Id(name))
                }
                Token::Open => {
                    self.index += 1;
                    let node = self.or(depth + 1)?;
                    (self.tokens.get(self.index) == Some(&Token::Close)).then_some(())?;
                    self.index += 1;
                    Some(node)
                }
                _ => None,
            }
        }
        fn unary(&mut self, depth: usize) -> Option<When> {
            let mut nots = 0;
            while self.tokens.get(self.index) == Some(&Token::Not) {
                self.index += 1;
                nots += 1;
                if nots > MAX_DEPTH {
                    return None;
                }
            }
            let mut node = self.primary(depth)?;
            for _ in 0..nots {
                node = When::Not(Box::new(node));
            }
            Some(node)
        }
        fn and(&mut self, depth: usize) -> Option<When> {
            let mut left = self.unary(depth)?;
            while self.tokens.get(self.index) == Some(&Token::And) {
                self.index += 1;
                left = When::And(Box::new(left), Box::new(self.unary(depth)?));
            }
            Some(left)
        }
        fn or(&mut self, depth: usize) -> Option<When> {
            let mut left = self.and(depth)?;
            while self.tokens.get(self.index) == Some(&Token::Or) {
                self.index += 1;
                left = When::Or(Box::new(left), Box::new(self.and(depth)?));
            }
            Some(left)
        }
    }
    let mut parser = Parser { tokens, index: 0 };
    let ast = parser.or(0)?;
    (parser.index == parser.tokens.len()).then_some(ast)
}

/// `buildWhenVariableOptions`: the core variables first, then the others
/// the default bindings use, alphabetically.
pub(crate) fn variables() -> Vec<String> {
    let mut extra: Vec<String> = Vec::new();
    for (_, _, when) in crate::ui::keybindings::default_whens() {
        if let Some(node) = parse(when) {
            collect(&node, &mut extra);
        }
    }
    extra.retain(|name| !CORE.contains(&name.as_str()));
    extra.sort();
    extra.dedup();
    CORE.iter().map(|s| (*s).to_owned()).chain(extra).collect()
}

fn collect(node: &When, out: &mut Vec<String>) {
    match node {
        When::Id(name) => out.push(name.clone()),
        When::Not(node) => collect(node, out),
        When::And(left, right) | When::Or(left, right) => {
            collect(left, out);
            collect(right, out);
        }
    }
}

const CORE: [&str; 6] = [
    "terminalFocus",
    "terminalOpen",
    "isWeb",
    "isDesktop",
    "true",
    "false",
];
/// `DEFAULT_WHEN_VARIABLE`: the first non-literal variable.
const DEFAULT_VARIABLE: &str = "terminalFocus";

/// The builder's draft: the tree, the typed expression and which select is open.
pub(crate) struct WhenDraft {
    pub value: Option<When>,
    pub text: String,
    pub valid: bool,
    /// The open select: its node path, whether it picks the operator, and its anchor.
    menu: Option<(Vec<usize>, bool, Rect)>,
}

impl WhenDraft {
    pub(crate) fn new(value: Option<When>) -> Self {
        Self {
            text: value.as_ref().map(When::expression).unwrap_or_default(),
            value,
            valid: true,
            menu: None,
        }
    }

    pub(crate) fn expression(&self) -> String {
        self.value
            .as_ref()
            .map(When::expression)
            .unwrap_or_default()
    }

    pub(crate) fn menu_open(&self) -> bool {
        self.menu.is_some()
    }

    fn set(&mut self, value: Option<When>) {
        self.text = value.as_ref().map(When::expression).unwrap_or_default();
        self.valid = true;
        self.value = value;
    }
}

/// Edits applied after painting, addressed by child-index paths.
enum Edit {
    Replace(Vec<usize>, Option<When>),
}

/// Outline `size="compact"` button with an optional leading plus.
fn compact_button(
    ui: &mut Ui,
    rect: Rect,
    id: impl std::hash::Hash + std::fmt::Debug,
    label: &str,
    primary: bool,
) -> bool {
    let response = ui.interact(rect, ui.id().with(("when-button", id)), Sense::click());
    let (fill, stroke, color, icon) = if primary {
        let fill = if response.hovered() {
            theme::primary_hover()
        } else {
            theme::primary()
        };
        (fill, fill, Color32::WHITE, Color32::WHITE)
    } else {
        let fill = if response.hovered() {
            theme::overlay(0.06)
        } else {
            theme::control_surface()
        };
        (
            fill,
            theme::input(),
            theme::foreground(),
            theme::muted_foreground(),
        )
    };
    ui.painter().rect(
        rect,
        CornerRadius::same(8),
        fill,
        Stroke::new(1.0, stroke),
        StrokeKind::Inside,
    );
    theme::paint_icon(
        ui,
        "plus",
        Rect::from_min_size(
            Pos2::new(rect.left() + 6.0, rect.top()),
            Vec2::new(14.0, rect.height()),
        ),
        14.0,
        icon,
    );
    widgets::text_in(
        ui,
        Rect::from_min_max(Pos2::new(rect.left() + 24.0, rect.top()), rect.max),
        label,
        font(14.0, Weight::Medium),
        color,
    );
    response.clicked()
}

fn compact_width(ui: &Ui, label: &str) -> f32 {
    (widgets::text_width(ui, label, font(14.0, Weight::Medium)) + 31.0).ceil()
}

fn remove_button(
    ui: &mut Ui,
    rect: Rect,
    id: impl std::hash::Hash + std::fmt::Debug,
    tip: &str,
) -> bool {
    widgets::icon_button(
        ui,
        rect,
        ("when-remove", id),
        "minus",
        14.0,
        theme::muted_foreground(),
        tip,
    )
    .clicked()
}

/// `whenNodeRemoveLabel`.
fn remove_label(node: &When, depth: usize) -> &'static str {
    if depth == 0 {
        "Clear all conditions"
    } else if node.condition().is_some() {
        "Remove condition"
    } else {
        "Remove group and its conditions"
    }
}

/// A compact select trigger; returns whether it was clicked.
fn select(
    ui: &mut Ui,
    rect: Rect,
    id: impl std::hash::Hash + std::fmt::Debug,
    label: &str,
) -> bool {
    let response = ui.interact(rect, ui.id().with(("when-select", id)), Sense::click());
    ui.painter().rect(
        rect,
        CornerRadius::same(8),
        if response.hovered() {
            theme::overlay(0.06)
        } else {
            theme::control_surface()
        },
        Stroke::new(1.0, theme::input()),
        StrokeKind::Inside,
    );
    widgets::text_in(
        ui,
        Rect::from_min_max(
            Pos2::new(rect.left() + 8.0, rect.top()),
            Pos2::new(rect.right() - 24.0, rect.bottom()),
        ),
        label,
        font(14.0, Weight::Regular),
        theme::foreground(),
    );
    theme::paint_icon(
        ui,
        "chevron-down",
        Rect::from_min_size(
            Pos2::new(rect.right() - 16.0, rect.center().y - 6.0),
            Vec2::splat(12.0),
        ),
        12.0,
        theme::muted_foreground(),
    );
    response.clicked()
}

/// The "Not" toggle (`variant="outline" size="compact"`).
fn not_toggle(
    ui: &mut Ui,
    rect: Rect,
    id: impl std::hash::Hash + std::fmt::Debug,
    pressed: bool,
) -> bool {
    let response = ui.interact(rect, ui.id().with(("when-not", id)), Sense::click());
    ui.painter().rect(
        rect,
        CornerRadius::same(8),
        if pressed {
            theme::input()
        } else if response.hovered() {
            theme::overlay(0.06)
        } else {
            theme::control_surface()
        },
        Stroke::new(1.0, theme::input()),
        StrokeKind::Inside,
    );
    let width = widgets::text_width(ui, "Not", font(14.0, Weight::Medium));
    widgets::text_in(
        ui,
        Rect::from_min_max(
            Pos2::new(rect.center().x - width / 2.0, rect.top()),
            rect.max,
        ),
        "Not",
        font(14.0, Weight::Medium),
        theme::foreground(),
    );
    response.clicked()
}

/// The painted height of a node editor.
fn node_height(node: &When) -> f32 {
    if node.condition().is_some() {
        return 2.0 + 2.0 * GAP + ROW;
    }
    match node {
        When::Not(child) => 2.0 + 2.0 * GAP + ROW + GAP + node_height(child),
        _ => {
            let or = node.operator().unwrap_or(false);
            let children = node.children(or);
            let rows: f32 = children.iter().map(node_height).sum::<f32>()
                + GAP * (children.len().saturating_sub(1)) as f32;
            2.0 + 2.0 * GAP + ROW + GAP + rows
        }
    }
}

/// Paints one node editor at `rect.min` with `rect.width()`; pushes edits.
fn node_editor(
    ui: &mut Ui,
    draft: &mut WhenDraft,
    node: &When,
    path: &[usize],
    (origin, width): (Pos2, f32),
    (depth, removable): (usize, bool),
    edits: &mut Vec<Edit>,
) {
    let height = node_height(node);
    let frame = Rect::from_min_size(origin, Vec2::new(width, height));
    let id = path.to_vec();
    if let Some((name, negated)) = node.condition() {
        ui.painter().rect(
            frame,
            CornerRadius::same(8),
            theme::alpha(theme::background(), 0.6),
            Stroke::new(1.0, theme::alpha(theme::border(), 0.7)),
            StrokeKind::Inside,
        );
        let top = frame.top() + 1.0 + GAP;
        let toggle = Rect::from_min_size(Pos2::new(frame.left() + 9.0, top), Vec2::new(40.0, ROW));
        if not_toggle(ui, toggle, &id, negated) {
            let condition = When::Id(name.to_owned());
            let next = if negated {
                condition
            } else {
                When::Not(Box::new(condition))
            };
            edits.push(Edit::Replace(id.clone(), Some(next)));
        }
        let right = frame.right() - 9.0 - if removable { ROW + GAP } else { 0.0 };
        let trigger = Rect::from_min_max(
            Pos2::new(toggle.right() + GAP, top),
            Pos2::new(right, top + ROW),
        );
        if select(ui, trigger, &id, name) {
            draft.menu = Some((id.clone(), false, trigger));
        }
        if removable {
            let button =
                Rect::from_min_size(Pos2::new(frame.right() - 9.0 - ROW, top), Vec2::splat(ROW));
            if remove_button(ui, button, &id, remove_label(node, depth)) {
                edits.push(Edit::Replace(id.clone(), None));
            }
        }
        return;
    }
    // Groups: `rounded-lg border bg-muted/10 p-2`, nested ones on the background.
    let (fill, border) = if depth > 0 {
        (
            theme::alpha(theme::background(), 0.55),
            theme::alpha(theme::border(), 0.7),
        )
    } else {
        (
            theme::alpha(theme::muted(), 0.1),
            theme::alpha(theme::border(), 0.6),
        )
    };
    ui.painter().rect(
        frame,
        CornerRadius::same(10),
        fill,
        Stroke::new(1.0, border),
        StrokeKind::Inside,
    );
    let inner_left = frame.left() + 1.0 + GAP;
    let inner_right = frame.right() - 1.0 - GAP;
    let top = frame.top() + 1.0 + GAP;
    let line = if depth == 0 {
        theme::border()
    } else {
        theme::alpha(theme::border(), 0.7)
    };
    let connector = |ui: &Ui, child_top: f32, child_height: f32| {
        ui.painter().rect_filled(
            Rect::from_min_size(
                Pos2::new(inner_left + 6.0, child_top),
                Vec2::new(1.0, child_height),
            ),
            0.0,
            line,
        );
        ui.painter().rect_filled(
            Rect::from_min_size(
                Pos2::new(inner_left + 6.0, child_top + 16.0),
                Vec2::new(10.0, 1.0),
            ),
            0.0,
            line,
        );
    };
    match node {
        When::Not(child) => {
            let toggle = Rect::from_min_size(Pos2::new(inner_left, top), Vec2::new(40.0, ROW));
            if not_toggle(ui, toggle, &id, true) {
                edits.push(Edit::Replace(id.clone(), Some((**child).clone())));
            }
            if removable {
                let button =
                    Rect::from_min_size(Pos2::new(inner_right - ROW, top), Vec2::splat(ROW));
                if remove_button(ui, button, &id, remove_label(node, depth)) {
                    edits.push(Edit::Replace(id.clone(), None));
                }
            }
            let child_top = top + ROW + GAP;
            connector(ui, child_top, node_height(child));
            let mut child_path = id.clone();
            child_path.push(0);
            node_editor(
                ui,
                draft,
                child,
                &child_path,
                (
                    Pos2::new(inner_left + INDENT, child_top),
                    inner_right - inner_left - INDENT,
                ),
                (depth + 1, false),
                edits,
            );
        }
        _ => {
            let or = node.operator().unwrap_or(false);
            let operator = Rect::from_min_size(Pos2::new(inner_left, top), Vec2::new(143.0, ROW));
            if select(ui, operator, ("op", &id), if or { "or" } else { "and" }) {
                draft.menu = Some((id.clone(), true, operator));
            }
            let condition_width = compact_width(ui, "Condition");
            let add_condition = Rect::from_min_size(
                Pos2::new(operator.right() + GAP, top),
                Vec2::new(condition_width, ROW),
            );
            let children = node.children(or);
            if compact_button(ui, add_condition, ("condition", &id), "Condition", false) {
                let mut next = children.clone();
                next.push(default_condition());
                edits.push(Edit::Replace(id.clone(), group(next, or)));
            }
            let add_group = Rect::from_min_size(
                Pos2::new(add_condition.right() + GAP, top),
                Vec2::new(compact_width(ui, "Group"), ROW),
            );
            if compact_button(ui, add_group, ("group", &id), "Group", false) {
                let mut next = children.clone();
                next.push(default_group(!or));
                edits.push(Edit::Replace(id.clone(), group(next, or)));
            }
            if removable {
                let button =
                    Rect::from_min_size(Pos2::new(inner_right - ROW, top), Vec2::splat(ROW));
                if remove_button(ui, button, &id, remove_label(node, depth)) {
                    edits.push(Edit::Replace(id.clone(), None));
                }
            }
            let mut child_top = top + ROW + GAP;
            for (index, child) in children.iter().enumerate() {
                let child_height = node_height(child);
                connector(ui, child_top, child_height);
                let mut child_path = id.clone();
                child_path.push(index);
                node_editor(
                    ui,
                    draft,
                    child,
                    &child_path,
                    (
                        Pos2::new(inner_left + INDENT, child_top),
                        inner_right - inner_left - INDENT,
                    ),
                    (depth + 1, true),
                    edits,
                );
                child_top += child_height + GAP;
            }
        }
    }
}

/// Applies a replacement at `path` (child indices through groups and nots);
/// `None` removes the node.
fn replace(node: &When, path: &[usize], next: Option<When>) -> Option<When> {
    let Some((&first, rest)) = path.split_first() else {
        return next;
    };
    match node {
        When::Not(child) => replace(child, rest, next)
            .map(|child| When::Not(Box::new(child)))
            .or(Some(default_condition())),
        When::And(..) | When::Or(..) => {
            let or = node.operator().unwrap_or(false);
            let mut children = node.children(or);
            let Some(target) = children.get(first).cloned() else {
                return Some(node.clone());
            };
            match replace(&target, rest, next) {
                Some(updated) => children[first] = updated,
                None => {
                    children.remove(first);
                }
            }
            group(children, or).or(Some(default_condition()))
        }
        _ => Some(node.clone()),
    }
}

/// The node at `path`.
fn at<'a>(node: &'a When, path: &[usize]) -> Option<&'a When> {
    let Some((&first, rest)) = path.split_first() else {
        return Some(node);
    };
    match node {
        When::Not(child) => at(child, rest),
        When::And(..) | When::Or(..) => {
            // Children are rebuilt on demand, so walk a flattened copy.
            let or = node.operator().unwrap_or(false);
            let children = node.children(or);
            let child = children.get(first)?;
            // Find the same child inside the tree for a stable reference.
            find(node, child, or).and_then(|found| at(found, rest))
        }
        _ => None,
    }
}

fn find<'a>(node: &'a When, target: &When, or: bool) -> Option<&'a When> {
    match (node, or) {
        (When::And(left, right), false) | (When::Or(left, right), true) => {
            find(left, target, or).or_else(|| find(right, target, or))
        }
        _ => (node == target).then_some(node),
    }
}

/// The builder body in `popup` (`w-[34rem]` plus padding); returns its height.
pub(crate) fn builder_height(ui: &Ui, draft: &WhenDraft) -> f32 {
    let _ = ui;
    let tree = match (&draft.value, draft.valid) {
        (Some(value), _) => node_height(value),
        (None, _) => 2.0 + 24.0 + ROW,
    };
    let error = if draft.valid { 0.0 } else { 6.0 + 16.0 };
    17.0 + ROW + 12.0 + 30.0 + error + 12.0 + tree + 17.0
}

/// Paints the builder; returns whether the draft changed.
pub(crate) fn builder(ui: &mut Ui, popup: Rect, draft: &mut WhenDraft) -> bool {
    let before = (draft.value.clone(), draft.text.clone());
    let left = popup.left() + 17.0;
    let right = popup.right() - 17.0;
    let top = popup.top() + 17.0;
    widgets::text(
        ui,
        Pos2::new(left, top),
        "When",
        font(14.0, Weight::Medium),
        theme::foreground(),
        20.0,
    );
    let group_width = compact_width(ui, "Group");
    let add_group = Rect::from_min_size(
        Pos2::new(right - group_width, top),
        Vec2::new(group_width, ROW),
    );
    let condition_width = compact_width(ui, "Condition");
    let add_condition = Rect::from_min_size(
        Pos2::new(add_group.left() - GAP - condition_width, top),
        Vec2::new(condition_width, ROW),
    );
    let mut root_condition =
        compact_button(ui, add_condition, "root-condition", "Condition", false);
    let mut root_group = compact_button(ui, add_group, "root-group", "Group", false);

    // The expression field.
    let input = Rect::from_min_size(
        Pos2::new(left, top + ROW + 12.0),
        Vec2::new(right - left, 30.0),
    );
    ui.painter().rect(
        input,
        CornerRadius::same(8),
        theme::control_surface(),
        Stroke::new(1.0, theme::alpha(theme::error_foreground(), 0.36)),
        StrokeKind::Inside,
    );
    let field = Rect::from_min_max(
        input.min + Vec2::new(10.0, 6.0),
        input.max - Vec2::new(10.0, 6.0),
    );
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(field));
    let response = child.add(
        egui::TextEdit::singleline(&mut draft.text)
            .id(egui::Id::new("when-expression"))
            .frame(egui::Frame::NONE)
            .margin(egui::Margin::ZERO)
            .font(theme::mono(12.0))
            .text_color(theme::foreground())
            .hint_text(
                egui::RichText::new("Always")
                    .font(theme::mono(12.0))
                    .color(theme::muted_foreground()),
            )
            .desired_width(field.width()),
    );
    if response.changed() {
        let trimmed = draft.text.trim();
        if trimmed.is_empty() {
            draft.value = None;
            draft.valid = true;
        } else if let Some(value) = parse(trimmed) {
            draft.value = Some(value);
            draft.valid = true;
        } else {
            draft.valid = false;
        }
    }
    let mut y = input.bottom();
    if !draft.valid {
        y += 6.0;
        theme::paint_icon(
            ui,
            "circle-x",
            Rect::from_min_size(Pos2::new(left, y + 1.0), Vec2::splat(14.0)),
            14.0,
            theme::error_foreground(),
        );
        widgets::text(
            ui,
            Pos2::new(left + 20.0, y),
            "Use variables with !, &&, ||, and parentheses.",
            font(10.0, Weight::Regular),
            theme::error_foreground(),
            16.0,
        );
        y += 16.0;
    }
    y += 12.0;

    // The tree, or the empty state's buttons.
    let mut edits = Vec::new();
    let tree_top = y;
    match draft.value.clone() {
        Some(value) => node_editor(
            ui,
            draft,
            &value,
            &[],
            (Pos2::new(left, y), right - left),
            (0, true),
            &mut edits,
        ),
        None => {
            let empty = Rect::from_min_size(
                Pos2::new(left, y),
                Vec2::new(right - left, 2.0 + 24.0 + ROW),
            );
            ui.painter().rect(
                empty,
                CornerRadius::same(8),
                theme::alpha(theme::muted(), 0.15),
                Stroke::new(1.0, theme::alpha(theme::border(), 0.8)),
                StrokeKind::Inside,
            );
            let first = Rect::from_min_size(
                Pos2::new(empty.left() + 13.0, empty.top() + 13.0),
                Vec2::new(condition_width, ROW),
            );
            root_condition |= compact_button(ui, first, "empty-condition", "Condition", true);
            let second = Rect::from_min_size(
                Pos2::new(first.right() + GAP, first.top()),
                Vec2::new(group_width, ROW),
            );
            root_group |= compact_button(ui, second, "empty-group", "Group", false);
        }
    }
    if !draft.valid {
        let height = draft.value.as_ref().map_or(2.0 + 24.0 + ROW, node_height);
        let cover = Rect::from_min_size(Pos2::new(left, tree_top), Vec2::new(right - left, height));
        ui.painter().rect(
            cover,
            CornerRadius::same(10),
            theme::alpha(theme::background(), 0.75),
            Stroke::new(1.0, theme::alpha(theme::error_foreground(), 0.3)),
            StrokeKind::Inside,
        );
        let message = "Fix the expression above to continue editing visually.";
        let width = widgets::text_width(ui, message, font(12.0, Weight::Regular));
        widgets::text_in(
            ui,
            Rect::from_center_size(cover.center(), Vec2::new(width + 1.0, 16.0)),
            message,
            font(12.0, Weight::Regular),
            theme::error_foreground(),
        );
    }

    if root_condition {
        let next = match draft.value.take() {
            None => default_condition(),
            Some(value) => When::And(Box::new(value), Box::new(default_condition())),
        };
        draft.set(Some(next));
    }
    if root_group {
        let next = match draft.value.take() {
            None => default_group(true),
            Some(value) => When::And(Box::new(value), Box::new(default_group(true))),
        };
        draft.set(Some(next));
    }
    for Edit::Replace(path, next) in edits {
        if let Some(value) = draft.value.clone() {
            let updated = if path.is_empty() {
                next
            } else {
                replace(&value, &path, next)
            };
            draft.set(updated);
        }
    }

    // The open select's options.
    if let Some((path, operator, anchor)) = draft.menu.clone() {
        let options: Vec<String> = if operator {
            vec!["and".into(), "or".into()]
        } else {
            variables()
        };
        let current = draft
            .value
            .as_ref()
            .and_then(|value| at(value, &path))
            .map(|node| {
                if operator {
                    if node.operator() == Some(true) {
                        "or"
                    } else {
                        "and"
                    }
                    .to_owned()
                } else {
                    node.condition()
                        .map(|(name, _)| name.to_owned())
                        .unwrap_or_default()
                }
            });
        let entries: Vec<widgets::MenuEntry> = options
            .iter()
            .map(|option| {
                widgets::MenuEntry::Item(widgets::MenuItem {
                    selected: current.as_deref() == Some(option.as_str()),
                    ..widgets::MenuItem::new(option)
                })
            })
            .collect();
        let mut open = Some("when-select".to_owned());
        let chosen =
            widgets::menu_aligned(ui, &mut open, "when-select", anchor, false, false, &entries);
        if let Some(index) = chosen
            && let Some(value) = draft.value.clone()
            && let Some(node) = at(&value, &path)
        {
            let next = if operator {
                let or = node.operator().unwrap_or(false);
                group(node.children(or), index == 1)
            } else {
                let name = options[index].clone();
                match node.condition() {
                    Some((_, true)) => Some(When::Not(Box::new(When::Id(name)))),
                    _ => Some(When::Id(name)),
                }
            };
            let updated = if path.is_empty() {
                next
            } else {
                replace(&value, &path, next)
            };
            draft.set(updated);
        }
        if open.is_none() || chosen.is_some() {
            draft.menu = None;
        }
    }
    (draft.value.clone(), draft.text.clone()) != before
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expressions_parse_and_print_like_upstream() {
        let node = parse("!terminalFocus && (isDesktop || modelPickerOpen)").unwrap();
        assert_eq!(
            node.expression(),
            "!terminalFocus && (isDesktop || modelPickerOpen)"
        );
        assert_eq!(parse("a || b && c").unwrap().expression(), "a || (b && c)");
        assert!(parse("a &&").is_none());
        assert!(parse("(a").is_none());
        assert!(parse("a $ b").is_none());
        assert!(parse("").is_none());
    }

    #[test]
    fn edits_rebuild_groups() {
        let node = parse("a && b && c").unwrap();
        assert_eq!(node.children(false).len(), 3);
        let removed = replace(&node, &[1], None).unwrap();
        assert_eq!(removed.expression(), "a && c");
        let negated = replace(&node, &[0], Some(parse("!a").unwrap())).unwrap();
        assert_eq!(negated.expression(), "(!a && b) && c");
        assert_eq!(
            group(node.children(false), true).unwrap().expression(),
            "(a || b) || c"
        );
    }
}
