//! Modal dialogs for creating, renaming and deleting.
//!
//! Each dialog owns its own draft state. Nothing here changes the view; the app
//! does that from the [`Report`] it gets back.

use eframe::egui::{self, Ui};

use crate::model::{NodeId, Status, Tree, TreeError};
use crate::ui::rows;
use crate::ui::theme::{self, color, metric};

/// The dialog currently up, if any.
pub enum Dialog {
    /// A blank task has already been created and inserted, per the spec, so it
    /// shows in the sidebar and the listing straight away. Cancelling removes it
    /// again.
    NewTask {
        id: NodeId,
        title: String,
        description: String,
        status: Status,
        reason: String,
    },
    NewFolder {
        parent: Option<NodeId>,
        name: String,
    },
    RenameFolder {
        id: NodeId,
        name: String,
    },
    DeleteFolder {
        id: NodeId,
    },
    /// A comment space is only ever removed from the notebook that is showing
    /// it, so the folder travels with the index.
    DeleteCommentSpace {
        folder: NodeId,
        index: usize,
    },
}

impl Dialog {
    /// Starts the new task flow for an already created blank task.
    #[must_use]
    pub fn new_task(id: NodeId) -> Self {
        Self::NewTask {
            id,
            title: String::new(),
            description: String::new(),
            status: Status::Open,
            reason: String::new(),
        }
    }
}

/// How a dialog ended, if it has.
///
/// An enum rather than a `close` plus `cancel` pair, which allowed the
/// nonsensical "cancelled but still open".
#[derive(Default, PartialEq, Eq)]
enum Outcome {
    #[default]
    Open,
    Confirmed,
    Cancelled,
}

/// What a dialog body reports back about its own state.
#[derive(Default)]
struct Body {
    outcome: Outcome,
    /// The confirm action is currently available, which is what Enter needs.
    valid: bool,
    /// Enter should be read as confirm. False while the caret is in a multiline
    /// field, where Enter means a new line and nothing else.
    enter_confirms: bool,
}

impl Body {
    fn confirm(&mut self) {
        self.outcome = Outcome::Confirmed;
    }
    fn cancel(&mut self) {
        self.outcome = Outcome::Cancelled;
    }
    const fn finished(&self) -> bool {
        !matches!(self.outcome, Outcome::Open)
    }
}

#[derive(Default)]
pub struct Report {
    /// The tree was mutated, so the app should schedule a save.
    pub changed: bool,
    /// Something newly created. The app expands the tree down to it so it is
    /// visible, but deliberately does NOT open it: filling out a hierarchy
    /// means creating several things in a row, and being thrown into each one's
    /// own view every time makes that painful.
    pub reveal: Option<NodeId>,
    /// A node that no longer exists, so the app can drop it from the view.
    pub removed: Option<NodeId>,
    /// Where the comments notebook should point after a space was removed.
    pub comment_index: Option<usize>,
    pub error: Option<String>,
}

/// Shows whichever dialog is up. Clears `dialog` when it finishes.
pub fn show(ui: &Ui, tree: &mut Tree, dialog: &mut Option<Dialog>) -> Report {
    let mut report = Report::default();
    let Some(current) = dialog.as_mut() else {
        return report;
    };

    // Every dialog is dismissable with Escape or a click on the backdrop, and
    // dismissing must behave exactly like Cancel. For the new task flow that
    // means removing the blank task, otherwise Escape would leave an untitled
    // stub behind.
    let mut body = Body::default();

    let modal = egui::Modal::new(egui::Id::new("trackcrab_dialog")).show(ui.ctx(), |ui| {
        let width = theme::dialog_width(ui.ctx());
        ui.set_min_width(width);
        ui.set_max_width(width);
        theme::scale_text(ui, metric::DIALOG_TEXT_SCALE);
        match current {
            Dialog::NewTask {
                title,
                description,
                status,
                reason,
                ..
            } => new_task_body(ui, title, description, status, reason, &mut body),
            Dialog::NewFolder { parent, name } => {
                folder_name_body(ui, "New folder", name, *parent, &mut body);
            }
            Dialog::RenameFolder { id, name } => {
                let existing = tree.folder(*id).map(|f| f.name.clone()).unwrap_or_default();
                folder_rename_body(ui, name, &existing, &mut body);
            }
            Dialog::DeleteFolder { id } => {
                delete_folder_body(ui, tree, *id, &mut body);
            }
            Dialog::DeleteCommentSpace { folder, index } => {
                delete_comment_space_body(ui, tree, *folder, *index, &mut body);
            }
        }
    });

    if modal.should_close() {
        body.cancel();
    }

    // Enter confirms, which is what anyone expects of a small form. Checked
    // after the body so a multiline field has already had its go at the key,
    // and gated on the same condition that enables the confirm button, so Enter
    // can never do something the button would refuse.
    if body.enter_confirms
        && body.valid
        && !body.finished()
        && ui
            .ctx()
            .input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter))
    {
        body.confirm();
    }

    if body.finished() {
        let cancelled = body.outcome == Outcome::Cancelled;
        let finished = dialog.take().expect("a dialog was open");
        apply(tree, finished, cancelled, &mut report);
    }
    report
}

