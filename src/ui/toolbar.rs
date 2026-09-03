//! The formatting bar above a markdown field.
//!
//! For anyone who would rather not learn the syntax. Every button has a
//! keyboard equivalent, and every button is a **toggle**: pressing bold on
//! something already bold takes it off again.
//!
//! The block icons are *painted*, by the same functions that draw those markers
//! in the document. Two reasons. A button drawn by the code that produces the
//! thing cannot end up looking like something else, and it sidesteps the fonts
//! entirely: `\u{2713}` is in none of the four bundled faces and `\u{2261}` only
//! in the monospace one, which is the trap the burger and the notebook arrows
//! both fell into.

use eframe::egui::{self, Ui};

use crate::markdown::edit::{Block, Wrap};
use crate::markdown::inline::Palette;

use super::theme::{self, color, metric};

/// What the toolbar was asked to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Wrap(Wrap),
    Block(Block),
    Divider,
    CodeBlock,
    Link,
}

/// Draws the bar, returning whatever was clicked.
pub fn show(ui: &mut Ui, salt: egui::Id) -> Option<Action> {
    let mut action = None;
    // Wrapped, because the notebook panel is narrow enough that thirteen
    // controls will not always fit on one line.
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;

        headings(ui, salt, &mut action);
        separator(ui);

        // Each label is styled as the thing it does, which is both a hint and
        // free: the formatting already exists.
        for (glyph, style, wrap, name, keys) in [
            ("B", Emphasis::Bold, Wrap::Bold, "Bold", "Ctrl+B"),
            ("I", Emphasis::Italic, Wrap::Italic, "Italic", "Ctrl+I"),
            (
                "U",
                Emphasis::Underline,
                Wrap::Underline,
                "Underline",
                "Ctrl+U",
            ),
            ("S", Emphasis::Strike, Wrap::Strike, "Strikethrough", ""),
        ] {
            if styled_button(ui, glyph, style, name, keys).clicked() {
                action = Some(Action::Wrap(wrap));
            }
        }
        if text_button(ui, "<>", "Inline code").clicked() {
            action = Some(Action::Wrap(Wrap::Code));
        }
        separator(ui);

        if painted_button(ui, Icon::Bullet, "Bulleted list").clicked() {
            action = Some(Action::Block(Block::Bullet));
        }
        if painted_button(ui, Icon::Numbered, "Numbered list").clicked() {
            action = Some(Action::Block(Block::Numbered));
        }
        if painted_button(ui, Icon::Task, "Checklist").clicked() {
            action = Some(Action::Block(Block::Task));
        }
        separator(ui);

        if painted_button(ui, Icon::Rule, "Divider").clicked() {
            action = Some(Action::Divider);
        }
        if text_button(ui, "{ }", "Code block").clicked() {
            action = Some(Action::CodeBlock);
        }
        if text_button(ui, "link", "Link").clicked() {
            action = Some(Action::Link);
        }
        highlights(ui, salt, &mut action);
    });
    action
}

/// The heading picker.
///
/// A menu rather than four buttons: four more controls would push the bar onto
/// a second line in the notebook, and the levels are one choice, not four.
fn headings(ui: &mut Ui, salt: egui::Id, action: &mut Option<Action>) {
    ui.menu_button("H", |ui| {
        for level in 1_u8..=4 {
            let label = egui::RichText::new(format!("Heading {level}"))
                .size(metric::HEADING_SCALE[level as usize - 1] * 13.0)
                .font(theme::bold_font(
                    metric::HEADING_SCALE[level as usize - 1] * 13.0,
                ));
            if ui.button(label).clicked() {
                *action = Some(Action::Block(Block::Heading(level)));
                ui.close();
            }
        }
    })
    .response
    .on_hover_text("Heading level")
    .id
    .with(salt);
}

