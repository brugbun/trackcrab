# TrackCrab

A local-only task tracker. Folders nest as deep as you like, tasks live inside
them, and everything is a single JSON file on your own disk. No account, no
sync, no network.

Written in Rust with [egui](https://github.com/emilk/egui). Builds for Linux and
Windows, both from WSL.

## What it does

**Folders** hold tasks and other folders, to any depth. A folder's `updated_at`
reflects the most recent change *anywhere* beneath it, at any depth, so a stale
top-level folder really is stale.

**Tasks** always live in a folder and can never be orphaned. Each has a title, an
optional description, a status, timestamps, and manually attributed time in hours
and minutes.

**Five statuses**, colour coded down the left of every row:

| Status | Colour |
|---|---|
| Open | light blue |
| In Progress | yellow |
| Completed | mint green |
| Blocked | near black |
| Cancelled | red |

A Blocked task must carry a reason. That is enforced in the type, not by
convention: the reason lives inside the `Status::Blocked` variant.

## Getting around

The burger top left opens the folder tree. It tracks an eighth of the window
width until you drag its edge, after which your width sticks. The tree scrolls in
both directions and draws indent guides that brighten along the path to whatever
you have open.

Clicking a folder opens it as a file-explorer style listing: status dot, title,
attributed time, timestamp, with a divider between every row. Clicking a task
opens its detail view.

Items can be dragged between folders in either panel. Targets go green or red
before you release, so a refused drop is never a surprise. A folder cannot be
dropped inside itself.

### Keyboard

| Keys | Does |
|---|---|
| `Ctrl+Left` / `Ctrl+Right` | Show or hide the sidebar |
| `Ctrl+F` | Open the sidebar and jump to search |
| `Ctrl+N` | New task in the folder you are in |
| `Ctrl+Shift+N` | New folder |
| `Ctrl+S` | Save now |
| `Up` / `Down` | Move through the tree as if it were flat, ignoring depth |
| `Enter` | Open the highlighted folder or task |
| `Left` / `Right` | Step through comments |
| `Delete` | Delete what is open, after confirming |
| `Escape` | Close a dialog, or clear the filter |
| `Ctrl+=` / `Ctrl+-` / `Ctrl+0` | Zoom the interface |

`Ctrl+B` used to toggle the sidebar. It is now **Bold**, because `Ctrl+Left` and
`Ctrl+Right` already do that job and the ambiguity was not worth keeping.

Enter never submits while the caret is in a description, where it means a new
line, and no shortcut that would type into your text fires while you are typing.
No shortcut fires at all while a dialog is open.

## Writing

Descriptions, notes and comments take markdown, and they take it **live**. There
is no edit mode and no preview pane: type `# ` and the line is already a heading,
type `**bold**` and it is already bold. The markup does not disappear, it gets
out of the way, dimmed and squeezed to almost nothing. Put the caret on a line
and its own markup comes back, full size, so you can see and fix exactly what you
wrote.

The dialect is Discord's, so it should already be in your fingers:

| Type | Get |
|---|---|
| `# ` .. `#### ` | Headings, four levels |
| `**bold**` | **bold** |
| `*italic*` or `_italic_` | *italic* |
| `__underline__` | underlined |
| `~~strike~~` | struck through |
| `` `code` `` | inline code |
| ```` ```py ```` .. ```` ``` ```` | a code block, language tagged |
| `- item` | a bullet |
| `1. item` | a numbered item |
| `- [ ] item` / `- [x] item` | a checkbox |
| `---` | a divider |
| `[label](url)` | a link |
| `https://...` | also a link, no markup needed |
| `==text==` | highlighted |
| `==green\|text==` | highlighted in a named colour |
| `==#f2c14e\|text==` | highlighted in any colour you like |

Highlighting is not standard markdown. The eight named colours are yellow, green,
blue, pink, purple, orange, red and grey; the hex form is there for when none of
them is the one you wanted. Either way the text colour is chosen for you, by
measured contrast against whatever background you asked for, so a highlight is
never unreadable.

Lists nest. Tab indents an item, `Shift+Tab` outdents it, and Enter carries the
marker to the next line, renumbering as it goes. Enter on an empty item ends the
list instead of adding another blank one, and Backspace at the start of an item
takes the marker off rather than eating the line above.

Checkboxes are clickable. Click the box and it ticks, and because the gutter is
not text the caret stays exactly where it was, so ticking something off a list
you were only reading does not drop you into editing it. `Ctrl+Enter` does the
same from the keyboard, on whichever line the caret is on.

Links are clickable too, with `Ctrl` held, which opens them in whatever you have
set as your browser. `Ctrl` rather than a bare click because these fields are
always editable and a plain click has to keep meaning "put the caret here".
Hovering a link shows you where it goes, which for a `[label](url)` is the only
way to see the address without putting the caret on it.

Paste an address over some selected text and it becomes a link to it. Paste
anything else, or paste with nothing selected, and it pastes exactly as you would
expect.

### Searching it

The sidebar search matches **what you can see**, not what is stored. A note
reading `the **strong white** flour` is found by searching "strong white flour",
which the raw text would miss because of the asterisks in the middle of it, and
searching for `**` finds nothing at all, because there are no asterisks on the
page. Markers, link addresses and highlight colours are all off the page and so
out of the search; a link's label is on it and stays searchable. Code is the one
exception, and matched exactly as written, since nothing formats inside it and a
snippet has to be findable by the characters it contains.

Titles are one-line fields with no markdown in them, so a title is matched
exactly as typed.

### The toolbar

Every notes and comments field has a button bar above it, for the days you cannot
remember which asterisk does what:

```
H | B I U S <> | list number check | rule code link highlight
```

`H` and the highlight dot open menus, the rest apply immediately. They work on a
selection or at the caret, and they put the caret back somewhere sensible either
way. Everything on the bar has a keyboard equivalent, and the three you will
actually use are the three you would guess:

| Keys | Does |
|---|---|
| `Ctrl+B` | Bold |
| `Ctrl+I` | Italic |
| `Ctrl+U` | Underline |
| `Ctrl+Enter` | Tick or untick the checkbox on this line |
| `Ctrl`+click | Open a link in your browser |
| `Escape` | Leave the field, now that Tab indents |

## Your data

| Platform | Path |
|---|---|
| Linux | `~/.local/share/trackcrab/data.json` |
| Windows | `%APPDATA%\trackcrab\data.json` |

Interface preferences live in `settings.json` beside it, so a preference can
never put your tasks at risk.

Saves are debounced and atomic: the file is written to a temp sibling, flushed,
then renamed over the original, so a crash mid-write leaves the previous good
file intact. Repeated saves of an unchanged tree are byte identical, so the file
does not churn.

If the file cannot be parsed it is **renamed aside**, never deleted, to
`data.corrupt.<timestamp>.json`, and the app starts empty and tells you where the
original went.

Set `TRACKCRAB_DATA` to point somewhere else, which is how you share one file
between a WSL build and a Windows build.

## Building

See [BUILDING.md](BUILDING.md). Short version, from WSL:

```sh
make run        # debug, opens a window
make linux      # release  -> target/release/trackcrab
make win-doctor # check the Windows cross toolchain first
make windows    # release  -> target/x86_64-pc-windows-gnu/release/trackcrab.exe
```

## Development

```sh
make test   # the whole suite
make lint   # clippy, pedantic, warnings as errors
make fmt
```

The suite splits by layer, deliberately:

| File | Covers |
|---|---|
| `tests/model.rs` | The tree: nesting, `updated_at` bubbling, cycle rejection, deletion rules |
| `tests/persistence.rs` | Round trips, atomic writes, and all four corruption paths |
| `tests/ui.rs` | The real interface, clicked through egui's accessibility tree |
| `tests/dnd.rs` | Drag and drop legality, checked against what the tree actually permits |
| `tests/filter.rs` | The sidebar search and status filter |
| `tests/theme.rs` | Design invariants, as computed WCAG contrast ratios |
| `tests/icon.rs` | The generated window icon |
| `tests/markdown.rs` | The parser: lines, inline spans, delimiter edge cases, fuzzed |
| `tests/layout.rs` | What the parser turns into, span by span and format by format |
| `tests/edits.rs` | Tab, Enter, Backspace and every toolbar action, at every caret position |
| `tests/blocks.rs` | Bullets, numbers, checkboxes, rules and code backgrounds |

`cargo run --release --example bench_search` prints what a search costs, which
is what justifies the memo in `ui::search`, so a change to the parser can be
checked against the claim rather than trusted.

The markdown core is pure functions over `&str`, which is why it can be tested
this hard: `markdown::parse` and `ui::text::layout` take text and return
structure, and the egui wrappers over them are kept deliberately thin. Two of
those files are property tests, and they earned it, finding a delimiter escape
that stepped over the character after it and two editing operations that
corrupted a line from the right caret position.

The invariant the whole feature rests on is that the laid-out text must equal the
source byte for byte, because a text field maps the caret through it. Markup is
therefore styled down to nothing, never removed. A `debug_assert`, a corpus test
and a UI test through the real widget all hold that line.

The UI tests use [`egui_kittest`](https://crates.io/crates/egui_kittest) to click
the actual widgets rather than calling helpers underneath them. That is what
caught a collapse arrow rendered permanently unclickable, and a row highlight
swallowing the widget beneath it.

`cargo run --example render_icon > icon.ppm` dumps the generated icon if you want
to look at it.

Set `TRACKCRAB_SEED=1` on an empty store to get a demo tree to poke at. It only
ever writes into a store with nothing in it.
