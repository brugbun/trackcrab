//! Markdown to `LayoutJob`: the rendering half of the editor.
//!
//! [`layout`] is a **pure function** from a string to a laid-out job, with no
//! `Ui` and no context. That is deliberate: it is the only way to assert what
//! bold actually renders as, since egui's accessibility tree exposes a
//! `TextEdit` as its raw string and says nothing at all about formatting. The
//! egui-facing wrapper is [`layouter`], which memoises this and hands the
//! result to a `TextEdit`.
//!
//! # The invariant that matters
//!
//! `job.text` must equal the source string **byte for byte**. A `TextEdit` maps
//! caret positions through the galley, so a job that dropped a character, or
//! reordered one, would put the caret in the wrong place and corrupt edits.
//! That is why markup is *styled down* rather than removed, and why D3 will
//! collapse delimiters by shrinking them rather than by leaving them out.

use eframe::egui::{self, Color32, FontId, Stroke, TextFormat, text::LayoutJob};

use crate::markdown::{self, LineKind, Piece, Style};

use super::theme::{self, color, metric};

/// Which lines are showing their markup.
///
/// Delimiters hide once the caret leaves their line, the way Discord and
/// Obsidian do it. Hiding them everywhere would make the document uneditable;
/// showing them everywhere is what D2 did and it reads as source code.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Reveal {
    /// Nothing is being edited, so every collapsible marker is hidden.
    #[default]
    Nothing,
    /// The caret, or a selection, covers this byte range. Every line it touches
    /// shows its markup.
    At(std::ops::Range<usize>),
}

impl Reveal {
    /// Does this line show its markup?
    #[must_use]
    pub fn covers(&self, line: &markdown::Line) -> bool {
        match self {
            Self::Nothing => false,
            // Inclusive at both ends, and asymmetric by construction: a caret
            // sitting at the end of one line is *not* at the start of the next,
            // because the newline sits between them. So exactly one line reveals.
            Self::At(range) => line.range.start <= range.end && range.start <= line.range.end,
        }
    }
}

/// Font size a width-collapsed marker is drawn at.
///
/// Not zero: `0.0` trips a debug assertion in epaint's glyph cache. At 0.01 two
/// asterisks measure 0.03px against 35.6px of text and the row height does not
/// move, so the markup is gone for every practical purpose while the characters
/// stay in the job, which is what keeps the caret honest.
const COLLAPSED: f32 = 0.01;

/// How a marker is hidden.
///
/// Two modes, because the row height is shared but the width is not. A marker
/// sharing its row with text has to lose its **width**, or hiding it would
/// leave a gap. A marker that *is* the whole row has to keep its width and lose
/// only its **ink**: shrink it and the row collapses to nothing, leaving a
/// divider with no height to draw in, a code fence with nowhere to put its
/// language tag, and a line the caret cannot be clicked onto.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Collapse {
    /// Visible, because the caret is on this line.
    No,
    /// Shrunk to nothing. For markers that share a row with text.
    Width,
    /// Made transparent at full size. For rows that are entirely markup.
    Ink,
}

/// Everything the layout needs from the theme, resolved up front.
///
/// Passed in rather than read from a `Style` so the function stays pure and a
/// test can lay text out with known values.
#[derive(Clone, Copy, Debug)]
pub struct Config {
    /// Body text size. Headings are multiples of this.
    pub size: f32,
    /// Monospace size, which is set separately: at equal point sizes a
    /// monospace face reads noticeably larger than a proportional one.
    pub mono_size: f32,
    pub text: Color32,
    /// Delimiters, while they are showing.
    pub markup: Color32,
    pub code_bg: Color32,
    pub code_text: Color32,
    pub link: Color32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            size: 16.5,
            mono_size: 15.0,
            text: color::TEXT,
            markup: color::MARKUP,
            code_bg: color::CODE_BG,
            code_text: color::CODE_TEXT,
            link: color::LINK,
        }
    }
}

impl Config {
    /// Reads the sizes from a live style, keeping the field in step with the
    /// rest of the interface and with the zoom factor.
    #[must_use]
    pub fn from_style(style: &egui::Style) -> Self {
        let size = |text_style: &egui::TextStyle| {
            style
                .text_styles
                .get(text_style)
                .map_or(16.5, |font| font.size)
        };
        Self {
            size: size(&egui::TextStyle::Body),
            mono_size: size(&egui::TextStyle::Monospace),
            ..Self::default()
        }
    }
}

