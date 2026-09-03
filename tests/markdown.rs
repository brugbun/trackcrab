//! The markdown parser.
//!
//! Headless and exhaustive on purpose: every rendering decision downstream is
//! driven by these ranges, so a parse bug shows up as text that will not format
//! rather than as a crash. Far cheaper to pin down here than through a window.

use trackcrab::markdown::{
    Document, HighlightColor, Inline, LineKind, Palette, Style, line, parse, plain, spans,
};

// ------------------------------------------------------------------ helpers

/// The classified kinds of every line, for asserting shape at a glance.
fn kinds(text: &str) -> Vec<LineKind> {
    line::lines(text).into_iter().map(|l| l.kind).collect()
}

/// The text of each line's marker, so an assertion says what is hidden.
fn markers(text: &str) -> Vec<&str> {
    line::lines(text)
        .into_iter()
        .map(|l| &text[l.marker])
        .collect()
}

/// The text of each line's content.
fn contents(text: &str) -> Vec<&str> {
    line::lines(text)
        .into_iter()
        .map(|l| &text[l.content])
        .collect()
}

fn depths(text: &str) -> Vec<usize> {
    line::lines(text).into_iter().map(|l| l.depth).collect()
}

/// Inline parse of a whole single-line string.
fn inline(text: &str) -> Inline {
    spans(text, 0..text.len())
}

/// Span texts paired with a short description of their style, which reads far
/// better in a failure than a debug dump of five booleans.
fn styled(text: &str) -> Vec<(&str, String)> {
    inline(text)
        .spans
        .into_iter()
        .map(|s| (&text[s.range], describe(&s.style)))
        .collect()
}

fn describe(style: &Style) -> String {
    let mut out = Vec::new();
    if style.bold {
        out.push("bold".to_owned());
    }
    if style.italic {
        out.push("italic".to_owned());
    }
    if style.underline {
        out.push("underline".to_owned());
    }
    if style.strike {
        out.push("strike".to_owned());
    }
    if style.code {
        out.push("code".to_owned());
    }
    if let Some(colour) = style.highlight {
        out.push(match colour {
            HighlightColor::Default => "mark".to_owned(),
            HighlightColor::Named(p) => format!("mark:{}", p.name()),
            HighlightColor::Rgb([r, g, b]) => format!("mark:#{r:02x}{g:02x}{b:02x}"),
        });
    }
    if style.link.is_some() {
        out.push("link".to_owned());
    }
    if out.is_empty() {
        "plain".to_owned()
    } else {
        out.join("+")
    }
}

/// The markup runs, as text, in order.
fn hidden(text: &str) -> Vec<&str> {
    inline(text).markup.into_iter().map(|r| &text[r]).collect()
}

/// Where each span's link points.
fn links(text: &str) -> Vec<&str> {
    inline(text)
        .spans
        .into_iter()
        .filter_map(|s| s.style.link.map(|url| &text[url]))
        .collect()
}

// ------------------------------------------------------- structural promises

#[test]
fn spans_and_markup_together_cover_the_input_exactly() {
    // The promise the renderer relies on: nothing is lost and nothing is
    // double counted, so a `LayoutJob` built from spans plus collapsed markup
    // is the same string the user typed.
    for sample in SAMPLES {
        let doc = parse(sample);
        for (line, inline) in doc.rows() {
            let mut covered: Vec<_> = inline
                .spans
                .iter()
                .map(|s| s.range.clone())
                .chain(inline.markup.iter().cloned())
                .collect();
            covered.sort_by_key(|r| r.start);

            let mut at = line.content.start;
            for range in &covered {
                assert_eq!(
                    range.start,
                    at,
                    "gap or overlap at {at} in {:?} of {sample:?}",
                    &sample[line.content.clone()]
                );
                at = range.end;
            }
            assert_eq!(
                at,
                line.content.end,
                "content of {:?} not covered to the end in {sample:?}",
                &sample[line.content.clone()]
            );
        }
    }
}

#[test]
fn a_marker_and_its_content_partition_the_line() {
    for sample in SAMPLES {
        for line in line::lines(sample) {
            assert_eq!(line.marker.start, line.range.start, "in {sample:?}");
            assert_eq!(line.marker.end, line.content.start, "in {sample:?}");
            assert_eq!(line.content.end, line.range.end, "in {sample:?}");
        }
    }
}

