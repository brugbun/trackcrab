# Markdown - Build Plan

Live markdown in notes, comments and task descriptions. Formatting applies as
you type, Discord style, with a toolbar for anyone who does not want to learn
the syntax.

## The dialect

Discord's, not CommonMark. Confirmed with Kyle.

| Syntax | Means |
|---|---|
| `**text**` | bold |
| `*text*`, `_text_` | italic |
| `__text__` | **underline**, not bold |
| `~~text~~` | strikethrough |
| `` `text` `` | inline code |
| ```` ```lang ```` | fenced code block |
| `# ` to `#### ` | headings 1 to 4 |
| `- `, `* `, `1. ` | lists, nested by indent |
| `- [ ] `, `- [x] ` | task checkboxes |
| `---` | divider |
| `[text](url)`, bare `https://` | links, opened in the default browser |
| `==text==` | highlight, default colour |
| `==yellow\|text==` | highlight, named palette colour |
| `==#f2c14e\|text==` | highlight, any hex colour |

`__text__` meaning underline is the one real divergence from CommonMark. It is
what people expect from a chat-style editor, and the only way to reach underline
without inventing a syntax.

The highlight syntax is ours alone. Named colours are the default route because
hex codes in prose are unreadable and a palette stays themeable; hex is the
escape hatch for the cases a palette cannot cover.

## Confirmed decisions

- **Descriptions get it too**, not just notes and comments. Leaving them plain
  next to two rich fields would look like an oversight.
- **Nested lists**, indented with two spaces per level. A tab in pasted text
  counts as one level, but Tab typed in the editor inserts spaces, so a file
  never ends up with both.
- **Basic code fences only for now.** Mono font, background, language tag. Real
  per-language syntax highlighting is a separate question Kyle will come back to
  if the plain version disappoints.
- **Checkboxes are in**, as a just-in-case.
- **`Ctrl+B` moves to bold.** It currently toggles the folder tree, and
  `Ctrl+Right` already does that, so the binding is freed rather than fought
  over. `Ctrl+I` and `Ctrl+U` were unused.
- **The edit/preview fallback stays on the shelf.** If the live approach does not
  work out, raw markdown while focused and rendered when not is roughly a third
  of the effort and has no caret weirdness. Not the plan, but the plan B.

### Two things this changes elsewhere

- **Tab stops being an exit.** `lock_focus(true)` is what makes Tab indent
  rather than move focus, so Escape or a click becomes the way out of a note.
- **`Ctrl+N` gets gated.** It is currently checked before the typing guard, so
  `Ctrl+N` mid-note creates a task. Pre-existing, fixed as part of D6.

## What the model does *not* do

Nothing. Markdown is the source of truth and stays a plain `String`, so there is
no schema bump, files stay readable and grep-able, and the whole feature is
reversible by deleting the renderer.

The one visible consequence: existing notes are reinterpreted. A note that
literally says `**important**` starts rendering bold. Harmless, and the stored
text never changes.

## Steps

| | Step | Risk |
|---|---|---|
| D1 | Parser, headless | low |
| D2 | Inline rendering through a custom layouter | low |
| D3 | Hide and reveal the markup | **high** |
| D4 | Block rendering | medium |
| D5 | Editing behaviour: Tab, Enter, Backspace | **high** |
| D6 | Toolbar and shortcuts | medium |
| D7 | Clicks: links and checkboxes | low |
| D8 | Search and finish | low |

D3 and D5 stay standalone whatever else gets merged: they are where this will
feel wrong, and they are much easier to judge in isolation.

---

## D1 as built - done

The parser. 75 tests, clippy clean at pedantic, no UI whatsoever.

`src/markdown/` in three files: `line.rs` for the block pass, `inline.rs` for
the character pass, `mod.rs` for the document-level API and the dialect
documentation.

### Shape of the output

**One `Line` per source line**, not one per logical block. Every block-level
thing here is line-scoped anyway, and the renderer works from egui galley rows,
which map to lines. A fenced block therefore comes out as an opening line, some
`Code` lines and a closing line, which is exactly what a painter needs to draw
one background across the run.

`marker` and `content` always partition the line, so hiding the marker leaves
exactly the content. The marker swallows any leading indent, so collapsing it
collapses the whitespace too and D4 is free to draw its own indent instead. A
paragraph is the exception: it has no marker, so swallowing its indent would
silently reflow text the user typed.

**Inline output is flat and non-overlapping**, each span carrying the
accumulated style of every delimiter it sits inside. Flat because that is
precisely the shape a `LayoutJob` wants, so D2 becomes a loop with no traversal
of its own. `**bold *and italic* here**` comes out as three spans, the middle
one carrying both flags.

Markup ranges come back separately from spans. That is what makes D3 possible
without ever rewriting the source string.

### Rules that are decisions, not accidents

- **Inline markup never crosses a line.** One stray `**` cannot reformat the
  rest of a long note. Matches Discord.
- **`_` respects word boundaries, `*` does not.** `snake_case_name` stays plain.
  This is the single most common complaint about markdown editors, and
  CommonMark treats the two delimiters differently for the same reason.
- **Unmatched delimiters are literal.** `**bold` with no close renders as typed
  and hides nothing. Half-typed markup must never make text vanish.
- **Nothing applies inside code.** A Rust `*ptr` in a fence stays a pointer.
- **Dividers are dashes only.** CommonMark also allows `***` and `___`, which
  collide with the bold and underline delimiters; accepting them would make
  `***` ambiguous.
- **Five hashes is a paragraph.** Nothing styles a heading that small, so it
  should not silently render as an H4.
- **A hash needs its space.** Otherwise every `#tag` in prose is a heading.
- **An unrecognised highlight prefix is content, not a failure.**
  `==yelow|text==` is a default highlight showing the stray word, which is
  obvious. Making the whole highlight vanish would not be.

### Testing

Three structural invariants are asserted over a shared corpus of awkward inputs,
which is what makes the rest safe:

1. Spans plus markup cover each line's content **exactly**, no gaps and no
   overlaps. This is the promise the renderer stands on.
2. Every range lands on a character boundary. Byte ranges go straight to egui,
   which panics on a range that splits a character.
3. No empty span or markup range is ever emitted.