/// Commits a finished dialog, or undoes it if the user backed out.
fn apply(tree: &mut Tree, finished: Dialog, cancel: bool, report: &mut Report) {
    match finished {
        Dialog::NewTask {
            id,
            title,
            description,
            status,
            reason,
        } => {
            if cancel {
                // The blank task was created up front, so backing out removes it.
                if tree.delete_task(id).is_ok() {
                    report.changed = true;
                    report.removed = Some(id);
                }
                return;
            }
            let status = match status {
                Status::Blocked(_) => Status::Blocked(reason),
                other => other,
            };
            if let Err(err) = tree.edit_task(id, |task| {
                task.title = title;
                task.set_description(description);
                task.status = status;
            }) {
                report.error = Some(format!("Could not save the new task: {err}"));
            } else {
                report.changed = true;
                report.reveal = Some(id);
            }
        }
        Dialog::NewFolder { parent, name } => {
            if cancel {
                return;
            }
            match tree.create_folder(parent, name) {
                Ok(id) => {
                    report.changed = true;
                    report.reveal = Some(id);
                }
                Err(err) => report.error = Some(format!("Could not create that folder: {err}")),
            }
        }
        Dialog::RenameFolder { id, name } => {
            if cancel {
                return;
            }
            match tree.rename_folder(id, name) {
                Ok(()) => report.changed = true,
                Err(err) => report.error = Some(format!("Could not rename that folder: {err}")),
            }
        }
        Dialog::DeleteFolder { id } => {
            if cancel {
                return;
            }
            match tree.delete_folder(id) {
                Ok(()) => {
                    report.changed = true;
                    report.removed = Some(id);
                }
                Err(err @ TreeError::FolderNotEmpty { .. }) => {
                    report.error = Some(err.to_string());
                }
                Err(err) => report.error = Some(format!("Could not delete that folder: {err}")),
            }
        }
        Dialog::DeleteCommentSpace { folder, index } => {
            if cancel {
                return;
            }
            match tree.delete_comment_space(folder, index) {
                // The tree hands back the index that is still in range, so the
                // notebook never ends up pointing past the end.
                Ok(next) => {
                    report.changed = true;
                    report.comment_index = Some(next);
                }
                Err(err) => {
                    report.error = Some(format!("Could not delete that comment space: {err}"));
                }
            }
        }
    }
}

fn new_task_body(
    ui: &mut Ui,
    title: &mut String,
    description: &mut String,
    status: &mut Status,
    reason: &mut String,
    out: &mut Body,
) {
    ui.heading("New task");
    ui.add_space(10.0);

    ui.label(
        egui::RichText::new("TITLE")
            .small()
            .color(color::TEXT_FAINT),
    );
    let title_box = ui.add(
        egui::TextEdit::singleline(title)
            .hint_text("What needs doing?")
            .desired_width(f32::INFINITY),
    );
    // Land the caret in the title so you can just start typing.
    if !title_box.has_focus() && title.is_empty() {
        title_box.request_focus();
    }
    ui.add_space(8.0);

    ui.label(
        egui::RichText::new("DESCRIPTION (OPTIONAL)")
            .small()
            .color(color::TEXT_FAINT),
    );
    let description_box = crate::ui::text::edit(
        ui,
        &crate::ui::text::Field {
            id: egui::Id::new("trackcrab_new_task_description"),
            rows: Some(3),
            // No bar here. The dialog is a quick-entry form and already tall,
            // and the shortcuts still work, so the syntax is not out of reach.
            toolbar: false,
            ..crate::ui::text::Field::default()
        },
        description,
    );
    ui.add_space(8.0);

    ui.label(
        egui::RichText::new("STATUS")
            .small()
            .color(color::TEXT_FAINT),
    );
    rows::status_pills(ui, status);

    // A blocked task needs its reason before it can be created at all. Nothing
    // else is at stake in this dialog, so refusing is clearer than the detail
    // view's hold-the-status-back behaviour.
    let needs_reason = status.is_blocked() && reason.trim().is_empty();
    // A title is required, a description never is, and Blocked needs its reason.
    out.valid = !title.trim().is_empty() && !needs_reason;
    // Enter in the description means a new line, so it must not submit there.
    out.enter_confirms = !description_box.has_focus();
    if status.is_blocked() {
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new("BLOCKED REASON")
                .small()
                .color(color::TEXT_FAINT),
        );
        ui.add(
            egui::TextEdit::singleline(reason)
                .hint_text("What is it waiting on?")
                .desired_width(f32::INFINITY),
        );
    }

    ui.add_space(14.0);
    ui.horizontal(|ui| {
        if ui.button("Cancel").clicked() {
            out.cancel();
        }
        if ui
            .add_enabled(out.valid, egui::Button::new("Create"))
            .clicked()
        {
            out.confirm();
        }
        if needs_reason {
            ui.label(
                egui::RichText::new("A blocked task needs a reason.")
                    .small()
                    .color(color::DANGER),
            );
        } else if title.trim().is_empty() {
            ui.label(
                egui::RichText::new("A task needs a title.")
                    .small()
                    .color(color::TEXT_FAINT),
            );
        }
    });
}