/// Lays out a markdown document.
#[must_use]
pub fn layout(source: &str, wrap_width: f32, cfg: &Config, reveal: &Reveal) -> LayoutJob {
    let mut job = LayoutJob {
        wrap: egui::text::TextWrapping {
            max_width: wrap_width,
            ..Default::default()
        },
        break_on_newline: true,
        ..Default::default()
    };

    let doc = markdown::parse(source);
    // Tracks the end of the previous line, so the newline bytes between lines
    // are appended too. Doing it this way rather than pushing "\n" handles CRLF
    // without a special case, and guarantees `job.text == source`.
    let mut at = 0;

    for (line, inline) in doc.rows() {
        if line.range.start > at {
            job.append(&source[at..line.range.start], 0.0, plain(cfg));
        }

        let heading = match line.kind {
            LineKind::Heading(level) => Some(level),
            _ => None,
        };
        let shown = reveal.covers(line);

        // Two independent questions. A *block* marker's mode depends on what
        // replaces it; *inline* delimiters always sit beside text, so they
        // always collapse by width. Deriving the second from the first stopped
        // a plain paragraph hiding its own delimiters, since a paragraph has no
        // block marker to take a mode from.
        let block = if shown {
            Collapse::No
        } else {
            marker_collapse(&line.kind)
        };
        let inline_markup = if shown { Collapse::No } else { Collapse::Width };
        if !line.marker.is_empty() {
            job.append(
                &source[line.marker.clone()],
                0.0,
                marker_format(cfg, line, block),
            );
        }

        // Space for the drawn marker, applied to the first piece of content.
        // `leading_space` is a section property, and epaint only honours it at
        // the start of a paragraph, so a list item that wraps loses the indent
        // on its continuation rows. There is a TODO in epaint's layout code
        // saying as much; nothing here can fix it.
        // `indent_for` already answers zero for anything that is not a list, so
        // there is nothing to branch on here.
        let mut indent = indent_for(line, shown);
        for piece in inline.pieces() {
            let format = match &piece {
                Piece::Markup(_) => marker_format(cfg, line, inline_markup),
                Piece::Text(span) => text_format(cfg, &span.style, heading, line.is_code()),
            };
            job.append(&source[piece.range()], std::mem::take(&mut indent), format);
        }
        if indent > 0.0 {
            // An empty list item still needs its indent, or the bullet would be
            // drawn against text that starts at the margin.
            job.append("", indent, plain(cfg));
        }
        at = line.range.end;
    }
    if at < source.len() {
        job.append(&source[at..], 0.0, plain(cfg));
    }

    debug_assert_eq!(
        job.text, source,
        "the job text must match the source byte for byte or the caret desyncs"
    );
    job
}

/// The unstyled format, used for newlines and anything unclassified.
fn plain(cfg: &Config) -> TextFormat {
    TextFormat {
        font_id: theme::body_font(cfg.size),
        color: cfg.text,
        ..Default::default()
    }
}

/// How a block marker or an inline delimiter is drawn.
///
/// Kept at the size of the text it belongs to, not at body size: a `## ` in
/// front of a heading has to occupy the same line height as the heading, or the
/// row jumps as the caret moves in and out of it.
fn marker_format(cfg: &Config, line: &markdown::Line, collapse: Collapse) -> TextFormat {
    let size = match line.kind {
        LineKind::Heading(level) => cfg.size * heading_scale(level),
        _ => cfg.size,
    };
    let mut format = TextFormat {
        font_id: theme::body_font(size),
        color: cfg.markup,
        ..Default::default()
    };
    // Monospace, but *no* background: the block decoration paints one rounded
    // rectangle behind the whole run, and a per-character background on top of
    // it shows through as a second, squarer shade.
    if line.is_code() {
        format.font_id = FontId::monospace(cfg.mono_size);
    }
    match collapse {
        Collapse::No => {}
        Collapse::Width => {
            // Shrunk *and* made transparent. The size is what removes the
            // width; transparency is because a sub-pixel glyph can still leave
            // a smudge.
            format.font_id = FontId::new(COLLAPSED, format.font_id.family.clone());
            format.color = Color32::TRANSPARENT;
            format.background = Color32::TRANSPARENT;
        }
        Collapse::Ink => {
            format.color = Color32::TRANSPARENT;
        }
    }
    format
}

/// How this kind of block marker is hidden, once it may be.
///
/// Every block marker now has something drawn in its place, so all of them
/// hide. Which *mode* depends on whether the marker shares its row with text: a
/// heading's hashes and a list's bullet do, so they lose their width and the
/// content is indented past a drawn marker instead. A divider and a code fence
/// are the whole row, so they keep their height and lose only their ink.
const fn marker_collapse(kind: &LineKind) -> Collapse {
    match kind {
        LineKind::Heading(_) | LineKind::Bullet | LineKind::Numbered(_) | LineKind::Task(_) => {
            Collapse::Width
        }
        LineKind::Divider | LineKind::FenceOpen { .. } | LineKind::FenceClose => Collapse::Ink,
        LineKind::Blank | LineKind::Paragraph | LineKind::Code => Collapse::No,
    }
}

/// Horizontal space reserved before a line's content.
///
/// Two cases, and the difference matters more than it looks. While the marker
/// is hidden the space covers both the nesting depth and the gutter the drawn
/// marker sits in. While it is **revealed** the raw `- ` is occupying roughly
/// that gutter itself, so only the depth is reserved.
///
/// Dropping the whole indent on reveal, which is what the first attempt did,
/// made a nested item jump about 30px left as the caret arrived and land at its
/// parent's indent, so the nesting appeared to collapse. Keeping the depth
/// leaves only the difference between the gutter and the marker's own width,
/// which is a few pixels, and the nesting stays readable.
fn indent_for(line: &markdown::Line, revealed: bool) -> f32 {
    if !line.kind.is_list() {
        return 0.0;
    }
    let depth = theme::list_indent(line.depth);
    if revealed {
        depth
    } else {
        depth + metric::GUTTER
    }
}

