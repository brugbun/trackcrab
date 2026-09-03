//! Block decoration planning.
//!
//! The half of D4 that can be asserted without a window: which lines get a
//! bullet, which run of lines is one code block, what number is drawn. The
//! drawing itself is deliberately thin, and is checked by eye and by the
//! geometry tests in `tests/theme.rs`.

use trackcrab::markdown::parse;
use trackcrab::ui::blocks::{Decoration, plan};

fn planned(source: &str) -> Vec<Decoration> {
    plan(&parse(source))
}

#[test]
fn nothing_is_decorated_in_ordinary_prose() {
    assert!(planned("just some words\n\nand more").is_empty());
    assert!(planned("# A heading\n**bold** text").is_empty());
}

#[test]
fn a_bullet_is_planned_per_bullet_line() {
    assert_eq!(
        planned("- one\n- two"),
        [
            Decoration::Bullet { line: 0, depth: 0 },
            Decoration::Bullet { line: 1, depth: 0 },
        ]
    );
}

#[test]
fn nesting_depth_is_carried_through() {
    // The depth is what the painter offsets the marker by, and what the
    // layouter indents the text by. If they disagree the bullet misses its own
    // line, which is why both read it from the same place.
    assert_eq!(
        planned("- a\n  - b\n    - c"),
        [
            Decoration::Bullet { line: 0, depth: 0 },
            Decoration::Bullet { line: 1, depth: 1 },
            Decoration::Bullet { line: 2, depth: 2 },
        ]
    );
}

#[test]
fn a_number_carries_the_value_that_was_typed() {
    // Not a recount. Renumbering behind the user's back would fight them as
    // they edit, and the parser already keeps what they wrote.
    assert_eq!(
        planned("3. three\n7. seven"),
        [
            Decoration::Number {
                line: 0,
                depth: 0,
                value: 3
            },
            Decoration::Number {
                line: 1,
                depth: 0,
                value: 7
            },
        ]
    );
}

#[test]
fn checkboxes_carry_their_state() {
    assert_eq!(
        planned("- [ ] open\n- [x] done"),
        [
            Decoration::Check {
                line: 0,
                depth: 0,
                checked: false
            },
            Decoration::Check {
                line: 1,
                depth: 0,
                checked: true
            },
        ]
    );
}

#[test]
fn a_checkbox_is_not_also_a_bullet() {
    // `- [x] ` starts with `- `, so a classification slip would draw both a
    // bullet and a box in the same gutter.
    let planned = planned("- [x] done");
    assert_eq!(planned.len(), 1);
    assert!(matches!(planned[0], Decoration::Check { .. }));
}

#[test]
fn a_divider_gets_one_rule() {
    assert_eq!(planned("above\n---\nbelow"), [Decoration::Rule { line: 1 }]);
}

#[test]
fn a_code_block_is_one_decoration_spanning_its_fences() {
    // One background rather than one per line, so the corners round and the
    // block reads as a single object.
    let source = "before\n```rust\nlet x = 1;\nlet y = 2;\n```\nafter";
    let planned = planned(source);
    assert_eq!(planned.len(), 1);
    let Decoration::Code { first, last, lang } = &planned[0] else {
        panic!("expected a code block, got {planned:?}")
    };
    assert_eq!(
        (*first, *last),
        (1, 4),
        "the band should include both fences"
    );
    assert_eq!(&source[lang.clone()], "rust");
}

#[test]
fn a_fence_with_no_language_still_plans_a_block() {
    let source = "```\nplain\n```";
    let planned = planned(source);
    let Decoration::Code { lang, .. } = &planned[0] else {
        panic!("expected a code block")
    };
    assert!(source[lang.clone()].is_empty());
}

