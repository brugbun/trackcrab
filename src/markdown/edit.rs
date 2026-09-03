//! Markdown-aware editing: what Tab, Enter and Backspace mean inside a list.
//!
//! Pure, like the parser, and for the same reason: this is custom editing logic
//! sitting next to egui's own, which is exactly the sort of thing that goes
//! subtly wrong. Every operation is a function from a string and a caret to a
//! new string and a new caret, so the whole surface can be asserted without a
//! window.
//!
//! `None` always means **not our business**: the key was not one of ours, or
//! the caret was somewhere the rule does not apply. The caller then lets egui
//! do whatever it would normally have done, which is the behaviour to fall back
//! to in every case that is not specifically a list.

use std::ops::Range;

use super::{INDENT, Line, LineKind, MAX_DEPTH, line};

/// One of the markdown-aware editing operations.
pub type Operation = fn(&str, &Range<usize>) -> Option<Edit>;

/// The result of an edit: the whole new text and where the caret ends up.
///
/// The whole text rather than a splice, because these documents are small and a
/// caller that has to apply offsets correctly is a caller that can get them
/// wrong.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edit {
    pub text: String,
    /// Byte range in the **new** text. Empty for a plain caret.
    pub caret: Range<usize>,
}

/// Tab: indent.
///
/// A list item steps one level deeper. Anything else gets [`INDENT`] spaces at
/// the caret, so a Tab typed in a note never puts a literal tab in the file,
/// which would leave a document indented two ways at once.
#[must_use]
pub fn tab(text: &str, caret: &Range<usize>) -> Option<Edit> {
    let lines = line::lines(text);
    let (first, last) = touched(&lines, caret)?;

    if first != last {
        // A selection spanning lines indents all of them, list or not, which is
        // what every editor does.
        return Some(indent_lines(text, &lines, first, last, caret));
    }

    let target = &lines[first];
    // Never inside a fence delimiter. Two spaces in the middle of the backticks
    // silently stops it being a fence at all, and everything below it is
    // reinterpreted as prose.
    if matches!(
        target.kind,
        LineKind::FenceOpen { .. } | LineKind::FenceClose
    ) {
        return None;
    }
    if target.kind.is_list() {
        if depth_allowed(&lines, first) <= target.depth {
            // Already as deep as it may go.
            return None;
        }
        return Some(indent_lines(text, &lines, first, last, caret));
    }
    // Inside a code block, or in prose, the spaces go at the caret. Indenting
    // code is a normal thing to want.
    Some(insert(text, caret, &" ".repeat(INDENT)))
}

/// Shift+Tab: outdent.
#[must_use]
pub fn untab(text: &str, caret: &Range<usize>) -> Option<Edit> {
    let lines = line::lines(text);
    let (first, last) = touched(&lines, caret)?;
    // Nothing to give back.
    if (first..=last).all(|index| indent_len(text, &lines[index]) == 0) {
        return None;
    }
    Some(outdent_lines(text, &lines, first, last, caret))
}

/// Enter: continue the list, or leave it.
#[must_use]
pub fn enter(text: &str, caret: &Range<usize>) -> Option<Edit> {
    // A selection means "replace this", which is egui's job.
    if !caret.is_empty() {
        return None;
    }
    let lines = line::lines(text);
    let index = line_at(&lines, caret.start)?;
    let target = &lines[index];
    if target.is_code() || !target.kind.is_list() {
        return None;
    }

    let content = &text[target.content.clone()];
    if content.trim().is_empty() {
        // An empty item is how you get *out* of a list: the marker goes and the
        // line is left blank, rather than another empty item appearing below.
        let mut out = String::with_capacity(text.len());
        out.push_str(&text[..target.range.start]);
        out.push_str(&text[target.range.end..]);
        let at = target.range.start;
        return Some(Edit {
            text: out,
            caret: at..at,
        });
    }

    // At or before the content, Enter pushes the item down rather than making
    // a new one above it. Inserting a marker here produced an empty item and,
    // for a numbered list, an extra number out of nowhere.
    if caret.start <= target.content.start {
        return None;
    }

    let indent = text[target.marker.clone()]
        .chars()
        .take_while(|c| c.is_whitespace())
        .collect::<String>();
    let marker = next_marker(&target.kind);
    let inserted = format!("\n{indent}{marker}");
    let mut edit = insert(text, caret, &inserted);
    // Past the marker, not before it: the point is to carry on typing.
    edit.caret = caret.start + inserted.len()..caret.start + inserted.len();
    Some(edit)
}