/// How one run of text is drawn.
fn text_format(
    cfg: &Config,
    style: &Style,
    heading: Option<u8>,
    in_code_block: bool,
) -> TextFormat {
    let size = heading.map_or(cfg.size, |level| cfg.size * heading_scale(level));
    // A heading is bold as well as bigger. Size alone reads as a zoomed
    // paragraph rather than a heading.
    let bold = style.bold || heading.is_some();

    let mut format = TextFormat {
        font_id: if bold {
            theme::bold_font(size)
        } else {
            theme::body_font(size)
        },
        color: cfg.text,
        italics: style.italic,
        ..Default::default()
    };

    if style.code {
        // Code wins the family: there is no bold monospace face bundled, and a
        // proportional font would defeat the point of marking it as code.
        format.font_id = FontId::monospace(cfg.mono_size);
        format.color = cfg.code_text;
        // An *inline* span paints its own background, since there is no block
        // decoration behind it. A line inside a fence leaves that to the block.
        if !in_code_block {
            format.background = cfg.code_bg;
        }
    }
    if let Some(colour) = style.highlight {
        let background = theme::mark_color(colour);
        format.background = background;
        format.expand_bg = metric::MARK_EXPAND;
        // Chosen from the background's luminance, so a hex nobody anticipated
        // still leaves its own text readable.
        format.color = theme::readable_on(background);
    }
    if style.link.is_some() {
        format.color = cfg.link;
        format.underline = Stroke::new(1.0, cfg.link);
    }
    if style.underline {
        format.underline = Stroke::new(1.0, format.color);
    }
    if style.strike {
        // Struck text is being discarded, so it steps back rather than staying
        // at full strength with a line through it.
        format.color = format.color.gamma_multiply(0.75);
        format.strikethrough = Stroke::new(1.0, format.color);
    }
    format
}

/// Size multiplier for a heading level, clamped so an out-of-range level cannot
/// index past the scale.
fn heading_scale(level: u8) -> f32 {
    let index = (level.max(1) as usize - 1).min(metric::HEADING_SCALE.len() - 1);
    metric::HEADING_SCALE[index]
}

/// Memoised layout, for handing to `TextEdit::layouter`.
///
/// The layouter runs every frame, and re-parsing a long note sixty times a
/// second is waste. This caches the *job* rather than the galley, because
/// epaint already caches galleys internally, so the job is the only part worth
/// keeping.
///
/// Where the caret is, as a byte range, or `Nothing` if this field is not the
/// one being edited.
///
/// Read from the *stored* `TextEditState`, which is last frame's caret: the
/// layouter runs during `show`, before the widget has processed this frame's
/// input, so there is nothing fresher to read. [`edit`] closes the one frame
/// gap by asking for a repaint when the caret has moved.
fn reveal_for(ctx: &egui::Context, id: egui::Id, source: &str) -> Reveal {
    if ctx.memory(egui::Memory::focused) != Some(id) {
        return Reveal::Nothing;
    }
    let Some(state) = egui::widgets::text_edit::TextEditState::load(ctx, id) else {
        return Reveal::Nothing;
    };
    let Some(range) = state.cursor.char_range() else {
        return Reveal::Nothing;
    };
    Reveal::At(bytes_of(source, range))
}

/// Byte offset of a character index, clamped to the end of the string.
fn byte_of(source: &str, chars: usize) -> usize {
    source
        .char_indices()
        .nth(chars)
        .map_or(source.len(), |(byte, _)| byte)
}

/// Frames the field keeps asking for focus back after a toolbar action.
///
/// One request is not enough, and the reason is worth writing down because the
/// symptom is baffling. egui resolves focus at the *start* of a frame from the
/// previous frame's pointer events, so a `request_focus` issued on the click
/// frame is overridden a frame later. A menu is worse: the click that picks an
/// item lands outside the field, the popup is closing, and the widget that was
/// clicked no longer exists on the next frame, so focus settles on nothing.
///
/// So the request repeats until it sticks, and asks for the frames it needs
/// rather than assuming any will arrive. Both halves matter. Without the retry
/// the caret is left wherever it was and the next thing typed goes in at the
/// click, which is how a highlight came out as `==yellow|==some` instead of
/// `==yellow|some==`. Without the repaint the retry never runs at all, because
/// egui is idle driven and nothing else was asking for a frame.
const REFOCUS_FRAMES: u8 = 10;

