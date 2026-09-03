//! The markdown layouter.
//!
//! Asserted here rather than through the window because egui's accessibility
//! tree exposes a `TextEdit` as its raw string and says nothing whatsoever
//! about formatting: a UI test can prove the text is present and nothing about
//! whether it is bold. `layout` is a pure function precisely so this file can
//! ask that question directly.

use eframe::egui::{FontFamily, TextFormat};
use trackcrab::ui::text::{Config, Reveal, layout};
use trackcrab::ui::theme;

/// Wide enough that nothing wraps, so a test is about formatting and not about
/// line breaking.
const WIDE: f32 = 4000.0;

fn cfg() -> Config {
    Config::default()
}

/// Every line showing its markup, which is the state a line is in while the
/// caret sits on it. Most tests here are about how *text* renders, so they run
/// with the delimiters visible; the hiding itself is tested separately below.
fn shown(source: &str) -> Reveal {
    Reveal::At(0..source.len())
}

/// The format covering the first byte of `needle`.
///
/// Looked up by position rather than by section index, because `LayoutJob`
/// merges adjacent sections that share a format. Asserting on section counts
/// would be asserting on that optimisation instead of on the rendering.
fn format_of(source: &str, needle: &str) -> TextFormat {
    let at = source
        .find(needle)
        .unwrap_or_else(|| panic!("{needle:?} is not in {source:?}"));
    let job = layout(source, WIDE, &cfg(), &shown(source));
    job.sections
        .iter()
        .find(|s| s.byte_range.contains(&at.into()))
        .unwrap_or_else(|| panic!("no section covers {needle:?}"))
        .format
        .clone()
}

fn is_bold(format: &TextFormat) -> bool {
    format.font_id.family == FontFamily::Name(theme::BOLD.into())
}

fn is_mono(format: &TextFormat) -> bool {
    format.font_id.family == FontFamily::Monospace
}

// ------------------------------------------------------- the core invariant

#[test]
fn the_job_text_matches_the_source_byte_for_byte() {
    // The invariant everything else stands on. A `TextEdit` maps caret
    // positions through the galley, so a job that dropped or reordered a byte
    // would put the caret in the wrong place and corrupt edits. It is also why
    // hiding markup later has to shrink it rather than leave it out.
    for source in SAMPLES {
        for reveal in [Reveal::Nothing, shown(source)] {
            let job = layout(source, WIDE, &cfg(), &reveal);
            assert_eq!(
                job.text, *source,
                "job text diverged for {source:?} with {reveal:?}"
            );
        }
    }
}

#[test]
fn the_sections_cover_the_text_with_no_gaps() {
    // egui documents this as an invariant of `LayoutJob` and checks it only in
    // debug builds, so it is worth asserting rather than assuming.
    for source in SAMPLES {
        for reveal in [Reveal::Nothing, shown(source)] {
            let job = layout(source, WIDE, &cfg(), &reveal);
            let mut at = 0;
            for section in &job.sections {
                assert_eq!(
                    usize::from(section.byte_range.start),
                    at,
                    "gap or overlap in {source:?} with {reveal:?}"
                );
                at = section.byte_range.end.into();
            }
            assert_eq!(
                at,
                job.text.len(),
                "sections stop short in {source:?} with {reveal:?}"
            );
        }
    }
}

#[test]
fn newlines_survive_including_crlf() {
    // The line ranges from the parser exclude the newline, so the bytes between
    // lines are appended separately. If that slipped, a multi-line note would
    // collapse into one row.
    for source in ["a\nb", "a\r\nb", "\n\n\n", "a\n", "\na"] {
        assert_eq!(layout(source, WIDE, &cfg(), &Reveal::Nothing).text, source);
    }
}

const SAMPLES: &[&str] = &[
    "",
    "\n",
    "plain",
    "# Heading",
    "**bold** *italic* __under__ ~~strike~~ `code`",
    "***all***",
    "==yellow|mark== ==#fff|hex== ==plain==",
    "[label](https://example.com) https://bare.example.com",
    "- one\n  - two\n1. three\n- [x] four",
    "---",
    "```rust\nlet x = *p;\n```",
    "unclosed **bold and `code",
    "escaped \\*star\\*",
    "\u{1f980} emoji and accents \u{e9}",
    "a\r\nb\r\n",
    "  indented",
    "#### four\n##### five",
];

