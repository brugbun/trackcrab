//! Markdown-aware editing: Tab, Enter and Backspace inside a list.
//!
//! This is custom editing logic sitting next to egui's own, which is exactly
//! the sort of thing that goes subtly wrong, so it is pure and asserted here
//! rather than clicked at through a window.

use trackcrab::markdown::edit::{backspace, enter, tab, untab};

/// Applies an operation to text with the caret marked by `|`, and returns the
/// result with the new caret marked the same way.
///
/// Reads far better than byte offsets in an assertion: the test says what you
/// would type and what you would see.
fn apply(
    op: fn(&str, &std::ops::Range<usize>) -> Option<trackcrab::markdown::Edit>,
    marked: &str,
) -> Option<String> {
    let (text, caret) = unmark(marked);
    let edit = op(&text, &caret)?;
    Some(mark(&edit.text, &edit.caret))
}

/// As [`apply`], but panics if the operation declined, for the cases that are
/// meant to do something.
fn expect(
    op: fn(&str, &std::ops::Range<usize>) -> Option<trackcrab::markdown::Edit>,
    marked: &str,
) -> String {
    apply(op, marked).unwrap_or_else(|| panic!("declined to act on {marked:?}"))
}

/// Strips the caret markers. One `|` is a caret; two delimit a selection.
fn unmark(marked: &str) -> (String, std::ops::Range<usize>) {
    let first = marked.find('|').expect("no caret in the sample");
    let text = marked.replacen('|', "", 1);
    match text.find('|') {
        Some(second) => (text.replacen('|', "", 1), first..second),
        None => (text, first..first),
    }
}

fn mark(text: &str, caret: &std::ops::Range<usize>) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push_str(&text[..caret.start]);
    out.push('|');
    if caret.start != caret.end {
        out.push_str(&text[caret.start..caret.end]);
        out.push('|');
    }
    out.push_str(&text[caret.end..]);
    out
}

#[test]
fn the_test_helpers_round_trip() {
    // Everything below is read through these, so they get their own check.
    for sample in ["|abc", "a|bc", "abc|", "a|b|c", "|a\nb", "a\n|b"] {
        let (text, caret) = unmark(sample);
        assert_eq!(mark(&text, &caret), sample);
    }
}

// -------------------------------------------------------------------- Tab

#[test]
fn tab_indents_a_list_item() {
    assert_eq!(expect(tab, "- one\n- t|wo"), "- one\n  - t|wo");
}

#[test]
fn tab_in_prose_inserts_spaces_rather_than_a_tab_character() {
    // A literal tab would leave the file indented two ways at once, and the
    // parser would have to treat both as a level forever.
    assert_eq!(expect(tab, "plain| text"), "plain  | text");
    assert!(!expect(tab, "plain| text").contains('\t'));
}

#[test]
fn tab_will_not_nest_an_item_under_nothing() {
    // The first item of a list has no parent to sit under, so markdown has
    // nowhere to put it and the render would show a stray indent.
    assert_eq!(apply(tab, "- fir|st"), None);
    assert_eq!(apply(tab, "text\n- fir|st"), None);
}

#[test]
fn tab_will_not_nest_more_than_one_level_past_the_item_above() {
    // `- a` then an item at depth 2 is ambiguous markdown and renders with a
    // gap where the missing level should be.
    assert_eq!(expect(tab, "- a\n- b|"), "- a\n  - b|");
    assert_eq!(apply(tab, "- a\n  - b|"), None);
    assert_eq!(expect(tab, "- a\n  - b\n  - c|"), "- a\n  - b\n    - c|");
}

#[test]
fn a_blank_line_does_not_end_a_list_for_indent_purposes() {
    // Blank lines inside a list are common. Anything else does end it.
    assert_eq!(expect(tab, "- a\n\n- b|"), "- a\n\n  - b|");
    assert_eq!(apply(tab, "- a\n\ntext\n- b|"), None);
}

#[test]
fn tab_indents_every_line_of_a_selection() {
    assert_eq!(
        expect(tab, "- a\n|- b\n- c|\n- d"),
        "- a\n|  - b\n  - c|\n- d"
    );
}

