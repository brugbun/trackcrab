# TrackCrab - Build Plan

Local-only Rust desktop task tracker. Folders nest infinitely, tasks live only inside folders.

## Stack decisions (confirmed 2026-09-01)

| Concern | Choice | Why |
|---|---|---|
| GUI | `eframe` / `egui` 0.36.1 | Pure Rust, no C deps, cleanest WSL cross-compile. Verified `cargo check` green on Linux with `glow` + `x11` + `wayland` features. |
| Persistence | Single JSON file, `serde_json` | Human-readable, git-friendly, whole tree fits in memory. Atomic temp+rename write. |
| Windows build | Cross-compile `x86_64-pc-windows-gnu` from WSL | Stays entirely inside WSL as required. |
| Tree storage | Flat arena (`HashMap<NodeId, Node>` + parent pointers) | Avoids egui borrow fights while rendering, makes `updated_at` bubbling and drag-reparenting trivial, serialises flat. |
| IDs | `uuid` v4 | Stable across sessions, no reindexing on delete. |
| Time | `chrono` with local timezone for display | `DateTime<Utc>` stored, formatted `HH:MM:SS DD/MM/YYYY`. |

### egui 0.36 APIs confirmed present (read from crate source, not docs)

- `SidePanel::left(..).show_collapsible(&mut is_expanded, ..)` - built-in animated slide for the burger toggle, plus drag-to-collapse and drag-to-reopen for free.
- `SidePanel::default_size` / `min_size` / `max_size` / `size_range` / `resizable` - sidebar width control.
- `ScrollArea::both()` - the 2D sidebar scroll.
- `CollapsingState::load_with_default_open` + `show_header` + `show_body_indented`, with animated `openness` - the smooth expand for folders.
- `Context::animate_bool_with_time_and_easing` - the blocked-reason reveal.
- `Modal::new(..).show(..)` - the new-task dialog.
- `Ui::dnd_drag_source` / `Ui::dnd_drop_zone` - drag to move items between folders, no extra crate.

### API shape corrections found while building

eframe 0.36 reworked the app trait. It is **not** the `update(&Context, &mut Frame)` that
every tutorial and older example still shows:

- `fn ui(&mut self, ui: &mut egui::Ui, frame: &mut Frame)` is the paint entry point, and it
  hands you a root `Ui` rather than a `Context`.
- `fn logic(&mut self, ctx: &egui::Context, frame: &mut Frame)` runs once per frame and paints
  nothing. The debounced save lives here.
- Panels therefore nest inside a `Ui`, not a `Context`: `CentralPanel::default().show(ui, ..)`
  and `SidePanel::left(id).show_collapsible(ui, &mut open, ..)`. `SidePanel::left` takes an
  `Id`, not a name string.

Also renamed in 0.36, found during M2:

- `SidePanel` and `TopBottomPanel` are gone, replaced by one `Panel` type:
  `Panel::left(id)`, `Panel::top(id)`, sized with `exact_size` / `default_size`.
- `Context::screen_rect()` is now `Context::viewport_rect()`.
- `Context::style()` / `set_style()` are now `all_styles_mut()` /
  `set_style_of(theme, ..)`.

## Proposed module layout

```
src/
  main.rs              eframe bootstrap, native options, window icon
  app.rs               App struct, View enum, update() loop, dirty/save debounce
  model/
    mod.rs
    ids.rs             NodeId newtype
    status.rs          Status enum, colour, dot glyph, blocked-reason invariant
    node.rs            Task, Folder, Node
    tree.rs            arena, CRUD, updated_at bubbling, move with cycle check
  store/
    mod.rs             data dir resolution, load_or_default
    persist.rs         atomic save, schema_version, corrupt-file quarantine
  ui/
    theme.rs           colour tokens, spacing, row metrics
    sidebar.rs         recursive animated tree
    rows.rs            shared row renderer (dot | title | updated_at | time)
    views/
      welcome.rs
      folder.rs        file-explorer list
      task.rs          task detail editor
    dialogs.rs         new task modal, confirm-delete modal
```

## Milestones