// ------------------------------------------------------------------ emphasis

#[test]
fn bold_uses_the_bold_family() {
    // The whole reason a font is bundled: epaint has no faux bold, so if this
    // is not the bold family then bold is not bold.
    assert!(is_bold(&format_of("**loud**", "loud")));
    assert!(!is_bold(&format_of("**loud** quiet", " quiet")));
}

#[test]
fn italic_is_the_faux_slant_and_needs_no_font() {
    let format = format_of("*lean*", "lean");
    assert!(format.italics);
    assert!(!is_bold(&format), "italic must not silently become bold");
}

#[test]
fn bold_and_italic_compose() {
    let format = format_of("***both***", "both");
    assert!(is_bold(&format) && format.italics);
}

#[test]
fn underline_draws_a_stroke_in_the_text_colour() {
    let format = format_of("__under__", "under");
    assert!(format.underline.width > 0.0);
    assert_eq!(format.underline.color, format.color);
}

#[test]
fn strikethrough_steps_the_text_back() {
    // Struck text is being discarded, so leaving it at full strength with a
    // line through it reads as emphasis rather than deletion.
    let struck = format_of("~~gone~~", "gone");
    let plain = format_of("kept", "kept");
    assert!(struck.strikethrough.width > 0.0);
    assert!(
        theme::relative_luminance(struck.color) < theme::relative_luminance(plain.color),
        "struck text should be dimmer than body text"
    );
}

// ---------------------------------------------------------------------- code

#[test]
fn inline_code_gets_the_monospace_family_and_a_background() {
    let format = format_of("a `snippet` b", "snippet");
    assert!(is_mono(&format));
    assert_eq!(format.background, Config::default().code_bg);
}

#[test]
fn code_wins_the_family_over_bold() {
    // There is no bold monospace face bundled, and a proportional font would
    // defeat the point of marking a run as code.
    let format = format_of("**bold `c` more**", "c");
    assert!(is_mono(&format), "code should stay monospace inside bold");
}

#[test]
fn a_code_block_is_monospace_but_paints_no_background_of_its_own() {
    // The block *decoration* paints one rounded rectangle behind the whole run.
    // A per-character background as well showed through it as a second, squarer
    // shade wherever the two disagreed, so the text layer leaves it alone.
    let source = "```rust\nlet x = 1;\n```";
    let body = format_of(source, "let x = 1;");
    assert!(is_mono(&body));
    assert_eq!(
        body.background,
        eframe::egui::Color32::TRANSPARENT,
        "a line inside a fence should leave its background to the block"
    );

    let fence = format_of(source, "```rust");
    assert!(is_mono(&fence));
    assert_eq!(fence.background, eframe::egui::Color32::TRANSPARENT);
}

#[test]
fn an_inline_code_span_still_paints_its_own_background() {
    // There is no block decoration behind an inline span, so it has to.
    let format = format_of("a `snippet` b", "snippet");
    assert_eq!(format.background, Config::default().code_bg);
}

#[test]
fn markup_inside_a_code_block_is_not_formatted() {
    let source = "```\n**not bold**\n```";
    let format = format_of(source, "**not bold**");
    assert!(!is_bold(&format), "a code block must not be parsed");
}

// ------------------------------------------------------------------ headings

#[test]
fn headings_are_bigger_and_bolder_with_level_one_the_largest() {
    let sizes: Vec<f32> = ["# a", "## a", "### a", "#### a"]
        .iter()
        .map(|s| format_of(s, "a").font_id.size)
        .collect();
    let body = format_of("a", "a").font_id.size;

    assert!(sizes[0] > sizes[1], "h1 should beat h2");
    assert!(sizes[1] > sizes[2], "h2 should beat h3");
    assert!(sizes[2] > sizes[3], "h3 should beat h4");
    assert!(sizes[3] > body, "even h4 should beat body text");
    // Bold as well as bigger: size alone reads as a zoomed paragraph.
    assert!(is_bold(&format_of("# a", "a")));
}

