//! The collapsible folder tree.
//!
//! Scrolls in both directions, animates open and closed, and reports clicks
//! back to the app rather than mutating anything itself.

use eframe::egui::{self, Ui};
use egui::containers::collapsing_header::CollapsingState;

use super::rows::{self, Dnd, Glyph};
use super::theme::{color, metric};
use super::{Action, Filter, can_drop, collapse_id, row_label, sorted_children};
use crate::app::View;
use crate::model::{NodeId, Tree};

/// Where the folder tree's keyboard cursor is.
///
/// Separate from the open item: the cursor is where Enter would take you, which
/// is not necessarily where you already are.
#[derive(Clone, Copy, Default)]
pub struct Nav {
    pub cursor: Option<NodeId>,
    /// Scroll the cursor's row into view this frame, because it just moved.
    /// Only set on a key press, so the tree never fights the scrollbar.
    pub reveal: bool,
}

/// Which folders sit on the path to whatever is currently open.
///
/// Used to tier the indent guides: the open item's immediate parent is bright,
/// its higher ancestors are a middle tier, everything else stays quiet.
struct ActivePath {
    /// The open item's immediate parent folder, if any.
    nearest: Option<NodeId>,
    /// Every folder above that, up to the top level.
    higher: Vec<NodeId>,
}

impl ActivePath {
    fn of(tree: &Tree, view: &View) -> Self {
        let open = match view {
            View::Welcome => None,
            View::Folder(id) | View::Task(id) => Some(*id),
        };
        let Some(open) = open else {
            return Self {
                nearest: None,
                higher: Vec::new(),
            };
        };
        // Always the open item's *parent*, whether it is a folder or a task.
        // Lighting up an open folder's own guide put the bright line below its
        // row instead of leading to it, which read as broken.
        let mut chain = tree.ancestors(open).into_iter();
        let nearest = chain.next();
        Self {
            nearest,
            higher: chain.collect(),
        }
    }

    fn tier(&self, folder: NodeId) -> Option<egui::Color32> {
        if self.nearest == Some(folder) {
            Some(color::GUIDE_ACTIVE)
        } else if self.higher.contains(&folder) {
            Some(color::GUIDE_ANCESTOR)
        } else {
            None
        }
    }
}

/// Everything the tree render needs that is not the tree itself.
struct Ctx<'a> {
    view: &'a View,
    active: ActivePath,
    nav: Nav,
    /// `None` means no filter, draw everything.
    visible: Option<std::collections::HashSet<NodeId>>,
    /// The open item's row, recorded as it is drawn. The nearest parent's guide
    /// is cut off here so it visibly *ends* at the item it leads to, rather than
    /// running on to the folder's last child.
    open_row: std::cell::Cell<Option<egui::Rect>>,
}

impl Ctx<'_> {
    const fn filtering(&self) -> bool {
        self.visible.is_some()
    }

    fn shows(&self, id: NodeId) -> bool {
        self.visible.as_ref().is_none_or(|set| set.contains(&id))
    }

    fn is_open(&self, id: NodeId) -> bool {
        matches!(self.view, View::Folder(open) | View::Task(open) if *open == id)
    }

    fn mark(&self, id: NodeId) -> rows::Mark {
        rows::Mark {
            open: self.is_open(id),
            cursor: self.nav.cursor == Some(id),
        }
    }
}