### M0 - Toolchain and build harness
- `rust-toolchain.toml` pinning stable (edition 2024 needs 1.85+).
- `.cargo/config.toml` wiring `x86_64-pc-windows-gnu` to `x86_64-w64-mingw32-gcc`.
- Install step for WSL: `rustup target add x86_64-pc-windows-gnu` and `pacman -S mingw-w64-gcc` (Arch).
- `Makefile` with `run`, `check`, `test`, `fmt`, `lint`, `linux`, `windows`.
- Confirm a bare window opens under WSLg.

**Done when:** `make run` opens an empty window, `make windows` emits a `.exe`.

**Status: complete.** Release profile verified to link (9.6 MB stripped LTO binary).

### M1 - Domain model and persistence (headless)
- `Status` enum, `Blocked(String)` carrying its reason so the invariant is unrepresentable-if-wrong.
- `Task`, `Folder`, `Node`, arena `Tree` with a virtual root that accepts folders only.
- CRUD: create/rename/delete folder, create/update/delete task, `move_node` with descendant-cycle rejection.
- `updated_at` bubbling to every ancestor on any child mutation.
- `serde` round-trip, `schema_version` field, atomic write, load-or-default, quarantine on parse failure.
- Unit tests: deep nesting, bubbling, root rejects tasks, blocked reason survives round-trip, cycle rejection, atomic write leaves no partial file.

**Done when:** `cargo test` green. No UI yet.

**Status: complete.** 36 tests passing, `cargo clippy --all-targets -- -D warnings` clean with `clippy::pedantic` enabled from the manifest.

### M2 - App shell and sidebar tree
- `TopBottomPanel` with the burger button top-left.
- `SidePanel::left` via `show_collapsible`, default width = 12.5% of window width (tracked until the user drags it, then their width wins), `size_range` clamped to sane bounds, `ScrollArea::both`.
- Recursive `CollapsingState` render with animated expansion.
- `View` enum: `Welcome | Folder(NodeId) | Task(NodeId)`. Clicking a folder expands it in the sidebar and swaps the view.
- Welcome page in `CentralPanel`.

**Done when:** seeded fake data is navigable end to end.

**Status: complete.** 12 UI tests drive the real interface through egui's
accessibility tree with `egui_kittest`. Demo data is opt in via `WORKPP_SEED=1`,
never invented into a real file.

### M3 - Folder view (file explorer)
- Row renderer producing exactly your target shape:
  `[dot] Some task | 12:07:36 01/09/2026 | 15h`
- Colour-coded status dot on the left, title, `updated_at`, attributed time right-aligned and dropped first when width is tight.
- `Separator` between every row, folders and tasks alike.
- Header: folder name plus a breadcrumb whose ancestors are clickable, so you
  can walk back up without the sidebar.
- The `+` buttons move to M5. A visibly dead button is worse than no button, and
  they only mean anything once the creation flow behind them exists.
- Click folder row opens it and expands the sidebar node. Click task row opens the task view.

**Done when:** the list matches your example lines.

**Status: complete.** 15 UI tests. Meta columns drop right to left under width pressure, asserted as an ordering across six window widths rather than against hand computed pixel thresholds.

### M4 - Task detail view
- Title (inline editable), `created_at` / `updated_at` line, description multiline.
- Status selector, five options with colour swatches.
- Blocked reason field, animated reveal, only when status is Blocked. Saving a Blocked task with an empty reason is blocked with an inline warning.
- Time logging: hours box + minutes box, minutes normalised over 60.
- Delete button at the bottom with a confirm modal.
- Every edit stamps `updated_at` and bubbles.

**Done when:** an edit survives an app restart.

**Status: complete.** 60 tests, 24 of them UI. Includes a genuine restart test:
edit through the widgets, flush, build a fresh `App` over the same file, and
assert the change and the bubbled parent timestamp both came back off disk.

### M5 - Creation flow
- `+` in a folder creates a blank task, inserts it into sidebar and view, and simultaneously opens the modal for title / description / status (default Open, description optional).
- Cancel removes the blank task, confirm commits it.
- Folder create, inline rename, delete. Delete is refused while the folder still has any children, with an inline message saying so.

