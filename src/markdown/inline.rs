//! Inline spans: the character level pass.
//!
//! Output is a **flat, non-overlapping** list of spans covering the input range
//! exactly, each carrying the accumulated [`Style`] of every delimiter it sits
//! inside. Flat rather than a tree because that is precisely the shape an
//! `egui::text::LayoutJob` wants, so the renderer becomes a loop with no
//! traversal of its own. Nesting is handled while scanning: `**bold *and
//! italic* here**` yields three spans, the middle one carrying both flags.

use std::ops::Range;

/// One of the named highlight colours.
///
/// A small closed palette rather than free hex everywhere, so highlights stay
/// legible against the app's own background and can be re-tuned in one place if
/// the theme changes. Hex is still accepted for the cases a palette cannot
/// cover, but it is the escape hatch, not the default.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Palette {
    Yellow,
    Green,
    Blue,
    Pink,
    Purple,
    Orange,
    Red,
    Grey,
}

impl Palette {
    /// Every palette entry, in the order a picker should show them.
    #[must_use]
    pub const fn variants() -> [Self; 8] {
        [
            Self::Yellow,
            Self::Green,
            Self::Blue,
            Self::Pink,
            Self::Purple,
            Self::Orange,
            Self::Red,
            Self::Grey,
        ]
    }

    /// The name as it is written in the markup.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Yellow => "yellow",
            Self::Green => "green",
            Self::Blue => "blue",
            Self::Pink => "pink",
            Self::Purple => "purple",
            Self::Orange => "orange",
            Self::Red => "red",
            Self::Grey => "grey",
        }
    }

    /// Parses a name. `gray` is accepted alongside `grey`, because half the
    /// world spells it the other way and a highlight is not worth a spelling
    /// argument.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "yellow" => Some(Self::Yellow),
            "green" => Some(Self::Green),
            "blue" => Some(Self::Blue),
            "pink" => Some(Self::Pink),
            "purple" => Some(Self::Purple),
            "orange" => Some(Self::Orange),
            "red" => Some(Self::Red),
            "grey" | "gray" => Some(Self::Grey),
            _ => None,
        }
    }
}

/// Which colour a highlight is drawn in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HighlightColor {
    /// `==text==`, with no colour asked for.
    Default,
    /// `==yellow|text==`.
    Named(Palette),
    /// `==#f2c14e|text==`.
    Rgb([u8; 3]),
}

/// Everything that applies to one span of text.
///
/// A set of accumulating flags rather than an enum of styles: bold, italic and
/// underline compose, and the layout format they turn into composes the same
/// way, so anything that had to pick one would be lying.
// The one place `struct_excessive_bools` is wrong. It suggests a state machine,
// but these are not states: a span can be bold and italic and underlined at
// once, which is the whole point of the recursive scan. `egui::TextFormat`
// models the same thing the same way, so folding them into an enum would only
// have to be unfolded again at the point of use.
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent, composing flags"
)]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Style {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    /// Inline code. Nothing else applies inside it.
    pub code: bool,
    pub highlight: Option<HighlightColor>,
    /// Byte range of this span's target URL. For a bare autolink that is the
    /// span's own range; for `[text](url)` it is the part inside the brackets.
    /// One field either way, so the renderer never asks which kind it is.
    pub link: Option<Range<usize>>,
}

impl Style {
    /// Is this the plain, unformatted style?
    #[must_use]
    pub fn is_plain(&self) -> bool {
        *self == Self::default()
    }
}

/// A run of text with one style.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    pub range: Range<usize>,
    pub style: Style,
}

/// The inline parse of one line's content.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Inline {
    /// In order, non-overlapping, covering the input range exactly. Empty
    /// ranges are never emitted.
    pub spans: Vec<Span>,
    /// Delimiter runs, in order. These are the bytes the renderer collapses
    /// when the caret is not on this line, and the reason the source string
    /// stays the source of truth: nothing is rewritten to render it.
    pub markup: Vec<Range<usize>>,
}

/// One piece of a line, in reading order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Piece<'a> {
    /// Text the reader sees.
    Text(&'a Span),
    /// Delimiter bytes: the reader sees these only while editing the line.
    Markup(Range<usize>),
}

impl Piece<'_> {
    #[must_use]
    pub fn range(&self) -> Range<usize> {
        match self {
            Self::Text(span) => span.range.clone(),
            Self::Markup(range) => range.clone(),
        }
    }
}

