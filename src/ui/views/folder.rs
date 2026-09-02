//! Folder listing. A file explorer over one folder's contents.

use eframe::egui::{self, Ui};

use crate::model::{NodeId, Tree};
use crate::ui::Action;
use crate::ui::rows::{self, Dnd, Glyph, Meta};
use crate::ui::theme::{color, status_color};
use crate::ui::{can_drop, local_stamp, row_label, sorted_children};

pub fn show(ui: &mut Ui, tree: &Tree, folder: NodeId) -> Option<Action> {
    let mut action = None;

    let Ok(current) = tree.folder(folder) else {
        ui.label(egui::RichText::new("That folder is gone.").color(color::DANGER));
        return None;
    };

    action = breadcrumb(ui, tree, folder).or(action);

    ui.horizontal(|ui| {
        ui.heading(egui::RichText::new(&current.name).color(color::TEXT));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .button("+ Folder")
                .on_hover_text("New folder inside this one")
                .clicked()
            {
                action = Some(Action::NewFolderIn(Some(folder)));
            }
            if ui
                .button("+ Task")
                .on_hover_text("New task in this folder")
                .clicked()
            {
                action = Some(Action::NewTaskIn(folder));
            }
            if ui
                .button("Comments")
                .on_hover_text("Project notes for this folder (Ctrl+Left)")
                .clicked()
            {
                action = Some(Action::ToggleComments);
            }
        });
    });
    ui.label(
        egui::RichText::new(format!("Updated {}", local_stamp(current.updated_at)))
            .small()
            .color(color::TEXT_WEAK),
    );
    ui.add_space(10.0);

    let children = sorted_children(tree, Some(folder));
    if children.is_empty() {
        ui.label(
            egui::RichText::new("This folder is empty.")
                .italics()
                .color(color::TEXT_FAINT),
        );
        ui.label(
            egui::RichText::new("Add a task or a folder with the buttons above.")
                .small()
                .color(color::TEXT_FAINT),
        );
        return action;
    }

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for (index, child) in children.iter().enumerate() {
                // A divider between every item, folders and tasks alike.
                if index > 0 {
                    rows::divider(ui);
                }
                if let Some(next) = child_row(ui, tree, *child) {
                    action = Some(next);
                }
            }
        });

    action
}

/// One item in the listing, whether folder or task.
fn child_row(ui: &mut Ui, tree: &Tree, child: NodeId) -> Option<Action> {
    let node = tree.get(child)?;
    let is_folder = node.is_folder();
    let glyph = node
        .as_task()
        .map_or(Glyph::Folder, |task| Glyph::Dot(status_color(&task.status)));
    let stamp = local_stamp(node.updated_at());
    let attributed = node
        .as_task()
        .map(crate::model::Task::attributed_label)
        .unwrap_or_default();
    // Only a blocked task carries a reason, and only there is it shown.
    let reason = node
        .as_task()
        .and_then(|task| task.status.blocked_reason())
        .unwrap_or_default();

    // Folders accept drops, tasks only offer themselves.
    let accepts = |dragged: NodeId| can_drop(tree, dragged, Some(child));
    let dnd = if is_folder {
        Dnd {
            id: child,
            accepts: Some(&accepts),
        }
    } else {
        Dnd::source_only(child)
    };

    let row = rows::listing_row(
        ui,
        child,
        rows::Mark::opened(false),
        glyph,
        row_label(node),
        Meta {
            stamp: &stamp,
            attributed: &attributed,
            reason,
        },
        &dnd,
    );

    if let Some(moved) = row.dropped {
        return Some(Action::Move {
            node: moved,
            into: Some(child),
        });
    }
    if row.response.clicked() {
        return Some(if is_folder {
            Action::OpenFolder(child)
        } else {
            Action::OpenTask(child)
        });
    }
    None
}

/// The path to this folder, each ancestor clickable so you can walk back up.
/// The folder itself is shown but inert, since you are already looking at it.
fn breadcrumb(ui: &mut Ui, tree: &Tree, folder: NodeId) -> Option<Action> {
    let mut action = None;
    let mut chain = tree.ancestors(folder);
    chain.reverse();

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        for ancestor in chain {
            let Some(node) = tree.get(ancestor) else {
                continue;
            };
            let crumb = ui.add(
                egui::Label::new(
                    egui::RichText::new(node.display_name())
                        .small()
                        .color(color::TEXT_WEAK),
                )
                .selectable(false)
                .sense(egui::Sense::click()),
            );
            if crumb.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if crumb.clicked() {
                action = Some(Action::OpenFolder(ancestor));
            }
            ui.label(egui::RichText::new("/").small().color(color::TEXT_FAINT));
        }
        if let Some(node) = tree.get(folder) {
            ui.label(
                egui::RichText::new(node.display_name())
                    .small()
                    .color(color::TEXT_FAINT),
            );
        }
    });

    action
}