#[test]
fn a_heading_marker_matches_the_size_of_its_heading() {
    // Or the row height jumps as the caret moves in and out of the marker,
    // which looks like the text twitching.
    let source = "# Big";
    let marker = format_of(source, "# ").font_id.size;
    let heading = format_of(source, "Big").font_id.size;
    assert!(
        (marker - heading).abs() < f32::EPSILON,
        "{marker} vs {heading}"
    );
}

#[test]
fn five_hashes_is_body_sized() {
    let five = format_of("##### deep", "deep").font_id.size;
    let body = format_of("deep", "deep").font_id.size;
    assert!((five - body).abs() < f32::EPSILON, "{five} vs {body}");
}

// ---------------------------------------------------------------- highlights

#[test]
fn a_highlight_fills_the_background_and_keeps_its_text_readable() {
    let format = format_of("==marked==", "marked");
    assert_ne!(format.background, eframe::egui::Color32::TRANSPARENT);
    assert!(format.expand_bg > 0.0, "a highlight needs a little padding");
}

#[test]
fn every_named_colour_is_distinct() {
    // Eight names that rendered the same colour would be eight ways to write
    // one highlight.
    let mut seen = std::collections::HashSet::new();
    for colour in trackcrab::markdown::Palette::variants() {
        let source = format!("=={}|x==", colour.name());
        let background = format_of(&source, "x").background;
        assert!(
            seen.insert(background.to_array()),
            "{} duplicates another colour",
            colour.name()
        );
    }
    assert_eq!(seen.len(), 8);
}

#[test]
fn a_hex_highlight_uses_exactly_the_colour_asked_for() {
    let format = format_of("==#f2c14e|x==", "x");
    assert_eq!(format.background.to_array()[..3], [0xf2, 0xc1, 0x4e]);
}

#[test]
fn text_on_a_light_highlight_flips_to_dark() {
    // The reason the text colour is chosen from the background's luminance:
    // nothing stops someone writing `==#ffffff|text==`, and light on white
    // would make their own note unreadable.
    let on_white = format_of("==#ffffff|x==", "x");
    let on_black = format_of("==#000000|x==", "x");
    assert!(
        theme::relative_luminance(on_white.color) < theme::relative_luminance(on_black.color),
        "text should darken on a light highlight and lighten on a dark one"
    );
}

#[test]
fn a_highlight_survives_being_bold() {
    let format = format_of("==blue|**x**==", "x");
    assert!(is_bold(&format));
    assert_ne!(format.background, eframe::egui::Color32::TRANSPARENT);
}

// --------------------------------------------------------------------- links

#[test]
fn links_are_coloured_and_underlined() {
    let format = format_of("[label](https://example.com)", "label");
    assert_eq!(format.color, Config::default().link);
    assert!(format.underline.width > 0.0);
}

#[test]
fn a_bare_url_is_styled_the_same_as_an_explicit_link() {
    let bare = format_of("go https://example.com/x", "https://example.com/x");
    let explicit = format_of("[l](https://example.com)", "l");
    assert_eq!(bare.color, explicit.color);
}

// -------------------------------------------------------------------- markup

#[test]
fn delimiters_are_drawn_quietly_rather_than_hidden() {
    // D2 still shows them. They must be dimmer than the text they wrap, so the
    // eye reads past them, and they must still be there, since D3 collapses
    // them by shrinking rather than by removing.
    let source = "**loud**";
    let marker = format_of(source, "**");
    let text = format_of(source, "loud");
    assert!(
        theme::relative_luminance(marker.color) < theme::relative_luminance(text.color),
        "delimiters should be quieter than their content"
    );
    assert!(
        layout(source, WIDE, &cfg(), &shown(source))
            .text
            .contains("**")
    );
}

#[test]
fn a_block_marker_is_drawn_quietly_too() {
    let source = "- item";
    assert!(
        theme::relative_luminance(format_of(source, "- ").color)
            < theme::relative_luminance(format_of(source, "item").color)
    );
}

// --------------------------------------------------------------------- wrapping