/// Backspace: strip the block marker when the caret is right after it.
///
/// Applies to headings as well as lists. It is the same rule either way, and
/// the alternative, deleting one invisible character of collapsed markup, is
/// never what anyone wants.
#[must_use]
pub fn backspace(text: &str, caret: &Range<usize>) -> Option<Edit> {
    if !caret.is_empty() {
        return None;
    }
    let lines = line::lines(text);
    let index = line_at(&lines, caret.start)?;
    let target = &lines[index];

    let strippable = target.kind.is_list() || matches!(target.kind, LineKind::Heading(_));
    if !strippable || target.marker.is_empty() || caret.start != target.marker.end {
        return None;
    }

    // The indent stays. Removing it too would jump the item to the top level in
    // one keystroke; Shift+Tab is how you change depth.
    let keep = target.range.start + indent_len(text, target);
    let mut out = String::with_capacity(text.len());
    out.push_str(&text[..keep]);
    out.push_str(&text[target.marker.end..]);
    Some(Edit {
        text: out,
        caret: keep..keep,
    })
}

/// The marker a new item after this one should carry.
fn next_marker(kind: &LineKind) -> String {
    match kind {
        // A fresh item is not done, whatever the one above it says.
        LineKind::Task(_) => "- [ ] ".to_owned(),
        LineKind::Numbered(value) => format!("{}. ", value.saturating_add(1)),
        _ => "- ".to_owned(),
    }
}

/// The deepest this line may legally sit.
///
/// One level past the nearest list item above it, so an item cannot be nested
/// under nothing. Blank lines are stepped over, since a blank line inside a
/// list is common enough; anything else ends the list and resets the ceiling.
fn depth_allowed(lines: &[Line], index: usize) -> usize {
    for above in lines[..index].iter().rev() {
        if above.kind.is_list() {
            return (above.depth + 1).min(MAX_DEPTH);
        }
        if above.kind != LineKind::Blank {
            break;
        }
    }
    0
}

/// Byte length of a line's leading whitespace.
fn indent_len(text: &str, target: &Line) -> usize {
    text[target.range.clone()]
        .chars()
        .take_while(|c| c.is_whitespace())
        .map(char::len_utf8)
        .sum()
}

/// The line containing a byte offset.
fn line_at(lines: &[Line], at: usize) -> Option<usize> {
    // Inclusive of both ends, taking the earlier line at a boundary, which
    // matches how the renderer decides which line the caret is on.
    lines
        .iter()
        .position(|line| line.range.start <= at && at <= line.range.end)
}

/// The range of lines a caret or selection touches.
fn touched(lines: &[Line], caret: &Range<usize>) -> Option<(usize, usize)> {
    let first = line_at(lines, caret.start)?;
    let last = line_at(lines, caret.end).unwrap_or(first);
    Some((first, last.max(first)))
}

