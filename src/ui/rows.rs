//! The shared row primitives.
//!
//! Two row shapes, one for the sidebar tree and one for the folder listing, both
//! finished by [`finish_row`] so their highlight, hit testing and drag and drop
//! behaviour cannot drift apart.

use eframe::egui::{self, Color32, Response, Sense, Ui};

use crate::model::{NodeId, Status};

use super::theme::{self, color, metric};

/// How a row takes part in drag and drop.
pub struct Dnd<'a> {
    /// This row's node, offered as the payload while it is being dragged.
    pub id: NodeId,
    /// Whether a given dragged node may be dropped here. `None` means this row
    /// is not a drop target at all, which is the case for every task.
    ///
    /// A predicate rather than a flag, because legality depends on the tree and
    /// the hovering payload is not known until the row's response exists.
    pub accepts: Option<&'a dyn Fn(NodeId) -> bool>,
}

impl Dnd<'_> {
    /// A row that can be picked up but never dropped onto.
    #[must_use]
    pub const fn source_only(id: NodeId) -> Self {
        Self { id, accepts: None }
    }
}

/// A row's response, plus anything dropped onto it.
pub struct RowResponse {
    pub response: Response,
    /// A node released over this row, already checked as a legal drop.
    pub dropped: Option<NodeId>,
}

/// Why a row is standing out.
///
/// Two independent things, deliberately not one enum: what the main panel is
/// showing, and where the keyboard is. They can be the same row or different
/// rows, and they are drawn differently on purpose. A fill says "this is what
/// you are looking at"; an outline says "this is what Enter would open".
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct Mark {
    /// This row's item is open in the main panel.
    pub open: bool,
    /// This row is where the folder tree's keyboard cursor is sitting.
    pub cursor: bool,
}

impl Mark {
    /// A row marked only by what is open, for views with no keyboard cursor.
    #[must_use]
    pub const fn opened(open: bool) -> Self {
        Self {
            open,
            cursor: false,
        }
    }
}

/// What to draw at the left of a row.
#[derive(Clone, Copy)]
pub enum Glyph {
    /// A coloured status dot, for tasks.
    Dot(Color32),
    /// A folder marker. The sidebar draws its own collapse arrow instead, so
    /// this is for the listing view.
    Folder,
    /// Nothing, but still take up the dot column so titles stay aligned.
    None,
}

/// A sidebar row: glyph, then whatever the caller draws.
///
/// The background must be painted *behind* the content but its rect is only
/// known after the content is laid out, so a placeholder shape is reserved first
/// and filled in by [`finish_row`] afterwards.
///
/// The clickable area starts at the row's own content, NOT at the enclosing
/// `Ui`'s left edge. In the sidebar these rows sit inside a collapsing header
/// whose arrow is drawn to the left of the content, and a rect stretched to the
/// enclosing edge would cover that arrow. Because the row registers its
/// interaction last it would then win every click, leaving the arrow dead and
/// folders impossible to collapse.
pub fn row(
    ui: &mut Ui,
    id_salt: impl std::hash::Hash + std::fmt::Debug,
    mark: Mark,
    glyph: Glyph,
    dnd: &Dnd<'_>,
    content: impl FnOnce(&mut Ui),
) -> RowResponse {
    let bg = ui.painter().add(egui::Shape::Noop);

    let inner = ui.horizontal(|ui| {
        ui.set_min_height(metric::ROW_HEIGHT);
        draw_glyph(ui, glyph);
        content(ui);
    });

    finish_row(ui, bg, &inner.response, id_salt, mark, dnd)
}

/// Meta columns on a listing row.
///
/// The attributed time is always shown, falling back to `0h 0m`, and sits to the
/// *left* of the timestamp. Under width pressure the timestamp is what goes,
/// since it is much the wider of the two and the time is the figure worth
/// keeping.
#[derive(Clone, Copy)]
pub struct Meta<'a> {
    pub stamp: &'a str,
    /// Empty means nothing is logged, which displays as `0h 0m`.
    pub attributed: &'a str,
    /// The blocked reason, shown after the title. Empty for anything that is
    /// not blocked.
    pub reason: &'a str,
}