Then a **deterministic fuzz**: 2,800 delimiter-dense inputs from a seeded
generator, asserting the same invariants. It found nothing, but it is the test
most likely to catch a future change, and it comes with a guard against becoming
vacuous: if the corpus stops producing nested markup, the test fails on that
rather than passing while testing nothing.

Writing the fuzz did surface a real weakness in the fuzz itself. A stray
```` ``` ```` opens a fence and everything after it is verbatim, so most of each
multi-line sample was quietly retired from the part of the parser under test.
There is now a single-line variant so the inline scanner sees every sample.

### Two things found by the tests

- **A latent bug in the escape skip.** While searching for a closing delimiter, a
  backslash that escapes *nothing* was skipping the following character anyway,
  which could step over a delimiter one character further on. Now it skips only
  itself.
- **Three test expectations of mine were simply wrong**, on the fence language
  offset, the escaped-star span count, and tab indent depth. Worth recording
  because in each case the code was right and the test was the thing that needed
  fixing.

### One clippy lint deliberately silenced

`struct_excessive_bools` on `Style`. It suggests a state machine, but these are
not states: a span can be bold and italic and underlined at once, which is the
whole point of the recursive scan. `egui::TextFormat` models the same thing the
same way, so folding them into an enum would only have to be unfolded again at
the point of use. Silenced with `#[expect]` and a reason, so it comes back if
the lint ever stops applying.

### Also shipped

`ui::Filter` will want `markdown::plain`, which strips every marker and
delimiter. That is D8's whole job, and since it is parser work it is done and
tested now: searching for `*` no longer matches every emphasised word.

`cargo run --example parse_markdown` prints the parse of the built-in sample, or
of any file. Useful from D2 onward: when something will not format, it says
whether the parser or the renderer is at fault.

---

## D2 as built - done

Inline formatting, live, in all three fields: task descriptions, task notes and
comment spaces, plus the new-task dialog's description so the four are
consistent. 314 tests, clippy clean at pedantic.

### The layouter is a pure function, on purpose

`ui::text::layout(source, wrap_width, &Config) -> LayoutJob` takes no `Ui` and
no context. That is the only way to assert what bold *renders as*: egui's
accessibility tree exposes a `TextEdit` as its raw string and says nothing at
all about formatting, so a UI test can prove the text is present and nothing
about whether it is emphasised. `tests/layout.rs` asks the question directly,
looking formats up by byte position rather than by section index, because
`LayoutJob::append` merges adjacent sections that share a format and asserting
on section counts would be asserting on that optimisation.

`ui::text::layouter` is the thin egui-facing wrapper. It memoises the job
through `egui::cache::FrameCache`, keyed on the text, the wrap width and the
body size, since the layouter runs every frame and re-parsing a long note sixty
times a second is waste. It caches the *job* rather than the galley because
epaint already caches galleys internally.

### The invariant that holds the whole feature up

**`job.text` must equal the source byte for byte.** A `TextEdit` maps caret
positions through the galley, so a job that dropped or reordered a byte would
put the caret in the wrong place and corrupt edits. `layout` carries a
`debug_assert_eq!` for it, `tests/layout.rs` asserts it over a corpus, and a UI
test renders a document using every feature through the real widget, which makes
the debug assertion fire on the wiring rather than on the function.

This is also why **D3 must collapse markup by shrinking it, not by omitting
it**. Leaving delimiters out of the job would break the caret. D1's decision to
report markup as ranges rather than strip it is what makes that possible.

### The font

egui ships **Ubuntu-Light and nothing else** for proportional text: no bold
face, and no faux bold anywhere in epaint either, so `**bold**` was literally
unreachable without adding a font. Ubuntu is not obtainable in this environment
and no crate vendors it, so the family had to change.

**Work Sans Regular + Bold** is bundled in `assets/fonts/`, SIL Open Font
License, 380KB for the pair. Humanist like Ubuntu, so it is the smallest visual
change that buys a real bold. It goes in *front* of the existing chain rather
than replacing it, so Ubuntu-Light stays as a fallback and, more importantly,
the two emoji fonts stay reachable: the burger is U+2630, which lives in
`emoji-icon-font` and nowhere else bundled. There is a test asserting the burger
still measures differently from the replacement character, because that is
exactly the bug M6 spent time on.

Italic is **not** bundled. `TextFormat::italics` is a real faux slant applied in
the tessellator, so it costs nothing. Real italic faces exist in the same family
and are a drop-in later if the slant disappoints: two more families and two more
arms in the lookup, nothing else.

Bold is asserted twice over, deliberately. The layout tests prove the layouter
*asks for* the bold family; a theme test installs the fonts into a real context
and measures the same string in both families, because if the registration were
broken bold would silently fall back and the layout tests would still pass.

### Highlight colours

The named palette is deliberately **muted**, not the marker-pen colours the
names suggest: this is a dark interface, and a saturated yellow behind light
text is unreadable. A test asserts 4.5:1 contrast for all nine.

Arbitrary hex is the interesting case, since nothing stops someone writing
`==#ffffff|text==`. The text colour is chosen from the background, and the first
attempt used a luminance threshold. A test sweeping the RGB cube found the worst
case at **2.04:1** on a mid pink, because a threshold has to guess where the
crossover is and guesses wrong in the middle of the range. Measuring both
candidates and taking the winner costs nothing and cannot be wrong: the worst
case over the whole cube is now **3.75:1**, which clears WCAG AA for large text
and is the mathematical limit for two fixed text colours. The test's threshold
sits just under that so a regression to the threshold approach fails at once.

### Small decisions

- **Code wins the family over bold.** No bold monospace face is bundled, and a
  proportional font would defeat the point of marking a run as code.
- **A block marker is drawn at the size of the text it belongs to**, so a `## `
  in front of a heading occupies the same line height. Otherwise the row jumps
  as the caret moves in and out of the marker, which looks like the text
  twitching.
- **Headings are bold as well as bigger.** Size alone reads as a zoomed
  paragraph rather than a heading.
- **Struck text steps back to 75% brightness.** Left at full strength with a
  line through it, it reads as emphasis rather than deletion.
- **Heading scale is tight** (1.60 / 1.38 / 1.20 / 1.08 of body). The notes
  panel is narrow, and an H1 at twice body size wraps after three words.