/// Adds one indent level to a run of lines.
///
/// Kept apart from [`outdent_lines`] on purpose. Doing both in one function
/// meant tracking a signed delta across the walk, which is fiddly to read and
/// was the only place in the crate needing `usize` to `isize` casts. Each
/// direction on its own is plain addition or plain subtraction.
fn indent_lines(
    text: &str,
    lines: &[Line],
    first: usize,
    last: usize,
    caret: &Range<usize>,
) -> Edit {
    let pad = " ".repeat(INDENT);
    let mut out = String::with_capacity(text.len() + (last - first + 1) * INDENT);
    let mut at = 0;
    for target in &lines[first..=last] {
        out.push_str(&text[at..target.range.start]);
        out.push_str(&pad);
        out.push_str(&text[target.range.clone()]);
        at = target.range.end;
    }
    out.push_str(&text[at..]);

    // Every line at or before an endpoint has grown by one pad.
    //
    // `anchor` distinguishes the two cases that differ at a line boundary. A
    // plain caret follows its own text, so an offset sitting exactly at a line
    // start moves past the new indent. A *selection's* start stays where it is,
    // so the selection keeps covering whole lines and Tab can be pressed again.
    let moved = |offset: usize, anchor: bool| {
        let passed = lines[first..=last]
            .iter()
            .filter(|line| {
                if anchor {
                    line.range.start < offset
                } else {
                    line.range.start <= offset
                }
            })
            .count();
        offset + passed * INDENT
    };
    let selecting = !caret.is_empty();
    Edit {
        text: out,
        caret: moved(caret.start, selecting)..moved(caret.end, false),
    }
}

/// Removes up to one indent level from a run of lines.
fn outdent_lines(
    text: &str,
    lines: &[Line],
    first: usize,
    last: usize,
    caret: &Range<usize>,
) -> Edit {
    // How much each touched line gives back. A tab counts as a whole level, so
    // pasted tab-indented text outdents one step at a time like anything else.
    let removed: Vec<usize> = lines[first..=last]
        .iter()
        .map(|target| {
            let indent = &text[target.range.clone()][..indent_len(text, target)];
            if indent.starts_with('\t') {
                1
            } else {
                indent.len().min(INDENT)
            }
        })
        .collect();

    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    for (target, drop) in lines[first..=last].iter().zip(&removed) {
        out.push_str(&text[at..target.range.start]);
        out.push_str(&text[target.range.start + drop..target.range.end]);
        at = target.range.end;
    }
    out.push_str(&text[at..]);

    let moved = |offset: usize| {
        let mut shifted = offset;
        for (target, drop) in lines[first..=last].iter().zip(&removed) {
            if target.range.start + drop <= offset {
                // Entirely past the removed run.
                shifted -= drop;
            } else if target.range.start <= offset {
                // Inside it, so the caret lands where the indent used to start
                // rather than drifting into the previous line.
                shifted = target.range.start;
                break;
            }
        }
        shifted
    };
    let start = moved(caret.start);
    let end = moved(caret.end);
    Edit {
        text: out,
        caret: start.min(end)..end.max(start),
    }
}

/// Plain insertion at the caret.
fn insert(text: &str, caret: &Range<usize>, what: &str) -> Edit {
    let mut out = String::with_capacity(text.len() + what.len());
    out.push_str(&text[..caret.start]);
    out.push_str(what);
    out.push_str(&text[caret.end..]);
    let at = caret.start + what.len();
    Edit {
        text: out,
        caret: at..at,
    }
}

// ------------------------------------------------- toolbar and shortcuts (D6)

/// An inline style the toolbar or a shortcut can apply.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Wrap {
    Bold,
    Italic,
    Underline,
    Strike,
    Code,
    /// The colour token as it is written in the markup: a palette name, a
    /// `#hex`, or `None` for the default.
    Highlight(Option<String>),
}

impl Wrap {
    /// The delimiters, opening then closing.
    fn delimiters(&self) -> (String, &'static str) {
        match self {
            Self::Bold => ("**".to_owned(), "**"),
            Self::Italic => ("*".to_owned(), "*"),
            Self::Underline => ("__".to_owned(), "__"),
            Self::Strike => ("~~".to_owned(), "~~"),
            Self::Code => ("`".to_owned(), "`"),
            Self::Highlight(None) => ("==".to_owned(), "=="),
            Self::Highlight(Some(colour)) => (format!("=={colour}|"), "=="),
        }
    }