#[test]
fn every_range_lands_on_a_character_boundary() {
    // Byte ranges are handed straight to egui, which will panic on a range that
    // splits a multi-byte character.
    for sample in SAMPLES {
        let doc = parse(sample);
        for (line, inline) in doc.rows() {
            for range in [&line.range, &line.marker, &line.content] {
                assert!(sample.is_char_boundary(range.start), "in {sample:?}");
                assert!(sample.is_char_boundary(range.end), "in {sample:?}");
            }
            for span in &inline.spans {
                assert!(sample.is_char_boundary(span.range.start), "in {sample:?}");
                assert!(sample.is_char_boundary(span.range.end), "in {sample:?}");
            }
        }
    }
}

#[test]
fn no_empty_spans_are_ever_emitted() {
    for sample in SAMPLES {
        for inline in parse(sample).inline {
            assert!(
                inline.spans.iter().all(|s| !s.range.is_empty()),
                "empty span in {sample:?}"
            );
            assert!(
                inline.markup.iter().all(|r| !r.is_empty()),
                "empty markup in {sample:?}"
            );
        }
    }
}

/// Every awkward input in one place, so the structural tests above all see the
/// same corpus and a new edge case is covered everywhere at once.
const SAMPLES: &[&str] = &[
    "",
    "\n",
    "plain text",
    "# Heading\n## Two\n### Three\n#### Four\n##### Five",
    "**bold** and *italic* and __under__ and ~~gone~~ and `code`",
    "***all three***",
    "**bold with *nested italic* inside**",
    "unclosed **bold",
    "**",
    "****",
    "`**not bold**`",
    "snake_case_name stays plain",
    "_real italic_",
    "==marked== ==yellow|named== ==#f2c14e|hex== ==a|b==",
    "- one\n- two\n  - nested\n    - deeper",
    "1. first\n2. second\n10. tenth",
    "- [ ] todo\n- [x] done\n- [X] also done",
    "---\n-----\n--",
    "```rust\nlet x = *ptr;\n```",
    "```\nno language\n```",
    "unclosed fence:\n```py\nstill code",
    "[label](https://example.com) and https://bare.example.com/path",
    "see https://x.example.com. Trailing full stop.",
    "escaped \\*not italic\\* here",
    "\\\\ a literal backslash",
    "emoji \u{1f980} and accents \u{e9}\u{e8}\u{ea} with **bold \u{1f980}**",
    "  indented paragraph",
    "\t- tab indented bullet",
    "trailing spaces   ",
    "\r\nCRLF line\r\n",
    "#not a heading",
    "-no space bullet",
    "[](https://empty.example.com)",
    "[text]()",
    "mixed **bold `code` inside**",
];

// ------------------------------------------------------------------- headings

#[test]
fn headings_one_to_four_are_recognised() {
    assert_eq!(
        kinds("# a\n## b\n### c\n#### d"),
        [
            LineKind::Heading(1),
            LineKind::Heading(2),
            LineKind::Heading(3),
            LineKind::Heading(4),
        ]
    );
    assert_eq!(markers("# a\n## b"), ["# ", "## "]);
    assert_eq!(contents("# a\n## b"), ["a", "b"]);
}

#[test]
fn five_hashes_is_not_a_heading() {
    // Nothing styles a heading that small, so it stays text rather than
    // silently rendering as an H4.
    assert_eq!(kinds("##### deep"), [LineKind::Paragraph]);
}

#[test]
fn a_hash_needs_a_space_to_be_a_heading() {
    // Otherwise every `#tag` written in prose becomes a heading.
    assert_eq!(kinds("#tag"), [LineKind::Paragraph]);
    assert_eq!(kinds("#"), [LineKind::Paragraph]);
}

#[test]
fn an_indented_heading_is_still_a_heading_at_depth_zero() {
    // Only lists nest, so a heading's indent is cosmetic. The marker swallows
    // it so hiding the marker hides the stray whitespace too.
    assert_eq!(kinds("   # a"), [LineKind::Heading(1)]);
    assert_eq!(depths("   # a"), [0]);
    assert_eq!(markers("   # a"), ["   # "]);
}

// ---------------------------------------------------------------------- lists

#[test]
fn bullets_take_a_dash_or_a_star() {
    assert_eq!(kinds("- a\n* b"), [LineKind::Bullet, LineKind::Bullet]);
    assert_eq!(markers("- a\n* b"), ["- ", "* "]);
}

#[test]
fn a_bullet_needs_its_space() {
    assert_eq!(kinds("-nope"), [LineKind::Paragraph]);
}

#[test]
fn numbered_items_keep_the_number_that_was_typed() {
    // Enter continuation needs to carry on from what is there, and the renderer
    // shows it verbatim rather than renumbering behind the user's back.
    assert_eq!(
        kinds("1. a\n7. b\n10. c"),
        [
            LineKind::Numbered(1),
            LineKind::Numbered(7),
            LineKind::Numbered(10),
        ]
    );
    assert_eq!(markers("10. c"), ["10. "]);
}

