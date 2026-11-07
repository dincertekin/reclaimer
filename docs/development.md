# Developing Reclaimer

Reclaimer is a native desktop app written in Rust with
[egui](https://github.com/emilk/egui). It's also a learning project:
priorities are learning Rust correctly, then working features, then speed.
Code is kept in small focused modules, avoids premature abstraction, and
comments explain *why* rather than *what*.

## Run it

You need [Rust](https://rustup.rs).

```bash
git clone https://github.com/dincertekin/reclaimer
cd reclaimer
cargo run                          # the app
cargo run -- path/to/disk.img      # open a disk image straight away
cargo run -- --diagnose            # print what Reclaimer can see (phones, drives)
cargo test                         # the test suite
```

## Project structure

```
src/
├── main.rs              # entry point, window, --diagnose
├── source/
│   ├── mod.rs           # BlockSource trait, FileSource, Piece + save()
│   ├── window.rs        # read-ahead buffer: few large aligned reads
│   └── drives/          # list and open physical drives: macos.rs, windows.rs, linux.rs
├── fs/                  # deleted files by name: partitions, fat, exfat, ntfs
├── scanner/
│   ├── mod.rs           # carving: scan() finds files by content
│   └── formats/         # one carver per format; FORMATS table in mod.rs
├── phone/               # iphone_usb.rs, android.rs (cable), iphone.rs (backups)
├── sqlite.rs            # small read-only SQLite reader, with write-ahead logs
├── media.rs             # file categories by extension
└── ui/
    ├── mod.rs           # app state, frame loop, actions
    ├── theme.rs         # the design system: colours, type, buttons, menus
    ├── choose.rs        # screen 1: pick a drive, phone, backup or disk image
    ├── results.rs       # screen 2: progress, grid, filters, sort, inspector
    ├── icons.rs         # line icons drawn in code
    ├── thumbs.rs        # background thumbnails with a capped cache
    ├── scan.rs          # background scan thread
    ├── types.rs         # shared types and formatting
    └── tests.rs         # drives the real UI headlessly (egui_kittest)
```

## Libraries

| Crate | Purpose |
|-------|---------|
| `eframe` + `egui` | GUI: immediate-mode, pure Rust, no web technology |
| `rfd` | Native file and folder pickers |
| `open` | Opens recovered files with the default app |
| `image` | App icon and thumbnails (PNG, JPEG, GIF); HEIC, AVIF and RAW thumbnails come from the system (`sips` on macOS, WIC on Windows, `heif-convert` on Linux) |
| `plist`, `libc` | iPhone backups, `diskutil` output; disk ioctls, `authopen`, local time |
| `idevice` | iPhone over USB: usbmuxd, pairing, AFC (TLS via `ring`) |
| `mtp-rs` | Android over USB (MTP; Windows' own MTP stack on Windows) |
| `tokio` | A small runtime for the two async phone libraries |
| `windows`, `windows-sys` | Drive geometry and model, HEIC thumbnails (WIC) on Windows |
| Inter (bundled) | Typeface, [SIL Open Font License](../assets/fonts/Inter-LICENSE.txt) |

## Tests and fixtures

`cargo test` runs everything. Formats are tested against files made by
independent tools, so a test can't pass just because the code agrees with
itself:

| Fixture | Made by | Rebuild with |
|---------|---------|--------------|
| `test.img`: two photos at known offsets | — | — |
| `testdata/fat12.img`, `fat16.img`, `exfat.img`: deleted files in folders | macOS | `testdata/make-fixtures.sh` |
| FAT32 volume | the `fatfs` crate, at test time | — |
| NTFS volume | built by hand in the tests (nothing here can format NTFS) | — |
| `testdata/sqlite-*.db`, `photo.heic` | SQLite, `sips` | `testdata/make-sqlite.py` |
| `testdata/iphone-backups/` | Python, in Finder's backup layout | `testdata/make-iphone-backup.py` |

The UI tests run the real app headlessly. Two extras:

```bash
RECLAIMER_SHOTS=/some/dir cargo test scan_an_image                        # a PNG per step
RECLAIMER_README_SHOTS=.github/assets cargo test readme_screenshots -- --ignored   # README screenshots
```

## Icon and banner

Both are drawn in code. On macOS, `swift icons/make-icon.swift icons`
regenerates `icon.png`, `icon.icns`, `icon.ico` and
`.github/assets/banner.png` (also used as GitHub's social preview).

## Building the app

| Platform | Command | Result |
|----------|---------|--------|
| macOS | `./packaging/macos/build-app.sh` | `target/release/Reclaimer.app`: drag it to Applications |
| Windows | `cargo build --release` | `target\release\Reclaimer.exe`, icon embedded, no console window |
| Linux | `./packaging/linux/install.sh` | installs to `~/.local` with a menu entry and icon |

The macOS app is signed ad hoc, not notarized: on another Mac, open it the
first time with right-click → Open.

To check Windows or Linux builds from a Mac without a C cross-compiler:

```bash
cargo clippy --target x86_64-pc-windows-msvc --no-default-features --features tls-rustcrypto
```

## Adding a file format

Create `src/scanner/formats/<name>.rs` with two functions:

```rust
pub fn matches(header: &[u8]) -> bool                 // does a file start here?
pub fn carve(w: &mut Window, start: u64, limit: u64)
    -> io::Result<Option<Carved>>                     // where does it end?
```

Then add one line to `FORMATS` in `src/scanner/formats/mod.rs`, listing the
extensions it covers. Use the existing carvers as a guide, and add a
`sample()` fixture with tests.
