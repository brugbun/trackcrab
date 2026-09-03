//! Task detail view: the first place the UI writes back to the model.

use eframe::egui::{self, Ui};
use egui::containers::collapsing_header::CollapsingState;

use crate::model::{NodeId, Status, Tree};
use crate::ui::local_stamp;
use crate::ui::rows;
use crate::ui::text;
use crate::ui::theme::{self, color, metric};

/// Editable buffers for one task.
///
/// The widgets need somewhere durable to write, and the model is behind the
/// tree, so the view keeps its own copy and commits through
/// [`Tree::edit_task`]. The buffers are reloaded when the task changes, or when
/// a number box loses focus and its value may have been normalised.
pub struct Editor {
    id: NodeId,
    title: String,
    description: String,
    /// The chosen variant. The blocked reason is held separately so switching
    /// away from Blocked and back does not lose what was typed.
    status: Status,
    blocked_reason: String,
    /// Free notes about this task, separate from the description.
    notes: String,
    hours: u32,
    minutes: u32,
    /// True when Blocked has been chosen but no reason given, which means the
    /// status change has deliberately NOT been written to the model.
    unsaved_blocked: bool,
    confirming_delete: bool,
}

impl Editor {
    /// Loads the buffers for a task, or `None` if it is not a task.
    #[must_use]
    pub fn load(tree: &Tree, id: NodeId) -> Option<Self> {
        let task = tree.task(id).ok()?;
        let (hours, minutes) = task.attributed_hm();
        Some(Self {
            id,
            title: task.title.clone(),
            description: task.description_str().to_owned(),
            status: task.status.clone(),
            blocked_reason: task.status.blocked_reason().unwrap_or_default().to_owned(),
            notes: task.notes.clone(),
            hours,
            minutes,
            unsaved_blocked: false,
            confirming_delete: false,
        })
    }

    #[must_use]
    pub const fn id(&self) -> NodeId {
        self.id
    }

    /// Raises the delete confirmation, as the Delete key does.
    pub const fn request_delete(&mut self) {
        self.confirming_delete = true;
    }

    /// Reloads the time boxes from the model, picking up normalisation such as
    /// 90 minutes becoming 1h 30m.
    fn reload_time(&mut self, tree: &Tree) {
        if let Ok(task) = tree.task(self.id) {
            let (hours, minutes) = task.attributed_hm();
            self.hours = hours;
            self.minutes = minutes;
        }
    }

    /// Writes every buffer to the model in one edit.
    ///
    /// A Blocked status with no reason is the one field held back rather than
    /// sent. `edit_task` rejects that combination and rolls the whole edit back,
    /// so sending it would silently discard the title and description the user
    /// may have just typed. Instead the status alone is left alone and the UI
    /// says why.
    fn commit(&mut self, tree: &mut Tree) {
        let status = match &self.status {
            Status::Blocked(_) if self.blocked_reason.trim().is_empty() => None,
            Status::Blocked(_) => Some(Status::Blocked(self.blocked_reason.clone())),
            other => Some(other.clone()),
        };
        self.unsaved_blocked = status.is_none();

        let title = self.title.clone();
        let description = self.description.clone();
        let notes = self.notes.clone();
        let minutes = self.hours.saturating_mul(60).saturating_add(self.minutes);

        if let Err(err) = tree.edit_task(self.id, |task| {
            task.title = title;
            task.set_description(description);
            task.notes = notes;
            task.attributed_minutes = minutes;
            if let Some(status) = status {
                task.status = status;
            }
        }) {
            log::error!("could not save task {}: {err}", self.id.short());
        }
    }
}

/// What the view wants the app to do about it.
#[derive(Default)]
pub struct Report {
    /// The tree was mutated, so the app should schedule a save.
    pub changed: bool,
    /// The user confirmed a delete. The app performs it so it can also move the
    /// view somewhere sensible.
    pub delete_confirmed: bool,
}

pub fn show(ui: &mut Ui, tree: &mut Tree, editor: &mut Editor) -> Report {
    let mut report = Report::default();

    if tree.task(editor.id).is_err() {
        ui.label(egui::RichText::new("That task is gone.").color(color::DANGER));
        return report;
    }

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            report.changed |= header(ui, tree, editor);
            ui.add_space(14.0);

            section(ui, "DESCRIPTION");
            report.changed |= description(ui, tree, editor);
            ui.add_space(14.0);

            section(ui, "STATUS");
            report.changed |= status_section(ui, tree, editor);
            ui.add_space(14.0);

            section(ui, "NOTES");
            report.changed |= notes(ui, tree, editor);
            ui.add_space(14.0);

            section(ui, "TIME LOGGED");
            report.changed |= time_section(ui, tree, editor);

            ui.add_space(24.0);
            if ui
                .button(egui::RichText::new("Delete task").color(color::DANGER))
                .clicked()
            {
                editor.confirming_delete = true;
            }
        });

    if editor.confirming_delete {
        report.delete_confirmed = delete_modal(ui, editor);
    }

    report
}

/// Breadcrumb, editable title, and the two timestamps.
fn header(ui: &mut Ui, tree: &mut Tree, editor: &mut Editor) -> bool {
    let (created, updated) = tree
        .task(editor.id)
        .map(|task| (local_stamp(task.created_at), local_stamp(task.updated_at)))
        .unwrap_or_default();

    let path = tree.path_names(editor.id);
    let parents = path
        .get(..path.len().saturating_sub(1))
        .unwrap_or_default()
        .join(" / ");
    if !parents.is_empty() {
        ui.label(
            egui::RichText::new(parents)
                .small()
                .color(color::TEXT_FAINT),
        );
    }

    let title = ui.add(
        egui::TextEdit::singleline(&mut editor.title)
            .font(egui::TextStyle::Heading)
            // Frameless so the title reads as a heading, not a form field,
            // until you click into it.
            .frame(egui::Frame::NONE)
            .hint_text("Untitled task")
            .desired_width(f32::INFINITY),
    );

    ui.label(
        egui::RichText::new(format!("Created {created}    Updated {updated}"))
            .small()
            .color(color::TEXT_WEAK),
    );

    if title.changed() {
        editor.commit(tree);
        return true;
    }
    false
}