impl Inline {
    /// Spans and markup merged into one ordered sequence.
    ///
    /// The parse keeps them apart because they are answers to different
    /// questions, but a renderer walks the line once from left to right, and
    /// since the two together partition the content exactly this is a merge
    /// rather than a sort.
    #[must_use]
    pub fn pieces(&self) -> Vec<Piece<'_>> {
        let mut out = Vec::with_capacity(self.spans.len() + self.markup.len());
        let mut spans = self.spans.iter().peekable();
        let mut markup = self.markup.iter().peekable();
        loop {
            let span_first = match (spans.peek(), markup.peek()) {
                (Some(span), Some(mark)) => span.range.start < mark.start,
                (Some(_), None) => true,
                (None, Some(_)) => false,
                (None, None) => break,
            };
            if span_first {
                out.push(Piece::Text(spans.next().expect("peeked")));
            } else {
                out.push(Piece::Markup(markup.next().expect("peeked").clone()));
            }
        }
        out
    }

    /// One unstyled span over the whole range, for text that must not be
    /// parsed at all. What a line inside a code fence gets.
    #[must_use]
    pub fn verbatim(range: Range<usize>) -> Self {
        let mut out = Self::default();
        if !range.is_empty() {
            out.spans.push(Span {
                range,
                style: Style {
                    code: true,
                    ..Style::default()
                },
            });
        }
        out
    }
}

/// The delimiter characters, for deciding what a backslash can escape.
const ESCAPABLE: &[char] = &['*', '_', '~', '`', '=', '[', ']', '(', ')', '\\', '#', '-'];

/// Parses the inline markup in `range`.
#[must_use]
pub fn spans(text: &str, range: Range<usize>) -> Inline {
    let mut out = Inline::default();
    scan(text, range, &Style::default(), &mut out);
    out
}

/// Scans a range, emitting spans in `style` and recursing into delimiters.
fn scan(text: &str, range: Range<usize>, style: &Style, out: &mut Inline) {
    let mut at = range.start;
    // Start of the run of plain text not yet emitted.
    let mut plain = at;

    while at < range.end {
        let rest = &text[at..range.end];

        // A backslash makes the next delimiter literal. The backslash itself is
        // markup, so it disappears once the caret leaves the line, and what is
        // left is the character the user wanted to see.
        if let Some(escaped) = escape(rest) {
            emit(out, plain..at, style);
            out.markup.push(at..at + 1);
            emit(out, at + 1..at + 1 + escaped, style);
            at += 1 + escaped;
            plain = at;
            continue;
        }

        if let Some(found) = delimiter(text, at, range.end, style) {
            emit(out, plain..at, style);
            out.markup.push(found.open);
            if found.verbatim {
                // Code: the contents are the contents, delimiters and all.
                emit(out, found.inner, &found.style);
            } else {
                scan(text, found.inner, &found.style, out);
            }
            out.markup.push(found.close);
            at = found.after;
            plain = at;
            continue;
        }

        // A bare URL becomes a link with no markup to hide, since the text and
        // the target are the same thing.
        if style.link.is_none()
            && at == word_start(text, at, range.start)
            && let Some(len) = autolink(rest)
        {
            emit(out, plain..at, style);
            emit(
                out,
                at..at + len,
                &Style {
                    link: Some(at..at + len),
                    ..style.clone()
                },
            );
            at += len;
            plain = at;
            continue;
        }

        at += char_len(rest);
    }
    emit(out, plain..range.end, style);
}

/// A matched delimiter pair.
struct Found {
    /// The opening delimiter's bytes.
    open: Range<usize>,
    /// What sits between the delimiters.
    inner: Range<usize>,
    /// The closing delimiter's bytes.
    close: Range<usize>,
    /// Where scanning resumes.
    after: usize,
    /// The style inside.
    style: Style,
    /// Whether the inside is exempt from further parsing.
    verbatim: bool,
}