/// One markdown text field: layout, reveal and the repaint that keeps them in
/// step.
///
/// All four fields go through here rather than building a `TextEdit` each, so
/// the reveal behaviour cannot drift between them and the id, which the reveal
/// depends on, cannot be forgotten.
pub fn edit(ui: &mut egui::Ui, field: &Field<'_>, text: &mut String) -> egui::Response {
    let id = field.id;
    // Read before anything else, because a click on a decoration has to undo
    // the caret move egui already made for it.
    let on_entry = Pointer {
        focused: ui.ctx().data(|data| data.get_temp::<bool>(id.with("edited"))) == Some(true),
        caret: caret_bytes(ui.ctx(), id, text),
    };

    // Asking for focus back after a toolbar action, until it actually sticks.
    // See [`REFOCUS_FRAMES`] for why once is not enough.
    let refocus = id.with("refocus");
    if let Some(left) = ui.ctx().data(|data| data.get_temp::<u8>(refocus)) {
        if left == 0 || ui.ctx().memory(egui::Memory::focused) == Some(id) {
            ui.ctx().data_mut(|data| data.remove::<u8>(refocus));
        } else {
            ui.ctx().memory_mut(|memory| memory.request_focus(id));
            ui.ctx().data_mut(|data| data.insert_temp(refocus, left - 1));
            // Asked for, not assumed: egui only draws when something wants it
            // to, so without this the retry would simply never run.
            ui.ctx().request_repaint();
        }
    }

    // Drawn first, so its action is applied to the same text the field is about
    // to render. Clicking a button moves focus off the field, so the caret is
    // read from storage either way.
    let clicked = if field.toolbar {
        super::toolbar::show(ui, id.with("toolbar"))
    } else {
        None
    };

    // Before anything else, because a key consumed here never reaches the
    // widget, and the widget would otherwise also act on it: Tab would move
    // focus, Enter would insert a bare newline, Backspace would eat one
    // invisible character of collapsed markup.
    let mut edited = apply_keys(ui.ctx(), id, text);
    if let Some(action) = clicked {
        edited |= apply_action(ui.ctx(), id, text, &action);
    }

    // What the layouter is about to see. Captured after any edit, since the
    // edit moved the caret, and before `show`, since `show` moves it too.
    let before = reveal_for(ui.ctx(), id, text);

    let output = if let Some(size) = field.size {
        // A sized field books its space first, so the panel it lives in can put
        // a footer under it.
        let mut child = ui.new_child(
            egui::UiBuilder::new().max_rect(egui::Rect::from_min_size(ui.cursor().min, size)),
        );
        let output = if field.scroll {
            // Enough rows to fill the box, so a short note is still one big
            // click target across the whole panel, and the scroll area takes
            // over from there rather than the text running off the bottom.
            let filling = size.y / child.text_style_height(&egui::TextStyle::Body);
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "a row count, floored and clamped above zero"
            )]
            let filling = filling.floor().max(1.0) as usize;
            egui::ScrollArea::vertical()
                .id_salt(id.with("scroll"))
                // Or the area shrinks to the text and a short note stops
                // filling the panel, which is the thing `filling` is for.
                .auto_shrink([false, false])
                .show(&mut child, |ui| render(ui, field, text, &before, filling))
                .inner
        } else {
            render(&mut child, field, text, &before, field.rows.unwrap_or(1))
        };
        ui.advance_cursor_after_rect(child.min_rect());
        output
    } else {
        render(ui, field, text, &before, field.rows.unwrap_or(1))
    };

    // This frame's truth, while the field still holds focus. One frame later a
    // click on the toolbar will have taken focus away and the selection with
    // it, so this is the last chance to write it down.
    if ui.ctx().memory(egui::Memory::focused) == Some(id)
        && let Some(cursor) = output.cursor_range
    {
        remember(ui.ctx(), id, &bytes_of(text, cursor));
    }

    // The caret may have moved during `show`, in which case the markup on
    // screen belongs to where it *was*. One more frame fixes it, and asking for
    // it only on a change means a focused field is not repainting for nothing.
    let after = reveal_from(id, text, output.cursor_range, ui.ctx());
    if after != before {
        ui.ctx().request_repaint();
    }

    // After `show`, because the galley a pointer is resolved against does not
    // exist until then. A change here therefore lands one frame late, which is
    // the same deal the reveal already lives with.
    let mut response = output.response.response.clone();
    if interact(ui, &output, text, &response, &on_entry) {
        edited = true;
        ui.ctx().request_repaint();
    }

    // For the next frame's `before`, since focus cannot be read back after the
    // fact.
    let focused = ui.ctx().memory(egui::Memory::focused) == Some(id);
    ui.ctx()
        .data_mut(|data| data.insert_temp(id.with("edited"), focused));
    if edited {
        // The buffer was changed behind the widget's back, so it has no idea
        // anything happened and the caller would never save.
        response.mark_changed();
    }
    response
}

