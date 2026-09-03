//! Design invariants.
//!
//! These guard the properties that made the near black Blocked dot invisible in
//! the first place, so a future palette tweak cannot quietly reintroduce it.

use eframe::egui::Color32;
use trackcrab::model::Status;
use trackcrab::ui::theme::{color, status_color};

/// Relative luminance, per the WCAG definition.
fn luminance(c: Color32) -> f64 {
    fn channel(v: u8) -> f64 {
        let v = f64::from(v) / 255.0;
        if v <= 0.039_28 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    }
    0.2126 * channel(c.r()) + 0.7152 * channel(c.g()) + 0.0722 * channel(c.b())
}

/// WCAG contrast ratio, 1.0 (identical) to 21.0 (black on white).
fn contrast(a: Color32, b: Color32) -> f64 {
    let (x, y) = (luminance(a), luminance(b));
    let (hi, lo) = if x > y { (x, y) } else { (y, x) };
    (hi + 0.05) / (lo + 0.05)
}

#[test]
fn the_five_status_colours_are_exactly_as_specified() {
    assert_eq!(
        status_color(&Status::Open),
        Color32::from_rgb(109, 190, 255)
    );
    assert_eq!(
        status_color(&Status::InProgress),
        Color32::from_rgb(240, 200, 60)
    );
    assert_eq!(
        status_color(&Status::Completed),
        Color32::from_rgb(110, 224, 170)
    );
    assert_eq!(
        status_color(&Status::Blocked(String::new())),
        Color32::from_rgb(58, 62, 68)
    );
    assert_eq!(
        status_color(&Status::Cancelled),
        Color32::from_rgb(232, 76, 76)
    );
}

#[test]
fn no_two_statuses_share_a_colour() {
    let colours: Vec<Color32> = Status::variants().iter().map(status_color).collect();
    for (i, a) in colours.iter().enumerate() {
        for b in &colours[i + 1..] {
            assert_ne!(a, b, "two statuses cannot share a colour");
        }
    }
}

#[test]
fn every_status_dot_is_locatable_against_the_panel() {
    // Blocked is deliberately near black, so its *fill* has almost no contrast
    // with the panel. That is the requested look, and it is exactly why the dot
    // is drawn with a ring. The ring is what has to be visible.
    let blocked = status_color(&Status::Blocked(String::new()));
    let blocked_contrast = contrast(blocked, color::PANEL);
    assert!(
        blocked_contrast < 2.0,
        "Blocked is meant to be near black against the panel, but reads at {blocked_contrast:.2}"
    );
    // And it is meant to be the quietest of the five, by a clear margin.
    for variant in Status::variants() {
        if variant.is_blocked() {
            continue;
        }
        assert!(
            contrast(status_color(&variant), color::PANEL) > blocked_contrast * 2.0,
            "{} should be far louder than Blocked",
            variant.label()
        );
    }

    for variant in Status::variants() {
        let fill = status_color(&variant);
        let best = contrast(fill, color::PANEL).max(contrast(color::DOT_RING, color::PANEL));
        assert!(
            best >= 1.6,
            "the {} dot would be invisible against the panel (best contrast {best:.2})",
            variant.label()
        );
    }
}

#[test]
fn body_text_meets_a_readable_contrast_against_both_surfaces() {
    for surface in [color::PANEL, color::CANVAS] {
        let ratio = contrast(color::TEXT, surface);
        assert!(
            ratio >= 7.0,
            "body text contrast is only {ratio:.1}, aim for 7 or better"
        );
    }
    // The muted tiers are allowed to be quieter, but still legible.
    assert!(contrast(color::TEXT_WEAK, color::PANEL) >= 3.5);
    assert!(contrast(color::TEXT_FAINT, color::PANEL) >= 2.0);
}

#[test]
fn the_divider_is_visible_without_being_loud() {
    let ratio = contrast(color::DIVIDER, color::PANEL);
    assert!(
        ratio > 1.15,
        "the divider is too close to the panel to be seen ({ratio:.3})"
    );
    assert!(
        ratio < 2.0,
        "the divider is louder than a divider should be ({ratio:.3})"
    );
}

#[test]
fn a_hovered_row_is_distinguishable_from_a_selected_one() {
    assert!(
        contrast(color::HOVER, color::SELECTED) > 1.05,
        "hover and selection look the same"
    );
    assert!(contrast(color::HOVER, color::PANEL) > 1.02);
}

// -------------------------------------------------------- markdown (D2)

