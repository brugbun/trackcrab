//! Block decorations: the things drawn *over* the text rather than laid out as
//! part of it.
//!
//! A galley is a run of laid-out text lines with no concept of blocks, so a
//! bullet, a rule and a code block's background cannot come out of the
//! layouter. They are painted afterwards, positioned from the galley's own
//! geometry.
//!
//! Split in two on purpose. [`plan`] is a **pure function** from a document to
//! a list of decorations that name a line and a shape but carry no coordinates,
//! so every interesting decision (which lines get a bullet, which run of lines
//! is one code block, what number to draw) is unit testable. [`paint`] resolves
//! those to screen rectangles and draws them, and is deliberately thin.

use std::ops::Range;

use eframe::egui::{self, Color32, Rect, Ui};

use crate::markdown::{self, LineKind};

use super::theme::{color, metric};

/// One thing to draw, identified by the line it belongs to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Decoration {
    /// A dot in the gutter.
    Bullet { line: usize, depth: usize },
    /// The number that was typed, in the gutter. Not a recount: renumbering
    /// behind the user's back would fight them as they edit.
    Number {
        line: usize,
        depth: usize,
        value: u32,
    },
    /// A checkbox in the gutter.
    Check {
        line: usize,
        depth: usize,
        checked: bool,
    },
    /// A rule across the whole row.
    Rule { line: usize },
    /// One background spanning a run of lines, fences included.
    ///
    /// A single decoration rather than one per line, so the corners round and
    /// the block reads as one object.
    Code {
        first: usize,
        last: usize,
        /// Byte range of the language tag, empty when none was given.
        lang: Range<usize>,
    },
}

impl Decoration {
    /// The first line this decoration touches, for ordering.
    #[must_use]
    pub const fn line(&self) -> usize {
        match self {
            Self::Bullet { line, .. }
            | Self::Number { line, .. }
            | Self::Check { line, .. }
            | Self::Rule { line } => *line,
            Self::Code { first, .. } => *first,
        }
    }
}

/// Works out what to draw for a document.
#[must_use]
pub fn plan(doc: &markdown::Document) -> Vec<Decoration> {
    let mut out = Vec::new();
    // The fence currently open, as the line it started on and its language.
    let mut fence: Option<(usize, Range<usize>)> = None;

    for (index, line) in doc.lines.iter().enumerate() {
        match &line.kind {
            LineKind::Bullet => out.push(Decoration::Bullet {
                line: index,
                depth: line.depth,
            }),
            LineKind::Numbered(value) => out.push(Decoration::Number {
                line: index,
                depth: line.depth,
                value: *value,
            }),
            LineKind::Task(checked) => out.push(Decoration::Check {
                line: index,
                depth: line.depth,
                checked: *checked,
            }),
            LineKind::Divider => out.push(Decoration::Rule { line: index }),
            LineKind::FenceOpen { lang } => fence = Some((index, lang.clone())),
            LineKind::FenceClose => {
                if let Some((first, lang)) = fence.take() {
                    out.push(Decoration::Code {
                        first,
                        last: index,
                        lang,
                    });
                }
            }
            LineKind::Blank | LineKind::Paragraph | LineKind::Heading(_) | LineKind::Code => {}
        }
    }
    // A fence left open runs to the end of the document, which is what the
    // parser decided and what makes a block usable while you are still typing
    // inside it. The background has to agree, or it would stop at the last
    // fence and leave the tail unpainted.
    if let Some((first, lang)) = fence {
        out.push(Decoration::Code {
            first,
            last: doc.lines.len().saturating_sub(1),
            lang,
        });
    }
    out
}

/// Where a line sits on screen, and how tall it is.
///
/// Resolved through `Galley::pos_from_cursor`, which takes a character cursor
/// and hands back the caret rectangle there. That avoids reaching into row
/// internals, and it is the same mapping the caret itself uses, so a decoration
/// cannot end up on a different row from the text it belongs to.
struct Rows<'a> {
    galley: &'a egui::Galley,
    origin: egui::Pos2,
    /// Character offset of each line's start, so the byte-to-character
    /// conversion is done once for the document rather than per lookup.
    starts: Vec<usize>,
}

impl Rows<'_> {
    /// The horizontal extent text can occupy, in screen coordinates.
    ///
    /// Taken from the galley's own wrap width rather than from a padding
    /// constant: the wrap width is by definition where the text stops, so a
    /// background derived from it cannot end up narrower than the code it sits
    /// behind, which a guessed padding did.
    ///
    /// Then clamped to `clip`, because the wrap width can exceed the visible
    /// area. Without the clamp the background's right edge, and the language
    /// tag pinned to it, land outside the clip rectangle and are silently
    /// dropped: the block looks right and the tag simply never appears.
    fn content(&self, clip: Rect) -> std::ops::RangeInclusive<f32> {
        let width = if self.galley.job.wrap.max_width.is_finite() {
            self.galley.job.wrap.max_width
        } else {
            self.galley.rect.width()
        };
        let left = self.origin.x.max(clip.left());
        let right = (self.origin.x + width).min(clip.right());
        left..=right.max(left)
    }
}