#[test]
fn the_wrap_width_is_passed_through() {
    let job = layout("some text", 123.0, &cfg(), &Reveal::Nothing);
    assert!((job.wrap.max_width - 123.0).abs() < f32::EPSILON);
    assert!(job.break_on_newline, "newlines must start new rows");
}

#[test]
fn the_config_follows_the_interface_scale() {
    // Zoom moves the body size, and headings are multiples of it, so a zoomed
    // heading has to scale with everything else rather than staying put.
    let small = Config {
        size: 10.0,
        ..Config::default()
    };
    let large = Config {
        size: 20.0,
        ..Config::default()
    };
    let size_at = |cfg: &Config| {
        layout("# a", WIDE, cfg, &shown("# a"))
            .sections
            .last()
            .unwrap()
            .format
            .font_id
            .size
    };
    assert!(size_at(&large) > size_at(&small) * 1.9);
}

// ----------------------------------------------- hiding and revealing (D3)

/// The format covering the first byte of `needle`, at a given reveal.
fn format_at(source: &str, needle: &str, reveal: &Reveal) -> TextFormat {
    let at = source
        .find(needle)
        .unwrap_or_else(|| panic!("{needle:?} is not in {source:?}"));
    layout(source, WIDE, &cfg(), reveal)
        .sections
        .iter()
        .find(|s| s.byte_range.contains(&at.into()))
        .unwrap_or_else(|| panic!("no section covers {needle:?}"))
        .format
        .clone()
}

/// Is this format a collapsed marker? Judged by size and transparency rather
/// than by an exported constant, so the test describes the *effect* and would
/// still catch a change of mechanism.
fn is_collapsed(format: &TextFormat) -> bool {
    format.font_id.size < 1.0 && format.color.a() == 0
}

/// Caret sitting at a byte offset, with no selection.
fn caret(at: usize) -> Reveal {
    Reveal::At(at..at)
}

#[test]
fn markup_collapses_when_nothing_is_being_edited() {
    let source = "**bold** and *italic*";
    assert!(is_collapsed(&format_at(source, "**", &Reveal::Nothing)));
    // The text between the delimiters is untouched.
    assert!(!is_collapsed(&format_at(source, "bold", &Reveal::Nothing)));
    assert!(is_bold(&format_at(source, "bold", &Reveal::Nothing)));
}

#[test]
fn the_caret_reveals_the_markup_on_its_own_line_only() {
    // The whole behaviour, in one assertion: editable where you are, clean
    // everywhere else.
    let source = "**one**\n**two**";
    let second = source.find("**two**").expect("second line");

    let on_first = caret(2);
    assert!(!is_collapsed(&format_at(source, "**one", &on_first)));
    assert!(is_collapsed(&format_at(source, "**two", &on_first)));

    let on_second = caret(second + 2);
    assert!(is_collapsed(&format_at(source, "**one", &on_second)));
    assert!(!is_collapsed(&format_at(source, "**two", &on_second)));
}

#[test]
fn a_caret_at_the_end_of_a_line_belongs_to_that_line_alone() {
    // The newline sits between the two, so the ranges cannot both claim it.
    // Getting this wrong reveals two lines at once, which looks like a glitch.
    let source = "**one**\n**two**";
    let end_of_first = source.find('\n').expect("a newline");

    let reveal = caret(end_of_first);
    assert!(!is_collapsed(&format_at(source, "**one", &reveal)));
    assert!(
        is_collapsed(&format_at(source, "**two", &reveal)),
        "the line below should stay collapsed"
    );

    // And the far side of the newline belongs to the second line.
    let reveal = caret(end_of_first + 1);
    assert!(is_collapsed(&format_at(source, "**one", &reveal)));
    assert!(!is_collapsed(&format_at(source, "**two", &reveal)));
}

#[test]
fn a_selection_reveals_every_line_it_touches() {
    let source = "**one**\n**two**\n**three**";
    let third = source.find("**three**").expect("third line");
    let reveal = Reveal::At(2..third + 2);

    for needle in ["**one", "**two", "**three"] {
        assert!(
            !is_collapsed(&format_at(source, needle, &reveal)),
            "{needle} should be revealed by a selection spanning it"
        );
    }
}