/// Every delimiter, longest first so `***` is not read as `*` then `**`.
///
/// Order is the precedence: code before everything, because nothing applies
/// inside it; the two character runs before the one character ones.
fn delimiter(text: &str, at: usize, end: usize, style: &Style) -> Option<Found> {
    // Inside code, nothing opens. Checked here rather than at every call site.
    if style.code {
        return None;
    }
    let rest = &text[at..end];

    if rest.starts_with('`') {
        return paired(text, at, end, "`", style, |s| s.code = true).map(|mut f| {
            f.verbatim = true;
            f
        });
    }
    if rest.starts_with("***") {
        return paired(text, at, end, "***", style, |s| {
            s.bold = true;
            s.italic = true;
        });
    }
    if rest.starts_with("**") {
        return paired(text, at, end, "**", style, |s| s.bold = true);
    }
    if rest.starts_with("~~") {
        return paired(text, at, end, "~~", style, |s| s.strike = true);
    }
    if rest.starts_with("==") {
        return highlight(text, at, end, style);
    }
    if rest.starts_with("__") && word_boundary_open(text, at) {
        return paired(text, at, end, "__", style, |s| s.underline = true);
    }
    if rest.starts_with('*') {
        return paired(text, at, end, "*", style, |s| s.italic = true);
    }
    if rest.starts_with('_') && word_boundary_open(text, at) {
        return paired(text, at, end, "_", style, |s| s.italic = true);
    }
    if rest.starts_with('[') {
        return link(text, at, end, style);
    }
    None
}

/// Finds the close for a symmetric delimiter.
fn paired(
    text: &str,
    at: usize,
    end: usize,
    delim: &str,
    style: &Style,
    apply: impl FnOnce(&mut Style),
) -> Option<Found> {
    let inner_start = at + delim.len();
    let close = find_close(text, inner_start, end, delim)?;
    // An empty pair is just four literal characters, not an empty styled span.
    if close == inner_start {
        return None;
    }
    let mut inside = style.clone();
    apply(&mut inside);
    Some(Found {
        open: at..inner_start,
        inner: inner_start..close,
        close: close..close + delim.len(),
        after: close + delim.len(),
        style: inside,
        verbatim: false,
    })
}

/// The next unescaped occurrence of `delim`, respecting the word boundary rule
/// for the underscore delimiters.
fn find_close(text: &str, from: usize, end: usize, delim: &str) -> Option<usize> {
    let underscored = delim.starts_with('_');
    let mut at = from;
    while at < end {
        let rest = &text[at..end];
        if rest.starts_with('\\') {
            // Skip the escaped character so it cannot close the run. A
            // backslash that escapes nothing skips only itself, or a delimiter
            // one character further on would be stepped over by mistake.
            at += 1 + escape(rest).unwrap_or(0);
            continue;
        }
        if rest.starts_with(delim) && (!underscored || word_boundary_close(text, at + delim.len()))
        {
            return Some(at);
        }
        at += char_len(rest);
    }
    None
}

/// `==text==`, `==yellow|text==`, `==#f2c14e|text==`.
///
/// An unrecognised prefix is not treated as a failure: `==a|b==` is a default
/// highlight over the literal text `a|b`. That way a mistyped colour shows up
/// as a stray word inside the highlight, which is obvious, rather than making
/// the whole highlight vanish, which is not.
fn highlight(text: &str, at: usize, end: usize, style: &Style) -> Option<Found> {
    let inner_start = at + 2;
    let close = find_close(text, inner_start, end, "==")?;
    if close == inner_start {
        return None;
    }
    let body = &text[inner_start..close];

    let (colour, content_start) = match body.split_once('|') {
        Some((prefix, _)) => match colour_spec(prefix) {
            Some(colour) => (colour, inner_start + prefix.len() + 1),
            None => (HighlightColor::Default, inner_start),
        },
        None => (HighlightColor::Default, inner_start),
    };
    if content_start >= close {
        return None;
    }

    let mut inside = style.clone();
    inside.highlight = Some(colour);
    Some(Found {
        // The colour prefix is markup: it is an instruction, not something to
        // read, so it hides with the delimiters.
        open: at..content_start,
        inner: content_start..close,
        close: close..close + 2,
        after: close + 2,
        style: inside,
        verbatim: false,
    })
}

/// A palette name or a hex triple.
fn colour_spec(prefix: &str) -> Option<HighlightColor> {
    let prefix = prefix.trim();
    if let Some(hex) = prefix.strip_prefix('#') {
        return hex_rgb(hex).map(HighlightColor::Rgb);
    }
    Palette::parse(prefix).map(HighlightColor::Named)
}