- Newlines are appended from the gaps *between* line ranges rather than as a
  literal `"\n"`, which handles CRLF with no special case and is what guarantees
  the byte invariant.

### Open question for Kyle

The font change affects the **whole interface**, not just the markdown fields.
Work Sans reads noticeably denser than Ubuntu-Light. Two options:

- **A, as shipped:** one family everywhere. Internally consistent.
- **B:** keep Ubuntu-Light for the panel chrome and use Work Sans only inside
  the three markdown fields. The app looks exactly as it did, and a UI font
  distinct from a content font is a normal typographic distinction. No mixing
  within a run of text, since regular and bold both come from Work Sans. About
  ten lines to switch.

---

## Option B - done

The panel chrome keeps egui's Ubuntu-Light; the bundled family is used only
inside the markdown fields. `Proportional` is left exactly as egui built it and
the two Work Sans faces are registered as named families (`theme::BODY`,
`theme::BOLD`) reached only from the layouter. Both chains keep the existing
fallbacks behind them, which matters for the burger (U+2630, `emoji-icon-font`
only) as much as for coverage.

There is no mixing *within* a run of text, which was the thing to avoid: regular
and bold both come from Work Sans, so a bold word sits in the same family as the
sentence around it.

### A real bug this uncovered, which predated D3

Plain markdown text was still being laid out in `FontFamily::Proportional`, so
in D2 **regular text was Ubuntu-Light while bold was Work Sans**: mixed families
inside one sentence, exactly what option A was supposed to avoid and option B
was supposed to make impossible.

The D2 tests missed it because they only asserted the *bold* family. The fix was
two lines; the more useful outcome is the test, which now walks every section
`layout` produces and fails on any run of real text laid out in the chrome face.
Asserting on the raw families alone is what let it through.

---

## D3 as built - done

Markup hides once the caret leaves its line, Discord and Obsidian style. 327
tests, clippy clean at pedantic.

### How markup is hidden

**Shrunk, not removed.** `layout` renders a collapsed marker at 0.01pt and
transparent. The characters stay in `job.text`, which is what keeps the caret
honest: a `TextEdit` maps caret positions through the galley, so omitting bytes
would move the caret and corrupt edits.

The number was measured, not guessed. `0.0` trips a debug assertion in epaint's
glyph cache. At 0.01 two asterisks measure 0.03px against 35.6px of text and the
row height does not move, so there are tests asserting both: that hiding the
delimiters leaves the same width as never typing them, and that the row height
is identical revealed or hidden. A row that changed height as the caret arrived
would look like the text twitching.

### Which markers hide, and when

The rule: **a marker collapses once the styling already carries its meaning.**

- **Hidden now:** every inline delimiter, the highlight colour prefix, the link
  target, escape backslashes, and a heading's hashes. A heading is already
  heading-sized, so `## ` is redundant.
- **Still visible:** list markers, checkboxes, dividers and code fences. Nothing
  yet *draws* a bullet or a rule, so hiding them would lose information. D4
  flips each one as it takes over the drawing.

This keeps D3 from leaving the app worse than D2 for lists, which a
hide-everything approach would have done.

### Reading the caret

`Reveal` is `Nothing` or `At(Range<usize>)`, and every line the range touches
shows its markup. A selection therefore reveals every line it spans.

The boundary case is asymmetric by construction: a caret at the end of one line
is not at the start of the next, because the newline sits between them, so
exactly one line reveals. There is a test for it, because revealing two lines at
once looks like a glitch.

The caret comes from the *stored* `TextEditState`, which is last frame's: the
layouter runs during `show`, before the widget has processed this frame's input,
so there is nothing fresher to read. `text::edit` closes the one frame gap by
computing the reveal before and after `show` and asking for a repaint when they
differ. Without that, clicking into a field would leave the markup hidden until
some unrelated event caused another frame. Asking only *on a change* means a
focused field is not repainting for nothing.

That is also why `TextEdit::show` is used rather than `ui.add`: only `show`
hands back the cursor.

### One wrapper for all four fields

`text::edit` with a `text::Field` describing the differences. All four fields go
through it rather than each building a `TextEdit`, so the reveal behaviour cannot
drift between them and the explicit id, which the reveal lookup depends on,
cannot be forgotten. A sized field reserves its space in a child `Ui` so both
shapes take the same path and the repaint logic applies to all four.

### Known and accepted

- **Arrowing across hidden markup takes two presses with no visible movement.**
  The characters are still there. Obsidian behaves identically; there is no fix
  that does not break the caret.
- **Revealing a marker shifts its line right** by the marker's width. Inherent to
  showing it at all, and again what Obsidian does.
- **The field ids are fixed**, so the caret position carries when you switch task
  or comment space. egui clamps it to the galley, so it is harmless, and landing
  in roughly the same place is arguably the nicer behaviour.

### One test worth calling out

`only_markup_is_ever_collapsed` asserts every collapsed section against the
*parser's own* markup ranges rather than a guessed character class. The first
version tried "does this look like markup" and failed on a link target, which
legitimately contains arbitrary URL characters. "Did the parser call it markup"
is the right question and reads from the same source of truth the renderer does.

---

## D4 as built - done

Bullets, numbers, checkboxes, dividers, code blocks and nested indents. 350
tests, clippy clean at pedantic. Every remaining block marker now hides.

### Split in two, so the decisions stay testable

A galley is a run of laid-out text with no concept of blocks, so none of this
can come out of the layouter. `ui::blocks` therefore has two halves:

- **`plan(&Document) -> Vec<Decoration>`** is pure. A decoration names a line
  and a shape and carries **no coordinates**, so every interesting decision is
  unit testable: which lines get a bullet, which run of lines is one code block,
  what number to draw.
- **`paint`** resolves those to screen rectangles and draws. Deliberately thin.

Line positions come from `Galley::pos_from_cursor`, which is the same mapping
the caret itself uses, so a decoration cannot end up on a different row from the
text it belongs to. No reaching into row internals.

### Two collapse modes, which was not obvious

D3 had one way to hide a marker: shrink it to 0.01pt. That breaks for a marker
that **is** the whole row. Shrink `---` and the row collapses to nothing,
leaving a divider with no height to draw in, a code fence with nowhere to put
its language tag, and a line the caret cannot be clicked onto.

