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

## Notes and comments

Two places to write things down, at two different scopes.

**Notes** are an addendum to a single task, under `STATUS` in its detail view
and separate from the description. What is going on with this one thing.

**Comments** are an addendum to a whole folder: what shapes the entire project.
They live in a notebook that slides in over the right of a folder listing, with
several titled spaces you cycle through like desktop workspaces. New spaces are
auto-numbered and the title is editable in place. `+` adds a space and takes you
straight to it, the chevrons either side of the title cycle with wraparound, and
a quiet `2 / 4` says where you are.

The notebook is deliberate: hidden by default, remembered between runs, and it
*overlaps* the listing rather than squeezing it, so it is allowed to cover the
timestamp and time columns while you are reading. Deleting a space always asks
first.

Both join the sidebar search. A folder is kept when any of its comment spaces
matches, and opening it lands on the space the text is actually in.

### Keyboard

| Keys | Does |
|---|---|
| `Ctrl+B` | Show or hide the sidebar |
| `Ctrl+Right` | Push the content right: folder tree on, notebook off |
| `Ctrl+Left` | Push the content left: notebook on, folder tree off |
| `Up` / `Down` | Move through the folder tree, one visible row at a time |
| `Enter` | Open whatever the tree cursor is on, folder or task |
| `Left` / `Right` | Cycle comment spaces, when the notebook is open |
| `Ctrl+F` | Open the sidebar and jump to search |
| `Ctrl+N` | New task in the folder you are in |
| `Ctrl+Shift+N` | New folder |
| `Ctrl+S` | Save now |
| `Enter` | Confirm the open dialog, when it has what it needs |
| `Delete` | Delete what is open, after confirming |
| `Escape` | Close a dialog, or clear the filter |
| `Ctrl+=` / `Ctrl+-` / `Ctrl+0` | Zoom the interface |

The two directional keys are a pair: think of them as pushing the content right
or left. Pressing the same one twice returns it to the middle. The two panels can
never both be open, which is modelled as one value rather than two flags.

The plain arrows follow whichever panel has the screen. With the folder tree
open, up and down walk it as a **flat** list, ignoring depth: down from a
folder's last task steps to the next top-level folder, exactly as the eye reads
it. Collapsed subtrees and rows a filter has hidden are skipped, so the cursor
can never land on something invisible, and the ends clamp rather than wrap. The
cursor is drawn as an outline, distinct from the fill that marks what is
actually open, and `Enter` opens it.

With the notebook open, left and right cycle its spaces, wrapping the way the
on-screen chevrons do.

Typing wins over navigation: a plain key belongs to whatever has the caret. The
one exception is the search box, and only for up and down, since a single-line
field has no use for them. That makes type, arrow, `Enter` one continuous
flow.

Enter never submits while the caret is in a description, a note or a comment,
where it means a new line. No shortcut fires while a dialog is open.

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

The suite is in seven files, and the split is deliberate:

| File | Covers |
|---|---|
| `tests/model.rs` | The tree: nesting, `updated_at` bubbling, cycle rejection, deletion rules |
| `tests/persistence.rs` | Round trips, atomic writes, and all four corruption paths |
| `tests/ui.rs` | The real interface, clicked through egui's accessibility tree |
| `tests/dnd.rs` | Drag and drop legality, checked against what the tree actually permits |
| `tests/filter.rs` | The sidebar search, the status filter, and comment matching |
| `tests/theme.rs` | Design invariants, as computed WCAG contrast ratios |
| `tests/icon.rs` | The generated window icon |

The UI tests use [`egui_kittest`](https://crates.io/crates/egui_kittest) to click
the actual widgets rather than calling helpers underneath them. That is what
caught a collapse arrow rendered permanently unclickable, a row highlight
swallowing the widget beneath it, and the comments notebook overflowing its own
panel by exactly its margins.

`cargo run --example render_icon > icon.ppm` dumps the generated icon if you want
to look at it.

Set `TRACKCRAB_SEED=1` on an empty store to get a demo tree to poke at. It only
ever writes into a store with nothing in it.
