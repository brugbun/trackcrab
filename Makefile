WIN_TARGET := x86_64-pc-windows-gnu
WIN_GCC    := x86_64-w64-mingw32-gcc
WIN_DLLTOOL := x86_64-w64-mingw32-dlltool

.PHONY: run check test fmt lint linux windows win-doctor all clean

run:
	cargo run

check:
	cargo check --all-targets

test:
	cargo test

fmt:
	cargo fmt --all

lint:
	cargo clippy --all-targets -- -D warnings

linux:
	cargo build --release
	@echo "-> target/release/trackcrab"

# Preflight the Windows cross toolchain. rustc calls dlltool for any crate using
# raw-dylib (getrandom, windows-sys, parking_lot_core all do), and rustup does
# not ship dlltool, so a gcc-only mingw install fails deep into the build with a
# confusing error. Catch it here instead.
win-doctor:
	@fail=0; \
	command -v $(WIN_GCC) >/dev/null 2>&1 \
	  || { echo "MISSING: $(WIN_GCC)   (Arch: pacman -S mingw-w64-gcc)"; fail=1; }; \
	command -v $(WIN_DLLTOOL) >/dev/null 2>&1 \
	  || { echo "MISSING: $(WIN_DLLTOOL)   (Arch: pacman -S mingw-w64-binutils)"; fail=1; }; \
	rustup target list --installed 2>/dev/null | grep -qx $(WIN_TARGET) \
	  || { echo "MISSING: rust target $(WIN_TARGET)   (rustup target add $(WIN_TARGET))"; fail=1; }; \
	if [ $$fail -eq 0 ]; then echo "Windows cross toolchain: ok"; else \
	  echo ""; echo "Fix the above, then re-run 'make windows'."; exit 1; fi

windows: win-doctor
	cargo build --release --target $(WIN_TARGET)
	@echo "-> target/$(WIN_TARGET)/release/trackcrab.exe"

all: linux windows

clean:
	cargo clean