So there are two:

- **Width** for a marker sharing its row with text (headings, list markers). The
  size removes the width; the content is indented past a drawn marker instead.
- **Ink** for a row that is entirely markup (dividers, fences). Keeps its height
  and loses only its colour.

### The alignment contract

The layouter reserves the space and the painter draws in it, so if the two
numbers disagree the bullet misses its own line. Both read
`theme::list_indent(depth)`, and `tests/layout.rs` pins the other term by
asserting the actual `leading_space` on the laid-out sections.

`leading_space` is also the limit of what is possible here: epaint only applies
it at the start of a paragraph, so **a list item that wraps loses the indent on
its continuation rows**. There is a TODO in epaint's own layout code saying as
much. Nothing in this codebase can fix it.

The indent is clamped at eight levels. Pasted text can carry absurd
indentation, and without a ceiling the content would be pushed off the right of
the panel with no way back.

### Four bugs the verification pass caught

- **The code background was narrower than the code.** It was derived from the
  clip rectangle less a padding constant, and the text is not subject to that
  padding, so long lines overflowed their own background. Now taken from the
  galley's **wrap width**, which is by definition where text stops.
- **Then the language tag vanished.** The wrap width can exceed the visible
  area, so the background's right edge, and the tag pinned to it, landed outside
  the clip rectangle and were silently dropped: the block looked right and the
  tag simply never appeared. The range is now clamped to the clip.
- **Two backgrounds behind every code block.** Fence and code lines still
  carried the per-character `background` from D2, which showed through the new
  rounded rectangle as a second, squarer shade. The text layer now leaves the
  background to the block; an *inline* code span still paints its own, since
  nothing is drawn behind it.
- **A revealed nested item jumped left onto its parent's indent.** Giving up the
  whole reserved space on reveal made the nesting appear to collapse as the
  caret arrived. A revealed item now keeps its depth and gives up only the
  gutter, since the raw `- ` is occupying roughly that gutter itself. What moves
  is the few pixels between the gutter and the marker's own width.

### And one bug D3's tests caught immediately

Deriving *inline* markup's collapse mode from the *block* marker's mode stopped a
plain paragraph hiding its own delimiters, because a paragraph has no block
marker to take a mode from. Two independent questions; they are now asked
separately.

### Small decisions

- **A revealed line's drawn marker steps aside**, so you see the source rather
  than a bullet on top of a dash. A code block keeps its background either way:
  the fence's backticks sitting on the background is right, and losing the whole
  block as the caret entered it would be jarring.
- **Numbers are drawn as typed, not recounted.** Renumbering behind the user as
  they edit would fight them, and they are right aligned in the gutter so `9.`
  and `10.` end at the same column.
- **The tick is painted, not typed.** The check glyphs are not in every bundled
  font, which is the trap the notebook arrows fell into in N4.
- **One background per code block, not per line**, so the corners round and the
  block reads as a single object. An unclosed fence runs to the end of the
  document, matching what the parser decided.

---

## D5 as built - done

Tab, Shift+Tab, Enter and Backspace, all list-aware. 392 tests, clippy clean at
pedantic.

### Pure, like the parser, and for the same reason

`markdown::edit` is four functions from a string and a caret to a new string and
a new caret. This is custom editing logic sitting next to egui's own, which is
exactly the sort of thing that goes subtly wrong, so all of it is asserted
headlessly. `tests/edits.rs` reads samples with the caret marked by a `|`, so an
assertion says what you would type and what you would see rather than talking in
byte offsets.

**`None` always means not our business.** The key was not ours, or the caret was
somewhere the rule does not apply, and egui then does whatever it would normally
have done. That is what keeps everything outside a list behaving exactly as
before.

### The behaviour

| Key | Does |
|---|---|
| `Tab` | Indents a list item one level; two spaces at the caret anywhere else |
| `Shift+Tab` | Outdents, treating a pasted tab as a whole level |
| `Enter` | Continues the list, incrementing a number, unticking a checkbox |
| `Enter` on an empty item | Removes the marker: the way *out* of a list |
| `Backspace` after a marker | Strips the marker, keeping the indent |

Two rules worth stating because they are judgement calls:

- **Tab will not nest an item under nothing**, and will not nest more than one
  level past the item above. `- a` followed by an item at depth 2 is ambiguous
  markdown and renders with a gap where the missing level should be. Blank lines
  are stepped over when working out the ceiling, since a blank line inside a
  list is common; anything else ends the list.
- **Backspace keeps the indent.** Removing it too would jump an item to the top
  level in one keystroke, and Shift+Tab is how depth is meant to change.

Backspace also strips a **heading** marker. Same rule, and deleting one
invisible character of collapsed markup instead is never what anyone wants.

### Two things about the wiring that were not obvious

**Consuming Tab is not enough to stop it moving focus.** egui resolves focus at
the *start* of the frame, from the raw events, long before any widget code runs,
and it is gated on the focused widget's event filter. So the field needs
`lock_focus(true)` as well, which tells egui the widget wants Tab. A UI test
asserts focus stays put, because the pure tests pass either way.

**The buffer is changed behind the widget's back**, so it has no idea anything
happened and `changed()` never fires, which means nothing is ever saved.
`text::edit` calls `Response::mark_changed()` when it applied an edit, and there
is a test that types Enter and then checks the result reached the disk.

Both Tabs are **always swallowed**, whether or not the operation acted. Since
`lock_focus` has told egui the widget wants Tab, a key handed back would be
inserted as a literal tab character and the file would end up indented two ways
at once. Enter and Backspace are handed back, which is the point of the `Option`.

**Escape is now the way out of a field.** That is egui's own behaviour, not
anything added here: a `TextEdit`'s event filter has `escape: false`, so egui's
focus system takes it.

### Two bugs the property tests caught

`no_operation_ever_loses_or_duplicates_text` runs all four operations at every
caret position in a document with one of everything in it:

- **Enter at the start of a list item inserted a marker above it**, producing an
  empty item and, for a numbered list, a stray number out of nowhere. At or
  before the content, Enter now pushes the item down instead.
