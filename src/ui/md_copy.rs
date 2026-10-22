//! Copying selected chat text back as markdown (`markdown-clipboard.ts`):
//! links, emphasis, code spans, lists, quotes, fences and tables keep their
//! markdown instead of flattening to plain text.
//!
//! egui's label selection keeps its range private, so the rendered markdown
//! registers how each selectable galley copies (its inlines' character
//! ranges and enclosing blocks), and the selection is rebuilt from where it
//! was pressed and where the pointer went. When the rebuilt text does not
//! match egui's copy, egui's plain text is kept.

use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;

use eframe::egui::{self, Galley};
use pulldown_cmark::Alignment;

use crate::ui::markdown::{Inline, inline_markdown};

/// A block around a galley, outermost first.
#[derive(Clone, Debug)]
pub(crate) enum Prefix {
    Quote,
    /// A list item: its identity, its list's and its marker ("- ", "3. ").
    Item {
        id: egui::Id,
        list: egui::Id,
        marker: String,
    },
}

#[derive(Clone, Debug)]
pub(crate) enum Kind {
    Paragraph,
    Heading(u8),
    Cell {
        table: egui::Id,
        row: usize,
        alignment: Alignment,
    },
    Code {
        language: String,
    },
}

/// How a selectable galley copies.
#[derive(Clone, Debug)]
pub(crate) struct Recipe {
    pub(crate) kind: Kind,
    pub(crate) prefixes: Vec<Prefix>,
    /// Each inline with its character range in the galley.
    pub(crate) segments: Vec<(Range<usize>, Inline)>,
}

/// A selectable galley placed this pass.
#[derive(Clone)]
pub(crate) struct Placed {
    pub(crate) id: egui::Id,
    pub(crate) galley: Arc<Galley>,
}

/// Text as egui's copy cleanup leaves it.
fn clean(text: &str) -> String {
    text.replace("\u{200B}\n", "").replace('\u{a0}', " ")
}

fn slice(text: &str, range: &Range<usize>) -> String {
    text.chars()
        .skip(range.start)
        .take(range.end.saturating_sub(range.start))
        .collect()
}

/// Characters a copy keeps whatever the separators: not whitespace, not
/// the layout-only break marker.
fn significant(c: char) -> bool {
    !c.is_whitespace() && c != '\u{200B}'
}

/// Matches `target[t..]` against `chars[from..]`, skipping insignificant
/// characters; returns where the match ends and how much of the target it
/// took, or None on a mismatch before the target ran out.
fn forward(chars: &[char], from: usize, target: &[char], mut t: usize) -> Option<(usize, usize)> {
    let mut end = from;
    for (i, c) in chars.iter().enumerate().skip(from) {
        if !significant(*c) {
            continue;
        }
        if t == target.len() {
            break;
        }
        if *c != target[t] {
            return None;
        }
        t += 1;
        end = i + 1;
    }
    Some((end, t))
}

/// `forward` from the end: matches `target[..t]` against `chars[..to]`.
fn backward(chars: &[char], to: usize, target: &[char], mut t: usize) -> Option<(usize, usize)> {
    let mut start = to;
    for i in (0..to.min(chars.len())).rev() {
        if !significant(chars[i]) {
            continue;
        }
        if t == 0 {
            break;
        }
        if chars[i] != target[t - 1] {
            return None;
        }
        t -= 1;
        start = i;
    }
    Some((start, t))
}