impl Rows<'_> {
    fn row(&self, line: usize) -> Option<Rect> {
        let start = *self.starts.get(line)?;
        let caret = self
            .galley
            .pos_from_cursor(egui::text::CCursor::new(egui::text::CharIndex(start)));
        Some(caret.translate(self.origin.to_vec2()))
    }

    /// The vertical extent of a run of lines.
    fn band(&self, first: usize, last: usize) -> Option<Rect> {
        let top = self.row(first)?;
        let bottom = self.row(last)?;
        Some(Rect::from_x_y_ranges(
            top.x_range(),
            top.top()..=bottom.bottom(),
        ))
    }
}

/// Draws the decorations for a field that has just been laid out.
///
/// `backdrop` is a shape index reserved *before* the text was painted, so a
/// code block's background can go behind it. Everything else is drawn on top,
/// which is safe: the gutter is empty, and a divider's own row is transparent.
pub fn paint(
    ui: &Ui,
    output: &egui::widgets::text_edit::TextEditOutput,
    source: &str,
    revealed: &dyn Fn(&markdown::Line) -> bool,
    backdrop: egui::layers::ShapeIdx,
) {
    let doc = markdown::parse(source);
    let plan = plan(&doc);
    if plan.is_empty() {
        ui.painter().set(backdrop, egui::Shape::Noop);
        return;
    }

    let rows = Rows {
        galley: &output.galley,
        origin: output.galley_pos,
        starts: char_starts(source, &doc),
    };
    // Clipped to the text area, so a decoration cannot spill over the field's
    // own edge when the content is scrolled.
    let painter = ui.painter().with_clip_rect(output.text_clip_rect);

    let mut behind = Vec::new();
    for item in &plan {
        // A revealed line is showing its source, so its drawn substitute gets
        // out of the way. A code block keeps its background either way: the
        // fence's own backticks sitting on the background is right, and losing
        // the whole block as the caret entered it would be jarring.
        let step_aside = doc.lines.get(item.line()).is_some_and(revealed)
            && !matches!(item, Decoration::Code { .. });
        if step_aside {
            continue;
        }
        match item {
            Decoration::Code { first, last, lang } => {
                if let Some(band) = rows.band(*first, *last) {
                    behind.push(code_background(
                        &painter,
                        band,
                        rows.content(painter.clip_rect()),
                        &source[lang.clone()],
                    ));
                }
            }
            Decoration::Rule { line } => {
                if let Some(row) = rows.row(*line) {
                    rule(&painter, row, rows.content(painter.clip_rect()));
                }
            }
            Decoration::Bullet { line, depth } => {
                if let Some(row) = rows.row(*line) {
                    bullet(&painter, gutter(row, *depth));
                }
            }
            Decoration::Number { line, depth, value } => {
                if let Some(row) = rows.row(*line) {
                    number(&painter, ui, gutter(row, *depth), *value, color::TEXT);
                }
            }
            Decoration::Check {
                line,
                depth,
                checked,
            } => {
                if let Some(row) = rows.row(*line) {
                    checkbox(&painter, gutter(row, *depth), *checked);
                }
            }
        }
    }
    ui.painter().set(backdrop, egui::Shape::Vec(behind));
}

/// Character offset of each line's start.
fn char_starts(source: &str, doc: &markdown::Document) -> Vec<usize> {
    // One pass over the string rather than a `chars().count()` per line, which
    // would make a long note quadratic.
    let mut starts = Vec::with_capacity(doc.lines.len());
    let mut chars = 0;
    let mut at = 0;
    for line in &doc.lines {
        chars += source[at..line.range.start].chars().count();
        starts.push(chars);
        chars += source[line.range.clone()].chars().count();
        at = line.range.end;
    }
    starts
}

/// Which checkbox, if any, sits under `pos`.
///
/// Answers with the **byte offset** of the box rather than a line index, so the
/// caller hands it straight to [`markdown::edit::toggle_task`] as a caret. The
/// alternative, returning a line, would mean the caller converting a line back
/// into an offset with logic that only exists here.
///
/// The box is drawn, not laid out, so it is not in the galley and the text
/// field knows nothing about it. This is therefore the one place a click has to
/// be resolved geometrically, and it reuses [`Rows`] and [`gutter`], the exact
/// pair that positioned the box in the first place: a checkbox you can see and
/// cannot hit would be worse than no checkbox at all.
#[must_use]
pub fn hit(
    output: &egui::widgets::text_edit::TextEditOutput,
    source: &str,
    pos: egui::Pos2,
) -> Option<usize> {
    if !output.text_clip_rect.contains(pos) {
        return None;
    }
    let doc = markdown::parse(source);
    let rows = Rows {
        galley: &output.galley,
        origin: output.galley_pos,
        starts: char_starts(source, &doc),
    };
    plan(&doc).into_iter().find_map(|item| {
        let Decoration::Check { line, depth, .. } = item else {
            return None;
        };
        let row = rows.row(line)?;
        gutter(row, depth).contains(pos).then(|| {
            doc.lines
                .get(line)
                .map_or(0, |target| target.marker.start)
        })
    })
}

