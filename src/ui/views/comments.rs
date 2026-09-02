//! The comments notebook: project level context for a folder.
//!
//! Several titled spaces per folder, cycled through like desktop workspaces, so
//! a kickoff note, a list of blockers and a scratch page can sit side by side
//! without running together.
//!
//! Drawn as an overlay on the right rather than a panel that squeezes the task
//! listing. It is something you open deliberately and read, so it is allowed to
//! cover the timestamp and time columns while it is up.

use eframe::egui::{self, Ui};

use crate::model::{NodeId, Tree};
use crate::ui::theme::{color, metric};

/// Padding between the notebook's border and its content.
const PAD: i8 = 12;
/// The notebook's border width.
const BORDER: f32 = 1.0;

/// What the notebook asked the app to do. One request per frame: every one of
/// these either moves the cursor or closes the panel, so two at once would be
/// nonsense.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Request {
    /// Hide the notebook.
    Close,
    /// Cycle one space left, wrapping.
    Previous,
    /// Cycle one space right, wrapping.
    Next,
    /// Add a space and switch to it.
    Add,
    /// Delete the space on screen, after confirming.
    Delete,
}

impl Request {
    /// Where the cursor lands, for the two navigation requests.
    ///
    /// Wraparound, so cycling never dead ends. `None` for the requests that are
    /// not a move.
    #[must_use]
    pub fn resolve(self, index: usize, count: usize) -> Option<usize> {
        if count == 0 {
            return None;
        }
        match self {
            Self::Previous => Some((index + count - 1) % count),
            Self::Next => Some((index + 1) % count),
            Self::Close | Self::Add | Self::Delete => None,
        }
    }
}

#[derive(Default)]
pub struct Report {
    /// The tree was mutated, so the app should schedule a save.
    pub changed: bool,
    /// What the user asked for, if anything.
    pub request: Option<Request>,
}

impl Report {
    /// Records a request, keeping the first one asked for in a frame.
    fn ask(&mut self, request: Request) {
        self.request = self.request.or(Some(request));
    }
}

/// Where the overlay sits: the right hand slice of `content`.
///
/// `slide` runs 0 to 1. At 0 the panel is entirely off the right edge, which is
/// what gives it the same tween as the folder sidebar.
#[must_use]
pub fn overlay_rect(content: egui::Rect, slide: f32) -> egui::Rect {
    let width = (content.width() * metric::COMMENTS_FRACTION)
        .clamp(metric::COMMENTS_MIN, metric::COMMENTS_MAX)
        // Never wider than what there is.
        .min(content.width());
    let hidden = (1.0 - slide.clamp(0.0, 1.0)) * width;
    egui::Rect::from_min_max(
        egui::pos2(content.right() - width + hidden, content.top()),
        egui::pos2(content.right() + hidden, content.bottom()),
    )
}

/// Draws the notebook for one folder.
///
/// `slide` is the same animation factor the folder sidebar uses, so both panels
/// move at one speed by construction rather than by a copied duration.
pub fn show(
    ui: &Ui,
    tree: &mut Tree,
    folder: NodeId,
    index: usize,
    content: egui::Rect,
    slide: f32,
) -> Report {
    let mut report = Report::default();
    let rect = overlay_rect(content, slide);

    egui::Area::new(egui::Id::new("trackcrab_comments"))
        // Above the panels, so it genuinely overlays rather than reflowing them.
        .order(egui::Order::Middle)
        .fixed_pos(rect.min)
        // Off, or egui would pull the panel back on screen and there would be
        // nothing left to slide.
        .constrain(false)
        .show(ui.ctx(), |ui| {
            egui::Frame::new()
                .fill(color::RAISED)
                .stroke(egui::Stroke::new(BORDER, color::DIVIDER))
                .inner_margin(egui::Margin::same(PAD))
                .show(ui, |ui| {
                    // Sized from the *inside*. Constraining the Area's own Ui
                    // instead makes the frame grow by its margins and hang off
                    // the right edge, which clips the last word of every line.
                    let chrome = f32::from(PAD).mul_add(2.0, BORDER * 2.0);
                    let inner = rect.size() - egui::vec2(chrome, chrome);
                    ui.set_min_size(inner);
                    ui.set_max_size(inner);
                    // Mid slide the panel is partly off screen, so interacting
                    // with it would mean aiming at something still moving.
                    if slide < 1.0 {
                        ui.disable();
                    }
                    body(ui, tree, folder, index, &mut report);
                });
        });

    report
}