/// Lays the field out and paints its block decorations, both against `ui`.
///
/// One function rather than inline, so the scrolled and unscrolled paths cannot
/// drift on the thing that matters here: a shape is reserved with the clip
/// rectangle of the `Ui` it was reserved on. Booking the backdrop on the outer
/// `Ui` and then filling it in would let a code block scrolled half out of view
/// paint its background over the toolbar above it.
///
/// The widget is built here rather than passed in because the buffer cannot be
/// borrowed twice: `show` has to consume the `TextEdit`, and give up its
/// mutable borrow, before the decorations can read the same text.
fn render(
    ui: &mut egui::Ui,
    field: &Field<'_>,
    text: &mut String,
    reveal: &Reveal,
    rows: usize,
) -> egui::widgets::text_edit::TextEditOutput {
    let id = field.id;
    let mut layouter = |ui: &egui::Ui, buffer: &dyn egui::TextBuffer, wrap: f32| {
        galley(
            ui,
            buffer.as_str(),
            wrap,
            &reveal_for(ui.ctx(), id, buffer.as_str()),
        )
    };

    let mut widget = egui::TextEdit::multiline(text)
        .id(id)
        .hint_text(field.hint)
        // Tells egui's focus system that this widget wants Tab. Consuming the
        // key in `apply_keys` is not enough on its own: focus is resolved at
        // the *start* of the frame, from the raw events, long before any widget
        // code runs, and it is gated on the focused widget's event filter.
        .lock_focus(true)
        .desired_rows(rows)
        // Infinite, not the box width: a multiline field shrinks its wrap width
        // to what is available, so this asks for "as wide as you have" and gets
        // correct wrapping inside a scroll area too.
        .desired_width(f32::INFINITY)
        .layouter(&mut layouter);
    if !field.frame {
        widget = widget.frame(egui::Frame::NONE);
    }
    // Reserved *before* the text is painted, so a code block's background can
    // be filled in behind it afterwards. The rect is not known until the galley
    // exists, which is only after `show`, so the slot has to be booked first.
    let backdrop = ui.painter().add(egui::Shape::Noop);

    // `show` rather than `ui.add`, because only `show` hands back the cursor
    // and the galley: the cursor is what the reveal is computed from, and the
    // galley is what the block decorations are positioned against.
    let output = widget.show(ui);

    // A revealed line shows its own markup, so the drawn substitute steps
    // aside rather than sitting on top of it.
    let revealed = |line: &crate::markdown::Line| reveal.covers(line);
    super::blocks::paint(ui, &output, text, &revealed, backdrop);
    output
}

