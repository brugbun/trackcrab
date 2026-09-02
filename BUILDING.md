# Building TrackCrab

All builds run from WSL.

## Linux (and running under WSLg)

```sh
make run       # debug build, opens the window
make linux     # release build -> target/release/trackcrab
```

## Windows, cross-compiled from WSL

One-time setup on Arch:

```sh
pacman -S mingw-w64-gcc mingw-w64-binutils
rustup target add x86_64-pc-windows-gnu
```

On Debian or Ubuntu the two packages are `gcc-mingw-w64-x86-64` and
`binutils-mingw-w64-x86-64`.

Then:

```sh
make win-doctor   # checks gcc, dlltool and the rust target are all present
make windows      # -> target/x86_64-pc-windows-gnu/release/trackcrab.exe
```

### Why mingw-w64-binutils is not optional

`mingw-w64-gcc` alone is not enough, even though it provides the linker.

Several crates in this dependency tree (`getrandom`, `windows-sys`,
`parking_lot_core`) declare their Windows imports with `#[link(kind =
"raw-dylib")]`. On any `*-windows-gnu` target rustc handles that by generating
import libraries with **`dlltool`**, and rustup does not ship `dlltool`. On Arch
it lives in `mingw-w64-binutils`, a separate package.

Without it the build gets a long way in and then fails with:

```
error: error calling dlltool 'x86_64-w64-mingw32-dlltool': No such file or directory (os error 2)
error: could not compile `parking_lot_core` (lib)
```

which reads like a broken crate rather than a missing package. `make windows`
runs `win-doctor` first so this surfaces as a one-line "MISSING" message instead.

## Keyboard shortcuts

| Keys | Does |
|---|---|
| `Ctrl+B` | Show or hide the folder sidebar |
| `Ctrl+F` | Open the sidebar and jump to the search box |
| `Ctrl+N` | New task in the folder you are in |
| `Ctrl+Shift+N` | New folder inside the one you are in, or at the top level |
| `Ctrl+S` | Save now, rather than waiting for the debounce |
| `Delete` | Delete whatever is open, after a confirmation |
| `Enter` | Confirm the open dialog, when it has what it needs |
| `Escape` | Close a dialog, or clear the search and status filter |
| `Ctrl+=` / `Ctrl+-` | Zoom the whole interface in or out |
| `Ctrl+0` | Back to 100% |

No shortcut fires while a dialog is open, since a modal owns the keyboard.

`Enter` never submits while the caret is in a multiline description, where it
means a new line. It is also gated on the same condition that enables the
confirm button, so it can never do something the button would refuse.

`Ctrl+Shift+N` is checked before `Ctrl+N`, otherwise the plainer chord would
swallow it.

## Where the data lives

| Platform | Path |
|---|---|
| Linux | `~/.local/share/trackcrab/data.json` |
| Windows | `%APPDATA%\trackcrab\data.json` |

Interface preferences (currently just the zoom level) live in `settings.json`
beside it, so a preference can never put the task data at risk.

Set `TRACKCRAB_DATA` to override, which is how you point a WSL build and a Windows
build at the same file:

```sh
TRACKCRAB_DATA=/mnt/c/Users/Kyle.James/trackcrab.json make run
```

A file that fails to parse is renamed to `data.corrupt.<timestamp>.json` and
never deleted. The app starts empty and tells you where the original went.