/// A context with the app's fonts actually installed.
fn context() -> eframe::egui::Context {
    let ctx = eframe::egui::Context::default();
    trackcrab::ui::theme::install(&ctx);
    // Fonts are applied at the start of the next pass, so one pass has to run
    // before anything can be measured. The output has to be consumed rather
    // than dropped: epaint panics on an unapplied texture delta, which is a
    // real guard against a renderer silently losing the font atlas.
    let mut output = ctx.run_ui(eframe::egui::RawInput::default(), |_| {});
    output.textures_delta.clear();
    ctx
}

/// Width of a string in a given family, in points.
fn width(ctx: &eframe::egui::Context, text: &str, family: eframe::egui::FontFamily) -> f32 {
    let font = eframe::egui::FontId::new(20.0, family);
    ctx.fonts_mut(|fonts| {
        fonts
            .layout_no_wrap(text.to_owned(), font, Color32::WHITE)
            .rect
            .width()
    })
}

#[test]
fn the_bold_family_is_registered_and_is_a_genuinely_different_face() {
    // The layout tests can only prove the layouter *asks* for the bold family.
    // If the registration were broken, bold would silently fall back and those
    // tests would still pass. A different advance width is proof a different
    // face is actually being used, which is the whole point of bundling a font:
    // epaint has no faux bold, so a fallback means no bold at all.
    let ctx = context();
    let bold = eframe::egui::FontFamily::Name(trackcrab::ui::theme::BOLD.into());
    let sample = "The quick brown fox jumps over the lazy dog";

    let regular_width = width(&ctx, sample, eframe::egui::FontFamily::Proportional);
    let bold_width = width(&ctx, sample, bold);

    assert!(
        regular_width > 0.0 && bold_width > 0.0,
        "neither family laid out"
    );
    assert!(
        (bold_width - regular_width).abs() > 1.0,
        "bold and regular measured the same ({regular_width:.1} vs {bold_width:.1}), \
         so the bold face is not installed and bold text is not bold"
    );
    assert!(
        bold_width > regular_width,
        "the bold face should be wider, not narrower"
    );
}

#[test]
fn the_burger_glyph_survives_the_new_font_chain() {
    // Work Sans goes in front of Ubuntu-Light, and U+2630 lives only in
    // `emoji-icon-font`. If inserting a font at the head of the chain dropped
    // the emoji fallbacks, the burger would go back to being a missing-glyph
    // box, which is exactly the bug M6 spent time on.
    let ctx = context();
    let burger = width(
        &ctx,
        trackcrab::app::BURGER,
        eframe::egui::FontFamily::Proportional,
    );
    let replacement = width(&ctx, "\u{fffd}", eframe::egui::FontFamily::Proportional);
    assert!(burger > 0.0, "the burger did not lay out at all");
    assert!(
        (burger - replacement).abs() > 0.5,
        "the burger is measuring the same as the replacement character, \
         so it is rendering as a missing-glyph box"
    );
}

#[test]
fn body_text_stays_readable_on_every_highlight_colour() {
    // Highlights are the one place a background sits directly behind body text.
    // A saturated marker-pen yellow would be unreadable on a dark interface,
    // which is why the palette is muted; this is what stops it drifting back.
    use trackcrab::markdown::{HighlightColor, Palette};
    use trackcrab::ui::theme::{mark_color, readable_on};

    let mut colours = vec![HighlightColor::Default];
    colours.extend(Palette::variants().map(HighlightColor::Named));

    for colour in colours {
        let background = mark_color(colour);
        let text = readable_on(background);
        let ratio = contrast(text, background);
        assert!(
            ratio >= 4.5,
            "{colour:?} gives a contrast of only {ratio:.2}:1 against its text"
        );
    }
}

#[test]
fn an_arbitrary_hex_highlight_still_gets_readable_text() {
    // The palette is tuned, but nothing stops someone writing any hex they
    // like. Picking the text colour from the background's luminance has to hold
    // up across the whole range, including the awkward middle.
    use trackcrab::ui::theme::readable_on;

    let mut worst = (f64::MAX, Color32::BLACK);
    for r in (0..=255).step_by(15) {
        for g in (0..=255).step_by(15) {
            for b in (0..=255).step_by(15) {
                let background = Color32::from_rgb(r, g, b);
                let ratio = contrast(readable_on(background), background);
                if ratio < worst.0 {
                    worst = (ratio, background);
                }
            }
        }
    }
    // Not the 4.5 the palette meets: with only two text colours to choose
    // between, a mid-tone background cannot reach that, so this asserts the
    // best available choice is being made rather than an impossible one. The
    // true worst case over the whole RGB cube is 3.75:1, which clears WCAG AA
    // for large text; the margin here is deliberately thin so that regressing
    // to a luminance threshold, which scores 2.04:1, fails immediately.
    assert!(
        worst.0 >= 3.5,
        "the worst hex is {:?} at only {:.2}:1",
        worst.1,
        worst.0
    );
}