/// Everything a pointer can do to a laid-out field, returning whether the text
/// changed.
///
/// Both jobs here are the same shape: something drawn or styled that the text
/// field itself knows nothing about, and a pointer that has to be resolved
/// against the galley to find it. Keeping them together keeps the one hit test
/// in one place.
fn interact(
    ui: &egui::Ui,
    output: &egui::widgets::text_edit::TextEditOutput,
    text: &mut String,
    response: &egui::Response,
    before: &Pointer,
) -> bool {
    let Some(pos) = ui.ctx().pointer_interact_pos() else {
        return false;
    };

    // Checkboxes first, and only on a plain click: the box is a target you aim
    // at, so a modifier held for something else must not tick it.
    let plain = ui.input(|i| i.modifiers.is_none());
    if response.clicked()
        && plain
        && let Some(at) = super::blocks::hit(output, text, pos)
        && let Some(edit) = markdown::edit::toggle_task(text, &(at..at))
    {
        *text = edit.text;
        // The gutter is not text, so a click on it must not behave like one.
        // Left alone, the caret the field just placed would reveal the line's
        // own source, so ticking a box would replace the box with `- [x] ` and
        // a caret: the wrong feedback entirely for the one action whose whole
        // point is the tick appearing.
        restore(ui.ctx(), response.id, text, before);
        return true;
    }

    if !response.hovered() && !response.clicked() {
        return false;
    }
    let Some(byte) = byte_under(&output.galley, output.galley_pos, pos) else {
        return false;
    };
    let Some(target) = markdown::parse(text).link_at(byte) else {
        return false;
    };
    let url = &text[target];

    // The address, whenever you are over it. Worth showing unconditionally,
    // because a `[label](url)` hides its address completely and this is the only
    // way to see where a link actually goes without putting the caret on it.
    egui::Tooltip::always_open(
        ui.ctx().clone(),
        ui.layer_id(),
        response.id.with("link"),
        egui::PopupAnchor::Pointer,
    )
    .at_pointer()
    .show(|ui| {
        ui.label(url);
    });

    // The hand only while the modifier is down, because that is the only time
    // clicking does anything but move the caret. A hand offered over an
    // editable field would be promising something a plain click will not do.
    let modified = ui.input(|i| i.modifiers.command || i.modifiers.ctrl);
    if modified {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if modified && response.clicked() {
        // `egui-winit` turns this into the platform's own "open a URL" call, so
        // it lands in whatever the user has set as their browser rather than
        // one we picked.
        ui.ctx().open_url(egui::OpenUrl::new_tab(url));
    }
    false
}

/// What the field looked like before egui got hold of this frame's click.
///
/// Both fields have to be captured by us rather than read back later. Focus is
/// resolved in `Memory::begin_pass`, from the raw events, before any widget
/// code runs, so by the time this function is reached a click has *already*
/// focused the field and there is no way to ask whether it was focused a
/// moment ago.
struct Pointer {
    focused: bool,
    caret: Option<std::ops::Range<usize>>,
}

/// Undoes the caret move a click on a decoration caused.
///
/// A field that was not being edited is handed back unfocused: ticking a box in
/// a note you were only reading should not drop you into editing it.
fn restore(ctx: &egui::Context, id: egui::Id, text: &str, before: &Pointer) {
    if before.focused {
        if let Some(caret) = &before.caret {
            set_caret(ctx, id, text, caret);
        }
    } else {
        ctx.memory_mut(|memory| memory.surrender_focus(id));
    }
    // One frame still renders with the caret where the click put it. Asking for
    // another is what makes that a frame rather than a state.
    ctx.request_repaint();
}

/// The source byte under `pos`, or `None` when the pointer is not over a glyph.
///
/// Walks the galley's rows and glyphs rather than going through
/// `Galley::cursor_from_pos`, which snaps to the *nearest* caret position: on a
/// short line that answers with the last character however far right the
/// pointer is, so a link ending a line would be live across the whole margin
/// beside it. A link is a containment question, not a nearest-neighbour one.
///
/// Collapsed markup falls out of this for free. A delimiter styled down to
/// [`COLLAPSED`] is 0.01px wide here too, so it is effectively unhittable,
/// which is why hovering where a hidden `](` sits reports nothing.
fn byte_under(galley: &egui::Galley, origin: egui::Pos2, pos: egui::Pos2) -> Option<usize> {
    let local = (pos - origin).to_pos2();
    let mut chars = 0usize;
    for row in &galley.rows {
        // Rows are in order down the galley, so the y check is a cheap way past
        // every row the pointer is nowhere near.
        if row.rect().contains(local) {
            let found = row.glyphs.iter().position(|glyph| {
                glyph
                    .logical_rect()
                    .translate(row.pos.to_vec2())
                    .contains(local)
            });
            if let Some(index) = found {
                return Some(byte_of(&galley.job.text, chars + index));
            }
        }
        // The newline is a character in the source but never a glyph in a row.
        chars += row.glyphs.len() + usize::from(row.ends_with_newline);
    }
    None
}

/// Handles the markdown-aware keys, returning whether the text changed.
///
/// Each key is *peeked* at first and only consumed if the operation actually
/// applies. That way declining hands the key straight back to egui, which is
/// the whole point of the `Option` the edit functions return: everything that
/// is not specifically a list behaves exactly as it did before.
fn apply_keys(ctx: &egui::Context, id: egui::Id, text: &mut String) -> bool {
    use crate::markdown::edit;

    if ctx.memory(egui::Memory::focused) != Some(id) {
        return false;
    }
    let Some(caret) = caret_bytes(ctx, id, text) else {
        return false;
    };

    // A URL pasted over a selection wraps it as a link. Checked before the
    // widget sees the event, because egui would otherwise have replaced the
    // selection with the address and there would be no label left to hang it
    // on. The event is only taken when the operation actually applies, so an
    // ordinary paste is untouched.
    if let Some(pasted) = ctx.input(|i| {
        i.events.iter().find_map(|event| match event {
            egui::Event::Paste(what) => Some(what.clone()),
            _ => None,
        })
    }) && let Some(result) = edit::paste_link(text, &caret, &pasted)
    {
        ctx.input_mut(|i| i.events.retain(|e| !matches!(e, egui::Event::Paste(_))));
        *text = result.text;
        set_caret(ctx, id, text, &result.caret);
        return true;
    }

    // Ticking a box from the keyboard. The box is drawn rather than laid out,
    // so it cannot be reached by moving the caret onto it, and a checkbox that
    // only a mouse can tick is a checkbox some people cannot tick at all.
    let toggle = egui::KeyboardShortcut::new(egui::Modifiers::CTRL, egui::Key::Enter);
    if ctx.input_mut(|i| i.consume_shortcut(&toggle))
        && let Some(result) = edit::toggle_task(text, &caret)
    {
        *text = result.text;
        set_caret(ctx, id, text, &result.caret);
        return true;
    }

    // Shift+Tab before Tab: the plainer chord would otherwise swallow it, the
    // same trap Ctrl+Shift+N fell into in M5.
    //
    // `swallow` says what happens when the operation declines. The two Tabs are
    // *always* ours, because `lock_focus` has told egui the widget wants Tab,
    // so a key handed back would be inserted as a literal tab character and the
    // file would end up indented two ways at once. Enter and Backspace are
    // handed back, which is the whole point of the `Option`: outside a list they
    // behave exactly as they did before.
    let candidates: [(egui::KeyboardShortcut, edit::Operation, bool); 4] = [
        (
            egui::KeyboardShortcut::new(egui::Modifiers::SHIFT, egui::Key::Tab),
            edit::untab,
            true,
        ),
        (
            egui::KeyboardShortcut::new(egui::Modifiers::NONE, egui::Key::Tab),
            edit::tab,
            true,
        ),
        (
            egui::KeyboardShortcut::new(egui::Modifiers::NONE, egui::Key::Enter),
            edit::enter,
            false,
        ),
        (
            egui::KeyboardShortcut::new(egui::Modifiers::NONE, egui::Key::Backspace),
            edit::backspace,
            false,
        ),
    ];

    // The emphasis chords, which route through the same code the toolbar does.
    //
    // `CTRL` rather than `COMMAND`, matching every other shortcut in the app.
    // `COMMAND` would also pick up Mac's Cmd, but it only matches a modifier
    // set that has `command` flagged, which the platform layer does and a
    // synthesised event does not: the chords worked in the app and silently did
    // nothing under test.
    for (shortcut, what) in [
        (
            egui::KeyboardShortcut::new(egui::Modifiers::CTRL, egui::Key::B),
            edit::Wrap::Bold,
        ),
        (
            egui::KeyboardShortcut::new(egui::Modifiers::CTRL, egui::Key::I),
            edit::Wrap::Italic,
        ),
        (
            egui::KeyboardShortcut::new(egui::Modifiers::CTRL, egui::Key::U),
            edit::Wrap::Underline,
        ),
    ] {
        if ctx.input_mut(|i| i.consume_shortcut(&shortcut)) {
            let result = edit::wrap(text, &caret, &what);
            *text = result.text;
            set_caret(ctx, id, text, &result.caret);
            return true;
        }
    }

    for (shortcut, operation, swallow) in candidates {
        let pressed = ctx.input(|i| {
            i.key_pressed(shortcut.logical_key) && i.modifiers.matches_exact(shortcut.modifiers)
        });
        if !pressed {
            continue;
        }
        let result = operation(text, &caret);
        if result.is_none() && !swallow {
            // Not ours. Leave the key for the widget.
            return false;
        }
        ctx.input_mut(|i| i.consume_shortcut(&shortcut));
        let Some(result) = result else {
            // Ours, but nothing to do: a Tab that cannot indent any further.
            return false;
        };
        *text = result.text;
        set_caret(ctx, id, text, &result.caret);
        return true;
    }
    false
}

/// Applies a toolbar action, returning whether the text changed.
///
/// Shared with the emphasis shortcuts, so `Ctrl+B` and the B button cannot
/// drift apart.
fn apply_action(
    ctx: &egui::Context,
    id: egui::Id,
    text: &mut String,
    action: &super::toolbar::Action,
) -> bool {
    use super::toolbar::Action;
    use crate::markdown::edit;

    // The remembered caret, not the widget's own: by the time a button has been
    // clicked the field is not focused, so its live cursor is not to be trusted
    // (see [`remember`]). Without one there is nothing to act on, and the end
    // of the text is the least surprising place.
    let caret = remembered(ctx, id)
        .filter(|caret| caret.end <= text.len())
        .unwrap_or(text.len()..text.len());
    let result = match action {
        Action::Wrap(what) => Some(edit::wrap(text, &caret, what)),
        Action::Block(what) => edit::block(text, &caret, *what),
        Action::Divider => edit::divider(text, &caret),
        Action::CodeBlock => edit::code_block(text, &caret),
        Action::Link => Some(edit::link(text, &caret)),
    };
    let Some(result) = result else {
        return false;
    };
    *text = result.text;
    set_caret(ctx, id, text, &result.caret);
    // Back to the field, so you can carry on typing where the button left the
    // caret rather than having to click into it again, and asked for again on
    // the frames after this one until it holds. See [`REFOCUS_FRAMES`].
    ctx.memory_mut(|memory| memory.request_focus(id));
    ctx.data_mut(|data| data.insert_temp(id.with("refocus"), REFOCUS_FRAMES));
    ctx.request_repaint();
    true
}

/// Where the caret was the last time the field was actually being edited.
///
/// A toolbar action cannot trust the widget's own cursor. Clicking a button
/// takes focus off the field, and a *menu* keeps it off for as long as it stays
/// open, during which egui collapses the stored selection to a bare caret. So
/// selecting a phrase and reaching for the highlight menu lost the selection on
/// the way, and the empty markup landed beside the words rather than around
/// them.
///
/// Remembered on every frame the field holds focus, and again whenever an
/// operation moves the caret, so two actions in a row still act on the same
/// text.
fn remember(ctx: &egui::Context, id: egui::Id, caret: &std::ops::Range<usize>) {
    ctx.data_mut(|data| data.insert_temp(id.with("caret"), (caret.start, caret.end)));
}

/// The caret [`remember`] last saw.
fn remembered(ctx: &egui::Context, id: egui::Id) -> Option<std::ops::Range<usize>> {
    ctx.data(|data| data.get_temp::<(usize, usize)>(id.with("caret")))
        .map(|(start, end)| start..end)
}

/// A cursor range as byte offsets into `source`.
fn bytes_of(source: &str, cursor: egui::text::CCursorRange) -> std::ops::Range<usize> {
    let (a, b) = (cursor.primary.index.0, cursor.secondary.index.0);
    byte_of(source, a.min(b))..byte_of(source, a.max(b))
}

/// The caret as a byte range, or `None` when there is none.
fn caret_bytes(ctx: &egui::Context, id: egui::Id, source: &str) -> Option<std::ops::Range<usize>> {
    let state = egui::widgets::text_edit::TextEditState::load(ctx, id)?;
    Some(bytes_of(source, state.cursor.char_range()?))
}

/// Moves the caret, in the text as it now stands.
///
/// Stored before `show`, so the widget picks it up this frame rather than
/// rendering with the caret where it was before the edit.
fn set_caret(ctx: &egui::Context, id: egui::Id, source: &str, caret: &std::ops::Range<usize>) {
    let mut state = egui::widgets::text_edit::TextEditState::load(ctx, id).unwrap_or_default();
    // Back to characters, counted against the *new* text: an edit that changed
    // the bytes before the caret changed the character indices too.
    let chars =
        |byte: usize| egui::text::CCursor::new(egui::text::CharIndex(char_of(source, byte)));
    state
        .cursor
        .set_char_range(Some(egui::text::CCursorRange::two(
            chars(caret.start),
            chars(caret.end),
        )));
    state.store(ctx, id);
    // Kept in step, so a second action in a row acts on where the first one
    // left things rather than on where the caret was before either.
    remember(ctx, id, caret);
}

/// Character index of a byte offset, clamped to the end.
fn char_of(source: &str, byte: usize) -> usize {
    source[..byte.min(source.len())].chars().count()
}

/// The reveal implied by a cursor range that has just been computed, rather
/// than by the one in storage.
fn reveal_from(
    id: egui::Id,
    source: &str,
    cursor: Option<egui::text::CCursorRange>,
    ctx: &egui::Context,
) -> Reveal {
    if ctx.memory(egui::Memory::focused) != Some(id) {
        return Reveal::Nothing;
    }
    let Some(range) = cursor else {
        return Reveal::Nothing;
    };
    Reveal::At(bytes_of(source, range))
}

/// How one markdown field differs from the others.
pub struct Field<'a> {
    /// Fixed, because the reveal is looked up by it. Two fields sharing an id
    /// would share a caret.
    pub id: egui::Id,
    pub hint: &'a str,
    /// `None` when the height is set by [`Self::size`] instead.
    pub rows: Option<usize>,
    /// Whether to draw egui's own text-field frame.
    pub frame: bool,
    /// An exact size, for the notebook, which fills the panel it is in.
    pub size: Option<egui::Vec2>,
    /// Whether the field scrolls once its content outgrows [`Self::size`].
    ///
    /// Only meaningful with a size. Without one the field simply grows and
    /// whatever contains it decides, which is what the task view's own scroll
    /// area already does for the description and the notes.
    pub scroll: bool,
    /// Whether to draw the formatting bar above the field.
    pub toolbar: bool,
}