#[test]
fn every_block_marker_hides_now_that_something_is_drawn_for_it() {
    // D3 kept list markers, dividers and fences visible because nothing yet
    // drew a replacement. D4 draws all of them, so all of them hide.
    for (source, marker) in [
        ("# Title", "# "),
        ("- item", "- "),
        ("1. item", "1. "),
        ("- [x] item", "- [x] "),
        ("---", "---"),
    ] {
        let format = format_at(source, marker, &Reveal::Nothing);
        assert_eq!(
            format.color.a(),
            0,
            "{marker:?} in {source:?} is still inked"
        );
    }
}

#[test]
fn a_marker_beside_text_loses_its_width_but_a_whole_row_keeps_its_height() {
    // The two collapse modes. A marker sharing a row with text has to lose its
    // width, or hiding it leaves a gap. A marker that *is* the row has to keep
    // its height, or a divider has nowhere to draw and the caret cannot be
    // clicked onto the line.
    for (source, marker) in [("# Title", "# "), ("- item", "- ")] {
        assert!(
            is_collapsed(&format_at(source, marker, &Reveal::Nothing)),
            "{marker:?} should have lost its width"
        );
    }
    for (source, marker) in [("---", "---"), ("```rust", "```rust")] {
        let format = format_at(source, marker, &Reveal::Nothing);
        assert_eq!(format.color.a(), 0, "{marker:?} should be transparent");
        assert!(
            format.font_id.size > 1.0,
            "{marker:?} shrank to {}, so its row has no height left",
            format.font_id.size
        );
    }
}

#[test]
fn a_heading_marker_comes_back_when_the_caret_is_on_it() {
    let source = "# Title";
    assert!(!is_collapsed(&format_at(source, "# ", &caret(1))));
}

#[test]
fn the_highlight_colour_prefix_hides_with_the_delimiters() {
    // `==yellow|` is an instruction, not something to read.
    let source = "==yellow|marked==";
    assert!(is_collapsed(&format_at(
        source,
        "==yellow|",
        &Reveal::Nothing
    )));
    assert!(!is_collapsed(&format_at(
        source,
        "marked",
        &Reveal::Nothing
    )));
}

#[test]
fn a_link_target_hides_leaving_only_the_label() {
    let source = "[label](https://example.com)";
    assert!(is_collapsed(&format_at(source, "[", &Reveal::Nothing)));
    assert!(is_collapsed(&format_at(
        source,
        "](https",
        &Reveal::Nothing
    )));
    let label = format_at(source, "label", &Reveal::Nothing);
    assert!(!is_collapsed(&label));
    assert_eq!(label.color, Config::default().link);
}

#[test]
fn a_collapsed_marker_keeps_its_characters_in_the_job() {
    // The reason collapsing shrinks rather than omits. A TextEdit maps caret
    // positions through the galley, so a missing byte moves the caret.
    let source = "# Title with **bold**";
    let job = layout(source, WIDE, &cfg(), &Reveal::Nothing);
    assert_eq!(job.text, source);
    assert!(job.text.starts_with("# "));
    assert!(job.text.contains("**"));
}

#[test]
fn only_markup_is_ever_collapsed() {
    // Asserted against the parser's own ranges rather than a guessed character
    // class: a link target legitimately contains arbitrary URL characters, so
    // "does it look like markup" is the wrong question. "Did the parser call it
    // markup" is the right one, and it is the same source of truth the renderer
    // reads.
    for source in SAMPLES {
        let doc = trackcrab::markdown::parse(source);
        let mut hideable: Vec<std::ops::Range<usize>> = Vec::new();
        for (line, inline) in doc.rows() {
            // Every block marker is now hideable, and a paragraph's is empty
            // anyway, so the whole set is "markers plus delimiters".
            hideable.push(line.marker.clone());
            hideable.extend(inline.markup.iter().cloned());
        }

        let job = layout(source, WIDE, &cfg(), &Reveal::Nothing);
        for section in &job.sections {
            if section.format.font_id.size >= 1.0 {
                continue;
            }
            let start = usize::from(section.byte_range.start);
            let end = usize::from(section.byte_range.end);
            let text = &job.text[start..end];
            assert!(
                hideable
                    .iter()
                    .any(|range| range.start <= start && end <= range.end),
                "collapsed {text:?} at {start}..{end} in {source:?}, \
                 which the parser did not report as markup"
            );
            assert!(
                !text.contains('\n'),
                "a newline was collapsed in {source:?}, which would merge two rows"
            );
        }
    }
}