fn body(ui: &mut Ui, tree: &mut Tree, folder: NodeId, index: usize, report: &mut Report) {
    let spaces = tree.comment_spaces(folder);
    let count = spaces.len();
    let Some(space) = spaces.get(index) else {
        ui.label(
            egui::RichText::new("No comment spaces here yet.")
                .italics()
                .color(color::TEXT_FAINT),
        );
        if ui.button("+ Add one").clicked() {
            report.ask(Request::Add);
        }
        return;
    };
    // Copied out, so the tree is free to be borrowed mutably below when a field
    // actually changed.
    let was_titled = space.title.clone();
    let mut title = was_titled.clone();
    let mut text = space.body.clone();

    top_row(ui, index, count, report);
    title_row(ui, &mut title, count, report);
    ui.add_space(8.0);

    // Reserve the footer, so the body does not grow over the delete control.
    let footer = metric::ROW_HEIGHT + 6.0;
    let body_size = egui::vec2(
        ui.available_width(),
        (ui.available_height() - footer).max(metric::ROW_HEIGHT * 2.0),
    );
    let body_response = ui.add_sized(
        body_size,
        egui::TextEdit::multiline(&mut text)
            .hint_text("Anything that shapes this whole project")
            .frame(egui::Frame::NONE),
    );

    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        if ui
            .small_button("Delete space")
            .on_hover_text("Remove this comment space")
            .clicked()
        {
            report.ask(Request::Delete);
        }
    });

    // Both fields commit through the tree's single comment write path, so the
    // folder's updated_at and its ancestors' cannot be missed.
    if body_response.changed()
        && tree
            .edit_comment_space(folder, index, |space| space.body = text)
            .is_ok()
    {
        report.changed = true;
    }
    if title != was_titled
        && tree
            .edit_comment_space(folder, index, |space| space.title = title)
            .is_ok()
    {
        report.changed = true;
    }
}

/// The label, the position, and the add and close controls.
fn top_row(ui: &mut Ui, index: usize, count: usize, report: &mut Report) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("COMMENTS")
                .small()
                .color(color::TEXT_FAINT),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // Both at the sidebar's plus size, which Kyle asked to be a proper
            // target rather than a hairline.
            if icon_button(ui, "\u{00d7}", "Close the notebook (Ctrl+Left)").clicked() {
                report.ask(Request::Close);
            }
            if icon_button(ui, "+", "New comment space").clicked() {
                report.ask(Request::Add);
            }
            // Which of how many, so a long row of spaces stays navigable.
            ui.label(
                egui::RichText::new(format!("{} / {count}", index + 1))
                    .small()
                    .color(color::TEXT_FAINT),
            );
        });
    });
}

/// A borderless text button at the notebook's icon size.
fn icon_button(ui: &mut Ui, glyph: &str, tip: &str) -> egui::Response {
    ui.add(
        egui::Button::new(
            egui::RichText::new(glyph)
                .size(metric::SIDEBAR_PLUS)
                .color(color::TEXT),
        )
        .frame(false)
        .min_size(egui::vec2(
            metric::SIDEBAR_PLUS_HIT,
            metric::SIDEBAR_PLUS_HIT,
        )),
    )
    .on_hover_text(tip)
}

/// A chevron, painted rather than typed.
///
/// The obvious \u{2190} and \u{2192} live only in `Hack-Regular`, which egui maps to the
/// monospace style, so a proportional button renders them as missing glyph
/// boxes: exactly the trap the burger fell into. Painting the shape sidesteps
/// the bundled fonts altogether and stays crisp at any zoom.
fn arrow_button(ui: &mut Ui, enabled: bool, left: bool, tip: &str) -> egui::Response {
    let size = egui::vec2(metric::COLLAPSE_ICON, metric::ROW_HEIGHT);
    // Hover only when there is nowhere to go, so a dead arrow cannot be clicked
    // and cannot look clickable either.
    let sense = if enabled {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(size, sense);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, tip));

    let color = if !enabled {
        color::DIVIDER
    } else if response.hovered() {
        color::ACCENT
    } else {
        color::TEXT
    };
    let reach = metric::COLLAPSE_ICON * 0.3;
    let centre = rect.center();
    let dir = if left { -1.0 } else { 1.0 };
    let point = egui::pos2(centre.x + dir * reach * 0.7, centre.y);
    let back = centre.x - dir * reach * 0.7;
    ui.painter().add(egui::Shape::line(
        vec![
            egui::pos2(back, centre.y - reach),
            point,
            egui::pos2(back, centre.y + reach),
        ],
        egui::Stroke::new(2.0, color),
    ));

    if enabled {
        response.on_hover_text(tip)
    } else {
        response
    }
}

/// The arrows either side of the editable title.
fn title_row(ui: &mut Ui, title: &mut String, count: usize, report: &mut Report) {
    let many = count > 1;
    ui.horizontal(|ui| {
        if arrow_button(ui, many, true, "Previous space").clicked() {
            report.ask(Request::Previous);
        }

        let arrows = (metric::COLLAPSE_ICON + metric::ITEM_SPACING_X) * 2.0;
        ui.add_sized(
            egui::vec2(
                (ui.available_width() - arrows).max(metric::REASON_MIN),
                metric::ROW_HEIGHT,
            ),
            egui::TextEdit::singleline(title)
                .font(egui::TextStyle::Heading)
                .frame(egui::Frame::NONE)
                .hint_text("Untitled"),
        );

        if arrow_button(ui, many, false, "Next space").clicked() {
            report.ask(Request::Next);
        }
    });
}