/// Draws the tree. Returns an action if something was clicked this frame.
pub fn show(
    ui: &mut Ui,
    tree: &Tree,
    view: &View,
    filter: &mut Filter,
    focus_search: bool,
    nav: Nav,
) -> Option<Action> {
    let mut action = None;

    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("FOLDERS")
                .size(metric::SIDEBAR_HEADER)
                .color(color::TEXT_WEAK),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // The only route to the very first folder, so it has to live
            // somewhere that exists before any folder does. Sized to match the
            // heading beside it, and to be an easy target.
            if ui
                .add(
                    egui::Button::new(
                        egui::RichText::new("+")
                            .size(metric::SIDEBAR_PLUS)
                            .color(color::TEXT),
                    )
                    .frame(false)
                    .min_size(egui::vec2(
                        metric::SIDEBAR_PLUS_HIT,
                        metric::SIDEBAR_PLUS_HIT,
                    )),
                )
                .on_hover_text("New folder at the top level")
                .clicked()
            {
                action = Some(Action::NewFolderIn(None));
            }
        });
    });
    filter_bar(ui, filter, focus_search);
    ui.add_space(2.0);

    egui::ScrollArea::both()
        // Without this the panel collapses to its content width and the
        // horizontal scrollbar never has anything to do.
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let roots = sorted_children(tree, None);
            if roots.is_empty() {
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new("No folders yet")
                        .italics()
                        .color(color::TEXT_FAINT),
                );
                ui.label(
                    egui::RichText::new("Use + above to make one")
                        .small()
                        .color(color::TEXT_FAINT),
                );
                return;
            }
            let ctx = Ctx {
                view,
                active: ActivePath::of(tree, view),
                nav,
                visible: super::visible(tree, filter),
                open_row: std::cell::Cell::new(None),
            };
            let shown = roots.iter().filter(|id| ctx.shows(**id)).count();
            if shown == 0 {
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new("Nothing matches")
                        .italics()
                        .color(color::TEXT_FAINT),
                );
            }
            for root in roots {
                node(ui, tree, root, &ctx, &mut action);
            }
            // The space below the tree is the drop target for "move to the top
            // level". Tasks are refused here, since a task always has a folder.
            if let Some(moved) = root_drop_zone(ui, tree) {
                action = Some(Action::Move {
                    node: moved,
                    into: None,
                });
            }
        });

    action
}

/// The search box and the five status toggles.
fn filter_bar(ui: &mut Ui, filter: &mut Filter, focus_search: bool) {
    let search = ui.add(
        egui::TextEdit::singleline(&mut filter.text)
            .id(super::search_box_id())
            .hint_text("Search")
            .desired_width(f32::INFINITY),
    );
    if focus_search {
        search.request_focus();
    }

    ui.horizontal_wrapped(|ui| {
        for variant in crate::model::Status::variants() {
            let index = variant.ordinal() as usize;
            let on = filter.statuses[index];
            let fill = super::theme::status_color(&variant);

            // A dot per status, lit once it has been picked out. Unlit is the
            // resting state, since an empty allowlist shows everything.
            let (rect, response) = ui.allocate_exact_size(
                egui::vec2(metric::ROW_HEIGHT * 0.8, metric::ROW_HEIGHT * 0.8),
                egui::Sense::click(),
            );
            let colour = if on { fill } else { fill.gamma_multiply(0.22) };
            ui.painter().circle(
                rect.center(),
                metric::DOT_RADIUS + 1.0,
                colour,
                egui::Stroke::new(
                    metric::DOT_RING_WIDTH,
                    if on { color::TEXT_WEAK } else { color::GUIDE },
                ),
            );
            if response.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if response.clicked() {
                filter.statuses[index] = !on;
            }
            // Wording that stays true whether or not other statuses are lit.
            response.on_hover_text(format!(
                "{} {} {} the filter",
                if on { "Remove" } else { "Add" },
                variant.label(),
                if on { "from" } else { "to" }
            ));
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if filter.is_active() {
                if ui.small_button("Clear").clicked() {
                    filter.clear();
                }
            } else {
                // Say what an empty allowlist means, since unlit dots could
                // otherwise read as "nothing shown".
                ui.label(
                    egui::RichText::new("All statuses")
                        .small()
                        .color(color::TEXT_FAINT),
                );
            }
        });
    });
}