impl Meta<'_> {
    /// What the time column actually shows.
    fn time_text(&self) -> &str {
        if self.attributed.is_empty() {
            "0h 0m"
        } else {
            self.attributed
        }
    }
}

/// Width the title keeps before the timestamp is sacrificed for it.
const MIN_TITLE_WIDTH: f32 = 110.0;
/// The divider drawn between columns.
const SEPARATOR: &str = "|";
/// What a cut off reason ends with. Three full stops rather than a single `…`
/// glyph, so the tail reads as a deliberate continuation.
const ELLIPSIS: &str = "...";

/// One row of the folder listing:
///
/// ```text
/// [dot] Some task                        15h | 12:07:36 01/09/2026
/// [dot] Blocked task | waiting on the c...  0h 0m | 12:07:36 01/09/2026
/// ```
///
/// The title truncates rather than wrapping, the timestamp drops out as the
/// panel narrows, and a blocked reason takes whatever is left over while
/// keeping clear of the time column.
pub fn listing_row(
    ui: &mut Ui,
    id_salt: impl std::hash::Hash + std::fmt::Debug,
    mark: Mark,
    glyph: Glyph,
    title: &str,
    meta: Meta<'_>,
    dnd: &Dnd<'_>,
) -> RowResponse {
    let small = egui::TextStyle::Small.resolve(ui.style());
    let measure = |ui: &Ui, text: &str| -> f32 { measure_with(ui, text, &small) };

    let gap = metric::ITEM_SPACING_X;
    // Reserve a right margin so nothing ends up against the window edge or
    // tucked under a scrollbar, which is how the time column got clipped before.
    let total = ui.available_width() - metric::ROW_PAD_RIGHT;
    let time_text = meta.time_text();
    let stamp_w = measure(ui, meta.stamp);
    let time_w = measure(ui, time_text);
    let sep_w = measure(ui, SEPARATOR);

    let for_title_and_meta = total - metric::DOT_COLUMN - gap;
    // The time column is never dropped.
    let time_block = time_w + gap;
    let stamp_block = stamp_w + sep_w + gap * 2.0;
    let show_stamp = for_title_and_meta - time_block - stamp_block - gap >= MIN_TITLE_WIDTH;

    let meta_w = time_block + if show_stamp { stamp_block } else { 0.0 };
    let content_w = (for_title_and_meta - meta_w - gap).max(MIN_TITLE_WIDTH * 0.5);

    // A blocked reason follows the title, separated by a divider, and gives up
    // `REASON_CLEARANCE` so it can never run into the time column.
    let title_font = egui::TextStyle::Body.resolve(ui.style());
    let (title_w, reason) = if meta.reason.is_empty() {
        (content_w, None)
    } else {
        let title_natural = measure_with(ui, title, &title_font);
        let budget = content_w - metric::REASON_CLEARANCE - sep_w - gap * 2.0;
        // The title keeps its natural width, but not so much that the reason is
        // squeezed out of existence.
        let title_w = title_natural
            .min((budget - metric::REASON_MIN).max(MIN_TITLE_WIDTH * 0.5))
            .max(0.0);
        let reason_w = budget - title_w;
        if reason_w < metric::REASON_MIN {
            // Not enough room to say anything useful, so say nothing.
            (content_w, None)
        } else {
            (
                title_w,
                elide(ui, meta.reason, &small, reason_w).map(|text| (text, reason_w)),
            )
        }
    };

    let bg = ui.painter().add(egui::Shape::Noop);
    let inner = ui.horizontal(|ui| {
        ui.set_min_height(metric::ROW_HEIGHT);
        draw_glyph(ui, glyph);

        ui.allocate_ui_with_layout(
            egui::vec2(title_w, metric::ROW_HEIGHT),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.add(
                    egui::Label::new(egui::RichText::new(title).color(color::TEXT))
                        .truncate()
                        .selectable(false),
                );
            },
        );

        if let Some((text, width)) = &reason {
            ui.add(
                egui::Label::new(
                    egui::RichText::new(SEPARATOR)
                        .small()
                        .color(color::TEXT_FAINT),
                )
                .selectable(false),
            );
            ui.allocate_ui_with_layout(
                egui::vec2(*width, metric::ROW_HEIGHT),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.add(
                        egui::Label::new(egui::RichText::new(text).small().color(color::TEXT_WEAK))
                            .wrap_mode(egui::TextWrapMode::Extend)
                            .selectable(false),
                    );
                },
            );
        }

        // Right to left, so the first thing added sits furthest right. The
        // timestamp goes on first, which puts the attributed time to its left.
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(metric::ROW_PAD_RIGHT);
            if show_stamp {
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(meta.stamp)
                            .small()
                            .color(color::TEXT_WEAK),
                    )
                    .selectable(false),
                );
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(SEPARATOR)
                            .small()
                            .color(color::TEXT_FAINT),
                    )
                    .selectable(false),
                );
            }
            ui.add(
                egui::Label::new(
                    egui::RichText::new(time_text)
                        .small()
                        .color(color::TEXT_WEAK),
                )
                .selectable(false),
            );
        });
    });

    finish_row(ui, bg, &inner.response, id_salt, mark, dnd)
}