    /// The character this style repeats, and how many of it.
    ///
    /// `None` for a coloured highlight, whose opening delimiter carries a
    /// payload and so is not a run of one character at all.
    const fn run(&self) -> Option<(char, usize)> {
        match self {
            Self::Bold => Some(('*', 2)),
            Self::Italic => Some(('*', 1)),
            Self::Underline => Some(('_', 2)),
            Self::Strike => Some(('~', 2)),
            Self::Code => Some(('`', 1)),
            Self::Highlight(None) => Some(('=', 2)),
            Self::Highlight(Some(_)) => None,
        }
    }
}

/// Is a style of `want` repeats already on, given a run of `have` beside the
/// selection?
///
/// This cannot be a string match, because `*` and `**` share a character:
/// `**bold**` *ends with* `*`, so asking "does an italic delimiter sit here"
/// with `ends_with` answers yes on bold and strips a layer off it. Pressing
/// italic on a bold word used to leave it italic and not bold.
///
/// A run is read the way markdown reads it: one is italic, two is bold, three
/// is both. So a style of two or more is on when the run is at least that long,
/// and a style of one is on when the run is **odd**. That is what makes the two
/// compose rather than cancel: italic on `**x**` gives `***x***`, and bold on
/// `***x***` gives `*x*`.
const fn present(have: usize, want: usize) -> bool {
    if want == 1 {
        have % 2 == 1
    } else {
        have >= want
    }
}

/// How many `ch` in a row `text` ends with.
fn run_before(text: &str, ch: char) -> usize {
    text.chars().rev().take_while(|c| *c == ch).count()
}

/// How many `ch` in a row `text` starts with.
fn run_after(text: &str, ch: char) -> usize {
    text.chars().take_while(|c| *c == ch).count()
}

/// A block marker the toolbar can toggle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Block {
    Heading(u8),
    Bullet,
    Numbered,
    Task,
}

/// Applies or removes an inline style.
///
/// Toggles, so the same button turns bold off again. Two ways a run can already
/// be wrapped: the delimiters sit just outside the selection, or the user
/// selected them too. Both are recognised, because both are what someone does
/// when they mean "undo this".
#[must_use]
pub fn wrap(text: &str, caret: &Range<usize>, what: &Wrap) -> Edit {
    let (open, close) = what.delimiters();
    let selected = &text[caret.clone()];

    if let Some((ch, want)) = what.run() {
        // Already wrapped, delimiters just outside the selection.
        let outside = run_before(&text[..caret.start], ch).min(run_after(&text[caret.end..], ch));
        if present(outside, want) {
            let outer = caret.start - want..caret.end + want;
            let mut edit = splice(text, &outer, selected);
            // The same text stays selected, now unwrapped.
            edit.caret = outer.start..outer.start + selected.len();
            return edit;
        }
        // Already wrapped, delimiters inside it. The other way people select an
        // emphasised word.
        let inside = run_after(selected, ch).min(run_before(selected, ch));
        if selected.len() > want * 2 && present(inside, want) {
            let inner = caret.start + want..caret.end - want;
            return splice(text, caret, &text[inner]);
        }
    } else {
        // A coloured highlight, whose opening delimiter is not a run, so it is
        // matched whole.
        if selected.len() > open.len() + close.len()
            && selected.starts_with(&open)
            && selected.ends_with(close)
        {
            let inner = caret.start + open.len()..caret.end - close.len();
            return splice(text, caret, &text[inner]);
        }
        if text[..caret.start].ends_with(&open) && text[caret.end..].starts_with(close) {
            let outer = caret.start - open.len()..caret.end + close.len();
            let mut edit = splice(text, &outer, selected);
            edit.caret = outer.start..outer.start + selected.len();
            return edit;
        }
    }

    let body = format!("{open}{selected}{close}");
    let mut edit = splice(text, caret, &body);
    edit.caret = if caret.is_empty() {
        // Nothing selected, so the caret goes between the delimiters, ready to
        // type the thing being emphasised.
        let at = caret.start + open.len();
        at..at
    } else {
        // The same text stays selected, now wrapped, so the button can be
        // pressed again to undo it.
        caret.start + open.len()..caret.start + open.len() + selected.len()
    };
    edit
}