#[test]
fn an_absurd_run_of_digits_does_not_overflow_the_parse() {
    let text = "12345678901234567890. item";
    assert_eq!(kinds(text), [LineKind::Paragraph]);
}

#[test]
fn checkboxes_read_both_cases() {
    assert_eq!(
        kinds("- [ ] a\n- [x] b\n- [X] c"),
        [
            LineKind::Task(false),
            LineKind::Task(true),
            LineKind::Task(true),
        ]
    );
    assert_eq!(markers("- [x] b"), ["- [x] "]);
    assert_eq!(contents("- [x] b"), ["b"]);
}

#[test]
fn a_checkbox_wins_over_the_bullet_that_starts_it() {
    // `- [x] ` starts with `- `, so order matters. Getting this wrong makes
    // every checkbox a bullet whose text begins with a bracket.
    assert!(matches!(kinds("- [ ] a")[0], LineKind::Task(false)));
}

#[test]
fn indent_gives_list_depth_and_a_tab_counts_as_one_level() {
    assert_eq!(depths("- a\n  - b\n    - c\n\t- t"), [0, 1, 2, 1]);
    assert_eq!(depths("\t\t- deep"), [2]);
}

#[test]
fn an_indented_paragraph_keeps_its_own_whitespace() {
    // There is no marker to hide, so swallowing the indent would reflow text
    // the user deliberately typed.
    assert_eq!(markers("   just text"), [""]);
    assert_eq!(contents("   just text"), ["   just text"]);
}

// ------------------------------------------------------------------ dividers

#[test]
fn three_or_more_dashes_make_a_divider() {
    assert_eq!(
        kinds("---\n----\n--"),
        [LineKind::Divider, LineKind::Divider, LineKind::Paragraph]
    );
}

#[test]
fn stars_and_underscores_are_not_dividers() {
    // They would be ambiguous against the bold and underline delimiters, so
    // `***` stays available as bold-italic.
    assert_eq!(kinds("***"), [LineKind::Paragraph]);
    assert_eq!(kinds("___"), [LineKind::Paragraph]);
}

#[test]
fn a_divider_is_all_marker_and_no_content() {
    assert_eq!(markers("---"), ["---"]);
    assert_eq!(contents("---"), [""]);
}

// ---------------------------------------------------------------- code fences

#[test]
fn a_fence_wraps_its_lines_in_code() {
    let text = "before\n```rust\nlet x = 1;\n```\nafter";
    let kinds = kinds(text);
    assert!(matches!(kinds[0], LineKind::Paragraph));
    assert!(matches!(kinds[1], LineKind::FenceOpen { .. }));
    assert_eq!(kinds[2], LineKind::Code);
    assert_eq!(kinds[3], LineKind::FenceClose);
    assert!(matches!(kinds[4], LineKind::Paragraph));

    let LineKind::FenceOpen { lang } = &kinds[1] else {
        panic!("expected a fence")
    };
    // Compared as text, not as an offset: an offset assertion tests the test's
    // own arithmetic rather than the parser.
    assert_eq!(&text[lang.clone()], "rust");
}

#[test]
fn a_fence_with_no_language_is_still_a_fence() {
    let text = "```\nplain\n```";
    assert!(matches!(kinds(text)[0], LineKind::FenceOpen { .. }));
    assert_eq!(kinds(text)[1], LineKind::Code);
}

#[test]
fn nothing_inside_a_fence_is_parsed_as_markup() {
    // A Rust `*ptr` inside a code block must not open an italic, and a Python
    // comment must not open a heading.
    let text = "```rust\nlet p = *ptr; // **not bold**\n```";
    let doc = parse(text);
    let code = &doc.inline[1];
    assert_eq!(code.spans.len(), 1, "code should be one unbroken span");
    assert!(code.markup.is_empty(), "nothing in code is markup");
    assert!(code.spans[0].style.code);
    assert_eq!(
        &text[code.spans[0].range.clone()],
        "let p = *ptr; // **not bold**"
    );
}

#[test]
fn a_hash_inside_a_fence_is_not_a_heading() {
    let text = "```py\n# a comment\n```";
    assert_eq!(kinds(text)[1], LineKind::Code);
}

#[test]
fn an_unclosed_fence_runs_to_the_end() {
    // What every editor does, and the only behaviour that lets you type inside
    // a fence you have not finished yet.
    let text = "```py\none\ntwo";
    assert_eq!(kinds(text)[1], LineKind::Code);
    assert_eq!(kinds(text)[2], LineKind::Code);
}