#[test]
fn an_unclosed_fence_runs_its_background_to_the_end() {
    // The parser lets an unclosed fence run to the end of the document, which
    // is what makes a block usable while you are still typing in it. The
    // background has to agree, or it would stop short and leave the tail
    // unpainted.
    let source = "```py\none\ntwo";
    let planned = planned(source);
    assert_eq!(planned.len(), 1);
    let Decoration::Code { first, last, .. } = &planned[0] else {
        panic!("expected a code block")
    };
    assert_eq!((*first, *last), (0, 2));
}

#[test]
fn markup_inside_a_code_block_plans_nothing() {
    // A dash in a shell script must not sprout a bullet.
    let source = "```sh\n- not a bullet\n# not a heading\n---\n```";
    let planned = planned(source);
    assert_eq!(planned.len(), 1, "only the block itself: {planned:?}");
    assert!(matches!(planned[0], Decoration::Code { .. }));
}

#[test]
fn two_code_blocks_are_two_decorations() {
    let source = "```\na\n```\nmiddle\n```\nb\n```";
    let blocks: Vec<_> = planned(source)
        .into_iter()
        .filter(|d| matches!(d, Decoration::Code { .. }))
        .collect();
    assert_eq!(blocks.len(), 2);
}

#[test]
fn decorations_come_out_in_document_order() {
    // The painter walks them once, and a code background has to be queued
    // before the markers that sit on top of it.
    let source = "- a\n---\n```\nx\n```\n1. b\n- [ ] c";
    let lines: Vec<usize> = planned(source).iter().map(Decoration::line).collect();
    assert!(
        lines.windows(2).all(|w| w[0] <= w[1]),
        "out of order: {lines:?}"
    );
}

#[test]
fn a_mixed_document_plans_exactly_what_it_should() {
    let source = "\
# Heading
- bullet
  - nested
1. numbered
- [x] done
- [ ] todo
---
```rust
code
```
plain";
    let planned = planned(source);
    // The language is compared as text, not as an offset: a hardcoded range
    // tests the test's own arithmetic rather than the planner.
    let Some(Decoration::Code { first, last, lang }) = planned.last() else {
        panic!("expected the code block last, got {planned:?}")
    };
    assert_eq!((*first, *last), (7, 9));
    assert_eq!(&source[lang.clone()], "rust");

    assert_eq!(
        planned[..planned.len() - 1],
        [
            Decoration::Bullet { line: 1, depth: 0 },
            Decoration::Bullet { line: 2, depth: 1 },
            Decoration::Number {
                line: 3,
                depth: 0,
                value: 1
            },
            Decoration::Check {
                line: 4,
                depth: 0,
                checked: true
            },
            Decoration::Check {
                line: 5,
                depth: 0,
                checked: false
            },
            Decoration::Rule { line: 6 },
        ]
    );
}

#[test]
fn every_planned_line_exists_in_the_document() {
    // A decoration naming a line past the end would be looked up against the
    // galley and silently skipped, hiding the mistake.
    for source in [
        "",
        "\n",
        "- a",
        "```",
        "```\n",
        "---",
        "- [x] x\n```py\nz",
        "1. a\n2. b\n---\n```\n```",
    ] {
        let doc = parse(source);
        for item in plan(&doc) {
            assert!(
                item.line() < doc.lines.len(),
                "{item:?} names a line past the end of {source:?} ({} lines)",
                doc.lines.len()
            );
            if let Decoration::Code { last, .. } = item {
                assert!(
                    last < doc.lines.len(),
                    "a code band ends past the document in {source:?}"
                );
            }
        }
    }
}

#[test]
fn the_indent_is_clamped_so_deep_nesting_cannot_run_off_the_panel() {
    // Pasted text can carry absurd indentation. Without a ceiling the content
    // would be pushed off the right of the panel with no way back.
    use trackcrab::ui::theme::list_indent;
    let deep = list_indent(1000);
    let capped = list_indent(trackcrab::ui::theme::metric::MAX_DEPTH);
    assert!((deep - capped).abs() < f32::EPSILON);
    assert!(list_indent(2) > list_indent(1), "and it still steps");
}