- **Tab inside a fence delimiter broke the fence.** Two spaces in the middle of
  the backticks silently stops it being a fence at all and everything below is
  reinterpreted as prose. Tab now declines on a fence line, but still indents
  inside a code *body*, since indenting code is a normal thing to want.

Getting that property to express itself correctly took three attempts, which is
worth recording. Comparing raw characters flags a correct edit as data loss: a
numbered marker contains a digit and a checkbox contains an `x`. It only works
stated against the parser's own **content** ranges with whitespace dropped,
which is precisely "the markers may change, the text may not".

### The test that would catch any of this regressing

`a_whole_list_can_be_typed_without_writing_a_single_marker` types a real
document through the real widget, using only the *first* marker of each list and
letting Enter, Tab and Shift+Tab produce every other one, then asserts the exact
markdown that comes out:

```
# Shopping
- milk
- bread
  - flour
  - plain
- wholemeal
1. first
2. second
- [ ] pay rent
- [ ] call mum
```

---

## D6 — Toolbar and shortcuts

*Done when: every toolbar button has a keyboard equivalent and both leave the
caret somewhere sensible.*

Done. 423 tests, clippy clean at pedantic.

### What it does

A button bar above every notes and comments field, and the three emphasis chords
everyone expects. The bar reads:

```
H | B I U S <> | list number check | rule code link highlight
```

`H` and the highlight dot open menus. Everything else applies on click. All of it
works on a selection or on a bare caret, and all of it puts the caret back
somewhere you would have put it yourself.

| Button | Does | Keys |
|---|---|---|
| `H` | Heading 1-4, from a menu | |
| `B` | Bold | `Ctrl+B` |
| `I` | Italic | `Ctrl+I` |
| `U` | Underline | `Ctrl+U` |
| `S` | Strikethrough | |
| `<>` | Inline code | |
| bullet | Bulleted list | |
| number | Numbered list | |
| check | Checkbox | |
| rule | Divider | |
| `{ }` | Code block | |
| `link` | Link, caret in the target | |
| dot | Highlight: eight named colours, Default, or any hex | |

The heading menu shows each level at its own scale, so you pick the size you can
see rather than a number. The highlight menu shows each colour as a swatch with
its own name written on it, in whatever text colour `readable_on` chose, which
means the menu doubles as proof the contrast rule works.

### Where the work went

**`markdown::edit` grew the operations.** D5 gave it `tab`, `untab`, `enter` and
`backspace`. D6 adds `wrap`, `block`, `divider`, `code_block` and `link`, all the
same pure shape: text and caret in, `Option<Edit>` out, `None` meaning "not our
business". So the toolbar and the keyboard call exactly the same functions, and
the tests never touch egui to prove a button works.

`wrap` toggles. If the selection is already bold it takes the asterisks off
rather than adding four. It recognises both wrapped shapes, the one where the
delimiters are inside the selection and the one where they are outside it, since
which you get depends on whether the user selected before or after the emphasis
went on.

`block` is `None` inside a code block, because a heading inside a fence is not a
heading and silently converting one would lose the user's text.

**`ui::toolbar` is a pure widget.** `show(ui, salt) -> Option<Action>` — it draws
buttons and reports what was clicked, and knows nothing about text. `ui::text`
maps the `Action` onto the `edit` operation and applies it. Two layers, one
mapping, no button that can do something the keyboard cannot.

The four painted icons reuse `ui::blocks::{bullet, number, checkbox, rule}`, the
same functions that draw them in the document. That is not thrift, it is the
guarantee that the icon on the button is the mark you get.

### Four things that went wrong

**`Modifiers::COMMAND` silently did nothing under test.** `cmd_ctrl_matches`
means a `COMMAND` pattern does not match a synthesised `CTRL` event; on a real
machine the platform layer sets both flags, so the chords worked in the app and
failed in the harness, which is the worst way round. Switched the chords to
`CTRL`, matching every other shortcut in the app, and gave the tests a
`select_all` helper that deliberately uses `COMMAND+A` because egui's own
select-all checks the `command` flag.

**A toolbar click blurred the field.** egui decides focus from the previous
frame's clicks, so a `request_focus` issued during the click frame gets overridden
one frame later. Fixed with a `refocus` temp flag that the next `edit` call
re-requests at the top. Ugly, but it is the frame ordering that is ugly.

**`block()` used one caret offset for both endpoints** of a selection, so
converting three lines to bullets moved the selection onto the wrong text. Rewrote
it with a per-endpoint closure that distinguishes a selection anchor sitting at a
line start, which should stay put, from a plain caret, which should follow its
text.

**The painted icons were oversized and colliding.** I had reused the document
painters at document scale: a 12px checkbox in a 6px row. `bullet`, `number` and
`checkbox` are now size-aware from the rect they are given, the icons dropped from
three rows to two, and the buttons went to `TOOLBAR_BUTTON * 1.15` wide.

Also: painted buttons had no accessibility label. A tooltip is not a label. Every
toolbar button now carries an explicit `WidgetInfo::labeled` with a descriptive
name — "Bold", "Bulleted list", "Code block" — not its glyph.

### The `Ctrl+B` rebind

`Ctrl+B` toggled the sidebar. It is now Bold. `Ctrl+Left` and `Ctrl+Right` already
do the sidebar, so nothing was lost, and `keys::TOGGLE_SIDEBAR` is gone rather
than left dangling.

While in there, `Ctrl+N` and `Ctrl+Shift+N` moved behind the typing guard. They
created a task while the caret was in a description, which was always wrong and
only now noticeable because there is more to type into.


---

## D7 — Clicks

*Done when: a link opens in the browser, a checkbox ticks, and a pasted URL
wraps a selection, all without the caret ending up somewhere stupid.*

Done. 461 tests, clippy clean at pedantic.

### What it does

| Do this | Get this |
|---|---|
| Hover a link | Its address, in a tooltip |
| `Ctrl` + hover | The pointing hand |
| `Ctrl` + click a link | Opens in your default browser |
| Click a checkbox | Ticks or unticks it, caret untouched |
| `Ctrl+Enter` | Same, from the keyboard |
| Paste a URL over a selection | `[selection](url)` |

`Ctrl` for links rather than a bare click, because these fields are always
editable and a plain click has to keep meaning "put the caret here". The hand
cursor is modifier gated for the same reason: offering a hand when a click will
only move the caret is a promise the app does not keep. The tooltip is not
gated, because for a `[label](url)` it is the only way to see the address
without putting the caret on the line.