#[test]
fn a_bare_fence_closes_rather_than_reopening() {
    let text = "```\na\n```\nb\n```\nc\n```";
    assert_eq!(
        kinds(text)
            .into_iter()
            .map(|k| matches!(k, LineKind::Code))
            .collect::<Vec<_>>(),
        [false, true, false, false, false, true, false]
    );
}

// ------------------------------------------------------------ inline emphasis

#[test]
fn the_four_emphasis_delimiters_work() {
    assert_eq!(styled("**b**"), [("b", "bold".to_owned())]);
    assert_eq!(styled("*i*"), [("i", "italic".to_owned())]);
    assert_eq!(styled("__u__"), [("u", "underline".to_owned())]);
    assert_eq!(styled("~~s~~"), [("s", "strike".to_owned())]);
}

#[test]
fn double_underscore_is_underline_not_bold() {
    // The one deliberate divergence from CommonMark, taken from Discord.
    let style = &inline("__u__").spans[0].style;
    assert!(style.underline);
    assert!(!style.bold);
}

#[test]
fn triple_stars_are_bold_and_italic_together() {
    assert_eq!(styled("***x***"), [("x", "bold+italic".to_owned())]);
}

#[test]
fn emphasis_nests_and_accumulates() {
    assert_eq!(
        styled("**bold *both* bold**"),
        [
            ("bold ", "bold".to_owned()),
            ("both", "bold+italic".to_owned()),
            (" bold", "bold".to_owned()),
        ]
    );
}

#[test]
fn underline_and_bold_compose() {
    assert_eq!(
        styled("**__both__**"),
        [("both", "bold+underline".to_owned())]
    );
}

#[test]
fn nothing_applies_inside_inline_code() {
    assert_eq!(styled("`**x**`"), [("**x**", "code".to_owned())]);
    assert!(hidden("`**x**`").iter().all(|m| *m == "`"));
}

#[test]
fn code_inside_emphasis_keeps_both() {
    assert_eq!(
        styled("**bold `c` more**"),
        [
            ("bold ", "bold".to_owned()),
            ("c", "bold+code".to_owned()),
            (" more", "bold".to_owned()),
        ]
    );
}

#[test]
fn an_unmatched_delimiter_is_literal() {
    // Half typed markup must never make text disappear.
    assert_eq!(styled("**bold"), [("**bold", "plain".to_owned())]);
    assert!(hidden("**bold").is_empty());
}

#[test]
fn an_empty_pair_is_literal() {
    assert_eq!(styled("****"), [("****", "plain".to_owned())]);
    assert_eq!(styled("``"), [("``", "plain".to_owned())]);
}

#[test]
fn emphasis_does_not_cross_a_line() {
    // One stray delimiter must not reformat the rest of a long note.
    let text = "start **here\nand **there";
    let doc = parse(text);
    for inline in &doc.inline {
        assert!(
            inline.spans.iter().all(|s| s.style.is_plain()),
            "a delimiter leaked across the newline"
        );
    }
}

#[test]
fn underscores_inside_a_word_are_left_alone() {
    // The single most common markdown-editor complaint, and the reason `_` gets
    // the word boundary rule while `*` does not.
    assert_eq!(
        styled("call snake_case_name now"),
        [("call snake_case_name now", "plain".to_owned())]
    );
    assert_eq!(
        styled("a __very_long_name__ b"),
        [
            ("a ", "plain".to_owned()),
            ("very_long_name", "underline".to_owned()),
            (" b", "plain".to_owned()),
        ]
    );
}

#[test]
fn stars_inside_a_word_still_emphasise() {
    // Deliberately asymmetric: `*` has no intraword problem worth solving, and
    // CommonMark treats the two differently for the same reason.
    assert_eq!(
        styled("un*frigging*believable"),
        [
            ("un", "plain".to_owned()),
            ("frigging", "italic".to_owned()),
            ("believable", "plain".to_owned()),
        ]
    );
}

// ----------------------------------------------------------------- highlights

#[test]
fn a_bare_highlight_takes_the_default_colour() {
    assert_eq!(styled("==x=="), [("x", "mark".to_owned())]);
}

#[test]
fn a_named_colour_is_parsed_and_the_name_is_hidden() {
    assert_eq!(styled("==yellow|x=="), [("x", "mark:yellow".to_owned())]);
    // The colour is an instruction, not something to read, so it hides with
    // the delimiters.
    assert_eq!(hidden("==yellow|x=="), ["==yellow|", "=="]);
}

#[test]
fn every_palette_name_round_trips() {
    for colour in Palette::variants() {
        let text = format!("=={}|x==", colour.name());
        let style = &spans(&text, 0..text.len()).spans[0].style;
        assert_eq!(
            style.highlight,
            Some(HighlightColor::Named(colour)),
            "{} did not parse",
            colour.name()
        );
    }
}

