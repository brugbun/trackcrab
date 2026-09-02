# Notes and Comments - Build Plan

Two additions. Tasks get local context, folders get project level context.

| | Notes | Comments |
|---|---|---|
| Belongs to | A task | A folder |
| Shape | One free text field | Many titled spaces, cycled through |
| Lives | Under STATUS in the task view | Overlay panel on the right of a folder |
| Scope | "what is going on with this one thing" | "what shapes this whole project" |

## Confirmed decisions

- **The first comment space is created automatically**, auto-numbered
  (`Comments 1`, `Comments 2`), and the title is editable in place.
- **The panel is hideable, remembered, and hidden by default.** It **overlaps**
  the task listing rather than squeezing it, and is allowed to cover the
  timestamp and time columns. It is a notebook you open deliberately.
- **Deleting a space always confirms**, consistent with folders and tasks, with
  Enter confirming.
- **The two side panels are mutually exclusive.** Opening one closes the other,
  however it was opened.

### New keybinds

| Keys | Does |
|---|---|
| `Ctrl+Right` | Push the content right: open folders, close comments |
| `Ctrl+Left` | Push the content left: open comments, close folders |

Pressing the same one again returns to no panel at all, so the two keys reach
every state. `Ctrl+B` keeps toggling the folder sidebar.

Because the two can never both be open, this is modelled as one enum
(`Panel::None | Folders | Comments`) rather than two booleans. Mutual exclusion
becomes structural instead of a rule to remember, the same reasoning that turned
`close` plus `cancel` into an `Outcome`.

## One interpretation to check

You chose "auto-create the first space on open", and I flagged that opening
fifty folders would create fifty empty spaces.

Since the panel is hidden by default and deliberate, I am reading "on open" as
**the first time the comments panel is opened for that folder**, not the first
time the folder is opened. From your side it is identical: the space is there the
instant you look. But merely browsing a folder writes nothing, so the data file
and the search results stay clean.

Say the word if you meant it literally and I will move it.

## Assumption

Comments belong to a folder, so the panel targets **the open folder, or the
folder holding the open task**. That means project context stays available while
you are working inside one of its tasks. On the welcome page there is no folder,
so `Ctrl+Left` does nothing.

## Steps

### N1 - Model and persistence, headless

- `Task.notes: String`. Empty is the resting state; nothing branches on
  present-versus-empty the way `description` does, so no `Option`.
- `CommentSpace { title, body, created_at, updated_at }`.
- `Folder.comments: Vec<CommentSpace>`.
- Tree API: `add_comment_space`, `edit_comment_space`, `rename_comment_space`,
  `delete_comment_space`. Every one validates its index and bubbles the
  folder's `updated_at` up the ancestor chain, exactly as a task edit does.
- **Schema version 1 to 2.** Both new fields are `#[serde(default)]`, so a
  version 1 file still loads and simply has no notes or comments.

**Done when:** `cargo test` green, including a hand written version 1 file
loading cleanly. No UI.

### N2 - Task notes in the detail view

- A `NOTES` section directly under `STATUS`, below the blocked reason when that
  is showing.
- Multiline, committed through the same single `edit_task` write path, so
  `updated_at` and bubbling cannot be forgotten.
- Notes join the sidebar search, alongside titles and descriptions.

**Done when:** a note survives a restart and is findable by searching for its
text.

### N3 - Panel state machine and the keybinds

- `sidebar_open: bool` becomes the `Panel` enum described above.
- `Ctrl+Left` and `Ctrl+Right` wired, `Ctrl+B` preserved.
- The comments panel renders as a right hand **overlay** over the listing, with
  a single space and no cycling yet, purely to prove the shell.
- A toggle button in the folder header, and the choice remembered in
  `settings.json` beside the zoom.

**Done when:** the two panels can never both be open, by construction, and the
keys move the content left and right as described.

### N4 - Comment spaces proper

- Header: `<-  Title  ->` with a `+` at the top right.
- Arrows cycle with wraparound; `+` adds a space and switches straight to it.
- A quiet `2 / 4` position indicator, so a long row of spaces is navigable.
- Title editable in place.
- Delete with a confirmation.
- Comments join the search: a folder is kept when any space's title or body
  matches, and the matching space is the one shown.

**Done when:** spaces can be added, cycled, renamed, deleted and searched, and
the whole lot survives a restart.

## Notes on the sequencing

N1 is deliberately headless and first, for the same reason M1 was: the schema
change and the bubbling rules are the part that would be expensive to get wrong,
and they are much easier to test without a window in the way.

N3 comes before N4 so the panel plumbing and the keybinds are settled before any
of the cycling behaviour is layered on top. That keeps the two hardest things,
the state machine and the multi-space navigation, in separate steps.

---

## N4 as built

All four steps are done. 186 tests green, clippy clean at pedantic.

### What shipped beyond the plan

- **The panel tweens.** It slides in from off the right edge using
  `ctx.animate_bool_responsive`, the *same call* the folder sidebar's own slide
  goes through, so the two move at one speed by construction rather than by a
  copied duration. It keeps being drawn while the factor is above zero, so
  closing tweens out instead of blinking away, and it is non-interactive
  mid-slide (aiming at something still moving is a mis-click waiting to happen).
