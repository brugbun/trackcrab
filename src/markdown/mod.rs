//! Markdown parsing, with no rendering and no egui.
//!
//! Deliberately headless and first, for the same reason M1 and N1 were: the
//! parse is the part that would be expensive to get wrong, and it is far easier
//! to be exhaustive about it without a window in the way. Everything here is a
//! pure function from `&str` to byte ranges, so the whole surface is unit
//! testable and the renderer that follows only has to turn ranges into
//! formatting.
//!
//! # Which markdown
//!
//! Discord's dialect, not `CommonMark`, chosen deliberately:
//!
//! | Syntax | Means |
//! |---|---|
//! | `**text**` | bold |
//! | `*text*`, `_text_` | italic |
//! | `__text__` | **underline**, not bold |
//! | `~~text~~` | strikethrough |
//! | `` `text` `` | inline code |
//! | ```` ```lang ```` | fenced code block |
//! | `# ` to `#### ` | headings 1 to 4 |
//! | `- `, `1. ` | lists |
//! | `- [ ] `, `- [x] ` | task checkboxes |
//! | `---` | divider |
//! | `[text](url)`, bare `https://` | links |
//! | `==text==`, `==yellow\|text==`, `==#f2c14e\|text==` | highlight, ours alone |
//!
//! `__text__` meaning underline is the one real divergence from `CommonMark`,
//! where it means bold. Discord's reading is what people expect from a chat
//! style editor, and it is the only way to reach underline at all without
//! inventing a syntax.
//!
//! # Rules that are decisions, not accidents
//!
//! - **Inline markup never crosses a line.** `**` opened on one line and closed
//!   on the next is two literal pairs of asterisks. Matches Discord, and it
//!   keeps a long note's formatting from collapsing because of one stray
//!   delimiter halfway up the document.
//! - **`_` respects word boundaries, `*` does not.** `snake_case_name` is not
//!   italic. Without this, ordinary identifiers in a note about code turn
//!   italic, which is by far the most common complaint about markdown editors.
//! - **Unmatched delimiters are literal.** `**bold` with no close renders as
//!   typed, and nothing is hidden. Half typed markup must not make text vanish.
//! - **Nothing applies inside inline code.** `` `**not bold**` `` is four
//!   asterisks, as it should be.
//! - **Dividers are dashes only.** `CommonMark` also allows `***` and `___`,
//!   which collide with the bold and underline delimiters. Accepting them would
//!   make `***` ambiguous, so `---` is the only divider.

pub mod edit;
pub mod inline;
pub mod line;

pub use edit::Edit;
pub use inline::{HighlightColor, Inline, Palette, Piece, Span, Style, spans};
pub use line::{Line, LineKind};

/// Spaces per indent level.
///
/// One level, not four: the notes panel is narrow, and four spaces of indent
/// per level eats it. A tab in pasted text counts as one level, but Tab typed
/// in the editor inserts spaces, so a file never ends up with both.
pub const INDENT: usize = 2;

/// Deepest list nesting the editor will produce.
///
/// A document-structure rule, not a visual one, so it lives here and the
/// theme's indent clamp reads it. Pasted text can carry absurd indentation, and
/// Tab should not be able to add to it forever.
pub const MAX_DEPTH: usize = 8;

/// A parsed document: every line classified, with each line's inline spans.
///
/// The two vectors are parallel, so `inline[i]` belongs to `lines[i]`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Document {
    pub lines: Vec<Line>,
    pub inline: Vec<Inline>,
}

impl Document {
    /// Lines paired with their inline spans.
    pub fn rows(&self) -> impl Iterator<Item = (&Line, &Inline)> {
        self.lines.iter().zip(&self.inline)
    }

    /// The target of the link covering `byte`, as a byte range into the source.
    ///
    /// One question for both link forms, because the parser already folded them
    /// into one: a bare `https://...` span points at itself, an explicit
    /// `[label](url)` span points inside its brackets. A caller asking "what
    /// would clicking here open" never has to know which it got.
    ///
    /// The *label* is what answers, not the delimiters. That falls out of the
    /// parse, where the brackets and the address are markup, and it is the
    /// behaviour you want: the address is collapsed to nothing on screen, so a
    /// pointer can never really be over it.
    #[must_use]
    pub fn link_at(&self, byte: usize) -> Option<std::ops::Range<usize>> {
        self.inline
            .iter()
            .flat_map(|inline| &inline.spans)
            .find(|span| span.range.contains(&byte))
            .and_then(|span| span.style.link.clone())
    }

    /// The line containing `byte`, if any.
    #[must_use]
    pub fn line_at(&self, byte: usize) -> Option<&Line> {
        self.lines
            .iter()
            .find(|line| line.range.start <= byte && byte <= line.range.end)
    }
}

/// Parses a whole document.
#[must_use]
pub fn parse(text: &str) -> Document {
    let lines = line::lines(text);
    let inline = lines
        .iter()
        .map(|line| {
            if line.kind.is_verbatim() {
                // Inside a fence the text is the text. Parsing it would turn a
                // Rust `*ptr` into an unclosed italic.
                Inline::verbatim(line.content.clone())
            } else {
                inline::spans(text, line.content.clone())
            }
        })
        .collect();
    Document { lines, inline }
}

/// The document with every marker and delimiter stripped out.
///
/// What the sidebar search matches against, so looking for `*` does not hit
/// every emphasised word in the vault, and looking for a phrase still finds it
/// when half of it happens to be bold.
#[must_use]
pub fn plain(text: &str) -> String {
    let doc = parse(text);
    let mut out = String::with_capacity(text.len());
    for (index, (line, inline)) in doc.rows().enumerate() {
        if index > 0 {
            out.push('\n');
        }
        if matches!(line.kind, LineKind::Divider | LineKind::FenceOpen { .. })
            || line.kind == LineKind::FenceClose
        {
            // Pure markup lines carry no words at all.
            continue;
        }
        for span in &inline.spans {
            out.push_str(&text[span.range.clone()]);
        }
    }
    out
}

/// Is the whole of `text` a URL?
///
/// Deliberately laxer than the autolinker. That has to decide where an address
/// *ends inside a sentence*, so it hands trailing punctuation back to the
/// prose; a pasted address has no sentence around it, so
/// `.../Foo_(bar)` keeps its bracket, which is part of the address and not a
/// typo. Two questions, two answers, rather than one rule bent to serve both.
#[must_use]
pub fn is_url(text: &str) -> bool {
    let text = text.trim();
    let Some(rest) = ["https://", "http://"]
        .into_iter()
        .find_map(|scheme| text.strip_prefix(scheme))
    else {
        return false;
    };
    // Something after the scheme, and no whitespace anywhere: a clipboard
    // holding two URLs, or a sentence with one in it, is not a URL.
    !rest.is_empty() && !text.chars().any(char::is_whitespace)
}
