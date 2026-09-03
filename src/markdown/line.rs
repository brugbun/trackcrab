//! Line classification: the block level pass.
//!
//! One [`Line`] per source line rather than one per logical block, on purpose.
//! Every block level thing this editor supports is line scoped anyway (a
//! heading, a list item, a divider), and the renderer works from egui galley
//! rows, which map to lines. A fenced code block therefore comes out as an
//! opening line, some [`LineKind::Code`] lines and a closing line, which is
//! exactly what a painter needs to draw one background across the run.

use std::ops::Range;

use super::INDENT;

/// What a line is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LineKind {
    /// Nothing but whitespace.
    Blank,
    /// Ordinary text.
    Paragraph,
    /// `# ` to `#### `. Level 1 to 4; five or more hashes is a paragraph, since
    /// nothing here styles a heading that small.
    Heading(u8),
    /// `- ` or `* `.
    Bullet,
    /// `1. `, keeping the number that was typed so the renderer shows it and
    /// Enter can carry on from it.
    Numbered(u32),
    /// `- [ ] ` or `- [x] `.
    Task(bool),
    /// `---` or longer.
    Divider,
    /// The ```` ``` ```` that opens a fence. The range is the language tag,
    /// empty when none was given.
    FenceOpen { lang: Range<usize> },
    /// The ```` ``` ```` that closes one.
    FenceClose,
    /// A line inside a fence.
    Code,
}

impl LineKind {
    /// Is this line's content exempt from inline parsing?
    #[must_use]
    pub const fn is_verbatim(&self) -> bool {
        matches!(self, Self::Code)
    }

    /// Is this a list item of any sort? The three share indent, Tab behaviour
    /// and Enter continuation, so asking once is better than matching three
    /// variants at every call site.
    #[must_use]
    pub const fn is_list(&self) -> bool {
        matches!(self, Self::Bullet | Self::Numbered(_) | Self::Task(_))
    }

    /// Is this line pure markup, with no words of its own?
    #[must_use]
    pub const fn is_rule(&self) -> bool {
        matches!(
            self,
            Self::Divider | Self::FenceOpen { .. } | Self::FenceClose
        )
    }
}

/// One classified source line.
///
/// `marker` and `content` always partition `range`, so
/// `marker.end == content.start` and hiding the marker leaves exactly the
/// content. The marker includes any leading indent, so collapsing it collapses
/// the whitespace too and the renderer is free to draw its own indent instead.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    /// The line's bytes, excluding the newline and any carriage return.
    pub range: Range<usize>,
    pub kind: LineKind,
    /// Leading indent plus the block marker, e.g. `"  - [x] "`. Empty for a
    /// paragraph with no indent.
    pub marker: Range<usize>,
    /// What is left after the marker. Where inline spans are parsed.
    pub content: Range<usize>,
    /// Indent level. Always zero for anything that is not a list item, since
    /// only lists nest.
    pub depth: usize,
}

impl Line {
    /// Does this line sit inside a fenced code block, fences included?
    #[must_use]
    pub const fn is_code(&self) -> bool {
        matches!(
            self.kind,
            LineKind::Code | LineKind::FenceOpen { .. } | LineKind::FenceClose
        )
    }
}

/// The fence delimiter.
const FENCE: &str = "```";
/// Shortest run of dashes that counts as a divider.
const DIVIDER_MIN: usize = 3;
/// Deepest heading that gets styled.
const HEADING_MAX: usize = 4;

/// Classifies every line in `text`.
///
/// One pass, because fences are stateful: whether a line is code depends on
/// whether an earlier line opened a fence. An unclosed fence runs to the end of
/// the document, which is what every editor does and what makes a fence usable
/// while you are still typing inside it.
#[must_use]
pub fn lines(text: &str) -> Vec<Line> {
    let mut out = Vec::new();
    let mut in_fence = false;

    for range in line_ranges(text) {
        let raw = &text[range.clone()];
        let (kind, marker_len, depth) = if in_fence {
            if is_fence_close(raw) {
                in_fence = false;
                (LineKind::FenceClose, raw.len(), 0)
            } else {
                (LineKind::Code, 0, 0)
            }
        } else if let Some(lang) = fence_open(raw) {
            in_fence = true;
            let lang = range.start + lang.start..range.start + lang.end;
            let len = raw.len();
            (LineKind::FenceOpen { lang }, len, 0)
        } else {
            classify(raw)
        };

        let marker = range.start..range.start + marker_len;
        let content = marker.end..range.end;
        out.push(Line {
            range,
            kind,
            marker,
            content,
            depth,
        });
    }
    out
}