/// Each selected galley's character range, rebuilt from where the selection
/// started and what egui copied: onwards from the anchor, back to it, or
/// around it (a double or triple click).
fn selected_ranges(
    placed: &[Placed],
    a: usize,
    k: usize,
    target: &[char],
) -> Option<Vec<(usize, Range<usize>)>> {
    let chars: Vec<Vec<char>> = placed
        .iter()
        .map(|p| p.galley.text().chars().collect())
        .collect();
    // Onwards.
    let mut parts = Vec::new();
    let mut t = 0;
    let mut index = a;
    let mut from = k;
    while index < chars.len() {
        let Some((end, taken)) = forward(&chars[index], from, target, t) else {
            parts.clear();
            break;
        };
        if taken > t {
            parts.push((index, from..end));
        }
        t = taken;
        if t == target.len() {
            return Some(parts);
        }
        index += 1;
        from = 0;
    }
    // Back to the anchor.
    let mut t = target.len();
    let mut index = a as isize;
    let mut to = k;
    let mut parts = Vec::new();
    while index >= 0 {
        let galley = &chars[index as usize];
        let Some((start, left)) = backward(galley, to.min(galley.len()), target, t) else {
            break;
        };
        if left < t {
            parts.insert(0, (index as usize, start..to.min(galley.len())));
        }
        t = left;
        if t == 0 {
            return Some(parts);
        }
        index -= 1;
        to = chars.get(index.max(0) as usize).map_or(0, Vec::len);
    }
    // Around the anchor, inside its galley.
    let galley = &chars[a];
    let positions: Vec<usize> = (0..galley.len())
        .filter(|i| significant(galley[*i]))
        .collect();
    let sig: Vec<char> = positions.iter().map(|i| galley[*i]).collect();
    (0..=sig.len().checked_sub(target.len())?)
        .filter(|start| sig[*start..*start + target.len()] == *target)
        .map(|start| positions[start]..positions[start + target.len() - 1] + 1)
        .find(|range| range.start <= k && k <= range.end)
        .map(|range| vec![(a, range)])
}

/// The markdown for the selection, or None when it is not in rendered
/// markdown or egui selected something the galleys cannot account for.
pub(crate) fn selection_markdown(
    placed: &[Placed],
    recipes: &HashMap<egui::Id, Arc<Recipe>>,
    anchor: (egui::Id, usize),
    copied: &str,
) -> Option<String> {
    let a = placed.iter().position(|p| p.id == anchor.0)?;
    let target: Vec<char> = copied.chars().filter(|c| significant(*c)).collect();
    if target.is_empty() {
        return None;
    }
    let parts = selected_ranges(placed, a, anchor.1, &target)?;
    if !parts
        .iter()
        .any(|(i, _)| recipes.contains_key(&placed[*i].id))
    {
        return None;
    }
    let pieces: Vec<Piece> = parts
        .iter()
        .map(|(i, range)| {
            let text = placed[*i].galley.text();
            match recipes.get(&placed[*i].id) {
                Some(recipe) => Piece::Recipe(recipe.clone(), range.clone(), text.to_owned()),
                None => Piece::Plain(clean(&slice(text, range))),
            }
        })
        .collect();
    Some(serialize(&pieces))
}

enum Piece {
    Recipe(Arc<Recipe>, Range<usize>, String),
    Plain(String),
}

/// Inline markdown for the selected characters of a galley.
fn inline_content(recipe: &Recipe, range: &Range<usize>, text: &str) -> String {
    let mut parts = Vec::new();
    for (segment, inline) in &recipe.segments {
        let from = segment.start.max(range.start);
        let to = segment.end.min(range.end);
        if from >= to {
            continue;
        }
        let part = match inline {
            Inline::Text(_, style) => Inline::Text(slice(text, &(from..to)), style.clone()),
            // The pads around a code span are not code.
            Inline::Code(code) => {
                let inner = (segment.start + 1)..(segment.end - 1).max(segment.start + 1);
                let from = from.max(inner.start);
                let to = to.min(inner.end);
                if from >= to {
                    continue;
                }
                let skip = from - inner.start;
                Inline::Code(code.chars().skip(skip).take(to - from).collect())
            }
            other => other.clone(),
        };
        parts.push(part);
    }
    inline_markdown(&parts)
}

