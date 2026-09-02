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
