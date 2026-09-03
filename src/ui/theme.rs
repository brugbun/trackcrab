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

    /// Markdown delimiters, while they are showing. Quiet enough to read past,
    /// visible enough to edit.
    pub const MARKUP: Color32 = Color32::from_rgb(0x5a, 0x62, 0x6e);
    /// Behind inline code and code blocks.
    pub const CODE_BG: Color32 = Color32::from_rgb(0x11, 0x13, 0x17);
    /// Inline code and code block text, a shade off body text so a code run is
    /// identifiable even where the background is subtle.
    pub const CODE_TEXT: Color32 = Color32::from_rgb(0xc8, 0xd8, 0xc0);
    /// Links.
    pub const LINK: Color32 = Color32::from_rgb(0x7c, 0xc7, 0xff);

    /// Highlight backgrounds.
    ///
    /// Muted rather than the vivid marker-pen colours the names suggest: this
    /// is a dark interface, and a saturated yellow behind light text is
    /// unreadable. Each is dark enough to keep body text legible on top, which
    /// `tests/theme.rs` asserts as a contrast ratio rather than by eye.
    pub mod mark {
        use eframe::egui::Color32;

        pub const DEFAULT: Color32 = Color32::from_rgb(0x4a, 0x42, 0x1c);
        pub const YELLOW: Color32 = Color32::from_rgb(0x4a, 0x42, 0x1c);
        pub const GREEN: Color32 = Color32::from_rgb(0x1e, 0x42, 0x2c);
        pub const BLUE: Color32 = Color32::from_rgb(0x1c, 0x36, 0x52);
        pub const PINK: Color32 = Color32::from_rgb(0x4c, 0x24, 0x3c);
        pub const PURPLE: Color32 = Color32::from_rgb(0x35, 0x28, 0x52);
        pub const ORANGE: Color32 = Color32::from_rgb(0x50, 0x33, 0x18);
        pub const RED: Color32 = Color32::from_rgb(0x4e, 0x22, 0x22);
        pub const GREY: Color32 = Color32::from_rgb(0x35, 0x3b, 0x44);
    }
    /// Ring drawn around every status dot. On the bright statuses it disappears
    /// into the fill; on the near black Blocked dot it is the only thing that
    /// makes the shape locatable against a dark panel.
    pub const DOT_RING: Color32 = Color32::from_rgb(0x5c, 0x64, 0x70);
    pub const DANGER: Color32 = Color32::from_rgb(0xe8, 0x4c, 0x4c);
}