- **Auto-numbered titles count as blank.** A space carrying only `Comments 2`
  and no body is treated as empty, so the delete confirmation reads "is empty
  and will be removed" rather than "and its 0 word(s)". A title you chose is
  never blank, however short.
- **A search hit opens the page holding it.** `Filter::matching_space` finds the
  first space whose title or body matches, and opening the folder lands there.
  With a dozen spaces, landing on page one and cycling to find the match would
  have made the search useless.
- **The status filter does not gate a comment match.** Statuses belong to tasks;
  a folder matched through its comments is not a task and must not be judged by
  one. There is a test for exactly this.

### Two bugs the verification pass caught

- **The arrows rendered as missing-glyph boxes.** `U+2190` and `U+2192` exist
  only in `Hack-Regular`, which egui maps to the monospace style, so a
  proportional button drew tofu. The same trap the burger fell into in M6. Fixed
  by *painting* the chevrons with the painter instead of typing them: no font
  dependency at all, crisp at any zoom, and the accessibility label became
  "Previous space" / "Next space", which reads better than an arrow anyway.
- **Every wrapped line lost its last word.** Sizing the `Area`'s own `Ui` to the
  panel rect made the `Frame` grow by its margins and hang 24px off the right
  edge. Nothing visibly moved, so only a measurement caught it. The notebook is
  now sized from the inside, and
  `the_notebook_content_stays_inside_the_panel` measures the writing area
  against the panel's inner edge so it cannot come back.

### On testing the tween

`egui_kittest` sets `animation_time = 0.0` deliberately, so tests never wait on
a tween, and `Harness::run` loops frames until nothing asks for a repaint, which
swallows an animation whole. Watching the slide therefore needs three things at
once: a harness built with a fine `step_dt`, the animation time put back after
build, and `step()` rather than `run()`. `harness_animated` does the first two
and is documented as to why. The geometry itself is checked separately as a pure
function, so the animation test only has to prove that the panel moves.

### The `Request` enum

Clippy flagged five bools on the notebook's report and it was right: `close`,
`add`, `delete` and the two arrows are mutually exclusive by nature. They became
`Option<Request>`, which also let the `usize::MAX` sentinels the arrows had been
using disappear entirely, with wraparound moving onto `Request::resolve`.

---

## N5 - keyboard navigation

Two additions, one per panel, following whichever has the screen. 203 tests
green, clippy clean at pedantic.

### The folder tree: up, down, Enter

The tree is walked as a **flat** list, depth ignored: down from a folder's last
task steps to the next top-level folder, exactly as the eye reads the panel.

`ui::flat_rows` builds that list from the same `collapse_id` and
`sorted_children` the sidebar renders from, so it cannot drift into being a
second, disagreeing copy. It reads collapse state with `CollapsingState::load`
rather than `load_with_default_open`, so merely asking which rows exist never
writes anything. Collapsed subtrees and rows a filter has hidden are simply not
in the list, which is how the cursor is prevented from landing on something
invisible rather than by a check at the point of use.
`the_flat_row_list_matches_what_is_on_screen` asserts the list against the rows
egui actually drew.

Two decisions worth stating:

- **The ends clamp, they do not wrap.** A tree has a real top and bottom, and
  holding a key down should stop there rather than teleport. The notebook wraps,
  because a carousel has no ends. Different shapes, different rule.
- **The cursor is not the selection.** What the main panel is showing keeps its
  filled row; the keyboard cursor is an *outline*. A fill says "this is what you
  are looking at", an outline says "this is what Enter would open", and they are
  frequently different rows. `rows::Mark` carries both, deliberately as two
  independent flags rather than one enum.

Clicking a row moves the cursor there, so the next arrow press carries on from
where you last looked instead of from wherever the keyboard was left.

### The notebook: left, right

Plain arrows cycle spaces, wrapping, reusing `Request::resolve` so the keys and
the on-screen chevrons cannot disagree about what "next" means.

### The typing rule, and one deliberate exception

A plain, unmodified key belongs to whatever has the caret: arrows move it, Enter
makes a new line, Delete deletes a character. A shortcut consumed in `shortcuts`
never reaches the widget, so all of these stand down while a field is focused.

The **search box** is the exception, and only for up and down. A single line
field has nothing to do with vertical arrows, so the tree keeps them, which
turns type, arrow, `Enter` into one flow. That needed the search box to have a
fixed id (`ui::search_box_id`) rather than one derived from layout, so the app
can tell "the caret is in the search box" from "the caret is in a body of text".

This also fixed a **pre-existing bug**: `Delete` was ungated, so pressing it
while editing a task's description raised that task's delete confirmation.

### One regression the verification pass caught

`Response::scroll_to_me` targets **both** axes. In a two dimensional scroll area
that dragged the tree sideways as well, so arrowing onto a deeply indented row
chopped the left off every label, roots included. The cursor is now followed
with a target whose horizontal range is exactly the range already on screen, so
only the vertical scroll can move, and it moves by the minimum needed rather
than recentring. Guarded by
`following_the_cursor_does_not_drag_the_tree_sideways`.
