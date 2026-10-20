//! Unified git patches, as returned by `orchestration.getTurnDiff`, parsed
//! into files, hunks and change blocks with word-level emphasis for changed
//! pairs. Blocks are realigned like @pierre/diffs' `parsePatchFiles`
//! (`realignChangeContent.ts`), which both the unified and split views use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineKind {
    Context,
    Addition,
    Deletion,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub kind: LineKind,
    pub text: String,
    /// Old-side line number (context and deletions).
    pub old: u32,
    /// New-side line number (context and additions).
    pub new: u32,
    /// Byte ranges of `text` that changed relative to the paired line.
    pub emphasis: Vec<(usize, usize)>,
}

impl Line {
    /// The unified gutter number: the new side unless the line was deleted.
    pub fn number(&self) -> u32 {
        if self.kind == LineKind::Deletion {
            self.old
        } else {
            self.new
        }
    }
}

/// A run of unchanged lines, or deletions paired index by index with the
/// additions that replace them.
#[derive(Clone, Debug, PartialEq)]
pub enum Block {
    Context(Vec<Line>),
    Change {
        deletions: Vec<Line>,
        additions: Vec<Line>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Hunk {
    pub old_start: u32,
    pub new_start: u32,
    pub blocks: Vec<Block>,
}

impl Hunk {
    /// Lines in unified order: each change lists its deletions first.
    pub fn lines(&self) -> impl Iterator<Item = &Line> {
        self.blocks
            .iter()
            .flat_map(|block| -> Box<dyn Iterator<Item = &Line> + '_> {
                match block {
                    Block::Context(lines) => Box::new(lines.iter()),
                    Block::Change {
                        deletions,
                        additions,
                    } => Box::new(deletions.iter().chain(additions.iter())),
                }
            })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileStatus {
    Modified,
    Added,
    Deleted,
    Renamed,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FileDiff {
    pub path: String,
    pub old_path: Option<String>,
    pub status: FileStatus,
    pub hunks: Vec<Hunk>,
    pub additions: u32,
    pub deletions: u32,
    pub binary: bool,
}

fn strip_prefix(path: &str) -> String {
    path.strip_prefix("a/")
        .or_else(|| path.strip_prefix("b/"))
        .unwrap_or(path)
        .to_owned()
}

fn parse_range(text: &str) -> u32 {
    text.split(',')
        .next()
        .and_then(|n| n.trim_start_matches(['-', '+']).parse().ok())
        .unwrap_or(0)
}

pub fn parse(patch: &str) -> Vec<FileDiff> {
    let mut files: Vec<FileDiff> = Vec::new();
    let mut old_line = 0u32;
    let mut new_line = 0u32;
    for raw in patch.lines() {
        if let Some(rest) = raw.strip_prefix("diff --git ") {
            let path = rest
                .rsplit_once(" b/")
                .map(|(_, b)| b.to_owned())
                .unwrap_or_else(|| strip_prefix(rest));
            files.push(FileDiff {
                path,
                old_path: None,
                status: FileStatus::Modified,
                hunks: Vec::new(),
                additions: 0,
                deletions: 0,
                binary: false,
            });
            continue;
        }
        let Some(file) = files.last_mut() else {
            continue;
        };
        if file.hunks.is_empty() {
            if raw.starts_with("new file mode") {
                file.status = FileStatus::Added;
            } else if raw.starts_with("deleted file mode") {
                file.status = FileStatus::Deleted;
            } else if let Some(from) = raw.strip_prefix("rename from ") {
                file.old_path = Some(from.to_owned());
                file.status = FileStatus::Renamed;
            } else if let Some(to) = raw.strip_prefix("rename to ") {
                file.path = to.to_owned();
            } else if raw.starts_with("Binary files ") {
                file.binary = true;
            } else if let Some(path) = raw.strip_prefix("+++ ") {
                if path != "/dev/null" {
                    file.path = strip_prefix(path);
                }
                continue;
            }
            if raw.starts_with("--- ") || raw.starts_with("+++ ") {
                continue;
            }
        }
        if let Some(header) = raw.strip_prefix("@@ ") {
            let mut parts = header.split_whitespace();
            let old = parts.next().map(parse_range).unwrap_or(0);
            let new = parts.next().map(parse_range).unwrap_or(0);
            old_line = old;
            new_line = new;
            file.hunks.push(Hunk {
                old_start: old,
                new_start: new,
                blocks: Vec::new(),
            });
            continue;
        }
        let Some(hunk) = file.hunks.last_mut() else {
            continue;
        };
        let (kind, text) = match raw.chars().next() {
            Some('+') => (LineKind::Addition, &raw[1..]),
            Some('-') => (LineKind::Deletion, &raw[1..]),
            Some(' ') => (LineKind::Context, &raw[1..]),
            Some('\\') => continue, // "\ No newline at end of file"
            None => (LineKind::Context, ""),
            _ => continue,
        };
        let line = Line {
            kind,
            text: text.to_owned(),
            old: if kind == LineKind::Addition {
                0
            } else {
                old_line
            },
            new: if kind == LineKind::Deletion {
                0
            } else {
                new_line
            },
            emphasis: Vec::new(),
        };
        match kind {
            LineKind::Addition => {
                new_line += 1;
                file.additions += 1;
            }
            LineKind::Deletion => {
                old_line += 1;
                file.deletions += 1;
            }
            LineKind::Context => {
                old_line += 1;
                new_line += 1;
            }
        }
        push_line(&mut hunk.blocks, line);
    }
    for file in &mut files {
        for hunk in &mut file.hunks {
            realign_by_similarity(&mut hunk.blocks);
            slide_blank_blocks_up(&mut hunk.blocks);
            for block in &mut hunk.blocks {
                if let Block::Change {
                    deletions,
                    additions,
                } = block
                {
                    emphasize(deletions, additions);
                }
            }
        }
    }
    files
}

/// Appends a parsed line: context extends a context run, a deletion opens a
/// change unless one is still collecting deletions, an addition joins the
/// current change.
fn push_line(blocks: &mut Vec<Block>, line: Line) {
    match (line.kind, blocks.last_mut()) {
        (LineKind::Context, Some(Block::Context(lines))) => lines.push(line),
        (LineKind::Context, _) => blocks.push(Block::Context(vec![line])),
        (
            LineKind::Deletion,
            Some(Block::Change {
                deletions,
                additions,
            }),
        ) if additions.is_empty() => deletions.push(line),
        (LineKind::Deletion, _) => blocks.push(Block::Change {
            deletions: vec![line],
            additions: Vec::new(),
        }),
        (LineKind::Addition, Some(Block::Change { additions, .. })) => additions.push(line),
        (LineKind::Addition, _) => blocks.push(Block::Change {
            deletions: Vec::new(),
            additions: vec![line],
        }),
    }
}

const MAX_ALIGNMENT_COMPARISONS: usize = 4096;
const MIN_IMPROVEMENT_PER_PAIR: f64 = 0.5;

/// `lineSimilarity` on whitespace-stripped text: shared prefix and suffix
/// over the longer length.
fn line_similarity(a: &[char], b: &[char]) -> f64 {
    if a == b {
        return 1.0;
    }
    let max = a.len().max(b.len());
    let min = a.len().min(b.len());
    if min == 0 {
        return 0.0;
    }
    let prefix = a.iter().zip(b).take_while(|(x, y)| x == y).count();
    let suffix = a
        .iter()
        .rev()
        .zip(b.iter().rev())
        .take(min - prefix)
        .take_while(|(x, y)| x == y)
        .count();
    (prefix + suffix) as f64 / max as f64
}

/// `realignChangeContentBySimilarity`: a change with more lines on one side
/// pairs its lines at the offset whose content matches best, splitting the
/// surplus into pure insertions or deletions around the pairs.
fn realign_by_similarity(blocks: &mut Vec<Block>) {
    let mut index = 0;
    while index < blocks.len() {
        let Block::Change {
            deletions,
            additions,
        } = &blocks[index]
        else {
            index += 1;
            continue;
        };
        let pairs = deletions.len().min(additions.len());
        let surplus = deletions.len().abs_diff(additions.len());
        if pairs == 0 || surplus == 0 || pairs * (surplus + 1) > MAX_ALIGNMENT_COMPARISONS {
            index += 1;
            continue;
        }
        let strip = |line: &Line| -> Vec<char> {
            line.text.chars().filter(|c| !c.is_whitespace()).collect()
        };
        let old: Vec<Vec<char>> = deletions.iter().map(strip).collect();
        let new: Vec<Vec<char>> = additions.iter().map(strip).collect();
        let additions_longer = additions.len() > deletions.len();
        let mut best_offset = 0;
        let mut best_score = -1.0;
        for offset in 0..=surplus {
            let score: f64 = (0..pairs)
                .map(|pair| {
                    let (d, a) = if additions_longer {
                        (pair, pair + offset)
                    } else {
                        (pair + offset, pair)
                    };
                    line_similarity(&old[d], &new[a])
                })
                .sum();
            if offset == 0 {
                best_score = score + pairs as f64 * MIN_IMPROVEMENT_PER_PAIR;
            } else if score > best_score {
                best_score = score;
                best_offset = offset;
            }
        }
        if best_offset == 0 {
            index += 1;
            continue;
        }
        let Block::Change {
            mut deletions,
            mut additions,
        } = blocks.remove(index)
        else {
            unreachable!("matched a change above");
        };
        let mut replacement = Vec::new();
        let mut push = |deletions: Vec<Line>, additions: Vec<Line>| {
            if !deletions.is_empty() || !additions.is_empty() {
                replacement.push(Block::Change {
                    deletions,
                    additions,
                });
            }
        };
        if additions_longer {
            let rest = additions.split_off(best_offset + pairs);
            let paired = additions.split_off(best_offset);
            push(Vec::new(), additions);
            push(deletions, paired);
            push(Vec::new(), rest);
        } else {
            let rest = deletions.split_off(best_offset + pairs);
            let paired = deletions.split_off(best_offset);
            push(deletions, Vec::new());
            push(paired, additions);
            push(rest, Vec::new());
        }
        let count = replacement.len();
        blocks.splice(index..index, replacement);
        index += count;
    }
}

/// `slideBlankBoundaryBlocksUp`: a pure insertion or deletion of identical
/// blank lines moves to the top of the blank run above it, so the change
/// anchors to the content before it.
fn slide_blank_blocks_up(blocks: &mut Vec<Block>) {
    let mut index = 1;
    while index < blocks.len() {
        let (before, after) = blocks.split_at_mut(index);
        let (
            Block::Context(previous),
            Block::Change {
                deletions,
                additions,
            },
        ) = (&mut before[index - 1], &mut after[0])
        else {
            index += 1;
            continue;
        };
        if !deletions.is_empty() && !additions.is_empty() {
            index += 1;
            continue;
        }
        let insert = !additions.is_empty();
        let lines = if insert { additions } else { deletions };
        let unit = lines[0].text.clone();
        if !unit.trim().is_empty() || lines.iter().any(|l| l.text != unit) {
            index += 1;
            continue;
        }
        let slide = previous
            .iter()
            .rev()
            .take_while(|line| line.text == unit)
            .count();
        if slide == 0 {
            index += 1;
            continue;
        }
        // The block takes the numbers of the last `slide` context lines,
        // which move below it.
        let length = lines.len() as u32;
        for line in lines.iter_mut() {
            if insert {
                line.new -= slide as u32;
            } else {
                line.old -= slide as u32;
            }
        }
        let mut moved = previous.split_off(previous.len() - slide);
        for line in &mut moved {
            if insert {
                line.new += length;
            } else {
                line.old += length;
            }
        }
        let previous_empty = previous.is_empty();
        match blocks.get_mut(index + 1) {
            Some(Block::Context(next)) => {
                moved.append(next);
                *next = moved;
            }
            _ => blocks.insert(index + 1, Block::Context(moved)),
        }
        if previous_empty {
            blocks.remove(index - 1);
        } else {
            index += 1;
        }
    }
}

/// Pairs a change's deletions with its additions line by line and marks the
/// tokens that differ.
fn emphasize(deletions: &mut [Line], additions: &mut [Line]) {
    for (old, new) in deletions.iter_mut().zip(additions.iter_mut()) {
        let (old_ranges, new_ranges) = token_diff(&old.text, &new.text);
        old.emphasis = old_ranges;
        new.emphasis = new_ranges;
    }
}

/// jsdiff `diffWordsWithSpace` tokens: words, whitespace runs, and every
/// other character on its own.
fn tokens(text: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut chars = text.char_indices().peekable();
    while let Some((start, c)) = chars.next() {
        let class = |c: char| {
            if c.is_alphanumeric() || c == '_' {
                1
            } else if c.is_whitespace() {
                2
            } else {
                0
            }
        };
        let kind = class(c);
        let mut end = start + c.len_utf8();
        if kind != 0 {
            while let Some(&(i, next)) = chars.peek() {
                if class(next) != kind {
                    break;
                }
                end = i + next.len_utf8();
                chars.next();
            }
        }
        out.push((start, end));
    }
    out
}

type Ranges = Vec<(usize, usize)>;

/// Lines longer than this get no word emphasis (`maxLineDiffLength`).
const MAX_LINE_DIFF_LENGTH: usize = 1000;

/// `computeLineDiffDecorations` with `lineDiffType: "word-alt"`: a token LCS,
/// then each side's spans, where a one-character unchanged gap between two
/// changes joins them (`pushOrJoinSpan`). Returns the changed byte ranges.
fn token_diff(old: &str, new: &str) -> (Ranges, Ranges) {
    if old.len() > MAX_LINE_DIFF_LENGTH || new.len() > MAX_LINE_DIFF_LENGTH {
        return (Vec::new(), Vec::new());
    }
    let a = tokens(old);
    let b = tokens(new);
    if a.len() * b.len() > 250_000 {
        return (Vec::new(), Vec::new());
    }
    let word = |text: &str, (s, e): (usize, usize)| text[s..e].to_owned();
    let mut table = vec![vec![0u16; b.len() + 1]; a.len() + 1];
    for i in (0..a.len()).rev() {
        for j in (0..b.len()).rev() {
            table[i][j] = if word(old, a[i]) == word(new, b[j]) {
                table[i + 1][j + 1] + 1
            } else {
                table[i + 1][j].max(table[i][j + 1])
            };
        }
    }
    // Walk the LCS into (changed?, range) items per side, merging runs.
    let mut old_items: Vec<(bool, (usize, usize))> = Vec::new();
    let mut new_items: Vec<(bool, (usize, usize))> = Vec::new();
    let push =
        |items: &mut Vec<(bool, (usize, usize))>, changed: bool, range: (usize, usize)| match items
            .last_mut()
        {
            Some((last_changed, last)) if *last_changed == changed && last.1 == range.0 => {
                last.1 = range.1
            }
            _ => items.push((changed, range)),
        };
    let (mut i, mut j) = (0, 0);
    while i < a.len() || j < b.len() {
        if i < a.len() && j < b.len() && word(old, a[i]) == word(new, b[j]) {
            push(&mut old_items, false, a[i]);
            push(&mut new_items, false, b[j]);
            i += 1;
            j += 1;
        } else if j < b.len() && (i >= a.len() || table[i][j + 1] >= table[i + 1][j]) {
            push(&mut new_items, true, b[j]);
            j += 1;
        } else {
            push(&mut old_items, true, a[i]);
            i += 1;
        }
    }
    let spans = |items: Vec<(bool, (usize, usize))>, text: &str| -> Ranges {
        let mut joined: Vec<(bool, (usize, usize))> = Vec::new();
        let count = items.len();
        for (index, (changed, range)) in items.into_iter().enumerate() {
            let last_item = index + 1 == count;
            match joined.last_mut() {
                Some((last_changed, last)) if !last_item => {
                    let one_char = text[range.0..range.1].chars().count() == 1;
                    if changed == *last_changed || (!changed && one_char && *last_changed) {
                        last.1 = range.1;
                        continue;
                    }
                    joined.push((changed, range));
                }
                _ => joined.push((changed, range)),
            }
        }
        joined
            .into_iter()
            .filter(|(changed, _)| *changed)
            .map(|(_, range)| range)
            .collect()
    };
    (spans(old_items, old), spans(new_items, new))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PATCH: &str = "diff --git a/src/theme.rs b/src/theme.rs
index 1111111..2222222 100644
--- a/src/theme.rs
+++ b/src/theme.rs
@@ -6,6 +6,4 @@
 const OLD: u8 = 6;
-const OLD: u8 = 8;
-const OLD: u8 = 9;
+const NEW: u8 = 1;
 const OLD: u8 = 7;
\\ No newline at end of file
diff --git a/new.txt b/new.txt
new file mode 100644
--- /dev/null
+++ b/new.txt
@@ -0,0 +1 @@
+hello
";

    #[test]
    fn parses_files_hunks_and_numbers() {
        let files = parse(PATCH);
        assert_eq!(files.len(), 2);
        let theme = &files[0];
        assert_eq!(theme.path, "src/theme.rs");
        assert_eq!((theme.additions, theme.deletions), (1, 2));
        let numbers: Vec<u32> = theme.hunks[0].lines().map(Line::number).collect();
        assert_eq!(numbers, [6, 7, 8, 7, 8]);
        assert_eq!(files[1].status, FileStatus::Added);
        assert_eq!(files[1].path, "new.txt");
    }

    #[test]
    fn changed_tokens_are_emphasized() {
        let files = parse(PATCH);
        let lines: Vec<&Line> = files[0].hunks[0].lines().collect();
        let text = &lines[1].text;
        let emphasized: Vec<&str> = lines[1]
            .emphasis
            .iter()
            .map(|(s, e)| &text[*s..*e])
            .collect();
        assert_eq!(emphasized, ["OLD", "8"]);
    }

    #[test]
    fn one_character_gaps_join_changed_words() {
        let (old, new) = token_diff("// app", "// edited first line");
        assert_eq!(old, [(3, 6)]);
        assert_eq!(new, [(3, 20)]);
    }

    fn change_shape(hunk: &Hunk) -> Vec<(usize, usize)> {
        hunk.blocks
            .iter()
            .map(|block| match block {
                Block::Context(lines) => (lines.len(), lines.len()),
                Block::Change {
                    deletions,
                    additions,
                } => (deletions.len(), additions.len()),
            })
            .collect()
    }

    #[test]
    fn surplus_additions_pair_by_similarity() {
        let mut patch = String::from(
            "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1,4 +1,8 @@\n ctx\n",
        );
        for n in 3..=4 {
            patch.push_str(&format!("-let old_line = {n}\n"));
        }
        for n in 1..=4 {
            patch.push_str(&format!("+let new_line = {n}\n"));
        }
        let files = parse(&patch);
        let hunk = &files[0].hunks[0];
        // The two leading additions become a pure insertion; old 3 and 4
        // pair with new 3 and 4.
        assert_eq!(change_shape(hunk), [(1, 1), (0, 2), (2, 2)]);
        let Block::Change { additions, .. } = &hunk.blocks[2] else {
            panic!("expected the paired change");
        };
        assert_eq!(additions[0].text, "let new_line = 3");
        assert_eq!(additions[0].new, 4);
    }

    #[test]
    fn blank_insertions_slide_to_the_top_of_their_run() {
        let patch =
            "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1,3 +1,4 @@\n x\n \n \n+\n";
        let files = parse(patch);
        let hunk = &files[0].hunks[0];
        assert_eq!(change_shape(hunk), [(1, 1), (0, 1), (2, 2)]);
        let Block::Change { additions, .. } = &hunk.blocks[1] else {
            panic!("expected the insertion");
        };
        assert_eq!(additions[0].new, 2);
        let Block::Context(after) = &hunk.blocks[2] else {
            panic!("expected trailing context");
        };
        assert_eq!((after[0].old, after[0].new), (2, 3));
    }
}