/// Applies or removes a block marker on every line the caret touches.
#[must_use]
pub fn block(text: &str, caret: &Range<usize>, what: Block) -> Option<Edit> {
    let lines = line::lines(text);
    let (first, last) = touched(&lines, caret)?;
    let run = &lines[first..=last];
    // Code is code. Turning a line of a shell script into a heading by pressing
    // a toolbar button would be a surprise.
    if run.iter().any(Line::is_code) {
        return None;
    }

    // Off again if every touched line already has exactly this marker, which is
    // what makes the buttons toggles rather than one-way switches.
    let already = run.iter().all(|line| matches(&line.kind, what));

    let mut out = String::with_capacity(text.len() + run.len() * 8);
    let mut at = 0;
    for (offset, line) in run.iter().enumerate() {
        out.push_str(&text[at..line.range.start]);
        out.push_str(&text[line.range.clone()][..indent_len(text, line)]);
        if !already {
            out.push_str(&marker_for(what, offset));
        }
        out.push_str(&text[line.content.clone()]);
        at = line.range.end;
    }
    out.push_str(&text[at..]);

    // Per endpoint, not one total for both: the end of a multi-line selection
    // sits past every marker in the run while the start sits before all of
    // them. Sharing a total left the selection covering the wrong text.
    //
    // `anchor` is the same distinction `indent_lines` makes. A selection's
    // start at a line start stays there, so the selection keeps covering whole
    // lines; anything else moves with its text.
    let moved = |offset: usize, anchor: bool| {
        let mut added = 0_usize;
        let mut removed = 0_usize;
        for (index, line) in run.iter().enumerate() {
            let before = if anchor {
                line.range.start < offset
            } else {
                line.range.start <= offset
            };
            if !before {
                continue;
            }
            removed += line.marker.len() - indent_len(text, line);
            if !already {
                added += marker_for(what, index).len();
            }
        }
        // Clamped, so a caret that sat inside a marker being removed cannot
        // slide up into the line above.
        (offset + added)
            .saturating_sub(removed)
            .max(run[0].range.start)
    };
    let selecting = !caret.is_empty();
    let start = moved(caret.start, selecting);
    let end = moved(caret.end, false).max(start);
    Some(Edit {
        text: out,
        caret: start..end,
    })
}

/// Inserts a divider below the caret's line.
#[must_use]
pub fn divider(text: &str, caret: &Range<usize>) -> Option<Edit> {
    let lines = line::lines(text);
    let index = line_at(&lines, caret.start)?;
    let target = &lines[index];

    // A blank line becomes the divider rather than growing a second one, so
    // pressing the button on an empty line does the obvious thing.
    let (span, body, caret_at) = if target.kind == LineKind::Blank {
        (target.range.clone(), "---", target.range.start + 3)
    } else {
        // Below the rule, which is where you carry on writing.
        (
            target.range.end..target.range.end,
            "\n---\n",
            target.range.end + 5,
        )
    };
    let mut edit = splice(text, &span, body);
    edit.caret = caret_at..caret_at;
    Some(edit)
}

/// Wraps the selection in a fenced code block, or inserts an empty one.
#[must_use]
pub fn code_block(text: &str, caret: &Range<usize>) -> Option<Edit> {
    let lines = line::lines(text);
    let (first, last) = touched(&lines, caret)?;
    let open = &lines[first];
    let close = &lines[last];

    if caret.is_empty() && open.kind == LineKind::Blank {
        // Nothing to wrap: leave an empty block with the caret inside it.
        let mut edit = splice(text, &open.range, "```\n\n```");
        let at = open.range.start + 4;
        edit.caret = at..at;
        return Some(edit);
    }

    let body = format!("```\n{}\n```", &text[open.range.start..close.range.end]);
    let span = open.range.start..close.range.end;
    let mut edit = splice(text, &span, &body);
    // Inside the fence, on the first line of code.
    let at = open.range.start + 4;
    edit.caret = at..at;
    Some(edit)
}

