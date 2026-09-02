//! Colour tokens, row metrics, and the base style.
//!
//! Everything visual lives here so the views stay about layout and behaviour.
//! The model deals in plain RGB, this module is the only place it becomes an
//! `egui::Color32`.

use eframe::egui::{self, Color32};

use crate::model::Status;

/// Surfaces and text.
pub mod color {
    use eframe::egui::Color32;

    pub const CANVAS: Color32 = Color32::from_rgb(0x14, 0x16, 0x1a);
    pub const PANEL: Color32 = Color32::from_rgb(0x1a, 0x1d, 0x22);
    pub const RAISED: Color32 = Color32::from_rgb(0x21, 0x25, 0x2b);

    pub const TEXT: Color32 = Color32::from_rgb(0xdf, 0xe3, 0xe8);
    pub const TEXT_WEAK: Color32 = Color32::from_rgb(0x8b, 0x93, 0xa0);
    pub const TEXT_FAINT: Color32 = Color32::from_rgb(0x64, 0x6b, 0x77);

    pub const DIVIDER: Color32 = Color32::from_rgb(0x33, 0x39, 0x42);

    /// Indent guides in the sidebar, in three tiers.
    ///
    /// The guide belonging to the open item's immediate parent is bright, every
    /// ancestor above it is a middle tier, and unrelated branches stay quiet.
    /// That makes the path from the top level down to where you are readable at
    /// a glance in a deep tree.
    pub const GUIDE: Color32 = Color32::from_rgb(0x2c, 0x31, 0x39);
    pub const GUIDE_ANCESTOR: Color32 = Color32::from_rgb(0x6b, 0x76, 0x86);
    pub const GUIDE_ACTIVE: Color32 = Color32::from_rgb(0xf2, 0xf5, 0xf8);

    /// A row that will accept the thing being dragged.
    pub const DROP_OK: Color32 = Color32::from_rgb(0x22, 0x44, 0x38);
    pub const DROP_OK_EDGE: Color32 = Color32::from_rgb(0x6e, 0xe0, 0xaa);
    /// A row that will refuse it, so the cursor is not a guessing game.
    pub const DROP_BAD: Color32 = Color32::from_rgb(0x40, 0x24, 0x26);
    pub const DROP_BAD_EDGE: Color32 = Color32::from_rgb(0xe8, 0x4c, 0x4c);
    pub const HOVER: Color32 = Color32::from_rgb(0x23, 0x27, 0x2e);
    pub const SELECTED: Color32 = Color32::from_rgb(0x2d, 0x35, 0x3f);

    pub const ACCENT: Color32 = Color32::from_rgb(0x6d, 0xbe, 0xff);
    /// Ring drawn around every status dot. On the bright statuses it disappears
    /// into the fill; on the near black Blocked dot it is the only thing that
    /// makes the shape locatable against a dark panel.
    pub const DOT_RING: Color32 = Color32::from_rgb(0x5c, 0x64, 0x70);
    pub const DANGER: Color32 = Color32::from_rgb(0xe8, 0x4c, 0x4c);
}

/// Sizes that several views need to agree on.
pub mod metric {
    /// Height of one row in the sidebar or the folder listing.
    pub const ROW_HEIGHT: f32 = 30.0;
    /// Radius of the status dot.
    pub const DOT_RADIUS: f32 = 4.5;
    /// Width of the ring around a status dot.
    pub const DOT_RING_WIDTH: f32 = 1.0;
    /// Width reserved for the dot column, so titles line up whether or not a
    /// row has a dot.
    pub const DOT_COLUMN: f32 = 16.0;
    /// Corner radius on row highlights.
    pub const ROW_ROUNDING: f32 = 4.0;
    /// Padding either side of a row highlight.
    pub const ROW_PAD_X: f32 = 6.0;
    /// Extra breathing room at the right of a listing row, so the meta columns
    /// never sit against the window edge or under a scrollbar.
    pub const ROW_PAD_RIGHT: f32 = 30.0;
    /// Clear space kept between a blocked reason and the time column, so the
    /// two never read as one run of text.
    pub const REASON_CLEARANCE: f32 = 40.0;
    /// A reason narrower than this is not worth showing at all.
    pub const REASON_MIN: f32 = 48.0;
    /// Indent per tree level. egui's collapse arrow occupies exactly this width
    /// immediately left of the header content, which is what lets the UI tests
    /// locate the arrow from this constant rather than a magic offset.
    pub const INDENT: f32 = 24.0;
    /// Thickness of an indent guide.
    pub const GUIDE_WIDTH: f32 = 2.0;
    /// The collapse triangle. Bigger than egui's default, which is a fiddly
    /// target in a dense tree.
    pub const COLLAPSE_ICON: f32 = 18.0;
    /// Horizontal gap between widgets in a row.
    pub const ITEM_SPACING_X: f32 = 6.0;
    pub const ITEM_SPACING_Y: f32 = 4.0;