#[test]
fn a_selection_is_indented_whether_or_not_it_is_a_list() {
    // What every editor does, and the depth rule is a list rule, not a
    // whitespace rule.
    assert_eq!(expect(tab, "|one\ntwo|"), "|  one\n  two|");
}

#[test]
fn numbered_and_task_items_indent_too() {
    assert_eq!(expect(tab, "1. a\n2. b|"), "1. a\n  2. b|");
    assert_eq!(expect(tab, "- [ ] a\n- [x] b|"), "- [ ] a\n  - [x] b|");
}

// -------------------------------------------------------------- Shift+Tab

#[test]
fn untab_outdents_a_list_item() {
    assert_eq!(expect(untab, "- a\n  - b|"), "- a\n- b|");
}

#[test]
fn untab_declines_at_the_left_margin() {
    // Nothing to give back, so egui keeps whatever Shift+Tab would have done.
    assert_eq!(apply(untab, "- a|"), None);
    assert_eq!(apply(untab, "plain|"), None);
}

#[test]
fn untab_removes_a_partial_indent_rather_than_refusing() {
    // A single stray space is still an indent, and leaving it would make
    // Shift+Tab appear to do nothing.
    assert_eq!(expect(untab, " - a|"), "- a|");
}

#[test]
fn untab_treats_a_tab_as_one_whole_level() {
    // Pasted tab-indented text should outdent a step at a time like anything
    // else, rather than needing one press per notional space.
    assert_eq!(expect(untab, "\t- a|"), "- a|");
}

#[test]
fn a_caret_inside_the_removed_indent_lands_at_the_line_start() {
    // Otherwise it drifts up into the previous line, which reads as the caret
    // jumping somewhere random.
    assert_eq!(expect(untab, "- a\n | - b"), "- a\n|- b");
}

#[test]
fn untab_outdents_every_line_of_a_selection() {
    assert_eq!(expect(untab, "  - a\n|  - b\n  - c|"), "  - a\n|- b\n- c|");
}

// ------------------------------------------------------------------ Enter

#[test]
fn enter_continues_a_bullet_list() {
    assert_eq!(expect(enter, "- one|"), "- one\n- |");
}

#[test]
fn enter_keeps_the_indent_of_the_item_it_continues() {
    assert_eq!(expect(enter, "- a\n  - two|"), "- a\n  - two\n  - |");
}

#[test]
fn enter_increments_a_numbered_item() {
    assert_eq!(expect(enter, "1. one|"), "1. one\n2. |");
    assert_eq!(expect(enter, "9. nine|"), "9. nine\n10. |");
}

#[test]
fn enter_after_a_task_makes_an_unticked_one() {
    // A fresh item is not done, whatever the one above it says.
    assert_eq!(expect(enter, "- [x] done|"), "- [x] done\n- [ ] |");
    assert_eq!(expect(enter, "- [ ] todo|"), "- [ ] todo\n- [ ] |");
}

#[test]
fn enter_on_an_empty_item_leaves_the_list() {
    // The universal way out of a list, and the reason it is not simply "always
    // add another item".
    assert_eq!(expect(enter, "- one\n- |"), "- one\n|");
    assert_eq!(expect(enter, "- [ ] |"), "|");
    assert_eq!(expect(enter, "3. |"), "|");
}

#[test]
fn enter_mid_item_carries_the_rest_onto_the_new_item() {
    assert_eq!(expect(enter, "- one|two"), "- one\n- |two");
}

#[test]
fn enter_in_prose_is_left_to_egui() {
    assert_eq!(apply(enter, "plain text|"), None);
    assert_eq!(apply(enter, "# heading|"), None);
}

#[test]
fn enter_inside_a_code_block_is_left_alone() {
    // A dash in a shell script must not sprout a list.
    assert_eq!(apply(enter, "```sh\n- not a list|\n```"), None);
}

#[test]
fn enter_with_a_selection_is_left_to_egui() {
    // A selection means "replace this", which is not a list operation.
    assert_eq!(apply(enter, "- |one|"), None);
}