#[test]
fn code_and_markup_colours_are_readable_where_they_are_used() {
    // Code text sits on its own darker background, and delimiters sit on the
    // panel. Both are deliberately quieter than body text, so they get a lower
    // bar, but not so quiet as to be invisible.
    assert!(
        contrast(color::CODE_TEXT, color::CODE_BG) >= 4.5,
        "code text is not readable on the code background"
    );
    let markup = contrast(color::MARKUP, color::PANEL);
    assert!(
        (1.7..4.5).contains(&markup),
        "delimiters should be visible but quiet, got {markup:.2}:1"
    );
    assert!(
        contrast(color::LINK, color::PANEL) >= 4.5,
        "links are not readable on the panel"
    );
}

#[test]
fn collapsed_markup_takes_up_no_width() {
    // The mechanism D3 rests on, measured rather than assumed. A collapsed
    // delimiter has to be invisible *and* occupy no space, or hiding it would
    // leave gaps and indent every heading.
    let ctx = context();
    let cfg = trackcrab::ui::text::Config::default();
    let source = "**bold**";

    let hidden =
        trackcrab::ui::text::layout(source, 4000.0, &cfg, &trackcrab::ui::text::Reveal::Nothing);
    let shown = trackcrab::ui::text::layout(
        source,
        4000.0,
        &cfg,
        &trackcrab::ui::text::Reveal::At(0..source.len()),
    );
    let bare =
        trackcrab::ui::text::layout("bold", 4000.0, &cfg, &trackcrab::ui::text::Reveal::Nothing);

    let width = |job| ctx.fonts_mut(|f| f.layout_job(job)).rect.width();
    let (hidden, shown, bare) = (width(hidden), width(shown), width(bare));

    assert!(
        (hidden - bare).abs() < 1.0,
        "hiding the delimiters should leave the same width as not typing them: \
         {hidden:.2} against {bare:.2}"
    );
    assert!(
        shown > hidden + 4.0,
        "showing them should visibly widen the line: {shown:.2} against {hidden:.2}"
    );
}

#[test]
fn hiding_markup_does_not_change_the_row_height() {
    // Row height is the max over the row, so a tiny section must not drag it
    // down, and revealing must not push it up. Either would make the line jump
    // as the caret arrives.
    let ctx = context();
    let cfg = trackcrab::ui::text::Config::default();
    let source = "# Heading with **bold**";

    let height = |reveal| {
        let job = trackcrab::ui::text::layout(source, 4000.0, &cfg, reveal);
        ctx.fonts_mut(|f| f.layout_job(job)).rect.height()
    };
    let hidden = height(&trackcrab::ui::text::Reveal::Nothing);
    let shown = height(&trackcrab::ui::text::Reveal::At(0..source.len()));
    assert!(
        (hidden - shown).abs() < 0.5,
        "the row height moved from {hidden:.2} to {shown:.2} when the markup appeared"
    );
}

#[test]
fn the_markdown_body_face_is_not_the_chrome_face() {
    // Option B: the panel chrome keeps Ubuntu-Light and only the markdown
    // fields use the bundled family. If the two families measured the same, the
    // separation would not be in effect and the whole app would have changed
    // font after all.
    let ctx = context();
    let body = eframe::egui::FontFamily::Name(trackcrab::ui::theme::BODY.into());
    let sample = "The quick brown fox jumps over the lazy dog";

    let chrome = width(&ctx, sample, eframe::egui::FontFamily::Proportional);
    let content = width(&ctx, sample, body.clone());
    assert!(chrome > 0.0 && content > 0.0);
    assert!(
        (chrome - content).abs() > 1.0,
        "the chrome and content faces measured the same ({chrome:.1} vs {content:.1}), \
         so option B is not actually wired"
    );

    // Asserted through `layout`, not just on the raw families. Checking the
    // families alone let a real bug through: plain markdown text was still
    // being laid out in the chrome face while only bold used the bundled one,
    // so a bold word sat in a different family from the sentence around it.
    let cfg = trackcrab::ui::text::Config::default();
    for source in ["plain words", "**bold words**", "# heading words"] {
        let job = trackcrab::ui::text::layout(
            source,
            4000.0,
            &cfg,
            &trackcrab::ui::text::Reveal::Nothing,
        );
        for section in &job.sections {
            // Collapsed markup keeps whatever family it had; only real text
            // matters here.
            if section.format.font_id.size < 1.0 {
                continue;
            }
            let family = &section.format.font_id.family;
            assert!(
                *family == body
                    || *family == eframe::egui::FontFamily::Name(trackcrab::ui::theme::BOLD.into())
                    || *family == eframe::egui::FontFamily::Monospace,
                "{source:?} laid out a run in {family:?}, which is the chrome face"
            );
        }
    }
}