/// `serializeNode` over the selected blocks, then `tidyMarkdown`.
fn serialize(pieces: &[Piece]) -> String {
    // A selection inside one code block copies as plain code.
    if let [Piece::Recipe(recipe, range, text)] = pieces
        && matches!(recipe.kind, Kind::Code { .. })
    {
        return slice(text, range);
    }
    let mut blocks: Vec<(Vec<Prefix>, String)> = Vec::new();
    let mut index = 0;
    while index < pieces.len() {
        match &pieces[index] {
            Piece::Plain(text) => blocks.push((Vec::new(), text.trim().to_owned())),
            Piece::Recipe(recipe, range, text) => match &recipe.kind {
                Kind::Paragraph => blocks.push((
                    recipe.prefixes.clone(),
                    inline_content(recipe, range, text).trim().to_owned(),
                )),
                Kind::Heading(level) => blocks.push((
                    recipe.prefixes.clone(),
                    format!(
                        "{} {}",
                        "#".repeat(*level as usize),
                        inline_content(recipe, range, text).trim()
                    ),
                )),
                Kind::Code { language } => {
                    let code = slice(text, range);
                    let longest = code
                        .split(|c| c != '`')
                        .map(str::len)
                        .filter(|n| *n >= 3)
                        .max()
                        .unwrap_or(0);
                    let fence = "`".repeat(3.max(longest + 1));
                    let language = if language == "text" { "" } else { language };
                    blocks.push((
                        recipe.prefixes.clone(),
                        format!(
                            "{fence}{language}\n{}\n{fence}",
                            code.trim_end_matches('\n')
                        ),
                    ));
                }
                Kind::Cell { table, .. } => {
                    // `serializeTable`: the table's selected cells by row,
                    // the separator after the first.
                    let mut rows: Vec<(usize, Vec<String>, Vec<Alignment>)> = Vec::new();
                    while let Some(Piece::Recipe(cell, range, text)) = pieces.get(index) {
                        let Kind::Cell {
                            table: t,
                            row,
                            alignment,
                            ..
                        } = &cell.kind
                        else {
                            break;
                        };
                        if t != table {
                            break;
                        }
                        let content = inline_content(cell, range, text)
                            .split('\n')
                            .filter(|s| !s.is_empty())
                            .collect::<Vec<_>>()
                            .join(" ")
                            .trim()
                            .replace('|', "\\|");
                        match rows.last_mut() {
                            Some((r, cells, aligns)) if r == row => {
                                cells.push(content);
                                aligns.push(*alignment);
                            }
                            _ => rows.push((*row, vec![content], vec![*alignment])),
                        }
                        index += 1;
                    }
                    let mut lines = Vec::new();
                    for (n, (_, cells, aligns)) in rows.iter().enumerate() {
                        lines.push(format!("| {} |", cells.join(" | ")));
                        if n == 0 {
                            let markers: Vec<&str> = aligns
                                .iter()
                                .map(|a| match a {
                                    Alignment::Center => ":---:",
                                    Alignment::Right => "---:",
                                    _ => "---",
                                })
                                .collect();
                            lines.push(format!("| {} |", markers.join(" | ")));
                        }
                    }
                    blocks.push((recipe.prefixes.clone(), lines.join("\n")));
                    continue;
                }
            },
        }
        index += 1;
    }
    // List markers on each item's first selected block, continuation
    // indents after; quotes prefix every line.
    let mut seen_items: Vec<egui::Id> = Vec::new();
    let mut out = String::new();
    let mut previous: &[Prefix] = &[];
    for (n, (prefixes, text)) in blocks.iter().enumerate() {
        if text.is_empty() {
            continue;
        }
        let mut lines: Vec<String> = text.split('\n').map(str::to_owned).collect();
        for prefix in prefixes.iter().rev() {
            match prefix {
                Prefix::Quote => {
                    for line in &mut lines {
                        *line = if line.is_empty() {
                            ">".to_owned()
                        } else {
                            format!("> {line}")
                        };
                    }
                }
                Prefix::Item { id, marker, .. } => {
                    let indent = " ".repeat(marker.len());
                    let first = !seen_items.contains(id);
                    if first {
                        seen_items.push(*id);
                    }
                    for (i, line) in lines.iter_mut().enumerate() {
                        if i == 0 && first {
                            *line = format!("{marker}{line}");
                        } else if !line.is_empty() {
                            *line = format!("{indent}{line}");
                        }
                    }
                }
            }
        }
        if n > 0 && !out.is_empty() {
            out.push_str(if tight(previous, prefixes) {
                "\n"
            } else {
                "\n\n"
            });
        }
        out.push_str(&lines.join("\n"));
        previous = prefixes;
    }
    tidy(&out)
}