/// `#rgb` or `#rrggbb`, shorthand expanded the usual way.
fn hex_rgb(hex: &str) -> Option<[u8; 3]> {
    let bytes = hex.as_bytes();
    if !bytes.iter().all(u8::is_ascii_hexdigit) {
        return None;
    }
    let nibble = |b: u8| -> u8 {
        match b {
            b'0'..=b'9' => b - b'0',
            b'a'..=b'f' => b - b'a' + 10,
            _ => b - b'A' + 10,
        }
    };
    match bytes.len() {
        // `#abc` means `#aabbcc`, so each nibble is doubled rather than shifted,
        // which keeps `#fff` fully white instead of `#f0f0f0`.
        3 => Some([
            nibble(bytes[0]) * 17,
            nibble(bytes[1]) * 17,
            nibble(bytes[2]) * 17,
        ]),
        6 => Some([
            nibble(bytes[0]) * 16 + nibble(bytes[1]),
            nibble(bytes[2]) * 16 + nibble(bytes[3]),
            nibble(bytes[4]) * 16 + nibble(bytes[5]),
        ]),
        _ => None,
    }
}

/// `[text](url)`.
fn link(text: &str, at: usize, end: usize, style: &Style) -> Option<Found> {
    let text_start = at + 1;
    let text_end = find_close(text, text_start, end, "]")?;
    let rest = &text[text_end + 1..end];
    if !rest.starts_with('(') {
        return None;
    }
    let url_start = text_end + 2;
    let url_end = find_close(text, url_start, end, ")")?;
    // Neither half may be empty: `[]()` is punctuation, not a link.
    if text_end == text_start || url_end == url_start {
        return None;
    }
    let mut inside = style.clone();
    inside.link = Some(url_start..url_end);
    Some(Found {
        open: at..text_start,
        inner: text_start..text_end,
        // The bracket, the parens and the URL are all markup: the reader wants
        // the label, not the address.
        close: text_end..url_end + 1,
        after: url_end + 1,
        style: inside,
        verbatim: false,
    })
}

/// Length of a bare URL at the start of `rest`, if there is one.
fn autolink(rest: &str) -> Option<usize> {
    let scheme = ["https://", "http://"]
        .into_iter()
        .find(|s| rest.starts_with(s))?;
    // Must have something after the scheme, or `https://` alone becomes a link
    // to nowhere.
    let end = rest.find(|c: char| c.is_whitespace()).unwrap_or(rest.len());
    let mut url = &rest[..end];
    // Trailing punctuation belongs to the sentence, not the address. A closing
    // bracket is trimmed too, so a URL inside parentheses does not swallow one.
    while let Some(last) = url.chars().last() {
        if matches!(
            last,
            '.' | ',' | ';' | ':' | '!' | '?' | ')' | ']' | '"' | '\''
        ) {
            url = &url[..url.len() - last.len_utf8()];
        } else {
            break;
        }
    }
    (url.len() > scheme.len()).then_some(url.len())
}

/// Whether `\` at the start of `rest` escapes something, and that something's
/// byte length.
fn escape(rest: &str) -> Option<usize> {
    let next = rest.strip_prefix('\\')?.chars().next()?;
    ESCAPABLE.contains(&next).then(|| next.len_utf8())
}

/// May an underscore delimiter open here? Only outside a word, so
/// `snake_case_name` stays one plain word.
fn word_boundary_open(text: &str, at: usize) -> bool {
    text[..at]
        .chars()
        .next_back()
        .is_none_or(|c| !c.is_alphanumeric())
}

/// May an underscore delimiter close here?
fn word_boundary_close(text: &str, after: usize) -> bool {
    text[after..]
        .chars()
        .next()
        .is_none_or(|c| !c.is_alphanumeric())
}

/// The start of the word containing `at`, not going back past `floor`.
///
/// An autolink must begin a word, so `see:https://x` does not become a link
/// halfway through a token.
fn word_start(text: &str, at: usize, floor: usize) -> usize {
    let mut start = at;
    while start > floor {
        let prev = text[floor..start]
            .chars()
            .next_back()
            .map_or(0, char::len_utf8);
        let candidate = start - prev;
        let c = text[candidate..].chars().next().unwrap_or(' ');
        if c.is_alphanumeric() {
            start = candidate;
        } else {
            break;
        }
    }
    start
}

/// Byte length of the first character, or 1 for an empty string so a scan can
/// never fail to advance.
fn char_len(rest: &str) -> usize {
    rest.chars().next().map_or(1, char::len_utf8)
}

/// Pushes a span, dropping empty ranges so the output never carries noise.
fn emit(out: &mut Inline, range: Range<usize>, style: &Style) {
    if range.is_empty() {
        return;
    }
    out.spans.push(Span {
        range,
        style: style.clone(),
    });
}