/// The highlight colour picker.
fn highlights(ui: &mut Ui, salt: egui::Id, action: &mut Option<Action>) {
    // `CloseOnClickOutside`, not egui's default. A menu normally closes on
    // *any* click, inside or out, which is right for a menu of choices and
    // wrong the moment one of them is a field you have to type into: clicking
    // the hex box shut the menu before a single character could be entered.
    // The swatches and Apply still close it themselves, explicitly.
    let config = egui::containers::menu::MenuConfig::new()
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside);
    let (button, _) = egui::containers::menu::MenuButton::new("\u{2022}")
        .config(config)
        .ui(ui, |ui| {
        ui.label(
            egui::RichText::new("HIGHLIGHT")
                .small()
                .color(color::TEXT_FAINT),
        );
        for colour in Palette::variants() {
            let background = theme::mark_color(crate::markdown::HighlightColor::Named(colour));
            let label = egui::RichText::new(colour.name())
                .background_color(background)
                .color(theme::readable_on(background));
            if ui.button(label).clicked() {
                *action = Some(Action::Wrap(Wrap::Highlight(Some(
                    colour.name().to_owned(),
                ))));
                ui.close();
            }
        }
        ui.separator();
        if ui.button("Default").clicked() {
            *action = Some(Action::Wrap(Wrap::Highlight(None)));
            ui.close();
        }

        // The escape hatch, for the colours a palette cannot cover. Typing the
        // markup by hand is always possible; this is for people who would
        // rather not.
        ui.separator();
        let heading = ui.label(egui::RichText::new("HEX").small().color(color::TEXT_FAINT));
        let hex_id = salt.with("hex");
        let mut hex = ui
            .data(|data| data.get_temp::<String>(hex_id))
            .unwrap_or_default();
        let field = ui
            .add(
                egui::TextEdit::singleline(&mut hex)
                    .hint_text("#f2c14e")
                    .desired_width(90.0),
            )
            // A hint is not a name. Without this the box is one unlabelled text
            // input among several on screen, to a screen reader and to a test
            // alike.
            .labelled_by(heading.id);
        if field.changed() {
            ui.data_mut(|data| data.insert_temp(hex_id, hex.clone()));
        }
        let valid = parse_hex(&hex).is_some();
        // Enter counts as Apply, because a field you have typed a value into
        // and then have to go and click a button beside is a field that feels
        // broken.
        //
        // Consumed, not merely observed: the field surrenders focus on Enter,
        // which hands it back to the note, and the same keypress then reached
        // the note as well and inserted a newline into the highlight it had
        // just applied.
        let entered = field.lost_focus()
            && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
        let applied = ui
            .add_enabled(valid, egui::Button::new("Apply"))
            .on_disabled_hover_text("Three or six hex digits, with or without the #")
            .clicked();
        if valid && (applied || entered) {
            *action = Some(Action::Wrap(Wrap::Highlight(Some(format!(
                "#{}",
                hex.trim().trim_start_matches('#')
            )))));
            ui.close();
        }
        });
    button.on_hover_text("Highlight");
}

/// Is this a hex colour the parser would accept?
///
/// Checked here so the Apply button can be greyed out rather than silently
/// producing a highlight in the default colour, which is what an unparseable
/// prefix does.
fn parse_hex(hex: &str) -> Option<()> {
    let digits = hex.trim().trim_start_matches('#');
    let ok = matches!(digits.len(), 3 | 6) && digits.bytes().all(|b| b.is_ascii_hexdigit());
    ok.then_some(())
}

/// How a button's letter is styled.
#[derive(Clone, Copy)]
enum Emphasis {
    Bold,
    Italic,
    Underline,
    Strike,
}

fn styled_button(
    ui: &mut Ui,
    glyph: &str,
    style: Emphasis,
    name: &str,
    keys: &str,
) -> egui::Response {
    let mut text = egui::RichText::new(glyph).color(color::TEXT);
    text = match style {
        Emphasis::Bold => text.font(theme::bold_font(metric::TOOLBAR_TEXT)),
        Emphasis::Italic => text.italics().size(metric::TOOLBAR_TEXT),
        Emphasis::Underline => text.underline().size(metric::TOOLBAR_TEXT),
        Emphasis::Strike => text.strikethrough().size(metric::TOOLBAR_TEXT),
    };
    let response = ui.add(egui::Button::new(text).min_size(button_size()).frame(false));
    let tip = if keys.is_empty() {
        name.to_owned()
    } else {
        format!("{name}  {keys}")
    };
    describe(&response, ui, name);
    response.on_hover_text(tip)
}