/// Items of one tight list, or a list nested in the previous item, sit on
/// adjacent lines; anything else is a new paragraph.
fn tight(previous: &[Prefix], next: &[Prefix]) -> bool {
    let innermost = |prefixes: &[Prefix]| {
        prefixes.iter().rev().find_map(|p| match p {
            Prefix::Item { id, list, .. } => Some((*id, *list)),
            Prefix::Quote => None,
        })
    };
    let (Some((previous_id, _)), Some((next_id, next_list))) =
        (innermost(previous), innermost(next))
    else {
        return false;
    };
    if previous_id == next_id {
        return false;
    }
    let in_list = |prefixes: &[Prefix], wanted: egui::Id| {
        prefixes
            .iter()
            .any(|p| matches!(p, Prefix::Item { list, .. } if *list == wanted))
    };
    let holds_item = |prefixes: &[Prefix], wanted: egui::Id| {
        prefixes
            .iter()
            .any(|p| matches!(p, Prefix::Item { id, .. } if *id == wanted))
    };
    in_list(previous, next_list) || holds_item(next, previous_id)
}

/// `tidyMarkdown`: no trailing spaces or runs of blank lines outside fences.
fn tidy(markdown: &str) -> String {
    let mut out = Vec::new();
    let mut in_fence = false;
    let mut blank = 0;
    for line in markdown.split('\n') {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
        }
        let line = if in_fence { line } else { line.trim_end() };
        if line.is_empty() && !in_fence {
            blank += 1;
            if blank > 1 {
                continue;
            }
        } else {
            blank = 0;
        }
        out.push(line);
    }
    out.join("\n").trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::markdown::Style;

    fn text_recipe(
        kind: Kind,
        prefixes: Vec<Prefix>,
        segments: Vec<(Range<usize>, Inline)>,
    ) -> Arc<Recipe> {
        Arc::new(Recipe {
            kind,
            prefixes,
            segments,
        })
    }

    #[test]
    fn selections_keep_emphasis_lists_and_code() {
        let bold = Style {
            bold: true,
            ..Style::default()
        };
        let paragraph = text_recipe(
            Kind::Paragraph,
            Vec::new(),
            vec![
                (0..5, Inline::Text("Voir ".into(), Style::default())),
                (5..10, Inline::Text("ceci ".into(), bold)),
                (10..13, Inline::Code("x".into())),
            ],
        );
        let item = |n: u64| Prefix::Item {
            id: egui::Id::new(n),
            list: egui::Id::new("list"),
            marker: "- ".into(),
        };
        let first = text_recipe(
            Kind::Paragraph,
            vec![item(1)],
            vec![(0..3, Inline::Text("un".into(), Style::default()))],
        );
        let second = text_recipe(
            Kind::Paragraph,
            vec![item(2)],
            vec![(0..4, Inline::Text("deux".into(), Style::default()))],
        );
        let pieces = vec![
            Piece::Recipe(paragraph, 5..13, "Voir ceci  x ".into()),
            Piece::Recipe(first, 0..2, "un".into()),
            Piece::Recipe(second, 0..4, "deux".into()),
        ];
        assert_eq!(serialize(&pieces), "**ceci** `x`\n\n- un\n- deux");
        let code = Arc::new(Recipe {
            kind: Kind::Code {
                language: "rust".into(),
            },
            prefixes: Vec::new(),
            segments: Vec::new(),
        });
        let text = "fn main() {}\n";
        assert_eq!(
            serialize(&[Piece::Recipe(code.clone(), 3..7, text.into())]),
            "main"
        );
        assert_eq!(
            serialize(&[
                Piece::Plain("Avant".into()),
                Piece::Recipe(code, 0..12, text.into())
            ]),
            "Avant\n\n```rust\nfn main() {}\n```"
        );
    }
}