/// Turns the selection into a link, ready for a URL.
#[must_use]
pub fn link(text: &str, caret: &Range<usize>) -> Edit {
    let label = &text[caret.clone()];
    let body = format!("[{label}]()");
    let mut edit = splice(text, caret, &body);
    // In the parentheses either way: with a label already written the address
    // is what is missing, and with nothing written it is still the harder half
    // to type from memory.
    let at = caret.start + label.len() + 3;
    edit.caret = at..at;
    edit
}

/// Ticks or unticks the checkbox on the caret's line.
///
/// The two states are the same length, so the caret does not move at all. That
/// is what makes a click on the box safe: the caret has already been placed by
/// the click itself, and toggling underneath it cannot shift the text out from
/// under it.
#[must_use]
pub fn toggle_task(text: &str, caret: &Range<usize>) -> Option<Edit> {
    let lines = line::lines(text);
    let index = line_at(&lines, caret.start)?;
    let target = &lines[index];
    let LineKind::Task(checked) = target.kind else {
        return None;
    };
    // Found by search rather than by arithmetic on the marker: the marker
    // includes the leading indent, so its length is not fixed.
    let marker = &text[target.marker.clone()];
    let at = target.marker.start + marker.find('[').map_or(0, |open| open + 1);
    let mut edit = splice(text, &(at..at + 1), if checked { " " } else { "x" });
    edit.caret = caret.clone();
    Some(edit)
}

/// Wraps a selection in a link pointing at a pasted address.
///
/// `None` hands the paste straight back to egui, which is the common case: this
/// only fires when what was pasted really is an address and there really is
/// something to hang it on.
#[must_use]
pub fn paste_link(text: &str, caret: &Range<usize>, pasted: &str) -> Option<Edit> {
    let url = pasted.trim();
    if !super::is_url(url) || caret.is_empty() {
        return None;
    }
    let label = &text[caret.clone()];
    // A label carrying any of the link punctuation would produce a link that
    // parses as something else, and one spanning lines is not a label at all.
    // Declining leaves the ordinary paste, which is never wrong, only plainer.
    if label.contains(['[', ']', '(', ')', '\n']) {
        return None;
    }
    let body = format!("[{label}]({url})");
    let mut edit = splice(text, caret, &body);
    let at = caret.start + body.len();
    edit.caret = at..at;
    Some(edit)
}

/// Does this line already carry exactly this marker?
fn matches(kind: &LineKind, what: Block) -> bool {
    match (kind, what) {
        (LineKind::Heading(a), Block::Heading(b)) => *a == b,
        (LineKind::Bullet, Block::Bullet)
        | (LineKind::Numbered(_), Block::Numbered)
        | (LineKind::Task(_), Block::Task) => true,
        _ => false,
    }
}

/// The marker text for a block, `offset` lines into a run.
fn marker_for(what: Block, offset: usize) -> String {
    match what {
        Block::Heading(level) => format!("{} ", "#".repeat(level.clamp(1, 4) as usize)),
        Block::Bullet => "- ".to_owned(),
        // Numbered sequentially across a selection, so turning three lines into
        // a list gives 1, 2, 3 rather than three number ones.
        Block::Numbered => format!("{}. ", offset + 1),
        Block::Task => "- [ ] ".to_owned(),
    }
}

/// Replaces a range with new text.
fn splice(text: &str, range: &Range<usize>, with: &str) -> Edit {
    let mut out = String::with_capacity(text.len() + with.len());
    out.push_str(&text[..range.start]);
    out.push_str(with);
    out.push_str(&text[range.end..]);
    let at = range.start + with.len();
    Edit {
        text: out,
        caret: at..at,
    }
}