fn text_button(ui: &mut Ui, glyph: &str, name: &str) -> egui::Response {
    let response = ui.add(
        egui::Button::new(
            egui::RichText::new(glyph)
                .size(metric::TOOLBAR_TEXT * 0.85)
                .color(color::TEXT_WEAK),
        )
        .min_size(button_size())
        .frame(false),
    );
    describe(&response, ui, name);
    response.on_hover_text(name)
}

/// Names a button for the accessibility tree.
///
/// The visible glyph is the label egui would otherwise expose, and `<>` or `B`
/// says nothing to a screen reader. The name is what the button *does*, which
/// is also what the UI tests look it up by.
fn describe(response: &egui::Response, ui: &Ui, name: &str) {
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), name));
}

/// The icons that have to be drawn rather than typed.
#[derive(Clone, Copy)]
enum Icon {
    Bullet,
    Numbered,
    Task,
    Rule,
}

fn painted_button(ui: &mut Ui, icon: Icon, tip: &str) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(button_size(), egui::Sense::click());
    // A painted button has no text, so it has nothing to expose to the
    // accessibility tree unless it is said explicitly. Without this the button
    // is invisible to a screen reader and to the UI tests alike.
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), tip));
    let painter = ui.painter();
    if response.hovered() {
        painter.rect_filled(rect, metric::ROW_ROUNDING, color::HOVER);
    }
    // Two rows, not three. Three reads as clutter at button size, and there is
    // not enough height to keep the markers from touching.
    match icon {
        Icon::Bullet | Icon::Numbered | Icon::Task => {
            for row in 0..2 {
                let marker = marker_rect(rect, row);
                match icon {
                    Icon::Bullet => super::blocks::bullet(painter, marker),
                    Icon::Numbered => {
                        super::blocks::number(painter, ui, marker, row + 1, color::TEXT_WEAK);
                    }
                    _ => super::blocks::checkbox(painter, marker, row == 0),
                }
                // A short rule beside each marker, so the icon reads as a list
                // rather than as one stray mark.
                painter.hline(
                    marker.right() + 1.5..=rect.right() - 4.0,
                    marker.center().y,
                    egui::Stroke::new(1.0, color::TEXT_FAINT),
                );
            }
        }
        Icon::Rule => super::blocks::rule(painter, rect, rect.left() + 4.0..=rect.right() - 4.0),
    }
    response
}

/// Where one of the two markers in a list icon goes.
fn marker_rect(rect: egui::Rect, row: u32) -> egui::Rect {
    let height = rect.height() * 0.30;
    let gap = rect.height() * 0.16;
    let top = rect.center().y - height - gap / 2.0
        + f32::from(u16::try_from(row).unwrap_or(0)) * (height + gap);
    egui::Rect::from_min_size(
        egui::pos2(rect.left() + 3.0, top),
        egui::vec2(height * 1.4, height),
    )
}

fn button_size() -> egui::Vec2 {
    // Wider than tall: the painted list icons need room for a marker and the
    // rule beside it, and a square button crushed them together.
    egui::vec2(metric::TOOLBAR_BUTTON * 1.15, metric::TOOLBAR_BUTTON)
}

fn separator(ui: &mut Ui) {
    ui.add_space(3.0);
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(1.0, metric::TOOLBAR_BUTTON * 0.6),
        egui::Sense::hover(),
    );
    ui.painter().vline(
        rect.center().x,
        rect.y_range(),
        egui::Stroke::new(1.0, color::DIVIDER),
    );
    ui.add_space(3.0);
}