**Done when:** the whole tree can be built without hand-editing JSON.

**Status: complete.** 71 tests. Cold start from an empty file is covered, as are
Cancel *and* Escape on the new task prompt, since either one would otherwise
orphan the blank task the spec asks us to create up front.

### M6 - Theme and polish
- Status colour tokens: Open light blue, In Progress yellow, Completed mint green, Blocked near-black grey, Cancelled red.
- Dark base theme, row heights, hover highlight, spacing and font pass.
- Empty states for empty folders and no-folders-yet.

**Done when:** it reads as a designed app rather than a debug UI.

**Status: complete.** 77 tests, six of them design invariants in
`tests/theme.rs` that compute real WCAG contrast ratios, so a future palette
tweak cannot quietly make a dot or a divider invisible again.

### M7 - Drag and drop reparenting
- `dnd_drag_source` on rows, `dnd_drop_zone` on folder rows and sidebar nodes.
- Reject dropping a folder into its own descendant, reject tasks at root, visual drop indicator.

**Done when:** items move by drag in both panels and the tree never corrupts.

**Status: complete.** 90 tests, ten of them in `tests/dnd.rs`. Built on egui's
*response level* drag and drop (`dnd_set_drag_payload` / `dnd_hover_payload` /
`dnd_release_payload`) rather than `dnd_drag_source`, so a row stayed one widget
that both navigates on click and lifts on drag. Targets colour green or red
before you release. The space under the tree is the move-to-top-level target.

The test that matters walks every node against every possible target including
the root, and for each pair `can_drop` offers, performs the real move and runs
the tree's structural validator. A green highlight the tree would reject fails
the build.

### M8 - Search, filter, shortcuts
- Sidebar filter box and status filter chips, tree auto-expands to matches.
- `Ctrl+B` burger, `Ctrl+N` new task, `Ctrl+Shift+N` new folder, `Ctrl+S` force save, `Del` delete, `Esc` close modal.

**Done when:** shortcuts documented in the README.

**Status: complete.** 114 tests, 11 of them in `tests/filter.rs` and 13 new UI
tests. Shortcuts are documented in `BUILDING.md`.

### M9 - Release hardening
- `#![windows_subsystem = "windows"]` on release so no console window appears.
- Release profile: LTO, `strip`, `opt-level = 3`.
- Window icon, app metadata.
- README: build instructions for both targets, data file location, schema notes.

**Done when:** a Linux binary and a Windows `.exe` both run clean from a cold
start.

**Status: complete.** 126 tests. Release build verified to link. The window icon
is drawn in code rather than shipped as a PNG, so there is no binary asset to
keep in step and no image decoder pulled in for one 64px picture;
`tests/icon.rs` guards that it is a crab silhouette and not a red square, and
`cargo run --example render_icon` dumps it to look at. `README.md` covers what
the app does, the data file and its guarantees, the shortcuts, and the test
layout.

## Progress

| Milestone | State |
|---|---|
| M0 Toolchain and build harness | Done |
| M1 Domain model and persistence | Done, 36 tests |
| M2 App shell and sidebar tree | Done, 12 UI tests |
| M3 Folder view rows | Done, 15 UI tests |
| M4 Task detail view | Done, 60 tests total |
| M5 Creation flow and folder CRUD | Done, 71 tests |
| M6 Theme and polish | Done, 77 tests |
| M7 Drag and drop reparenting | Done, 90 tests |
| M8 Search, filter, shortcuts | Done, 114 tests |
| M9 Release hardening | Done, 126 tests |

**All nine milestones complete.**

### Design decision: a reasonless Blocked status is held back, not rejected

`Tree::edit_task` rolls a rejected edit back wholesale, which is right for
atomicity but wrong for this screen. Choosing Blocked before typing a reason
would have failed the whole commit and silently binned whatever title or
description had just been typed.