#[test]
fn gray_and_grey_are_the_same_colour() {
    assert_eq!(Palette::parse("gray"), Palette::parse("grey"));
}

#[test]
fn colour_names_are_case_insensitive() {
    assert_eq!(styled("==Yellow|x=="), [("x", "mark:yellow".to_owned())]);
}

#[test]
fn hex_highlights_work_in_both_lengths() {
    assert_eq!(styled("==#f2c14e|x=="), [("x", "mark:#f2c14e".to_owned())]);
    // Shorthand doubles each nibble, so #fff is fully white rather than #f0f0f0.
    assert_eq!(styled("==#fff|x=="), [("x", "mark:#ffffff".to_owned())]);
    assert_eq!(styled("==#000|x=="), [("x", "mark:#000000".to_owned())]);
}

#[test]
fn a_bad_hex_falls_back_to_the_default_colour() {
    assert_eq!(styled("==#zz|x=="), [("#zz|x", "mark".to_owned())]);
}

#[test]
fn an_unrecognised_prefix_is_content_not_a_failure() {
    // A mistyped colour shows as a stray word inside the highlight, which is
    // obvious. Making the whole highlight vanish would not be.
    assert_eq!(styled("==a|b=="), [("a|b", "mark".to_owned())]);
    assert_eq!(styled("==yelow|x=="), [("yelow|x", "mark".to_owned())]);
}

#[test]
fn highlights_compose_with_emphasis() {
    assert_eq!(
        styled("==blue|**bold**=="),
        [("bold", "bold+mark:blue".to_owned())]
    );
}

// ---------------------------------------------------------------------- links

#[test]
fn an_explicit_link_shows_its_label_and_hides_its_target() {
    assert_eq!(
        styled("[label](https://example.com)"),
        [("label", "link".to_owned())]
    );
    assert_eq!(
        links("[label](https://example.com)"),
        ["https://example.com"]
    );
    assert_eq!(
        hidden("[label](https://example.com)"),
        ["[", "](https://example.com)"]
    );
}

#[test]
fn a_bare_url_becomes_a_link_pointing_at_itself() {
    let text = "go to https://example.com/x now";
    assert_eq!(links(text), ["https://example.com/x"]);
    assert!(hidden(text).is_empty(), "an autolink hides nothing");
}

#[test]
fn both_schemes_autolink() {
    assert_eq!(links("http://a.example.com"), ["http://a.example.com"]);
    assert_eq!(links("https://b.example.com"), ["https://b.example.com"]);
}

#[test]
fn trailing_punctuation_is_not_part_of_the_url() {
    assert_eq!(
        links("see https://example.com/x."),
        ["https://example.com/x"]
    );
    assert_eq!(links("(https://example.com/y)"), ["https://example.com/y"]);
    assert_eq!(
        links("https://example.com/z, and"),
        ["https://example.com/z"]
    );
}

#[test]
fn a_scheme_with_nothing_after_it_is_not_a_link() {
    assert!(links("https://").is_empty());
}

#[test]
fn a_url_mid_word_is_not_autolinked() {
    assert!(links("xhttps://example.com").is_empty());
}

#[test]
fn an_autolink_inside_an_explicit_link_does_not_double_up() {
    let text = "[https://shown.example.com](https://target.example.com)";
    assert_eq!(links(text), ["https://target.example.com"]);
}

#[test]
fn a_link_with_an_empty_half_is_not_a_link() {
    // Neither form makes an explicit link, so nothing is hidden and the
    // brackets stay on screen as the punctuation they are.
    let empty_label = "[](https://x.example.com)";
    assert!(
        hidden(empty_label).is_empty(),
        "an empty label must not hide the target"
    );
    assert!(
        styled(empty_label)
            .iter()
            .any(|(text, _)| text.contains("[](")),
        "the brackets should still be visible"
    );
    // The bare URL inside is still autolinked, which is right: it is a URL
    // sitting in the text like any other.
    assert_eq!(links(empty_label), ["https://x.example.com"]);

    assert!(links("[text]()").is_empty());
    assert!(hidden("[text]()").is_empty());
}

#[test]
fn emphasis_works_inside_a_link_label() {
    assert_eq!(
        styled("[**bold label**](https://example.com)"),
        [("bold label", "bold+link".to_owned())]
    );
}

// --------------------------------------------------------------------- escapes

#[test]
fn a_backslash_makes_a_delimiter_literal() {
    assert_eq!(
        styled("\\*not italic\\*"),
        [
            ("*", "plain".to_owned()),
            ("not italic", "plain".to_owned()),
            ("*", "plain".to_owned())
        ]
    );
    assert_eq!(hidden("\\*not italic\\*"), ["\\", "\\"]);
}

