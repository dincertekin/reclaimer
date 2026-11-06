# Reclaimer

A cross-platform file recovery tool written in Rust. Reclaimer scans raw disk images to find and recover deleted files using file signature carving. Built as a hands-on Rust learning project, with a focus on clean code and real forensic techniques.

> Work in progress — core carving pipeline works end-to-end, GUI is functional.

![Reclaimer after a scan: recovered files listed with type and size, and a detail panel for the selected file](.github/assets/screenshot.png)

<sub>Scan of a demo disk image.</sub>

---

## What Works Right Now

- Load a `.img`, `.dd`, `.raw`, or any raw disk image via the file picker or drag-and-drop
- Fast scanning — reads in large aligned chunks (about 1 GB in under a second from cache)
- Structure-aware carving — JPEG (JFIF and EXIF), PNG, GIF, MP4, MOV, HEIC, AVIF, PDF, ZIP, DOCX/XLSX/PPTX, SQLite
- Each file reports whether it was recovered intact or only partially
- Results table with filename, type badge, size, sector, and hex offset
- Detail panel for each recovered file with Open File and Copy Path actions
- Sidebar filter by file type (Images, Documents, Media, Other)
- Recovered files saved to `recovered/` relative to your working directory

---

## How File Carving Works

When a file is deleted, the OS marks its sectors as free but rarely overwrites them immediately. Reclaimer reads every sector looking for known **magic bytes** — the fixed byte sequences that start each file type.

Finding the start is easy; finding the **end** is what matters. Reclaimer never guesses: each format's carver walks the file's own structure until it ends.

| Format | Starts with | Ends where |
|--------|-------------|------------|
| JPEG | `FF D8 FF` | `FF D9` reached by hopping segment to segment (skips EXIF thumbnails) |
| PNG | `89 50 4E 47 0D 0A 1A 0A` | after the `IEND` chunk |
| GIF | `GIF87a` / `GIF89a` | the `3B` trailer, after walking all blocks |
| MP4 · MOV · HEIC · AVIF | `ftyp` box at offset 4 | after the last top-level box |
| PDF | `%PDF-` | last `%%EOF` (or the exact length in linearized PDFs) |
| ZIP · DOCX · XLSX · PPTX | `PK 03 04` | end-of-central-directory record that points back to this file |
| SQLite | `SQLite format 3` | page size × page count from the header |

After carving a file, the scan jumps past it rather than re-reading it.

---

## Project Structure

```
src/
├── main.rs              # eframe entry point, loads the Dock icon
├── source/
│   └── mod.rs           # BlockSource trait + ImageFile — where bytes come from
├── scanner/
│   ├── mod.rs           # scan() finds files, export() saves one
│   ├── window.rs        # read-ahead buffer: few large aligned reads
│   └── formats/         # one carver per format; FORMATS table in mod.rs
└── ui/
    ├── mod.rs           # ReclaimerApp state, eframe::App impl, channel polling
    ├── types.rs         # Palette constants, FoundFile, all shared enums
    ├── render.rs        # All egui rendering — toolbar, sidebar, table, detail panel
    └── scan.rs          # start_scan(), background scan thread
```

---

## Tech Stack

| Crate | Purpose |
|-------|---------|
| `eframe 0.34` + `egui 0.34` | GUI — immediate-mode, pure Rust, no web tech |
| `rfd 0.15` | Native OS file picker (NSOpenPanel on macOS) |
| `open 5` | Opens recovered files with the OS default app |
| `image 0.25` | Decodes the app icon PNG for the macOS Dock |

---

## Getting Started

**Requirements:** Rust (install via [rustup.rs](https://rustup.rs))

```bash
git clone https://github.com/dincertekin/reclaimer
cd reclaimer
cargo run
```

No npm, no build scripts, no extra CLI tools. Plain `cargo run`.

---

## Adding a New File Format

Create `src/scanner/formats/<name>.rs` with two functions:

```rust
pub fn matches(header: &[u8]) -> bool                 // does a file start here?
pub fn carve(w: &mut Window, start: u64, limit: u64)
    -> io::Result<Option<Carved>>                     // where does it end?
```

Then add one line to `FORMATS` in `src/scanner/formats/mod.rs`. Use the existing carvers as a guide, and add a `sample()` fixture with tests.

Run the tests with `cargo test`. Among other things, they check that `test.img` gives back the files in `originals/` byte for byte.

---

## Roadmap

- [ ] NTFS MFT parsing — read deleted file metadata (name, path, dates) from the Master File Table
- [ ] Physical disk and USB drive scanning — enumerate `/dev/rdiskN` on macOS, `\\.\PhysicalDriveN` on Windows
- [ ] Filter text input wired to the search box
- [ ] Configurable output directory

---

## Supported File Systems

| File System | Status |
|-------------|--------|
| Raw image (any) | Working — carving only |
| NTFS | Planned — MFT parsing |
| FAT32 / exFAT | Future |
| ext4 | Future |

---

## Learning Goals

This is a personal Rust learning project. Priorities in order: learning Rust correctly → working features → speed of development. The code is kept in small focused modules, avoids premature abstraction, and comments explain the *why* rather than the *what*.

---

## Disclaimer

Reclaimer is for legitimate data recovery and forensic learning only. Always recover files to a separate drive or folder — never write to the source disk, as that can overwrite the data you are trying to recover.

---

## License

MIT — see [LICENSE](./LICENSE).
