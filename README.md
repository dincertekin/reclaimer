# Reclaimer

A cross-platform file recovery tool written in Rust. Reclaimer scans raw disk images to find and recover deleted files using file signature carving. Built as a hands-on Rust learning project, with a focus on clean code and real forensic techniques.

> Work in progress — core carving pipeline works end-to-end, GUI is functional.

---

## What Works Right Now

- Load a `.img`, `.dd`, `.raw`, or any raw disk image via the file picker or drag-and-drop
- Sector-by-sector scanning with live progress
- File signature carving — detects and extracts JPEG, PNG, MP4, PDF
- Results table with filename, type badge, size, sector, and hex offset
- Detail panel for each recovered file with Open File and Copy Path actions
- Sidebar filter by file type (Images, Documents, Media, Other)
- Recovered files saved to `recovered/` relative to your working directory

---

## How File Carving Works

When a file is deleted, the OS marks its sectors as free but rarely overwrites them immediately. Reclaimer exploits this by scanning every sector looking for known **magic bytes** — the fixed byte sequences that identify file types:

| Format | Magic bytes | End marker |
|--------|-------------|------------|
| JPEG | `FF D8 FF E0` at offset 0 | `FF D9` (last occurrence) |
| PNG | `89 50 4E 47 0D 0A 1A 0A` at offset 0 | IEND chunk |
| MP4 | `66 74 79 70` at offset 4 (`ftyp` box) | size-limited |
| PDF | `25 50 44 46` at offset 0 | `%%EOF` |

When a signature is found, Reclaimer reads forward sector-by-sector until it hits the end marker (or a size limit), then writes the extracted bytes to a file. For JPEG specifically, `FF D9` can appear inside the file, so Reclaimer always uses the **last** occurrence to avoid truncation.

---

## Project Structure

```
src/
├── main.rs              # eframe entry point, loads the Dock icon
├── disk/
│   └── mod.rs           # DiskImage — opens a file and reads 512-byte sectors
├── scanner/
│   ├── mod.rs           # detect_signature(), extract_file()
│   └── signatures.rs    # SIGNATURES table — add new file formats here
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

## Adding a New File Signature

Open `src/scanner/signatures.rs` and add an entry to the `SIGNATURES` array:

```rust
FileSignature {
    name:           "GIF",
    magic:          &[0x47, 0x49, 0x46, 0x38],  // "GIF8"
    offset:         0,
    extension:      "gif",
    end_marker:     Some(&[0x00, 0x3B]),          // GIF trailer
    max_size_bytes: 5 * 1024 * 1024,
},
```

`detect_signature()` and `extract_file()` pick it up automatically — no other changes needed.

---

## Roadmap

- [ ] NTFS MFT parsing — read deleted file metadata (name, path, dates) from the Master File Table
- [ ] Physical disk and USB drive scanning — enumerate `/dev/rdiskN` on macOS, `\\.\PhysicalDriveN` on Windows
- [ ] Filter text input wired to the search box
- [ ] Configurable output directory
- [ ] More file signatures — GIF, ZIP, DOCX, SQLite, HEIC

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