#[test]
fn an_escaped_delimiter_does_not_close_an_open_one() {
    // Three spans, not two: the backslash is markup and hides, leaving the
    // star it escaped as a span of its own.
    assert_eq!(
        styled("**bold \\** still bold**"),
        [
            ("bold ", "bold".to_owned()),
            ("*", "bold".to_owned()),
            ("* still bold", "bold".to_owned()),
        ]
    );
    assert_eq!(hidden("**bold \\** still bold**"), ["**", "\\", "**"]);
}

#[test]
fn a_backslash_before_ordinary_text_is_just_a_backslash() {
    assert_eq!(
        styled("C:\\path\\to"),
        [("C:\\path\\to", "plain".to_owned())]
    );
}

// ------------------------------------------------------------------- unicode

#[test]
fn multibyte_text_is_split_on_character_boundaries() {
    let text = "**\u{1f980} crab \u{e9}**";
    assert_eq!(styled(text), [("\u{1f980} crab \u{e9}", "bold".to_owned())]);
}

#[test]
fn a_delimiter_after_an_accented_letter_still_opens() {
    // The word boundary rule uses `is_alphanumeric`, which must treat accented
    // letters as letters rather than as punctuation.
    assert_eq!(
        styled("caf\u{e9}_x_"),
        [("caf\u{e9}_x_", "plain".to_owned())],
        "an underscore inside an accented word should stay literal"
    );
}

// -------------------------------------------------------------- line handling

#[test]
fn crlf_endings_are_stripped_from_the_ranges() {
    let text = "one\r\ntwo\r\n";
    assert_eq!(contents(text), ["one", "two", ""]);
}

#[test]
fn an_empty_document_is_one_blank_line() {
    // The editor needs somewhere to put the caret.
    assert_eq!(kinds(""), [LineKind::Blank]);
}

#[test]
fn a_trailing_newline_leaves_a_blank_final_line() {
    assert_eq!(kinds("text\n"), [LineKind::Paragraph, LineKind::Blank]);
}

#[test]
fn a_blank_line_is_blank_not_an_empty_paragraph() {
    assert_eq!(
        kinds("a\n\nb"),
        [LineKind::Paragraph, LineKind::Blank, LineKind::Paragraph]
    );
}

#[test]
fn whitespace_only_lines_are_blank() {
    assert_eq!(
        kinds("   \n\t\n"),
        [LineKind::Blank, LineKind::Blank, LineKind::Blank]
    );
}

// ---------------------------------------------------------- helper predicates

#[test]
fn list_and_rule_predicates_agree_with_the_kinds() {
    assert!(LineKind::Bullet.is_list());
    assert!(LineKind::Numbered(1).is_list());
    assert!(LineKind::Task(false).is_list());
    assert!(!LineKind::Paragraph.is_list());
    assert!(!LineKind::Heading(1).is_list());

    assert!(LineKind::Divider.is_rule());
    assert!(LineKind::FenceClose.is_rule());
    assert!(!LineKind::Code.is_rule());
    assert!(LineKind::Code.is_verbatim());
    assert!(!LineKind::Paragraph.is_verbatim());
}

#[test]
fn a_fence_line_counts_as_code_for_painting() {
    let text = "```py\nx\n```";
    let lines = line::lines(text);
    assert!(lines.iter().all(trackcrab::markdown::Line::is_code));
}

// ------------------------------------------------------------ plain text view

#[test]
fn plain_strips_markers_and_delimiters() {
    // What the sidebar search matches, so searching for a phrase still finds it
    // when half of it is bold.
    assert_eq!(plain("# Heading"), "Heading");
    assert_eq!(plain("- **bold** item"), "bold item");
    assert_eq!(plain("==yellow|marked=="), "marked");
    assert_eq!(plain("[label](https://example.com)"), "label");
}

#[test]
fn plain_drops_pure_markup_lines_but_keeps_the_line_count() {
    assert_eq!(plain("a\n---\nb"), "a\n\nb");
}

#[test]
fn plain_keeps_code_verbatim() {
    assert_eq!(plain("```py\nx = **1**\n```"), "\nx = **1**\n");
}

#[test]
fn searching_for_a_delimiter_no_longer_matches_everything() {
    // The point of the whole exercise: `*` appears in the source of every
    // emphasised word, and in none of the text.
    let note = "**bold** and *italic* and a plain star: \\*";
    assert!(!plain(note).contains("**"));
    assert!(plain(note).contains('*'), "an escaped star is still a star");
}

#[test]
fn a_document_pairs_every_line_with_its_spans() {
    let doc: Document = parse("# a\n- b");
    assert_eq!(doc.lines.len(), doc.inline.len());
    assert_eq!(doc.rows().count(), 2);
}