// -------------------------------------------------------------- Backspace

#[test]
fn backspace_after_a_marker_strips_it() {
    assert_eq!(expect(backspace, "- |one"), "|one");
    assert_eq!(expect(backspace, "1. |one"), "|one");
    assert_eq!(expect(backspace, "- [x] |one"), "|one");
}

#[test]
fn backspace_strips_a_heading_marker_too() {
    // The same rule, and deleting one invisible character of collapsed markup
    // instead is never what anyone wants.
    assert_eq!(expect(backspace, "## |Title"), "|Title");
}

#[test]
fn stripping_a_marker_keeps_the_indent() {
    // Removing both would jump the item to the top level in one keystroke.
    // Shift+Tab is how you change depth.
    assert_eq!(expect(backspace, "- a\n  - |b"), "- a\n  |b");
}

#[test]
fn backspace_anywhere_else_is_left_to_egui() {
    assert_eq!(apply(backspace, "- o|ne"), None, "mid content");
    assert_eq!(apply(backspace, "|- one"), None, "before the marker");
    assert_eq!(apply(backspace, "plain|"), None, "no marker at all");
    assert_eq!(apply(backspace, "- |one|"), None, "with a selection");
}

// --------------------------------------------------------------- integrity

/// Every operation, over every caret position, on a document with one of
/// everything in it.
/// Every line's content, whitespace removed.
///
/// The markers are excluded on purpose: they are exactly what these operations
/// are allowed to change. What must never change is the text the user wrote.
fn visible(text: &str) -> String {
    let doc = trackcrab::markdown::parse(text);
    doc.lines
        .iter()
        .flat_map(|line| text[line.content.clone()].chars())
        .filter(|c| !c.is_whitespace())
        .collect()
}

#[test]
fn no_operation_ever_loses_or_duplicates_text() {
    // The property that matters: an editing bug that silently drops a character
    // is far worse than one that refuses to act.
    let source =
        "# Heading\n- one\n  - two\n1. numbered\n- [x] done\n\nplain\n```sh\n- code\n```\n";
    for at in 0..=source.len() {
        if !source.is_char_boundary(at) {
            continue;
        }
        for (name, op) in [
            ("tab", tab as fn(&str, &std::ops::Range<usize>) -> _),
            ("untab", untab),
            ("enter", enter),
            ("backspace", backspace),
        ] {
            let Some(edit) = op(source, &(at..at)) else {
                continue;
            };
            assert!(
                edit.text.is_char_boundary(edit.caret.start)
                    && edit.text.is_char_boundary(edit.caret.end),
                "{name} split a character at {at}"
            );
            assert!(
                edit.caret.end <= edit.text.len(),
                "{name} put the caret at {:?}, past the end of {} bytes, at {at}",
                edit.caret,
                edit.text.len()
            );
            // Compared against the parser's own *content* ranges, with
            // whitespace dropped. Nothing else expresses this correctly: a
            // marker legitimately appears and vanishes, and a numbered marker
            // contains a digit while a checkbox contains an `x`, so any filter
            // over raw characters flags a correct edit as data loss.
            assert_eq!(
                visible(source),
                visible(&edit.text),
                "the content changed at {at} under {name}, so text was lost or invented"
            );
        }
    }
}

#[test]
fn every_operation_leaves_a_document_the_parser_still_understands() {
    let source = "- a\n  - b\n1. c\n- [ ] d\n";
    for at in 0..=source.len() {
        for op in [tab, untab, enter, backspace] {
            let Some(edit) = op(source, &(at..at)) else {
                continue;
            };
            let doc = trackcrab::markdown::parse(&edit.text);
            assert_eq!(
                doc.lines.len(),
                doc.inline.len(),
                "the parse came apart after an edit at {at}"
            );
        }
    }
}

// ------------------------------------------- toolbar operations (D6)

use trackcrab::markdown::edit::{Block, Wrap, block, code_block, divider, link, wrap};

fn wrapped(marked: &str, what: &Wrap) -> String {
    let (text, caret) = unmark(marked);
    let edit = wrap(&text, &caret, what);
    mark(&edit.text, &edit.caret)
}