/// Byte ranges of each line, excluding the newline and any carriage return.
///
/// Hand rolled rather than `str::lines`, which discards the offsets, and every
/// range here has to be usable against the original string.
fn line_ranges(text: &str) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut start = 0;
    for (index, byte) in text.bytes().enumerate() {
        if byte == b'\n' {
            let mut end = index;
            if end > start && text.as_bytes()[end - 1] == b'\r' {
                end -= 1;
            }
            out.push(start..end);
            start = index + 1;
        }
    }
    // The trailing line, which has no newline of its own. An empty document is
    // still one blank line, so the editor has somewhere to put the caret.
    let mut end = text.len();
    if end > start && text.as_bytes()[end - 1] == b'\r' {
        end -= 1;
    }
    out.push(start..end);
    out
}

/// Classifies one line outside any fence, returning its kind, the byte length
/// of its marker, and its indent depth.
fn classify(raw: &str) -> (LineKind, usize, usize) {
    let indent = raw.len() - raw.trim_start().len();
    let body = &raw[indent..];

    if body.is_empty() {
        return (LineKind::Blank, 0, 0);
    }
    if is_divider(body) {
        return (LineKind::Divider, raw.len(), 0);
    }
    // Depth is only meaningful for lists, so it is computed from the indent
    // width with a tab counting as one level.
    let depth = indent_levels(&raw[..indent]);

    if let Some(level) = heading(body) {
        // A heading is not a list, so its indent is cosmetic and its depth is
        // zero. The marker still swallows the indent.
        let marker = indent + level as usize + 1;
        return (LineKind::Heading(level), marker, 0);
    }
    if let Some((checked, marker)) = task(body) {
        return (LineKind::Task(checked), indent + marker, depth);
    }
    if let Some(marker) = bullet(body) {
        return (LineKind::Bullet, indent + marker, depth);
    }
    if let Some((number, marker)) = numbered(body) {
        return (LineKind::Numbered(number), indent + marker, depth);
    }
    // A paragraph keeps its own leading whitespace: there is no marker to hide,
    // so swallowing the indent would silently reflow text the user typed.
    (LineKind::Paragraph, 0, 0)
}

/// Indent levels in a run of leading whitespace, a tab counting as one level.
fn indent_levels(indent: &str) -> usize {
    let width: usize = indent
        .chars()
        .map(|c| if c == '\t' { INDENT } else { 1 })
        .sum();
    width / INDENT
}

/// `---` or longer, and nothing else on the line.
fn is_divider(body: &str) -> bool {
    let trimmed = body.trim_end();
    trimmed.len() >= DIVIDER_MIN && trimmed.bytes().all(|b| b == b'-')
}

/// `#` to `####` followed by a space.
fn heading(body: &str) -> Option<u8> {
    let hashes = body.bytes().take_while(|b| *b == b'#').count();
    // The trailing space is required, so a `#tag` written in prose stays prose.
    if (1..=HEADING_MAX).contains(&hashes) && body.as_bytes().get(hashes) == Some(&b' ') {
        return u8::try_from(hashes).ok();
    }
    None
}

/// `- ` or `* `, returning the marker length.
fn bullet(body: &str) -> Option<usize> {
    let mut chars = body.chars();
    match (chars.next(), chars.next()) {
        (Some('-' | '*'), Some(' ')) => Some(2),
        _ => None,
    }
}

/// `- [ ] ` or `- [x] `, returning whether it is ticked and the marker length.
fn task(body: &str) -> Option<(bool, usize)> {
    let rest = body
        .strip_prefix("- ")
        .or_else(|| body.strip_prefix("* "))?;
    let checked = if rest.starts_with("[ ] ") {
        false
    } else if rest.starts_with("[x] ") || rest.starts_with("[X] ") {
        true
    } else {
        return None;
    };
    Some((checked, 2 + 4))
}

/// `1. `, returning the number and the marker length.
fn numbered(body: &str) -> Option<(u32, usize)> {
    let digits = body.bytes().take_while(u8::is_ascii_digit).count();
    // Capped so a long run of digits in prose cannot overflow the parse, and
    // because nobody writes a hundred-million-item list.
    if digits == 0 || digits > 9 {
        return None;
    }
    if body.as_bytes().get(digits) != Some(&b'.') || body.as_bytes().get(digits + 1) != Some(&b' ')
    {
        return None;
    }
    let number = body[..digits].parse().ok()?;
    Some((number, digits + 2))
}

/// The language tag of an opening fence, if this line is one.
fn fence_open(raw: &str) -> Option<Range<usize>> {
    let trimmed = raw.trim_start();
    let indent = raw.len() - trimmed.len();
    let rest = trimmed.strip_prefix(FENCE)?;
    // A language tag is a bare word. Anything else, including a second fence on
    // the same line, is not a tag we can use.
    let tag = rest.trim();
    if !tag
        .chars()
        .all(|c| c.is_alphanumeric() || c == '+' || c == '-' || c == '#')
    {
        return Some(0..0);
    }
    let start = indent + FENCE.len() + (rest.len() - rest.trim_start().len());
    Some(start..start + tag.len())
}

/// A line that closes a fence: the delimiter and nothing else.
fn is_fence_close(raw: &str) -> bool {
    raw.trim() == FENCE
}