fn description(ui: &mut Ui, tree: &mut Tree, editor: &mut Editor) -> bool {
    let response = text::edit(
        ui,
        &text::Field {
            id: egui::Id::new("trackcrab_description"),
            hint: "Optional",
            rows: Some(4),
            ..text::Field::default()
        },
        &mut editor.description,
    );
    if response.changed() {
        editor.commit(tree);
        return true;
    }
    false
}

/// Free notes about this one task.
///
/// Sits under the status, so the screen reads top to bottom as what the task is,
/// where it stands, and then what you know about it.
fn notes(ui: &mut Ui, tree: &mut Tree, editor: &mut Editor) -> bool {
    let response = text::edit(
        ui,
        &text::Field {
            id: egui::Id::new("trackcrab_notes"),
            hint: "Anything worth remembering about this task",
            rows: Some(5),
            ..text::Field::default()
        },
        &mut editor.notes,
    );
    if response.changed() {
        editor.commit(tree);
        return true;
    }
    false
}

/// The five status pills, plus the blocked reason revealed underneath them.
fn status_section(ui: &mut Ui, tree: &mut Tree, editor: &mut Editor) -> bool {
    let mut changed = false;
    if rows::status_pills(ui, &mut editor.status) {
        editor.commit(tree);
        changed = true;
    }

    // egui's collapsing state animates the height for us rather than us
    // reinventing a reveal.
    let mut reveal =
        CollapsingState::load_with_default_open(ui.ctx(), ui.id().with("blocked_reveal"), false);
    reveal.set_open(editor.status.is_blocked());
    reveal.show_body_unindented(ui, |ui| {
        ui.add_space(8.0);
        section(ui, "BLOCKED REASON");
        let reason = ui.add(
            egui::TextEdit::singleline(&mut editor.blocked_reason)
                .hint_text("What is it waiting on?")
                .desired_width(f32::INFINITY),
        );
        if reason.changed() {
            editor.commit(tree);
            changed = true;
        }
        if editor.unsaved_blocked {
            ui.label(
                egui::RichText::new(
                    "Not saved yet. A blocked task needs a reason, so the status is still the previous one.",
                )
                .small()
                .color(color::DANGER),
            );
        }
    });

    changed
}

/// Hours and minutes only. No seconds, by design.
fn time_section(ui: &mut Ui, tree: &mut Tree, editor: &mut Editor) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        let hours = ui.add(
            egui::DragValue::new(&mut editor.hours)
                .range(0..=9999)
                .speed(0.25)
                .suffix(" h"),
        );
        let minutes = ui.add(
            egui::DragValue::new(&mut editor.minutes)
                .range(0..=9999)
                .speed(0.5)
                .suffix(" m"),
        );
        if hours.changed() || minutes.changed() {
            editor.commit(tree);
            changed = true;
        }
        // Only fold minutes into hours once the box is done with, so the display
        // does not rewrite itself under the cursor mid-typing.
        if hours.lost_focus() || minutes.lost_focus() {
            editor.reload_time(tree);
        }
        if let Ok(task) = tree.task(editor.id) {
            let label = task.attributed_label();
            if !label.is_empty() {
                ui.label(
                    egui::RichText::new(format!("= {label}"))
                        .small()
                        .color(color::TEXT_WEAK),
                );
            }
        }
    });
    changed
}

/// Returns true when the user confirms.
///
/// Enter confirms, matching every other dialog. The modal has no text field, so
/// there is nothing for the key to mean instead.
fn delete_modal(ui: &Ui, editor: &mut Editor) -> bool {
    let mut confirmed = false;
    let title = editor.title.clone();
    let modal = egui::Modal::new(egui::Id::new("confirm_delete_task")).show(ui.ctx(), |ui| {
        let width = theme::dialog_width(ui.ctx());
        ui.set_min_width(width);
        ui.set_max_width(width);
        theme::scale_text(ui, metric::DIALOG_TEXT_SCALE);
        ui.heading("Delete this task?");
        ui.add_space(6.0);
        ui.label(if title.trim().is_empty() {
            "This untitled task will be removed. This cannot be undone.".to_owned()
        } else {
            format!("\"{title}\" will be removed. This cannot be undone.")
        });
        ui.add_space(14.0);
        ui.horizontal(|ui| {
            if ui.button("Cancel").clicked() {
                editor.confirming_delete = false;
            }
            if ui
                .button(egui::RichText::new("Delete").color(color::DANGER))
                .clicked()
            {
                editor.confirming_delete = false;
                confirmed = true;
            }
            ui.label(
                egui::RichText::new("Enter deletes")
                    .small()
                    .color(color::TEXT_FAINT),
            );
        });
    });

    if modal.should_close() {
        editor.confirming_delete = false;
    } else if !confirmed
        && ui
            .ctx()
            .input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter))
    {
        editor.confirming_delete = false;
        confirmed = true;
    }
    confirmed
}

/// A small uppercase section heading.
fn section(ui: &mut Ui, text: &str) {
    ui.label(egui::RichText::new(text).small().color(color::TEXT_FAINT));
}