// ------------------------------------------------- block indentation (D4)

/// The leading space on the section covering `needle`.
fn leading_at(source: &str, needle: &str, reveal: &Reveal) -> f32 {
    let at = source
        .find(needle)
        .unwrap_or_else(|| panic!("{needle:?} is not in {source:?}"));
    layout(source, WIDE, &cfg(), reveal)
        .sections
        .iter()
        .find(|s| s.byte_range.contains(&at.into()))
        .unwrap_or_else(|| panic!("no section covers {needle:?}"))
        .leading_space
}

#[test]
fn a_hidden_list_marker_reserves_the_gutter_the_painter_draws_in() {
    // The contract between the two halves of D4. The layouter reserves this
    // space and the painter puts the marker in it; if the two numbers ever
    // disagree, the bullet misses its own line. Both read `list_indent`, so
    // this pins the other term.
    use trackcrab::ui::theme::{list_indent, metric};

    let source = "- one\n  - two\n    - three";
    for (needle, depth) in [("one", 0), ("two", 1), ("three", 2)] {
        let space = leading_at(source, needle, &Reveal::Nothing);
        let expected = list_indent(depth) + metric::GUTTER;
        assert!(
            (space - expected).abs() < 0.01,
            "{needle:?} at depth {depth} reserved {space} rather than {expected}"
        );
    }
}

#[test]
fn a_revealed_list_item_keeps_its_depth_but_gives_up_the_gutter() {
    // The raw `- ` occupies roughly the gutter itself while it is showing.
    // Dropping the whole indent instead, which the first attempt did, made a
    // nested item jump about 30px left onto its parent's indent as the caret
    // arrived, so the nesting appeared to collapse.
    use trackcrab::ui::theme::{list_indent, metric};

    let source = "- one\n  - two";
    let at_two = source.find("- two").expect("second item");
    let reveal = Reveal::At(at_two..at_two);

    let space = leading_at(source, "two", &reveal);
    assert!(
        (space - list_indent(1)).abs() < 0.01,
        "a revealed item reserved {space} rather than its depth of {}",
        list_indent(1)
    );
    assert!(
        space > 0.0,
        "the nesting must survive being revealed, or the list looks flat"
    );
    // Its neighbour is untouched, so only the caret's own line moves.
    let neighbour = leading_at(source, "one", &reveal);
    assert!((neighbour - (list_indent(0) + metric::GUTTER)).abs() < 0.01);
}

#[test]
fn nothing_that_is_not_a_list_gets_indented() {
    for (source, needle) in [
        ("# Heading", "Heading"),
        ("plain words", "plain"),
        ("---", "---"),
        ("```rust", "```rust"),
    ] {
        let space = leading_at(source, needle, &Reveal::Nothing);
        assert!(
            space.abs() < 0.01,
            "{source:?} indented {needle:?} by {space}"
        );
    }
}

#[test]
fn an_empty_list_item_still_reserves_its_gutter() {
    // Otherwise the bullet would be drawn against a line that starts at the
    // margin, which is the one case with no content to push out of the way.
    use trackcrab::ui::theme::{list_indent, metric};

    let job = layout("- ", WIDE, &cfg(), &Reveal::Nothing);
    let reserved: f32 = job.sections.iter().map(|s| s.leading_space).sum();
    assert!(
        (reserved - (list_indent(0) + metric::GUTTER)).abs() < 0.01,
        "an empty item reserved {reserved}"
    );
}

#[test]
fn deep_nesting_stops_indenting_rather_than_running_off_the_panel() {
    use trackcrab::ui::theme::{list_indent, metric};

    let source = "                              - very deep";
    let space = leading_at(source, "very deep", &Reveal::Nothing);
    let ceiling = list_indent(metric::MAX_DEPTH) + metric::GUTTER;
    assert!(
        space <= ceiling + 0.01,
        "indented {space}, past the ceiling of {ceiling}"
    );
}