/// Sizes that several views need to agree on.
pub mod metric {
    /// Heading sizes, as a multiple of body text, for levels 1 to 4.
    ///
    /// Tightly spaced on purpose. The notes panel is narrow, so an H1 at twice
    /// body size wraps after three words; these stay distinguishable without
    /// taking the whole width.
    pub const HEADING_SCALE: [f32; 4] = [1.60, 1.38, 1.20, 1.08];
    /// Padding added around a highlight's background fill.
    pub const MARK_EXPAND: f32 = 1.5;
    /// Width reserved to the left of a list item for its drawn marker.
    ///
    /// A fixed gutter rather than the width of the markup it replaces, so a
    /// bullet, a `10.` and a checkbox all line up in the same column. Markdown's
    /// own markers are different widths, which would leave the text ragged.
    pub const GUTTER: f32 = 22.0;
    /// Horizontal step per level of list nesting.
    ///
    /// Narrower than the sidebar's indent: the notes panel is much narrower
    /// than the window, and three levels at 24px eats a third of it.
    pub const LIST_INDENT: f32 = 16.0;
    /// Deepest list nesting that still indents.
    ///
    /// The same ceiling the editor enforces on the text itself, so the drawn
    /// indent and the document cannot disagree about how deep is too deep.
    pub const MAX_DEPTH: usize = crate::markdown::MAX_DEPTH;
    /// Radius of a bullet dot.
    pub const BULLET_RADIUS: f32 = 2.6;
    /// Side length of a checkbox.
    pub const CHECKBOX: f32 = 12.0;
    /// Thickness of a divider rule.
    pub const RULE_WIDTH: f32 = 1.0;
    /// Corner radius on a code block's background.
    pub const CODE_ROUNDING: u8 = 4;
    /// Padding added around a code block's background.
    pub const CODE_PAD: f32 = 4.0;
    /// Size of the language tag on a code block.
    pub const CODE_TAG: f32 = 11.0;
    /// Side of a formatting toolbar button.
    pub const TOOLBAR_BUTTON: f32 = 24.0;
    /// Text size inside a toolbar button.
    pub const TOOLBAR_TEXT: f32 = 15.0;
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
    ctx.set_fonts(fonts());
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

/// The family name of the markdown body face, for [`egui::FontFamily::Name`].
///
/// Deliberately **not** `FontFamily::Proportional`. The panel chrome keeps
/// egui's Ubuntu-Light, which is what the app has always looked like, and the
/// bundled family is used only inside the markdown fields. A UI font distinct
/// from a content font is an ordinary typographic distinction, and it means
/// adding a bold weight did not have to restyle the whole interface.
///
/// There is no mixing *within* a run of text: regular and bold both come from
/// Work Sans, so a bold word sits in the same family as the sentence round it.
pub const BODY: &str = "trackcrab-body";

/// The family name of the bold face, for [`egui::FontFamily::Name`].
///
/// A separate family rather than a weight on the existing one, because that is
/// the only handle egui gives a `TextFormat`: a `FontId` names a family and a
/// size, and nothing else.
pub const BOLD: &str = "trackcrab-bold";

/// The bundled font set.
///
/// egui ships **Ubuntu-Light and nothing else** for proportional text: no bold
/// face, and no faux bold anywhere in epaint either, so `**bold**` is
/// unreachable without adding a font. Work Sans is bundled at Regular and Bold
/// (SIL Open Font License, see `assets/fonts/OFL.txt`): humanist like Ubuntu, so
/// it is the smallest visual change that buys a real bold, and small enough that
/// two faces cost under 400KB.
///
/// Italic is *not* bundled. `TextFormat::italics` is a real faux slant applied
/// in the tessellator, so it costs nothing. Real italic faces exist in the same
/// family and can be dropped in later if the slant disappoints; that would mean
/// two more families here and two more arms in the lookup, and nothing else.
fn fonts() -> egui::FontDefinitions {
    use egui::FontFamily;

    let mut fonts = egui::FontDefinitions::default();
    for (name, bytes) in [
        (
            "WorkSans",
            &include_bytes!("../../assets/fonts/WorkSans-Regular.ttf")[..],
        ),
        (
            "WorkSans-Bold",
            &include_bytes!("../../assets/fonts/WorkSans-Bold.ttf")[..],
        ),
    ] {
        fonts.font_data.insert(
            name.to_owned(),
            std::sync::Arc::new(egui::FontData::from_static(bytes)),
        );
    }

    // `Proportional` is left exactly as egui built it, so every label, button
    // and heading in the app keeps the Ubuntu-Light look it has always had. The
    // two new families are additions, reached only from the markdown layouter.
    //
    // Both chains keep the existing fallbacks behind them, which matters for
    // more than coverage: the burger is U+2630, which lives in
    // `emoji-icon-font` and nowhere else bundled.
    let fallbacks: Vec<String> = fonts
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();

    for (family, face) in [(BODY, "WorkSans"), (BOLD, "WorkSans-Bold")] {
        let mut chain = vec![face.to_owned()];
        chain.extend(fallbacks.iter().cloned());
        fonts
            .families
            .insert(FontFamily::Name(family.into()), chain);
    }

    fonts
}

/// Horizontal offset for a list item at a given nesting depth.
///
/// One function for both users: the layouter reserves this much space before the
/// text, and the painter puts the marker in it. If the two computed it
/// separately they could drift, and a bullet that does not line up with its own
/// text is worse than no bullet at all.
#[must_use]
pub fn list_indent(depth: usize) -> f32 {
    let depth = u8::try_from(depth.min(metric::MAX_DEPTH)).unwrap_or(u8::MAX);
    f32::from(depth) * metric::LIST_INDENT
}

/// A `FontId` in the markdown body family.
#[must_use]
pub fn body_font(size: f32) -> egui::FontId {
    egui::FontId::new(size, egui::FontFamily::Name(BODY.into()))
}

/// A `FontId` in the bold family.
#[must_use]
pub fn bold_font(size: f32) -> egui::FontId {
    egui::FontId::new(size, egui::FontFamily::Name(BOLD.into()))
}

/// Body text or dark text, whichever is readable on `background`.
///
/// Needed because a highlight colour can be **any hex the user types**. The
/// named palette is tuned to keep light text legible, but nothing stops someone
/// writing `==#ffffff|text==`, and light-on-white would make their own note
/// unreadable. Choosing by the background's luminance means every colour works,
/// including ones nobody anticipated.
#[must_use]
pub fn readable_on(background: Color32) -> Color32 {
    // Whichever of the two actually wins on contrast, rather than a luminance
    // threshold. A threshold has to guess where the crossover is, and it guesses
    // wrong in the middle of the range: a mid pink like #d287c3 sits just under
    // any sensible cutoff and so got light text, when dark text reads better on
    // it. Measuring both costs nothing and cannot be wrong.
    if contrast(color::CANVAS, background) >= contrast(color::TEXT, background) {
        color::CANVAS
    } else {
        color::TEXT
    }
}

/// WCAG contrast ratio, 1.0 for identical colours up to 21.0 for black on white.
#[must_use]
pub fn contrast(a: Color32, b: Color32) -> f32 {
    let (x, y) = (relative_luminance(a), relative_luminance(b));
    let (hi, lo) = if x > y { (x, y) } else { (y, x) };
    (hi + 0.05) / (lo + 0.05)
}

/// WCAG relative luminance, the same computation `tests/theme.rs` asserts
/// contrast ratios with.
#[must_use]
pub fn relative_luminance(colour: Color32) -> f32 {
    let channel = |v: u8| {
        let v = f32::from(v) / 255.0;
        if v <= 0.039_28 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(colour.r()) + 0.7152 * channel(colour.g()) + 0.0722 * channel(colour.b())
}

/// The background a highlight is drawn in.
#[must_use]
pub fn mark_color(colour: crate::markdown::HighlightColor) -> Color32 {
    use crate::markdown::{HighlightColor, Palette};
    match colour {
        HighlightColor::Default => color::mark::DEFAULT,
        HighlightColor::Rgb([r, g, b]) => Color32::from_rgb(r, g, b),
        HighlightColor::Named(name) => match name {
            Palette::Yellow => color::mark::YELLOW,
            Palette::Green => color::mark::GREEN,
            Palette::Blue => color::mark::BLUE,
            Palette::Pink => color::mark::PINK,
            Palette::Purple => color::mark::PURPLE,
            Palette::Orange => color::mark::ORANGE,
            Palette::Red => color::mark::RED,
            Palette::Grey => color::mark::GREY,
        },
    }
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