/// Background fill, hit rect, cursor and drag and drop for a row whose content
/// is already laid out. Shared by both row shapes so they behave identically.
fn finish_row(
    ui: &mut Ui,
    bg: egui::layers::ShapeIdx,
    content: &Response,
    id_salt: impl std::hash::Hash + std::fmt::Debug,
    mark: Mark,
    dnd: &Dnd<'_>,
) -> RowResponse {
    let mut rect = content.rect;
    rect.max.x = rect.max.x.max(ui.clip_rect().right() - metric::ROW_PAD_X);
    let rect = rect.expand2(egui::vec2(metric::ROW_PAD_X, 1.0));

    // click_and_drag, so a row is both a navigation target and something you can
    // pick up. `dnd_set_drag_payload` only fires on drag_started, so an ordinary
    // click is unaffected.
    let response = ui.interact(
        rect,
        ui.id().with(("row", id_salt)),
        Sense::click_and_drag(),
    );
    response.dnd_set_drag_payload(dnd.id);

    let mut dropped = None;
    // The keyboard cursor is an outline, so it reads as "where Enter would go"
    // rather than competing with the fill that marks what is actually open.
    let mut edge = mark.cursor.then_some(color::ACCENT);
    let mut fill = if mark.open {
        Some(color::SELECTED)
    } else if response.hovered() || mark.cursor {
        Some(color::HOVER)
    } else {
        None
    };

    if let Some(accepts) = dnd.accepts {
        // Answer before the drop, so aiming is not guesswork.
        if let Some(held) = response.dnd_hover_payload::<NodeId>() {
            if accepts(*held) {
                fill = Some(color::DROP_OK);
                edge = Some(color::DROP_OK_EDGE);
            } else {
                fill = Some(color::DROP_BAD);
                edge = Some(color::DROP_BAD_EDGE);
            }
        }
        if let Some(released) = response.dnd_release_payload::<NodeId>()
            && accepts(*released)
        {
            dropped = Some(*released);
        }
    }

    if let Some(fill) = fill {
        let mut shape = egui::epaint::RectShape::filled(rect, metric::ROW_ROUNDING, fill);
        if let Some(edge) = edge {
            shape.stroke = egui::Stroke::new(1.0, edge);
            shape.stroke_kind = egui::StrokeKind::Inside;
        }
        ui.painter().set(bg, shape);
    }
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }

    RowResponse { response, dropped }
}

fn draw_glyph(ui: &mut Ui, glyph: Glyph) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(metric::DOT_COLUMN, metric::ROW_HEIGHT),
        Sense::hover(),
    );
    match glyph {
        Glyph::Dot(fill) => {
            ui.painter().circle(
                rect.center(),
                metric::DOT_RADIUS,
                fill,
                egui::Stroke::new(metric::DOT_RING_WIDTH, color::DOT_RING),
            );
        }
        Glyph::Folder => {
            // A small filled square, deliberately quieter than a status dot so
            // tasks stay the thing your eye lands on.
            let side = metric::DOT_RADIUS * 1.8;
            let square = egui::Rect::from_center_size(rect.center(), egui::vec2(side, side));
            ui.painter().rect_filled(square, 1.0, color::TEXT_FAINT);
        }
        Glyph::None => {}
    }
}