### Where the work went

**Two pure lookups.** `Document::link_at(byte)` answers "what would clicking
here open", as a byte range into the source. One question for both link forms,
because the parser had already folded them into one: a bare `https://...` span
points at itself, an explicit `[label](url)` span points inside its brackets. A
caller never has to know which it got.

`markdown::is_url` is the paste test, and it is deliberately **laxer than the
autolinker**. The autolinker has to decide where an address ends inside a
sentence, so it hands trailing punctuation back to the prose; that is right in
prose and wrong for a paste, where `.../Foo_(bar)` keeps its bracket because the
bracket is part of the address. Two questions, two answers, both documented
next to each other, rather than one rule bent to serve both.

**Two more edit operations**, same pure shape as the rest. `toggle_task` flips
`[ ]` and `[x]`; `paste_link` wraps a selection, declining on a label carrying
link punctuation or a newline, because either would produce something that
parses as not-a-link. Declining leaves the ordinary paste, which is never wrong,
only plainer.

**The hit test walks glyphs.** `Galley::cursor_from_pos` was the obvious tool
and the wrong one: it snaps to the *nearest* caret position, so on a short line
it answers with the last character however far right the pointer is, and a link
ending a line would have been live across the whole margin beside it. A link is
a containment question. Walking `galley.rows` and each row's glyph rectangles
answers it exactly, and it makes collapsed markup unhittable for free, since a
delimiter styled down to 0.01px is 0.01px wide here too.

**The checkbox is the only geometric hit.** It is painted rather than laid out,
so it is not in the galley and the text field knows nothing about it.
`blocks::hit` reuses `Rows` and `gutter` — the exact pair that positioned the
box in the first place — so a box you can see and cannot click is not a state
this can reach.

### The one that mattered

The first version worked and felt wrong. Clicking a box ticked it, and because
the click had also placed the caret on that line, the line revealed its own
source: the box you just ticked was replaced by `- [x] ` and a blinking caret.
The tick was in the file and the feedback on screen was the wrong thing
entirely, for the one action whose whole point is the tick appearing.

The gutter is not text, so a click on it must not behave like one. `interact`
now captures the caret before the frame's click and puts it back, and a field
that was not being edited is handed back unfocused: ticking something off a note
you were only reading should not drop you into editing it.

That needed the focus state captured by hand. egui resolves focus in
`Memory::begin_pass`, from the raw events, before any widget code runs, so by
the time the field's own code executes a click has *already* focused it and
there is no way left to ask whether it was focused a moment ago. One temp flag
carried forward a frame answers it.

### Two smaller ones

**Paste is intercepted before the widget sees it.** Left to egui, the selection
would already have been replaced by the address and there would be no label left
to hang it on. The event is only taken when the operation applies, so an
ordinary paste is untouched.

**`Ctrl+Enter` exists because a mouse-only checkbox is a checkbox some people
cannot tick.** Every other button on the bar had a keyboard equivalent; a
clickable box drawn in the gutter cannot be reached by moving the caret onto it,
so it needed its own chord rather than inheriting one.

### Testing a click

The pure half is pinned where it always is: `toggle_task` and `paste_link` at
every caret position in a mixed document, `link_at` at every byte of
`[label](https://example.com)` asserting that only the five bytes of the label
resolve and that they resolve to the address.

The geometric half cannot be. So there are UI tests that click real coordinates
derived from the theme's own metrics, and they assert the awkward cases as well
as the happy one: a click past the end of a link opens nothing (the
nearest-caret trap), a click on the words beside a box leaves it alone, a click
on a bullet toggles nothing, a plain click on a link opens nothing, and a click
on a box in an unfocused field leaves it unfocused.

Reading the opened URL back needed its own care. `harness.output()` is the last
frame only, and the platform output is replaced every frame, so a command issued
on the click frame is long gone by the time a `run` loop has settled. The helper
steps frame by frame and collects. Holding `Ctrl` needed care too:
`InputState::modifiers` is moved by `ModifiersChanged` and by nothing else, so a
modifier flag on the button event alone is not a modifier being held.


---

## D8 — Search, and the finish

*Done when: the search matches what you can see, and the whole feature is
documented.*

Done. 478 tests, clippy clean at pedantic. That closes D1 to D8.

### What it does

The sidebar search now matches the page rather than the file.

| Search for | Before | Now |
|---|---|---|
| `strong white flour` in `the **strong white** flour` | missed | found |
| `**` | every emphasised note | nothing |
| `- [ ]` | every checklist | nothing |
| `example.com/m` in `[the method](https://example.com/m)` | found | nothing |
| `the method` in the same | found | found |
| `let x = *ptr;` inside a fence | found | found |

The two halves of that are the same rule: what is on the page is searchable and
what is not is not. Markers, link addresses and highlight colours are drawn or
hidden, so they are out. A link's label is on the page, so it stays in. Code is
matched exactly as written, because nothing formats inside a fence and a snippet
has to be findable by the characters it actually contains. Titles are one line
fields with no markdown in them, so an asterisk in a title is still an asterisk.

`markdown::plain` already existed and was already tested from D1. D8 is the
wiring, and the wiring turned out to have a cost worth writing down.

### The cost

`Filter` is asked about every node in the tree, twice a frame (the sidebar asks,
and the arrow keys ask again through `flat_rows`), for as long as the search box
has text in it. Every task carries two markdown fields. Measured on two thousand
notes of realistic shape:

```
  raw contains                187µs
  parse                      12.99ms
  plain + lowercase          12.87ms
  search, cold               17.37ms
  search, warm                550µs
```

So the parse is the entire cost: `plain`'s own string building and the
lowercasing together account for well under a millisecond of it. Which means no
amount of buffer reuse would have helped, and a cache was the only answer.

`ui::search` memoises the stripped text on **the field's own bytes**. Keying on
content rather than on a node id and a timestamp is what keeps this a memo
rather than a second copy of the state: the same input cannot map to a stale
answer, there is nothing to invalidate, no signature anywhere else had to
change, and emptying the whole thing changes only speed. Two tests pin exactly
that, one editing a note and checking the old text is not still matching, one
asserting that clearing the memo changes no answer.