fn blocked(marked: &str, what: Block) -> String {
    let (text, caret) = unmark(marked);
    let edit = block(&text, &caret, what).expect("declined");
    mark(&edit.text, &edit.caret)
}

#[test]
fn wrapping_a_selection_keeps_it_selected() {
    // So the same button can be pressed again to undo it, which is what makes
    // the toolbar feel like a toggle rather than a one-way switch.
    assert_eq!(
        wrapped("say |hello| there", &Wrap::Bold),
        "say **|hello|** there"
    );
}

#[test]
fn wrapping_nothing_leaves_the_caret_between_the_delimiters() {
    assert_eq!(wrapped("say |", &Wrap::Bold), "say **|**");
    assert_eq!(wrapped("say |", &Wrap::Code), "say `|`");
}

#[test]
fn every_inline_style_has_the_right_delimiters() {
    for (what, expected) in [
        (Wrap::Bold, "**|x|**"),
        (Wrap::Italic, "*|x|*"),
        (Wrap::Underline, "__|x|__"),
        (Wrap::Strike, "~~|x|~~"),
        (Wrap::Code, "`|x|`"),
        (Wrap::Highlight(None), "==|x|=="),
    ] {
        assert_eq!(wrapped("|x|", &what), expected, "{what:?}");
    }
}

#[test]
fn a_highlight_colour_goes_in_the_opening_delimiter() {
    assert_eq!(
        wrapped("|x|", &Wrap::Highlight(Some("yellow".to_owned()))),
        "==yellow||x|=="
    );
    assert_eq!(
        wrapped("|x|", &Wrap::Highlight(Some("#f2c14e".to_owned()))),
        "==#f2c14e||x|=="
    );
}

#[test]
fn wrapping_something_already_wrapped_unwraps_it() {
    // Delimiters just outside the selection: what you get by selecting a word
    // that is already bold.
    assert_eq!(wrapped("**|bold|**", &Wrap::Bold), "|bold|");
}

#[test]
fn selecting_the_delimiters_too_still_unwraps() {
    // The other way people select an emphasised word.
    assert_eq!(wrapped("|**bold**|", &Wrap::Bold), "bold|");
}

#[test]
fn unwrapping_only_matches_the_same_style() {
    // Bold on an italic run adds bold rather than stripping the italics.
    assert_eq!(wrapped("*|x|*", &Wrap::Bold), "***|x|***");
}

#[test]
fn a_block_marker_is_applied_to_the_caret_line() {
    assert_eq!(blocked("hello|", Block::Bullet), "- hello|");
    assert_eq!(blocked("hello|", Block::Heading(2)), "## hello|");
    assert_eq!(blocked("hello|", Block::Task), "- [ ] hello|");
    assert_eq!(blocked("hello|", Block::Numbered), "1. hello|");
}

#[test]
fn pressing_the_same_block_button_again_removes_it() {
    assert_eq!(blocked("- hello|", Block::Bullet), "hello|");
    assert_eq!(blocked("## hello|", Block::Heading(2)), "hello|");
}

#[test]
fn a_different_block_marker_replaces_rather_than_stacking() {
    assert_eq!(blocked("- hello|", Block::Heading(1)), "# hello|");
    assert_eq!(blocked("# hello|", Block::Task), "- [ ] hello|");
    assert_eq!(blocked("1. hello|", Block::Bullet), "- hello|");
}

#[test]
fn a_different_heading_level_replaces_the_old_one() {
    assert_eq!(blocked("# hello|", Block::Heading(3)), "### hello|");
}

#[test]
fn a_block_marker_keeps_the_indent() {
    assert_eq!(blocked("  - a|", Block::Task), "  - [ ] a|");
}

#[test]
fn numbering_a_selection_counts_up_through_it() {
    // Three number ones would be wrong, and the parser keeps what is typed, so
    // nothing downstream would fix it.
    assert_eq!(
        blocked("|one\ntwo\nthree|", Block::Numbered),
        "|1. one\n2. two\n3. three|"
    );
}