// ------------------------------------------------------------------- fuzzing

/// A tiny deterministic generator, so a failure is reproducible from its seed
/// and no dev-dependency is needed for what is a few lines of arithmetic.
fn noise(seed: u64, len: usize) -> String {
    generate(seed, len, true)
}

/// As [`noise`] but on one line, so the inline scanner sees the whole sample.
///
/// Needed because a stray ```` ``` ```` opens a fence and everything after it
/// becomes verbatim, which quietly retired most of a multi-line sample from the
/// part of the parser this is trying to stress.
fn line_noise(seed: u64, len: usize) -> String {
    generate(seed, len, false)
}

fn generate(seed: u64, len: usize, newlines: bool) -> String {
    // Characters chosen to be delimiter-dense: random prose would almost never
    // produce the pathological nesting this is looking for.
    const ALPHABET: &[char] = &[
        '*',
        '*',
        '_',
        '_',
        '~',
        '`',
        '=',
        '#',
        '-',
        '[',
        ']',
        '(',
        ')',
        '\\',
        '|',
        '.',
        ' ',
        '\n',
        '\t',
        'a',
        'b',
        '1',
        'h',
        ':',
        '/',
        '\u{1f980}',
        '\u{e9}',
    ];
    let mut state = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
    (0..len)
        .map(|_| {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let index = (state >> 33) as usize % ALPHABET.len();
            let c = ALPHABET[index];
            if !newlines && c == '\n' { ' ' } else { c }
        })
        .collect()
}

#[test]
fn delimiter_noise_never_panics_and_always_covers_its_input() {
    // The invariant the renderer stands on, checked against inputs no human
    // would think to write. A parser that loses or duplicates a byte here would
    // silently corrupt what the user sees.
    for seed in 0..400 {
        for len in [1, 2, 3, 7, 16, 64, 200] {
            let text = noise(seed, len);
            let doc = parse(&text);
            assert_eq!(doc.lines.len(), doc.inline.len(), "seed {seed} len {len}");

            for (line, inline) in doc.rows() {
                let mut covered: Vec<_> = inline
                    .spans
                    .iter()
                    .map(|s| s.range.clone())
                    .chain(inline.markup.iter().cloned())
                    .collect();
                covered.sort_by_key(|r| r.start);

                let mut at = line.content.start;
                for range in &covered {
                    assert_eq!(
                        range.start, at,
                        "seed {seed} len {len}: gap or overlap in {text:?}"
                    );
                    assert!(
                        text.is_char_boundary(range.start) && text.is_char_boundary(range.end),
                        "seed {seed} len {len}: split a character in {text:?}"
                    );
                    at = range.end;
                }
                assert_eq!(
                    at, line.content.end,
                    "seed {seed} len {len}: short of the end in {text:?}"
                );
            }
            // The plain view is what search reads, so it must survive the same
            // inputs rather than panicking on a range it built itself.
            let _ = plain(&text);
        }
    }
}

#[test]
fn delimiter_noise_covers_a_single_line_too() {
    let mut styled = 0_usize;
    let mut markup = 0_usize;

    for seed in 0..400 {
        for len in [1, 2, 3, 7, 16, 64, 200] {
            let text = line_noise(seed, len);
            let inline = spans(&text, 0..text.len());

            let mut covered: Vec<_> = inline
                .spans
                .iter()
                .map(|s| s.range.clone())
                .chain(inline.markup.iter().cloned())
                .collect();
            covered.sort_by_key(|r| r.start);

            let mut at = 0;
            for range in &covered {
                assert_eq!(range.start, at, "seed {seed} len {len}: gap in {text:?}");
                assert!(
                    text.is_char_boundary(range.start) && text.is_char_boundary(range.end),
                    "seed {seed} len {len}: split a character in {text:?}"
                );
                at = range.end;
            }
            assert_eq!(
                at,
                text.len(),
                "seed {seed} len {len}: short of the end in {text:?}"
            );

            styled += inline.spans.iter().filter(|s| !s.style.is_plain()).count();
            markup += inline.markup.len();
        }
    }

    // A guard against a tame corpus. If a change to the generator or the parser
    // stopped producing nested markup, every assertion above would still pass
    // while testing almost nothing.
    assert!(
        styled > 1_000 && markup > 1_000,
        "corpus produced only {styled} styled spans and {markup} markup runs, \
         which is too tame to be exercising the scanner"
    );
}

#[test]
fn noise_is_deterministic() {
    // Or a failure above could not be reproduced from its seed.
    assert_eq!(noise(7, 32), noise(7, 32));
    assert_ne!(noise(7, 32), noise(8, 32));
}