The map is keyed on the string itself rather than on a hash of it, so a lookup
hashes and then compares. A 64 bit collision is unlikely, but it would mean
searching one note and matching against another, and a wrong search result is
worse than a slow one.

`cargo run --release --example bench_search` prints the table above, so a change
to the parser can be checked against the claim in that module's documentation
rather than trusted.

### Two things tidied on the way through

**The needle was being rebuilt per node.** Every matcher did
`self.text.trim().to_lowercase()` for itself, which is a fresh allocation for
every node in the tree, every frame. It is now computed once per pass and handed
down.

**`matches_folder` lost its `self`.** Once the needle came in as an argument it
had nothing left to ask the filter, which is the honest shape: the status flags
describe tasks, and a folder has no status to pass or fail on. It is a free
function now, next to the comment space matcher.

`CommentSpace::matches` is gone. It did a raw substring match, which is the thing
D8 was removing, and putting the markdown rule in `model` would have pointed the
data layer at the UI's parser. The decision lives in `ui` where the rest of the
search does.

### On screenshots

The plan said README screenshots. I have not put images in the README: there are
none in it today, it reads well as text, and committing binaries into a repo
that still has no commits at all seemed like the wrong thing to decide on Kyle's
behalf. The verification captures went to him directly instead, as they have at
every stage. Easy to change if he wants them in.

---

## Where the feature landed

Eight stages, D1 to D8. What exists now:

| Layer | Module | Shape |
|---|---|---|
| Lines | `markdown::line` | pure, one `Line` per source line |
| Inline | `markdown::inline` | pure, flat non-overlapping spans plus markup ranges |
| Editing | `markdown::edit` | pure, `(text, caret) -> Option<Edit>` |
| Queries | `markdown` | pure, `parse`, `plain`, `is_url`, `link_at` |
| Layout | `ui::text::layout` | pure, source to `LayoutJob` |
| Decorations | `ui::blocks::plan` | pure, document to a list of shapes |
| Search | `ui::search` | memo over `plain` |
| Widgets | `ui::text::edit`, `ui::blocks::paint`, `ui::toolbar` | thin |

The invariant the whole thing rests on: **the laid-out text equals the source
byte for byte**, because a text field maps the caret through it. That is why
markup is styled down to nothing rather than removed, and it is held by a
`debug_assert`, a corpus test and a UI test through the real widget.

The split is what made every stage assertable. egui's accessibility tree exposes
a `TextEdit` as its raw string and says nothing about formatting, so a UI test
can prove text is present and nothing whatsoever about whether it is bold. Every
formatting decision therefore had to be a pure function, and every one is.

Four property tests earned their place by finding real bugs: the D1 delimiter
fuzz found an escape that stepped over the character after it, and the D5
all-operations-at-every-caret sweep found two operations that corrupted a line
from the right position.

### Tests

478, across eleven files:

| File | Count |
|---|---|
| `tests/ui.rs` | 154 |
| `tests/markdown.rs` | 86 |
| `tests/edits.rs` | 64 |
| `tests/layout.rs` | 43 |
| `tests/model.rs` | 37 |
| `tests/filter.rs` | 33 |
| `tests/blocks.rs` | 16 |
| `tests/persistence.rs` | 15 |
| `tests/theme.rs` | 14 |
| `tests/dnd.rs` | 10 |
| `tests/icon.rs` | 6 |

### Still open

Not part of this feature, but still true:

- The repo folder is called `work_plus_plus` and everything inside it is
  TrackCrab.
- **There are no git commits at all.** Git cannot unlink its own lock file
  through the device mount, so every write leaves a stale `index.lock` behind
  and I cannot make one from here. That needs Kyle to run git locally, or to
  approve delete permission for the folder.
- Syntax highlighting inside code fences was explicitly deferred: "we can try
  basic for now, if I'm not too happy with the full result then syntax
  highlighting will be something I'll chase for separately".


---

## After D8 — two fixes from Kyle

482 tests.

### Numbered markers were too dim

They were `TEXT_WEAK`, the same grey as the bullet dots. A dot is a mark and can
be quiet; a number is *text*, and reading `1.` at a lighter weight than the "one"
beside it looks like a rendering fault rather than a design choice.

The colour is now a parameter on `blocks::number`, and it is the only one of the
four markers that takes one. In a document a number is part of the sentence it
labels, so it is drawn in the body colour. On a toolbar button it is one stroke
of an icon and has to sit at the same weight as the dots and rules beside it, or
the glyph comes apart. Two callers, two answers, rather than one constant that
is wrong in one of the places it is used.

### The notebook did not scroll

A long note simply ran off the bottom of the panel, unreachable. The notebook is
the one field with a fixed box (`Field::size`), so it is the one field whose text
can outgrow what it was given: the task view's description and notes sit inside
that view's own scroll area and have always grown freely.

`Field::scroll` puts the field, and only the field, inside a vertical scroll
area. The formatting bar stays outside it, pinned. A test asserts exactly that,
because it is the thing that would silently regress.

Two details were not optional:

**A short note still has to fill the panel.** The field is the click target, so
a one line note has to stay clickable across the whole panel rather than
shrinking to one row with dead space under it. A naive scroll area takes that
away. The fix is a row count computed from the box height and handed to
`desired_rows`, which is a *minimum*: short content fills, long content grows and
scrolls. Pinned by a test asserting a one line note still occupies most of the
panel.

**The backdrop had to move inside.** A shape is reserved with the clip rectangle
of the `Ui` it was reserved on, and the code block background is reserved before
the text so it can be filled in behind it. Booked on the outer `Ui`, a code block
scrolled half out of view would have painted its background over the toolbar. So
the reservation, the layout and the decorations now all happen in one `render`
function against one `Ui`, and both the scrolled and unscrolled paths call it.
The widget is built inside that function rather than passed in, because the
buffer cannot be borrowed twice: `show` has to consume the `TextEdit` and give up
its mutable borrow before the decorations can read the same text.

Four tests: a long note scrolls, a note that fits does not, a short note still
fills the panel, and the toolbar stays put while the body moves.


---

## Highlights and the hex field

489 tests. Two defects, one root cause, and one more found by a test on the way.