/// A title that extends rather than wrapping, so long names push the row wider
/// and the enclosing 2D scroll area can reach them.
pub fn title(ui: &mut Ui, text: &str, strong: bool) {
    let mut rich = egui::RichText::new(text);
    if strong {
        rich = rich.color(color::TEXT);
    }
    ui.add(
        egui::Label::new(rich)
            .wrap_mode(egui::TextWrapMode::Extend)
            .selectable(false),
    );
}

/// A thin divider, used between every item in a listing.
pub fn divider(ui: &mut Ui) {
    // Span the visible width. Inside a scroll area the layout width can be much
    // wider than the panel, so the clip rect is the honest bound.
    let width = ui
        .available_width()
        .max(ui.clip_rect().width())
        .min(ui.clip_rect().width());
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 1.0), Sense::hover());
    ui.painter().hline(
        rect.x_range(),
        rect.center().y,
        egui::Stroke::new(1.0, color::DIVIDER),
    );
}

/// The five status choices as coloured pills. Returns true if the choice moved.
///
/// Shared by the task detail view and the new task dialog so the two can never
/// drift apart.
pub fn status_pills(ui: &mut Ui, status: &mut Status) -> bool {
    let mut changed = false;
    ui.horizontal_wrapped(|ui| {
        for variant in Status::variants() {
            let selected = status.same_variant(&variant);
            let fill = theme::status_color(&variant);

            let response = ui
                .scope(|ui| {
                    if selected {
                        // Tint the selected pill with its own status colour, so
                        // the choice is legible without reading the label.
                        let tint = fill.gamma_multiply(0.28);
                        let widgets = &mut ui.visuals_mut().widgets;
                        widgets.inactive.weak_bg_fill = tint;
                        widgets.hovered.weak_bg_fill = tint;
                        ui.visuals_mut().selection.bg_fill = tint;
                    }
                    ui.horizontal(|ui| {
                        let (rect, _) = ui.allocate_exact_size(
                            egui::vec2(metric::DOT_COLUMN, metric::ROW_HEIGHT),
                            Sense::hover(),
                        );
                        ui.painter().circle(
                            rect.center(),
                            metric::DOT_RADIUS,
                            fill,
                            egui::Stroke::new(metric::DOT_RING_WIDTH, color::DOT_RING),
                        );
                        ui.selectable_label(selected, variant.label())
                    })
                    .inner
                })
                .inner;

            if response.clicked() && !selected {
                // Only the variant moves; the editor keeps any reason it holds.
                *status = variant;
                changed = true;
            }
        }
    });
    changed
}

/// Width of `text` in a given font.
fn measure_with(ui: &Ui, text: &str, font: &egui::FontId) -> f32 {
    if text.is_empty() {
        return 0.0;
    }
    ui.ctx().fonts_mut(|fonts| {
        fonts
            .layout_no_wrap(text.to_owned(), font.clone(), Color32::PLACEHOLDER)
            .size()
            .x
    })
}

/// Shortens `text` to fit `max_width`, ending it with [`ELLIPSIS`].
///
/// Returns `None` when not even the ellipsis fits, so the caller can leave the
/// column out entirely rather than render a stub. Trimming walks back over
/// `char_indices`, so a multi byte character is never split down the middle.
fn elide(ui: &Ui, text: &str, font: &egui::FontId, max_width: f32) -> Option<String> {
    if measure_with(ui, text, font) <= max_width {
        return Some(text.to_owned());
    }
    if measure_with(ui, ELLIPSIS, font) > max_width {
        return None;
    }
    // Walk back one character at a time. Text on one row is short enough that
    // this is cheaper than the layout call a binary search would still need.
    let mut cut = text.len();
    for (index, _) in text.char_indices().rev() {
        cut = index;
        let candidate = format!("{}{ELLIPSIS}", text[..cut].trim_end());
        if measure_with(ui, &candidate, font) <= max_width {
            return Some(candidate);
        }
    }
    let _ = cut;
    Some(ELLIPSIS.to_owned())
}