/// The empty space under the tree, which accepts a folder being moved back up
/// to the top level.
fn root_drop_zone(ui: &mut Ui, tree: &Tree) -> Option<NodeId> {
    let height = ui.available_height().max(metric::ROW_HEIGHT * 2.0);
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width().max(1.0), height),
        egui::Sense::hover(),
    );
    let response = ui.interact(
        rect,
        ui.id().with("root_drop"),
        egui::Sense::click_and_drag(),
    );

    if let Some(held) = response.dnd_hover_payload::<NodeId>() {
        let ok = can_drop(tree, *held, None);
        let (fill, edge) = if ok {
            (color::DROP_OK, color::DROP_OK_EDGE)
        } else {
            (color::DROP_BAD, color::DROP_BAD_EDGE)
        };
        ui.painter().rect(
            rect.shrink(2.0),
            metric::ROW_ROUNDING,
            fill,
            egui::Stroke::new(1.0, edge),
            egui::StrokeKind::Inside,
        );
        ui.painter().text(
            rect.center_top() + egui::vec2(0.0, metric::ROW_HEIGHT * 0.4),
            egui::Align2::CENTER_CENTER,
            if ok {
                "Move to the top level"
            } else {
                "Tasks must stay in a folder"
            },
            egui::TextStyle::Small.resolve(ui.style()),
            edge,
        );
    }

    response
        .dnd_release_payload::<NodeId>()
        .filter(|held| can_drop(tree, **held, None))
        .map(|held| *held)
}

/// One node, recursing into folders.
fn node(ui: &mut Ui, tree: &Tree, id: NodeId, ctx: &Ctx<'_>, action: &mut Option<Action>) {
    if !ctx.shows(id) {
        return;
    }
    let Some(current) = tree.get(id) else { return };

    if current.is_task() {
        let mark = ctx.mark(id);
        let status = current.as_task().map(|t| t.status.clone());
        let dot = status
            .as_ref()
            .map_or(Glyph::None, |s| Glyph::Dot(super::theme::status_color(s)));
        let title = row_label(current).to_owned();
        // A task can be picked up but never dropped onto: nothing lives inside
        // a task.
        let row = rows::row(ui, id, mark, dot, &Dnd::source_only(id), |ui| {
            rows::title(ui, &title, mark.open);
        });
        if ctx.is_open(id) {
            ctx.open_row.set(Some(row.response.rect));
        }
        follow_cursor(ui, &row.response, mark, ctx);
        if row.response.clicked() {
            *action = Some(Action::OpenTask(id));
        }
        return;
    }

    // Folders get a collapsing row. egui animates the body open and closed for
    // us, and drives the arrow's rotation from the same value.
    let name = current.display_name().to_owned();
    let mark = ctx.mark(id);
    let children: Vec<NodeId> = sorted_children(tree, Some(id))
        .into_iter()
        .filter(|child| ctx.shows(*child))
        .collect();
    let has_children = !children.is_empty();

    // Under a filter the tree opens itself so matches are reachable, and it does
    // so in a separate id namespace, leaving your own arrangement untouched.
    let state = CollapsingState::load_with_default_open(
        ui.ctx(),
        collapse_id(id, ctx.filtering()),
        ctx.filtering(),
    );

    let accepts = |dragged: NodeId| can_drop(tree, dragged, Some(id));
    let dnd = Dnd {
        id,
        accepts: Some(&accepts),
    };

    let header = state.show_header(ui, |ui| {
        let row = rows::row(ui, id, mark, Glyph::None, &dnd, |ui| {
            rows::title(ui, &name, mark.open);
            if !has_children {
                ui.label(
                    egui::RichText::new("empty")
                        .small()
                        .color(color::TEXT_FAINT),
                );
            }
        });
        if ctx.is_open(id) {
            ctx.open_row.set(Some(row.response.rect));
        }
        follow_cursor(ui, &row.response, mark, ctx);
        if row.response.clicked() {
            *action = Some(Action::OpenFolder(id));
        }
        if let Some(moved) = row.dropped {
            *action = Some(Action::Move {
                node: moved,
                into: Some(id),
            });
        }
        // Right click is where folder management lives, so the tree stays a
        // tree rather than a row of buttons.
        row.response.context_menu(|ui| {
            if ui.button("New task").clicked() {
                *action = Some(Action::NewTaskIn(id));
                ui.close();
            }
            if ui.button("New folder").clicked() {
                *action = Some(Action::NewFolderIn(Some(id)));
                ui.close();
            }
            ui.separator();
            if ui.button("Rename").clicked() {
                *action = Some(Action::RenameFolder(id));
                ui.close();
            }
            if ui
                .button(egui::RichText::new("Delete").color(color::DANGER))
                .clicked()
            {
                *action = Some(Action::DeleteFolder(id));
                ui.close();
            }
        });
    });

    // `body` yields (toggle, header, body); only the body's rect is wanted here.
    let (_, _, body) = header.body(|ui| {
        for child in children {
            node(ui, tree, child, ctx, action);
        }
    });

    // The indent guide for this folder, spanning its open body.
    if let Some(body) = body {
        draw_guide(ui, id, body.response.rect, ctx);
    }
}