// ------------------------------------------------- link lookup (D7)

/// The URL a click at the first occurrence of `needle` would open.
fn link_under(text: &str, needle: &str) -> Option<String> {
    let at = text.find(needle).expect("needle should be in the sample");
    parse(text)
        .link_at(at)
        .map(|target| text[target].to_owned())
}

#[test]
fn a_bare_url_points_at_itself() {
    let text = "see https://example.com/docs for more";
    assert_eq!(
        link_under(text, "https"),
        Some("https://example.com/docs".to_owned())
    );
}

#[test]
fn an_explicit_link_points_at_its_target_from_anywhere_in_the_label() {
    let text = "read [the manual](https://example.com/m) first";
    for needle in ["the", "manual", "e manu"] {
        assert_eq!(
            link_under(text, needle),
            Some("https://example.com/m".to_owned()),
            "failed at {needle}"
        );
    }
}

#[test]
fn text_beside_a_link_is_not_a_link() {
    let text = "read [the manual](https://example.com/m) first";
    assert_eq!(link_under(text, "read"), None);
    assert_eq!(link_under(text, "first"), None);
}

#[test]
fn a_url_inside_code_is_not_a_link() {
    // Nothing formats inside code, and a URL in a snippet is a string literal,
    // not somewhere to send the reader.
    assert_eq!(link_under("`https://example.com`", "https"), None);
    assert_eq!(
        link_under("```\nhttps://example.com\n```", "https"),
        None
    );
}

#[test]
fn a_link_inside_a_list_item_still_resolves() {
    let text = "- [ ] read [docs](https://example.com/d)\n";
    assert_eq!(
        link_under(text, "docs"),
        Some("https://example.com/d".to_owned())
    );
}

#[test]
fn a_link_target_is_never_the_markup_itself() {
    // The address is collapsed to nothing on screen, so a pointer can never
    // really be over it; asking anyway must not answer with the brackets.
    let text = "[label](https://example.com)";
    let doc = parse(text);
    for at in 0..text.len() {
        if let Some(target) = doc.link_at(at) {
            assert_eq!(
                &text[target], "https://example.com",
                "offset {at} resolved to something other than the address"
            );
        }
    }
}

#[test]
fn only_the_label_of_a_link_is_clickable() {
    let text = "[label](https://example.com)";
    let doc = parse(text);
    let live: Vec<usize> = (0..text.len())
        .filter(|at| doc.link_at(*at).is_some())
        .collect();
    let label = text.find("label").expect("label");
    assert_eq!(live, (label..label + 5).collect::<Vec<_>>());
}

#[test]
fn line_lookup_finds_the_line_a_byte_is_on() {
    let text = "first\nsecond\nthird";
    let doc = parse(text);
    let line_of = |at: usize| doc.line_at(at).map(|l| text[l.content.clone()].to_owned());
    assert_eq!(line_of(0).as_deref(), Some("first"));
    assert_eq!(line_of(8).as_deref(), Some("second"));
    assert_eq!(line_of(text.len()).as_deref(), Some("third"));
}

// ------------------------------------------------- url recognition (D7)

#[test]
fn a_clipboard_holding_one_address_is_a_url() {
    for text in [
        "https://example.com",
        "http://example.com",
        "https://example.com/a/b?c=d#e",
        "https://en.wikipedia.org/wiki/Foo_(bar)",
        "  https://example.com  ",
        "https://example.com.",
    ] {
        assert!(trackcrab::markdown::is_url(text), "{text:?} is a URL");
    }
}

#[test]
fn anything_else_is_not_a_url() {
    for text in [
        "",
        "   ",
        "example.com",
        "www.example.com",
        "ftp://example.com",
        "https://",
        "http://",
        "see https://example.com",
        "https://a.com https://b.com",
        "https://a.com\nhttps://b.com",
    ] {
        assert!(!trackcrab::markdown::is_url(text), "{text:?} is not a URL");
    }
}

#[test]
fn the_paste_rule_and_the_autolinker_agree_on_ordinary_addresses() {
    // They differ deliberately at the edges, over trailing punctuation, but an
    // address with none must read the same to both or the two halves of the
    // feature would disagree about what a link is.
    for url in [
        "https://example.com",
        "https://example.com/docs",
        "http://sub.example.co.uk/a/b",
    ] {
        assert!(trackcrab::markdown::is_url(url));
        let text = format!("see {url} here");
        let at = text.find("https").or_else(|| text.find("http")).expect("scheme");
        assert_eq!(
            parse(&text).link_at(at).map(|t| text[t].to_owned()),
            Some((*url).to_owned()),
            "{url} did not autolink"
        );
    }
}