Kyle's diagnosis was right: "it's like the focus doesn't actually shift".

### The menu closed on any click

egui's default popup behaviour is `PopupCloseBehavior::CloseOnClick`, which
closes on a click **anywhere, inside or out**. Right for a menu of choices; wrong
the moment one of the choices is a field you have to type into. Clicking the hex
box shut the menu before a character could be entered, so Apply was unreachable.

The highlight menu is now `CloseOnClickOutside`, which puts the responsibility
for closing on the items themselves, where the swatches and Apply already called
`ui.close()`. A test pins that too, because a swatch that stopped closing the
menu would leave it sitting open over the text.

### Focus never came back, so the caret was wrong

`==yellow|==some` instead of `==yellow|some==`. The caret placement was correct
all along; what failed was getting focus back to the field so it could be used.

Picking a colour puts an empty highlight in and the caret inside it. If focus
does not return, the next thing typed goes in wherever the following click puts
the caret, which is at the end of the line, past the closing `==`. An empty
highlight is not a highlight, so it renders as literal text — which is why the
symptom reads as "highlights don't work" rather than "focus is wrong".

The refocus was a one-shot retry, and that was two things short:

**It did not repeat.** egui resolves focus at the *start* of a frame from the
previous frame's pointer events, so a request issued on the click frame is
overridden a frame later. A menu is worse: the click lands outside the field, the
popup is closing, and the widget that was clicked no longer exists on the next
frame, so focus settles on nothing. One retry is not enough for that.

**It did not ask for the frames it needed.** egui only draws when something wants
it to. With nothing else requesting a repaint, the retry never ran at all.

Both are now handled: the request repeats until focus is actually held, up to
`REFOCUS_FRAMES`, and requests a repaint each time.

### Why the tests did not catch it

They did not catch it because `settle()` runs thirty frames, and thirty frames of
free repaints hide exactly the defect: a retry that never asks for a frame still
gets thirty of them. The harness was more generous than the app.

So the new test asserts the mechanism rather than the outcome: after a toolbar
action, the context *has requested a repaint* and a refocus is scheduled, and
after settling the focus is held and the schedule has cleared. That is the half a
settled test cannot see.

A second harness trap, worth writing down: `Node::type_text` only pushes an
`Event::Text` at whatever already has focus — it does not focus the node it is
called on. The first hex test passed nothing into the field at all and failed for
the right reason; the fix was to focus it first, which is what a hand's click on
the box does anyway.

### And one the test found

`enter_in_the_hex_field_applies_it` came back with `==#3355ff|\n==`. Enter applied
the colour *and* inserted a newline into the highlight it had just made: the field
surrenders focus on Enter, which hands focus back to the note, and the same
keypress then reached the note as well. The key is consumed now rather than merely
observed.

### Verified in the real binary

The harness could not reproduce any of this, so all three paths were driven
through the actual app under Xvfb, which produced:

```
==yellow|a named colour==
==#ff8800|hex via Apply==
==#6ee7b7|hex via Enter==
plain again
```

rendering as three highlights with their markup hidden and their text colour
picked for contrast against each background.


---

## The selection was being lost on the way to the menu

506 tests. One bug reported, one found while fixing it.

### What was happening

Select a phrase, open the highlight menu, pick a colour, and the markup landed
*beside* the words rather than around them. The reason is exact and measurable:

```
after select_all:   Some((9, 0))     a selection
menu open, cursor:  Some((9, 9))     collapsed, before anything was picked
result:             some text==yellow|==
```

Opening the menu takes focus off the field and, unlike a plain button, keeps it
off for as long as the menu stays open. egui collapses the stored selection to a
bare caret while focus is away. So by the time a colour was picked there was no
selection left to wrap, and `wrap` did the only thing it could with an empty
range: insert an empty pair at the caret.

Which is also why `B` worked and the menu did not. A plain button hands its
action back on the same frame it was clicked, before the collapse; a menu cannot.

### The fix

A toolbar action can no longer read the widget's own cursor, because by the time
one fires the field is not focused and its cursor is not to be trusted. The
caret is written down on every frame the field *does* hold focus, and again
whenever an operation moves it, and the toolbar acts on that.

The second half matters as much as the first: keeping it in step through
`set_caret` is what lets two actions run back to back. Bold then Bold takes the
bold off; Bold then Italic composes. Both are tested through the buttons.

And the guard against the fix over-reaching: remembering the last selection must
not mean resurrecting it. Move the caret to a bare position and a toolbar action
belongs there and nowhere else, which `a_bare_caret_is_not_treated_as_an_old_selection`
pins.

### The one found on the way

`two_different_buttons_compose_on_one_selection` came back with `*some text*`.
Bold, then italic, and the bold was gone.

`**bold**` *ends with* `*`. So `wrap`'s "are the delimiters already here" test,
which was a pair of `starts_with`/`ends_with` string matches, answered yes when
asked about italic on a bold word, and stripped a layer off it. A latent bug in
the pure core since D6, nothing to do with the toolbar, and invisible until two
star styles met on one selection.

`*` and `**` share a character, so this cannot be a string match at all. The
toggle now reads the **run** of delimiter characters beside the selection, the
way markdown reads it: one is italic, two is bold, three is both. A style of two
or more is on when the run is at least that long; a style of one is on when the
run is *odd*. That is what makes them compose instead of cancel:

| On | Press | Get |
|---|---|---|
| `x` | italic | `*x*` |
| `*x*` | bold | `***x***` |
| `**x**` | italic | `***x***` |
| `***x***` | italic | `**x**` |
| `***x***` | bold | `*x*` |

A coloured highlight keeps the old whole-string match, because its opening
delimiter carries a payload (`==yellow|`) and is not a run of one character.

Eleven tests on this, including a round trip: bold, italic, bold, italic and
back to where it started, in either order.

Two of those tests were wrong first time round, both my fault and both worth
noting. One expectation was simply miscalculated. The other tried to write
`==yellow|x==` through the caret helper, where `|` *is* the caret marker: the
colour syntax and the test harness want the same character, so coloured
highlights are asserted with explicit offsets instead.

### Verified in the real binary

```
==yellow|select then highlight==
==#ff8800|select then hex==
***bold and italic***
done
```

Selection wrapped in both highlight cases, and the two star styles composed.