impl Default for Field<'_> {
    fn default() -> Self {
        Self {
            id: egui::Id::NULL,
            hint: "",
            rows: Some(4),
            frame: true,
            size: None,
            scroll: false,
            toolbar: true,
        }
    }
}

/// Memoised layout for one field.
fn galley(
    ui: &egui::Ui,
    source: &str,
    wrap_width: f32,
    reveal: &Reveal,
) -> std::sync::Arc<egui::Galley> {
    let cfg = Config::from_style(ui.style());
    let job = ui.ctx().memory_mut(|memory| {
        memory
            .caches
            .cache::<JobCache>()
            .get((
                source,
                wrap_width.to_bits(),
                cfg.size.to_bits(),
                key(reveal),
            ))
            .clone()
    });
    ui.fonts_mut(|fonts| fonts.layout_job(job))
}

/// The reveal, flattened into something hashable for the cache key.
///
/// `usize::MAX` for both ends is the "nothing revealed" sentinel, which cannot
/// collide with a real range: a document that long would not fit in memory.
const fn key(reveal: &Reveal) -> (usize, usize) {
    match reveal {
        Reveal::Nothing => (usize::MAX, usize::MAX),
        Reveal::At(range) => (range.start, range.end),
    }
}

type JobCache = egui::cache::FrameCache<LayoutJob, Layouter>;

#[derive(Default)]
struct Layouter;

// Keyed on the text, the wrap width and the body size. The width and size are
// hashed as bits because floats are not `Hash`; the zoom factor moves the size,
// which is why it is part of the key rather than assumed constant.
impl egui::cache::ComputerMut<(&str, u32, u32, (usize, usize)), LayoutJob> for Layouter {
    fn compute(
        &mut self,
        (source, wrap, size, reveal): (&str, u32, u32, (usize, usize)),
    ) -> LayoutJob {
        let cfg = Config {
            size: f32::from_bits(size),
            ..Config::default()
        };
        let reveal = if reveal == (usize::MAX, usize::MAX) {
            Reveal::Nothing
        } else {
            Reveal::At(reveal.0..reveal.1)
        };
        layout(source, f32::from_bits(wrap), &cfg, &reveal)
    }
}