#[test]
fn a_block_button_declines_inside_a_code_block() {
    // Turning a line of a shell script into a heading by pressing a button
    // would be a genuine surprise.
    let (text, caret) = unmark("```sh\necho hi|\n```");
    assert_eq!(block(&text, &caret, Block::Heading(1)), None);
}

#[test]
fn a_divider_goes_below_the_current_line() {
    let (text, caret) = unmark("above|");
    let edit = divider(&text, &caret).expect("declined");
    assert_eq!(mark(&edit.text, &edit.caret), "above\n---\n|");
}

#[test]
fn a_divider_on_a_blank_line_uses_that_line() {
    // Otherwise pressing the button on an empty line leaves a stray blank above
    // the rule.
    let (text, caret) = unmark("above\n|");
    let edit = divider(&text, &caret).expect("declined");
    assert_eq!(mark(&edit.text, &edit.caret), "above\n---|");
}

#[test]
fn a_code_block_wraps_the_selected_lines() {
    let (text, caret) = unmark("|let x = 1;|");
    let edit = code_block(&text, &caret).expect("declined");
    assert_eq!(mark(&edit.text, &edit.caret), "```\n|let x = 1;\n```");
}

#[test]
fn an_empty_code_block_leaves_the_caret_inside_it() {
    let (text, caret) = unmark("|");
    let edit = code_block(&text, &caret).expect("declined");
    assert_eq!(mark(&edit.text, &edit.caret), "```\n|\n```");
}

#[test]
fn a_link_puts_the_caret_where_the_address_goes() {
    // With a label written, the address is what is missing; with nothing
    // written it is still the harder half to type from memory.
    let (text, caret) = unmark("see |the docs|");
    let edit = link(&text, &caret);
    assert_eq!(mark(&edit.text, &edit.caret), "see [the docs](|)");

    let (text, caret) = unmark("see |");
    let edit = link(&text, &caret);
    assert_eq!(
        mark(&edit.text, &edit.caret),
        "see []()|".replace("()|", "(|)")
    );
}

#[test]
fn every_toolbar_operation_leaves_a_parseable_document() {
    let source = "# Heading\n- one\nplain\n1. two\n";
    for at in 0..=source.len() {
        if !source.is_char_boundary(at) {
            continue;
        }
        let caret = at..at;
        let mut results = vec![
            wrap(source, &caret, &Wrap::Bold),
            wrap(source, &caret, &Wrap::Highlight(Some("blue".to_owned()))),
            link(source, &caret),
        ];
        results.extend(
            [
                block(source, &caret, Block::Bullet),
                block(source, &caret, Block::Heading(1)),
                divider(source, &caret),
                code_block(source, &caret),
            ]
            .into_iter()
            .flatten(),
        );
        for edit in results {
            assert!(
                edit.text.is_char_boundary(edit.caret.start)
                    && edit.text.is_char_boundary(edit.caret.end)
                    && edit.caret.end <= edit.text.len(),
                "caret {:?} is not valid in {:?} at {at}",
                edit.caret,
                edit.text
            );
            let doc = trackcrab::markdown::parse(&edit.text);
            assert_eq!(
                doc.lines.len(),
                doc.inline.len(),
                "parse came apart at {at}"
            );
        }
    }
}

// ------------------------------------------------ clicks and paste (D7)

use trackcrab::markdown::edit::{paste_link, toggle_task};

fn pasted(marked: &str, url: &str) -> Option<String> {
    let (text, caret) = unmark(marked);
    let edit = paste_link(&text, &caret, url)?;
    Some(mark(&edit.text, &edit.caret))
}

#[test]
fn toggling_ticks_an_empty_box() {
    assert_eq!(expect(toggle_task, "- [ ] pay r|ent"), "- [x] pay r|ent");
}

#[test]
fn toggling_unticks_a_ticked_box() {
    assert_eq!(expect(toggle_task, "- [x] pay r|ent"), "- [ ] pay r|ent");
}