    /// Width of a modal dialog. Wide enough that the title and description
    /// fields are comfortable to type into rather than a slot.
    pub const DIALOG_WIDTH: f32 = 560.0;
    /// Dialogs run their text a step larger than the surrounding app, since they
    /// are the moments you are reading carefully rather than scanning.
    pub const DIALOG_TEXT_SCALE: f32 = 1.12;

    /// The sidebar's "FOLDERS" heading. Deliberately double the small text size.
    pub const SIDEBAR_HEADER: f32 = 24.0;
    /// The sidebar's new folder button.
    pub const SIDEBAR_PLUS: f32 = 26.0;
    pub const SIDEBAR_PLUS_HIT: f32 = 30.0;

    /// The comments overlay, as a fraction of the content width.
    pub const COMMENTS_FRACTION: f32 = 0.38;
    pub const COMMENTS_MIN: f32 = 340.0;
    pub const COMMENTS_MAX: f32 = 680.0;

    /// Sidebar width as a fraction of window width, until the user resizes it.
    pub const SIDEBAR_FRACTION: f32 = 0.125;
    // Raised alongside the larger type and indent. Below this a nested name has
    // almost nothing left to show, and the 2D scroll becomes the only way to
    // read the tree rather than a fallback.
    pub const SIDEBAR_MIN: f32 = 210.0;
    pub const SIDEBAR_MAX: f32 = 640.0;
}

#[must_use]
pub fn status_color(status: &Status) -> Color32 {
    let (r, g, b) = status.rgb();
    Color32::from_rgb(r, g, b)
}

/// Applies the base style.
///
/// `all_styles_mut` covers both the light and dark style slots, so nothing
/// changes if the OS theme flips underneath us.
pub fn install(ctx: &egui::Context) {
    ctx.all_styles_mut(|style| {
        style.visuals.dark_mode = true;
        style.visuals.panel_fill = color::PANEL;
        style.visuals.window_fill = color::RAISED;
        style.visuals.extreme_bg_color = color::CANVAS;
        style.visuals.override_text_color = Some(color::TEXT);

        style.visuals.widgets.noninteractive.bg_stroke.color = color::DIVIDER;
        style.visuals.widgets.inactive.weak_bg_fill = color::RAISED;
        style.visuals.widgets.hovered.weak_bg_fill = color::HOVER;
        style.visuals.widgets.active.weak_bg_fill = color::SELECTED;
        style.visuals.selection.bg_fill = color::SELECTED;
        style.visuals.selection.stroke.color = color::ACCENT;

        style.visuals.window_corner_radius = 8.into();
        style.visuals.menu_corner_radius = 6.into();

        // One deliberate type scale rather than egui's defaults, so headings,
        // body and the small meta text stay in proportion everywhere.
        style.text_styles = [
            (egui::TextStyle::Heading, egui::FontId::proportional(21.0)),
            (egui::TextStyle::Body, egui::FontId::proportional(16.5)),
            (egui::TextStyle::Button, egui::FontId::proportional(16.0)),
            (egui::TextStyle::Small, egui::FontId::proportional(14.0)),
            (egui::TextStyle::Monospace, egui::FontId::monospace(12.5)),
        ]
        .into();

        // Thin, unobtrusive scrollbars. The sidebar has two of them.
        style.spacing.scroll.bar_width = 7.0;
        style.spacing.scroll.bar_inner_margin = 2.0;
        style.spacing.scroll.bar_outer_margin = 1.0;
        style.spacing.scroll.handle_min_length = 24.0;
        style.spacing.item_spacing = egui::vec2(metric::ITEM_SPACING_X, metric::ITEM_SPACING_Y);
        style.spacing.button_padding = egui::vec2(8.0, 4.0);
        style.spacing.interact_size.y = metric::ROW_HEIGHT;
        style.spacing.indent = metric::INDENT;
        // A bigger collapse triangle. The default is a fiddly target.
        style.spacing.icon_width = metric::COLLAPSE_ICON;
        style.spacing.icon_width_inner = metric::COLLAPSE_ICON * 0.55;
    });
}

/// The dialog width to actually use, which is the preferred width unless the
/// window is too narrow to hold it.
#[must_use]
pub fn dialog_width(ctx: &egui::Context) -> f32 {
    let available = ctx.viewport_rect().width() - 48.0;
    metric::DIALOG_WIDTH.min(available.max(240.0))
}

/// Multiplies every text size in this `Ui` and its children.
///
/// Used by the dialogs so they read a step larger than the app behind them,
/// without maintaining a second copy of the type scale.
pub fn scale_text(ui: &mut egui::Ui, factor: f32) {
    let style = ui.style_mut();
    for font in style.text_styles.values_mut() {
        font.size *= factor;
    }
    style.spacing.interact_size.y *= factor;
    style.spacing.button_padding *= factor;
}