/// The square of gutter belonging to a line at a given depth.
fn gutter(row: Rect, depth: usize) -> Rect {
    let left = row.left() + super::theme::list_indent(depth);
    Rect::from_min_size(
        egui::pos2(left, row.top()),
        egui::vec2(metric::GUTTER, row.height()),
    )
}

/// A bullet dot.
///
/// Shared with the toolbar, deliberately: a button drawn by the same code as
/// the thing it produces cannot end up looking like something else. It is also
/// font-proof, which matters because the check and list glyphs are missing from
/// every bundled font.
pub fn bullet(painter: &egui::Painter, gutter: Rect) {
    // Sized from the rect it is given, not from the constant alone. The
    // toolbar draws these into a 24px button, where a marker tuned for a 30px
    // document row overflows and collides with its neighbour.
    let radius = metric::BULLET_RADIUS.min(gutter.height() * 0.22);
    painter.circle_filled(gutter.center(), radius, color::TEXT_WEAK);
}

/// The number that was typed, in the gutter.
///
/// The colour is a parameter, unlike the other three markers, because this one
/// is the only marker made of *text*. In a document a number reads as part of
/// the sentence it labels, so it is drawn in the body colour; on a toolbar
/// button it is one stroke of an icon, and has to sit at the same weight as the
/// dots and rules beside it or the glyph comes apart.
pub fn number(painter: &egui::Painter, ui: &Ui, gutter: Rect, value: u32, color: Color32) {
    // Right aligned in the gutter, so `9.` and `10.` end at the same column and
    // the text they label stays in line.
    let mut font = egui::TextStyle::Small.resolve(ui.style());
    font.size = font.size.min(gutter.height() * 0.8);
    painter.text(
        egui::pos2(gutter.right() - metric::ITEM_SPACING_X, gutter.center().y),
        egui::Align2::RIGHT_CENTER,
        format!("{value}."),
        font,
        color,
    );
}

pub fn checkbox(painter: &egui::Painter, gutter: Rect, checked: bool) {
    let side = metric::CHECKBOX.min(gutter.height() * 0.85);
    let box_rect = Rect::from_center_size(gutter.center(), egui::vec2(side, side));
    let (fill, edge) = if checked {
        (color::ACCENT, color::ACCENT)
    } else {
        (egui::Color32::TRANSPARENT, color::DOT_RING)
    };
    painter.rect(
        box_rect,
        3,
        fill,
        egui::Stroke::new(1.0, edge),
        egui::StrokeKind::Inside,
    );
    if checked {
        // A tick drawn rather than typed: the check glyphs are not in every
        // bundled font, and this is the same trap the notebook arrows fell into.
        let tick = box_rect.shrink(side * 0.26);
        painter.add(egui::Shape::line(
            vec![
                egui::pos2(tick.left(), tick.center().y),
                egui::pos2(tick.center().x - side * 0.02, tick.bottom()),
                egui::pos2(tick.right(), tick.top()),
            ],
            egui::Stroke::new((side * 0.14).max(1.0), color::CANVAS),
        ));
    }
}

pub fn rule(painter: &egui::Painter, row: Rect, content: std::ops::RangeInclusive<f32>) {
    painter.hline(
        content,
        row.center().y,
        egui::Stroke::new(metric::RULE_WIDTH, color::DIVIDER),
    );
}

/// The background for a code block, returned rather than drawn so it can go
/// behind the text.
fn code_background(
    painter: &egui::Painter,
    band: Rect,
    content: std::ops::RangeInclusive<f32>,
    lang: &str,
) -> egui::Shape {
    // Padding is taken *inside* the clamped range, never added outside it, or
    // the tag pinned to the right edge would be clipped away again.
    let rect = Rect::from_x_y_ranges(content.clone(), band.y_range());
    if !lang.is_empty() {
        // Drawn on top, since it sits in the fence row which has no ink of its
        // own. Right aligned so it never collides with the code.
        painter.text(
            egui::pos2(
                rect.right() - metric::CODE_PAD * 2.0,
                band.top() + metric::CODE_PAD,
            ),
            egui::Align2::RIGHT_TOP,
            lang,
            egui::FontId::monospace(metric::CODE_TAG),
            color::TEXT_FAINT,
        );
    }
    egui::Shape::rect_filled(rect, metric::CODE_ROUNDING, color::CODE_BG)
}