#[test]
fn toggling_never_moves_the_caret() {
    // The whole reason clicking a box is safe: the click has already placed the
    // caret, and the two states are the same length, so there is nothing to fix.
    let source = "- [ ] a\n- [x] b\n";
    for at in 0..=source.len() {
        let Some(edit) = toggle_task(source, &(at..at)) else {
            continue;
        };
        assert_eq!(edit.caret, at..at, "the caret moved from {at}");
        assert_eq!(edit.text.len(), source.len(), "the length changed at {at}");
    }
}

#[test]
fn toggling_works_from_anywhere_on_the_line() {
    for marked in [
        "|- [ ] task",
        "- |[ ] task",
        "- [ |] task",
        "- [ ] |task",
        "- [ ] task|",
    ] {
        let (text, caret) = unmark(marked);
        let edit = toggle_task(&text, &caret).expect("declined");
        assert_eq!(edit.text, "- [x] task", "failed from {marked}");
    }
}

#[test]
fn toggling_finds_the_box_through_an_indent() {
    assert_eq!(
        expect(toggle_task, "- one\n    - [ ] de|ep"),
        "- one\n    - [x] de|ep"
    );
}

#[test]
fn toggling_declines_on_anything_that_is_not_a_task() {
    for marked in ["- bul|let", "1. num|ber", "# head|ing", "pl|ain", "|"] {
        assert!(
            apply(toggle_task, marked).is_none(),
            "{marked} should not have been toggled"
        );
    }
}

#[test]
fn a_pasted_url_wraps_the_selection() {
    assert_eq!(
        pasted("see |the docs| for more", "https://example.com/a"),
        Some("see [the docs](https://example.com/a)| for more".to_owned())
    );
}

#[test]
fn a_pasted_url_with_nothing_selected_is_left_to_egui() {
    // Nothing to hang it on, and the bare address autolinks anyway.
    assert!(pasted("see |the docs", "https://example.com").is_none());
}

#[test]
fn pasting_something_that_is_not_a_url_is_left_to_egui() {
    for clipboard in [
        "just some words",
        "example.com",
        "ftp://example.com",
        "https://",
        "https://a.com https://b.com",
        "",
    ] {
        assert!(
            pasted("see |the docs| here", clipboard).is_none(),
            "{clipboard:?} should not have been treated as a URL"
        );
    }
}

#[test]
fn a_pasted_url_keeps_its_trailing_bracket() {
    // The autolinker gives a trailing `)` back to the sentence, because inside
    // prose it usually belongs there. A pasted address has no sentence around
    // it, so the bracket is part of the address.
    let url = "https://en.wikipedia.org/wiki/Foo_(bar)";
    assert_eq!(
        pasted("|Foo| is a thing", url),
        Some(format!("[Foo]({url})| is a thing"))
    );
}

#[test]
fn a_label_that_would_break_the_link_declines() {
    // Any of these would parse as something other than a link, so the ordinary
    // paste is the better answer: plainer, never wrong.
    for marked in [
        "a |[b]| c",
        "a |b(c)| d",
        "a |b]c| d",
        "a |one\ntwo| three",
    ] {
        assert!(
            pasted(marked, "https://example.com").is_none(),
            "{marked} should have declined"
        );
    }
}

#[test]
fn a_pasted_url_is_trimmed() {
    // Copying from a browser bar or a chat message picks up whitespace.
    assert_eq!(
        pasted("|here|", "  https://example.com\n"),
        Some("[here](https://example.com)|".to_owned())
    );
}

#[test]
fn a_pasted_link_reparses_as_the_link_it_looks_like() {
    let (text, caret) = unmark("see |the docs| now");
    let edit = paste_link(&text, &caret, "https://example.com/x").expect("declined");
    let doc = trackcrab::markdown::parse(&edit.text);
    let byte = edit.text.find("the docs").expect("label survived");
    let target = doc.link_at(byte).expect("the label should be a link");
    assert_eq!(&edit.text[target], "https://example.com/x");
}