So the editor sends every field *except* the status in that situation, leaves the
saved status alone, and says so in the UI ("Not saved yet. A blocked task needs a
reason..."). Typing a reason commits it. There is a test asserting an unrelated
edit survives a pending status change, since that is the exact failure the naive
version would have.

### Resolved: the Blocked dot was effectively invisible

Near black on a dark panel measures 1.57:1 against the panel, which is the look
you asked for and also unreadable. Rather than change the colour, every status
dot is now drawn with a 1px ring in a neutral grey. On the four bright statuses
the ring disappears into the fill; on Blocked it is the only thing defining the
shape. `tests/theme.rs` asserts both halves of that: Blocked stays the quietest
of the five by a factor of two, and every dot still clears a minimum contrast
once the ring is counted.

### Spec change: creating something no longer opens it

Originally a new folder or task was opened in the main panel. In practice
filling out a hierarchy means creating several things in a row, and being thrown
into each one's own view every time made that painful. Creation now expands the
tree down to the new item so you see it appear, and leaves you where you were.

The `Report::open` field became `Report::reveal` to make that explicit, and the
three tests that asserted the old navigation now assert the new behaviour.

### Sidebar hierarchy guides

Vertical indent guides, in three tiers driven by whatever is currently open:

- the guide belonging to the open item's nearest folder is near white
- every ancestor above it, up to the top level, is a middle grey
- unrelated branches stay barely there

For a folder the chain includes the folder itself, so its own guide lights up
rather than its parent's. Guides are 2px, and the indent went 18 to 24 to give
them room.

### Row layout correction

The attributed time was being clipped at the right edge. It now sits to the
*left* of the timestamp with a divider between them, always shows a value
(`0h 0m` when nothing is logged), and the row reserves an 18px right margin so
nothing can sit under the scrollbar. Under width pressure the *timestamp* is now
what drops, not the time, since the time is the figure worth keeping and much the
narrower of the two. There are tests for the ordering, the zero fallback, and the
right edge clearance.

### M6 changes

- One deliberate type scale (heading 21, body 13.5, button 13, small 11) rather
  than egui's defaults, so the three tiers stay in proportion everywhere.
- Row height 24 to 26 to 30, dot radius 4 to 4.5, thinner scrollbars.
- Second pass on request: every text tier up 3px (heading 24, body 16.5, button
  16, small 14), bigger collapse triangle, "FOLDERS" doubled to 24, the sidebar
  "+" at 26 with a 30px hit area, dialogs 560px wide with their text scaled
  1.12, sidebar minimum width 150 to 210, default window 1320x820.
- The selected status pill is tinted with its own status colour, so the choice
  reads from colour alone without looking at the label.
- The status picker was duplicated between the detail view and the new task
  dialog. Now one `rows::status_pills`, so they cannot drift apart.
- Empty states say what to do ("Use + above to make one") instead of only
  reporting that something is empty.

### Renamed to TrackCrab (2026-09-02)

Display name, crate, binary, widget ids, `TRACKCRAB_DATA` and `TRACKCRAB_SEED`.

The one part that was not cosmetic is the data directory, now
`%APPDATA%\\trackcrab\\data.json` and `~/.local/share/trackcrab/data.json`. To
avoid orphaning anything already created, `DataStore::load` falls back to the old
`work_plus_plus` location once, when the new file does not exist yet, and adopts
it. `DataStore::legacy_path` and its caller are marked safe to delete in a later
version.

### Enter confirms every dialog (revision, 2026-09-02)

Requested for a streamlined create-and-move-on flow, and applied to deletes too.
Two rules make it safe rather than surprising:

- It is gated on exactly the condition that enables the confirm button, so Enter
  can never do something the button would refuse. A blocked task with no reason,
  a folder with no name and a folder that still has contents are all untouched
  by it.
- It never fires while the caret is in a multiline description, where Enter
  means a new line. Checked after the body has had its go at the key.

`close` plus `cancel` became an `Outcome` enum in the process, since the pair
allowed the nonsensical "cancelled but still open".

### Tree guides, corrected (2026-09-02)

The first version lit up an *open folder's own* guide, which put the bright line
below its row instead of leading to it, and never visibly connected to anything.
Now the highlighted guide always belongs to the open item's **parent**, is drawn
bright only as far down as that item's row, and turns a short elbow into it. The
remainder below carries on in the quiet colour, and higher ancestors keep the
middle tier for their whole length.

### M8 design notes

- Filtering uses a **separate collapse id namespace**, so auto expanding to a
  match never disturbs how you had the tree arranged. Clearing the filter puts
  it back exactly as it was, and there is a test for that.
- A folder is kept when anything beneath it matches, so the path down to a match
  stays navigable. `retain` walks every branch rather than short circuiting on
  the first match, which a test specifically guards.
- Interface zoom is stored in `settings.json` **beside** `data.json`, not inside
  it. A preference should never be able to put task data at risk, and a corrupt
  preferences file costs a reset zoom and nothing more.
- No shortcut fires while a dialog is open, and `Ctrl+Shift+N` is checked before
  `Ctrl+N` so the plainer chord cannot swallow it. Both are tested.

## Notes for later milestones

- M5 owns the `+` buttons, moved out of M3's header work.
- `views::task::Editor` holds the blocked reason separately from the status, so
  switching away from Blocked and back does not lose what was typed.
- The time boxes only fold minutes into hours on `lost_focus`, otherwise the
  display rewrites itself under the cursor as you type "90".
- Text widgets expose their content to accessibility as a *value*, not a label,
  and hint text is not exposed at all. UI tests query those by role and value.
- The listing truncates long titles with an ellipsis; only the sidebar scrolls
  horizontally. Two different row shapes share one highlight and hit test
  routine (`rows::finish_row`) so they cannot drift apart.

- The status dot for **Blocked** is near black by request, which on the dark
  theme makes it the least visible of the five. Worth a look during M6 in case a
  ring or a slightly lighter fill reads better without losing the intent.
- `App::draw(&mut Ui)` is deliberately separate from the `eframe::App` impl. An
  `eframe::Frame` cannot be constructed outside eframe, and this is what lets the
  UI tests drive the real interface.
- UI tests click near a row label's **left edge**, not its centre. A deeply
  indented long name extends past the sidebar edge, so its centre can land in the
  main panel instead.

## Assumptions (all confirmed 2026-09-01)

1. **Sidebar contents.** Folders are the dropdowns, tasks are leaves shown inside them. Alternative is a folders-only sidebar, which stays shorter but makes the tree less useful.
2. **Attributed time formatting.** `15h`, `1h 30m`, `45m`. Zero renders as
   `0h 0m` in the listing (revised 2026-09-02), though `Task::attributed_label`
   itself still returns an empty string for zero, which the task view uses to
   hide its summary.
3. **Sidebar width.** Tracks 12.5% of window width until you drag the handle, after which your chosen width persists and no longer rescales.
4. **Deleting a folder is refused while it is non-empty.** You must empty it first. Tasks can never be orphaned, every task has a folder parent. Confirmed.
5. **Multiple root folders** are allowed. Root itself holds folders only, never tasks.
6. **Timestamps** display in local time, stored as UTC.
7. **Data file location** is the OS data dir (`~/.local/share/trackcrab/data.json` on Linux, `%APPDATA%\trackcrab\data.json` on Windows), overridable with a `TRACKCRAB_DATA` env var so a WSL and a Windows build can share one file if you want.

## Known risks

### Resolved: dlltool missing on the Windows cross build

First real `make windows` run failed with:

```
error: error calling dlltool 'x86_64-w64-mingw32-dlltool': No such file or directory
error: could not compile `parking_lot_core` (lib)
```

`getrandom`, `windows-sys` and `parking_lot_core` declare their Windows imports
as `raw-dylib`. On any `*-windows-gnu` target rustc generates import libraries
for those with `dlltool`, and rustup does not ship `dlltool`. On Arch it comes
from `mingw-w64-binutils`, which is a **separate package from
`mingw-w64-gcc`**. Fix: `pacman -S mingw-w64-binutils`.

`make windows` now runs a `win-doctor` preflight that checks gcc, dlltool and
the rust target up front, so a missing piece reports itself in one line rather
than failing several minutes into a build. See `BUILDING.md`.

### Resolved: the row highlight was swallowing the collapse arrow

The shared row primitive stretched its clickable rect to the enclosing `Ui`'s
left edge. In the sidebar those rows sit inside a collapsing header whose arrow
is drawn to the *left* of the row content, so the row covered it, and because the
row registers its interaction last it won every click. The arrow was completely
dead and folders could not be collapsed. Found by a UI test, not by looking.
The rect now starts at the row's own content and stretches right to the clip
rect.

### Resolved: the burger rendered as a missing glyph box

U+2261 IDENTICAL TO only exists in `Hack-Regular`, which egui maps to the
monospace text style, so a proportional button drew a placeholder box. Switched
to U+2630 TRIGRAM FOR HEAVEN, which lives in `emoji-icon-font` and is in the
proportional fallback chain. Verified by reading the bundled font `cmap` tables
rather than guessing.

### Watching

mingw can emit `corrupt .drectve` warnings while linking egui. They are noise,
not errors. The upstream rustc issue is closed.

### Resolved: the notebook arrows hit the same font trap as the burger

U+2190 and U+2192 also live only in `Hack-Regular`, so the comment space arrows
drew tofu on a proportional button. Rather than hunt for a third codepoint that
happens to be covered, the chevrons are now **painted** with the painter. No
font dependency, crisp at any zoom, and the accessibility labels became
"Previous space" and "Next space". Any future arrow should follow this route.

### Resolved: an overlay Area sized from the outside overflows by its margins

Calling `set_min_size(rect.size())` on the comments `Area`'s own `Ui` and then
putting a `Frame` with a 12px inner margin inside it makes the frame 24px wider
than the panel, so it hangs off the right edge of the window and clips the last
word of every wrapped line. Nothing visibly moves, so this is invisible to a
screenshot glance and only a measurement catches it. Overlays are now sized from
the **inside**: compute the content box as `rect.size()` minus padding and
border, and constrain the Ui *within* the frame. Guarded by
`the_notebook_content_stays_inside_the_panel`.

### Note: egui_kittest cannot see animations by default

`egui_kittest` sets `animation_time = 0.0` at build time, and `Harness::run`
loops frames until no repaint is requested, so any tween finishes inside one
call. A test whose subject is an animation needs a fine `step_dt`, the animation
time restored after build, and `step()` in place of `run()`. `harness_animated`
in `tests/ui.rs` packages that up.

### Resolved: `scroll_to_me` scrolls both axes

Bringing a keyboard cursor into view with `Response::scroll_to_me` also scrolls
*horizontally*, which in the sidebar's two dimensional `ScrollArea` dragged the
tree sideways and cut the left off every label, roots included. The fix is to
call `Ui::scroll_to_rect` with a rect whose x range is the range already on
screen (`ui.clip_rect().x_range()`) and whose y range is the row's, with `None`
alignment so it moves the minimum needed instead of recentring. Any future
"scroll this into view" in a both-axes scroll area should do the same.

### Resolved: plain keys were being taken from text fields

`shortcuts` consumes keys before any widget sees them, so an unmodified key
registered there is stolen from whatever has the caret. `Delete` had been
registered that way since M5, which meant pressing it while editing a
description raised the task's delete confirmation. Every plain key is now gated
on `ctx.memory(Memory::focused)`.

The one exception is the sidebar search box, and only for the vertical arrows: a
single line field has no use for up and down, so leaving them to the tree makes
type, arrow, Enter one flow. Telling that field apart from a body of text needs a
stable id, so the search box now carries `ui::search_box_id()` rather than one
derived from layout.

### Note: the flat row list must be derived, never stored

The keyboard walks `ui::flat_rows`, which is the sidebar's render order read as a
flat list. It is computed from the same `collapse_id` and `sorted_children` the
render uses, on purpose: a stored copy would drift the first time a folder was
expanded by some path that forgot to update it. It also uses
`CollapsingState::load` rather than `load_with_default_open`, so asking which
rows are on screen never writes collapse state as a side effect.
