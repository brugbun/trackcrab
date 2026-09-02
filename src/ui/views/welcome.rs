//! Default view. Shown until a folder or task is opened.

use eframe::egui::{self, Ui};

use crate::model::Tree;
use crate::ui::theme::color;

pub fn show(ui: &mut Ui, tree: &Tree, data_path: &std::path::Path) {
    ui.vertical_centered(|ui| {
        ui.add_space(ui.available_height() * 0.22);
        ui.label(
            egui::RichText::new("TrackCrab")
                .size(34.0)
                .color(color::TEXT),
        );
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new("Folders and tasks, nested as deep as you like")
                .color(color::TEXT_WEAK),
        );
        ui.add_space(28.0);

        let folders = tree
            .roots()
            .iter()
            .map(|r| 1 + tree.descendants(*r).len())
            .sum::<usize>();
        let summary = if tree.is_empty() {
            "Nothing here yet. Open the sidebar to make your first folder.".to_owned()
        } else {
            format!(
                "{folders} item(s) across {} root folder(s)",
                tree.roots().len()
            )
        };
        ui.label(egui::RichText::new(summary).color(color::TEXT_WEAK));

        ui.add_space(20.0);
        ui.label(
            egui::RichText::new(if tree.is_empty() {
                "Open the sidebar with the button top left, then press + to make a folder"
            } else {
                "Open the sidebar with the button top left to pick a folder"
            })
            .small()
            .color(color::TEXT_FAINT),
        );
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new(data_path.display().to_string())
                .small()
                .color(color::TEXT_FAINT),
        );
    });
}