#[test]
fn the_new_operations_lose_no_text_either() {
    let source = "- [ ] one\n- [x] two\n# Heading\nplain\n";
    for at in 0..=source.len() {
        if !source.is_char_boundary(at) {
            continue;
        }
        let mut results = vec![];
        results.extend(toggle_task(source, &(at..at)));
        // A selection to the end of the line, so paste_link has a label.
        let line_end = source[at..].find('\n').map_or(source.len(), |n| at + n);
        results.extend(paste_link(
            source,
            &(at..line_end),
            "https://example.com",
        ));
        for edit in results {
            assert!(
                edit.text.is_char_boundary(edit.caret.start)
                    && edit.text.is_char_boundary(edit.caret.end)
                    && edit.caret.end <= edit.text.len(),
                "caret {:?} invalid at {at}",
                edit.caret
            );
            let doc = trackcrab::markdown::parse(&edit.text);
            assert_eq!(doc.lines.len(), doc.inline.len(), "parse came apart at {at}");
        }
    }
}

// ------------------------------------------- nested emphasis (the * overlap)

#[test]
fn italic_over_bold_gives_both_rather_than_stripping_a_layer() {
    // `**bold**` ends with `*`, so a string match said "italic sits here" and
    // took a layer off. Bold and italic have to compose.
    assert_eq!(wrapped("**|x|**", &Wrap::Italic), "***|x|***");
}

#[test]
fn bold_over_italic_gives_both() {
    assert_eq!(wrapped("*|x|*", &Wrap::Bold), "***|x|***");
}

#[test]
fn italic_off_both_leaves_bold() {
    assert_eq!(wrapped("***|x|***", &Wrap::Italic), "**|x|**");
}

#[test]
fn bold_off_both_leaves_italic() {
    assert_eq!(wrapped("***|x|***", &Wrap::Bold), "*|x|*");
}

#[test]
fn the_star_styles_round_trip_in_either_order() {
    // Four presses, and back where it started, whichever way round.
    for (first, second) in [
        (&Wrap::Bold, &Wrap::Italic),
        (&Wrap::Italic, &Wrap::Bold),
    ] {
        let mut marked = "|x|".to_owned();
        for what in [first, second, first, second] {
            marked = wrapped(&marked, what);
        }
        assert_eq!(marked, "|x|", "did not round trip");
    }
}

#[test]
fn a_run_is_not_matched_across_a_selection_it_does_not_touch() {
    // The delimiters have to be *adjacent*. A bold word elsewhere on the line
    // is nothing to do with this selection.
    assert_eq!(
        wrapped("**bold** and |plain|", &Wrap::Italic),
        "**bold** and *|plain|*"
    );
}

#[test]
fn selecting_the_stars_too_still_composes_correctly() {
    // The delimiters-inside-the-selection path has the same overlap to get
    // right: an italic press on a selected `**x**` must not strip one star.
    assert_eq!(wrapped("|**x**|", &Wrap::Italic), "*|**x**|*");
}

#[test]
fn unwrapping_from_inside_the_selection_matches_the_style() {
    assert_eq!(wrapped("|***x***|", &Wrap::Bold), "*x*|");
    assert_eq!(wrapped("|*x*|", &Wrap::Italic), "x|");
}

#[test]
fn the_other_delimiters_have_no_overlap_to_worry_about() {
    for (marked, what, expected) in [
        ("~~|x|~~", &Wrap::Strike, "|x|"),
        ("__|x|__", &Wrap::Underline, "|x|"),
        ("`|x|`", &Wrap::Code, "|x|"),
        ("==|x|==", &Wrap::Highlight(None), "|x|"),
    ] {
        assert_eq!(wrapped(marked, what), expected, "failed on {marked}");
    }
}

#[test]
fn a_coloured_highlight_toggles_whole() {
    // Its opening delimiter carries a payload, so it is matched whole rather
    // than as a run. Written with explicit offsets rather than through the
    // caret helper, because the colour syntax needs the same `|` the helper
    // uses to mark a caret.
    let yellow = Wrap::Highlight(Some("yellow".to_owned()));

    let on = wrap("x", &(0..1), &yellow);
    assert_eq!(on.text, "==yellow|x==");

    let inner = on.text.find('x').expect("the label survived");
    let off = wrap(&on.text, &(inner..inner + 1), &yellow);
    assert_eq!(off.text, "x");
}