fn folder_name_body(
    ui: &mut Ui,
    heading: &str,
    name: &mut String,
    parent: Option<NodeId>,
    out: &mut Body,
) {
    ui.heading(heading);
    if parent.is_none() {
        ui.label(
            egui::RichText::new("At the top level")
                .small()
                .color(color::TEXT_FAINT),
        );
    }
    ui.add_space(10.0);
    name_field_and_buttons(ui, name, "Create", out);
}

fn folder_rename_body(ui: &mut Ui, name: &mut String, existing: &str, out: &mut Body) {
    ui.heading("Rename folder");
    ui.label(
        egui::RichText::new(format!("Currently \"{existing}\""))
            .small()
            .color(color::TEXT_FAINT),
    );
    ui.add_space(10.0);
    name_field_and_buttons(ui, name, "Rename", out);
}

fn name_field_and_buttons(ui: &mut Ui, name: &mut String, confirm: &str, out: &mut Body) {
    let field = ui.add(
        egui::TextEdit::singleline(name)
            .hint_text("Folder name")
            .desired_width(f32::INFINITY),
    );
    if !field.has_focus() && name.is_empty() {
        field.request_focus();
    }
    let blank = name.trim().is_empty();
    // A single line field, so Enter always means confirm here.
    out.valid = !blank;
    out.enter_confirms = true;

    ui.add_space(14.0);
    ui.horizontal(|ui| {
        if ui.button("Cancel").clicked() {
            out.cancel();
        }
        if ui.add_enabled(!blank, egui::Button::new(confirm)).clicked() {
            out.confirm();
        }
        if blank {
            ui.label(
                egui::RichText::new("A folder needs a name.")
                    .small()
                    .color(color::TEXT_FAINT),
            );
        }
    });
}

fn delete_folder_body(ui: &mut Ui, tree: &Tree, id: NodeId, out: &mut Body) {
    let Ok(folder) = tree.folder(id) else {
        out.cancel();
        return;
    };
    let count = folder.children.len();

    ui.heading("Delete folder");
    ui.add_space(6.0);

    // Enter confirms a delete too, per the streamlined flow, but only when there
    // is actually something to confirm.
    out.enter_confirms = true;
    out.valid = count == 0;

    if count == 0 {
        ui.label(format!(
            "\"{}\" is empty and will be removed. This cannot be undone.",
            folder.name
        ));
        ui.add_space(14.0);
        ui.horizontal(|ui| {
            if ui.button("Cancel").clicked() {
                out.cancel();
            }
            if ui
                .button(egui::RichText::new("Delete").color(color::DANGER))
                .clicked()
            {
                out.confirm();
            }
            ui.label(
                egui::RichText::new("Enter deletes")
                    .small()
                    .color(color::TEXT_FAINT),
            );
        });
    } else {
        // Refused rather than offered and then rejected, so the only button is
        // the one that does something.
        ui.label(
            egui::RichText::new(format!(
                "\"{}\" still contains {count} item(s). Empty it first, then delete it.",
                folder.name
            ))
            .color(color::TEXT),
        );
        ui.add_space(14.0);
        if ui.button("Close").clicked() {
            out.cancel();
        }
    }
}

/// Confirmation for removing one comment space.
///
/// Always asked, even for an empty space: a page of project context is not
/// something to lose to a mis-click, and the notebook has no undo.
fn delete_comment_space_body(
    ui: &mut Ui,
    tree: &Tree,
    folder: NodeId,
    index: usize,
    out: &mut Body,
) {
    let Some(space) = tree.comment_spaces(folder).get(index) else {
        out.cancel();
        return;
    };

    ui.heading("Delete comment space");
    ui.add_space(6.0);

    out.enter_confirms = true;
    out.valid = true;

    let title = if space.title.trim().is_empty() {
        "Untitled".to_owned()
    } else {
        space.title.clone()
    };
    let words = space.body.split_whitespace().count();
    // An empty page reads oddly as "and its 0 word(s)", and the two cases carry
    // genuinely different weight anyway.
    ui.label(if words == 0 {
        format!("\"{title}\" is empty and will be removed.")
    } else {
        format!("\"{title}\" and its {words} word(s) will be removed. This cannot be undone.")
    });

    ui.add_space(14.0);
    ui.horizontal(|ui| {
        if ui.button("Cancel").clicked() {
            out.cancel();
        }
        if ui
            .button(egui::RichText::new("Delete").color(color::DANGER))
            .clicked()
        {
            out.confirm();
        }
        ui.label(
            egui::RichText::new("Enter deletes")
                .small()
                .color(color::TEXT_FAINT),
        );
    });
}