/// Keeps the keyboard cursor on screen after it moves.
///
/// Only on the frame the key was pressed. Scrolling to it every frame would
/// fight anyone dragging the scrollbar.
///
/// Deliberately not `Response::scroll_to_me`, which targets **both** axes: in a
/// two dimensional scroll area that drags the view sideways as well, so arrowing
/// onto a deeply indented row chopped the left off every label. The target here
/// keeps the horizontal range exactly as it already is, so only the vertical
/// scroll can move, and it moves by the minimum needed rather than recentring.
fn follow_cursor(ui: &Ui, response: &egui::Response, mark: rows::Mark, ctx: &Ctx<'_>) {
    if !(mark.cursor && ctx.nav.reveal) {
        return;
    }
    let target = egui::Rect::from_x_y_ranges(ui.clip_rect().x_range(), response.rect.y_range());
    ui.scroll_to_rect(target, None);
}

/// The vertical guide belonging to one folder, spanning its open body.
///
/// For the open item's nearest parent the line is drawn bright only as far as
/// that item's row, with a short elbow into it, so the highlight visibly
/// connects the two. Whatever is below carries on in the quiet colour. Higher
/// ancestors get the middle tier for their whole length.
fn draw_guide(ui: &Ui, folder: NodeId, body: egui::Rect, ctx: &Ctx<'_>) {
    // Sit the guide in the middle of the indent egui just applied, so it lines
    // up under this folder's own collapse arrow.
    let x = (body.left() - metric::INDENT / 2.0).round();
    let quiet = egui::Stroke::new(metric::GUIDE_WIDTH, color::GUIDE);

    let Some(tier) = ctx.active.tier(folder) else {
        ui.painter().vline(x, body.y_range(), quiet);
        return;
    };

    let is_nearest = tier == color::GUIDE_ACTIVE;
    let stop = ctx.open_row.get().map(|row| row.center().y);

    match (is_nearest, stop) {
        // The bright case: down to the open row, elbow across, quiet below.
        (true, Some(y)) if body.y_range().contains(y) => {
            let bright = egui::Stroke::new(metric::GUIDE_WIDTH, color::GUIDE_ACTIVE);
            ui.painter()
                .vline(x, egui::Rangef::new(body.top(), y), bright);
            ui.painter()
                .hline(egui::Rangef::new(x, x + metric::INDENT * 0.55), y, bright);
            if y < body.bottom() {
                ui.painter()
                    .vline(x, egui::Rangef::new(y, body.bottom()), quiet);
            }
        }
        // An ancestor, or the nearest parent whose child is off screen.
        _ => {
            ui.painter().vline(
                x,
                body.y_range(),
                egui::Stroke::new(metric::GUIDE_WIDTH, tier),
            );
        }
    }
}
