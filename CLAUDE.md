# Reclaimer — Project Context

## What this is

A cross-platform file recovery tool written in Rust. Scans raw disk images (and eventually real Windows drives) to find and recover deleted files using file signature carving and, later, NTFS MFT parsing.

## Who's building it

A beginner-level Rust programmer learning Rust deeply through this project. Priorities, in order: learning Rust correctly > working features > speed of development. Explanations should teach concepts (ownership, error handling, structs, modules) not just hand over code. Code should be broken into small focused files/modules, not dumped into one giant file.

## Tech stack

- **Backend:** Rust
- **Frontend:** Tauri 2 (HTML/CSS/JS in `ui/` folder) — NOT egui. We migrated away from egui because of repeated version-mismatch build errors and because Tauri gives full design control via web tech.
- **Target platforms:** macOS (current dev machine), Windows (future), Linux (not prioritized)
- **Current focus:** Windows NTFS support is the long-term goal but not yet implemented. All work so far is on raw `.img` disk images for forensic practice.

## Project structure

```
reclaimer/
├── src/
│   ├── main.rs          # Tauri entry point only (~15 lines)
│   ├── commands.rs       # #[tauri::command] functions exposed to JS (scan_image, open_file)
│   ├── disk/
│   │   └── mod.rs        # DiskImage struct: open() and read_sector() for raw image reading
│   └── scanner/
│       └── mod.rs        # FileSignature struct, SIGNATURES const, detect_signature(), extract_file()
├── ui/
│   ├── index.html        # Main UI structure
│   ├── style.css          # Near-black + single blue accent theme (see Design below)
│   └── app.js              # Frontend logic — CURRENTLY USES FAKE/PLACEHOLDER DATA (fakeScan()), not yet wired to real scan_image command
├── build.rs                # Required by Tauri: calls tauri_build::build()
├── tauri.conf.json         # Tauri window config, bundle settings
├── Cargo.toml
├── icons/
│   ├── icon.png            # Source icon (1024x1024)
│   ├── icon.icns           # macOS icon
│   └── icon.ico             # Windows icon
├── test.img                 # Local 1.44MB blank test disk image (not in git ideally)
├── originals/               # Original test JPGs used to plant into test.img
└── recovered/                # Output folder where extracted files get saved
```

## Current state of features

### Working

- `DiskImage::open()` and `read_sector()` — raw sector-by-sector reading of `.img` files (512-byte sectors)
- `scanner::detect_signature()` — detects JPEG, PNG, MP4, PDF via magic bytes
- `scanner::extract_file()` — extracts a found file from disk image to `recovered/` folder, using end-marker detection (searches for the LAST occurrence of end marker, e.g. JPEG's `FF D9`, since it can appear mid-file) or a max size fallback
- Byte-perfect recovery verified with `diff`/`cmp` on real JPEGs
- Tauri backend compiles and runs with two commands: `scan_image` (returns `Vec<FoundFile>`, emits `scan-progress` events) and `open_file` (opens recovered file with OS default app via `tauri-plugin-opener`)
- UI design: near-black background (`#0A0E14`) with single blue accent (`#4A9EFF`), matching the app icon (a glowing blue wireframe file reconstructing from fragments). Deliberately restrained/single-accent based on color psychology research (trust/security software should avoid multi-color decoration).

### NOT yet done / next steps

1. **`ui/app.js` still uses fake placeholder data** (`fakeScan()` function with hardcoded results). Needs to be rewired to call the real `scan_image` Tauri command via `window.__TAURI__.core.invoke("scan_image", { imagePath })` and listen to `scan-progress` events for live progress bar updates.
2. **No real file picker yet.** The "Open" button in `index.html`/`app.js` just hardcodes `imagePath = "test.img"`. Need to wire up `tauri-plugin-dialog` (already installed) to show a real native file picker.
3. **NTFS MFT parsing** — not started. This is the big educational milestone planned after the GUI is fully wired to real data.
4. **Windows physical drive scanning** (`\\.\C:`) — not started, requires Windows-specific APIs (`windows` crate, `CreateFile`/`DeviceIoControl`), and requires testing on an actual Windows machine since the dev machine is a Mac.
5. **File extension warning**: `FileSignature.extension` field exists and is used now (for `.jpg`/`.png` filenames and for `open_file`), should no longer trigger dead_code warnings.
6. Icon `.ico` conversion for Windows was done manually via online converter (icoconvert.com), not via a Rust crate.

## Known gotchas / lessons learned (avoid repeating these debugging cycles)

- **eframe/egui version 0.34.3 had a corrupted/broken cargo cache on this machine** causing bizarre errors (eframe's own internal files failing to compile, claiming `mod disk`/`mod scanner`/`mod ui` not found — these are from eframe's OWN source, not the user's code). This is part of why we abandoned egui for Tauri.
- **`png` crate 0.18.1 API**: `Decoder::new()` requires `BufRead + Seek`, so wrap byte slices in `std::io::Cursor`. Both `reader.output_buffer_size()` AND `info.buffer_size()` return `Option<usize>` on this version's actual compiled behavior (despite some docs/examples showing plain `usize`) — always trust the compiler's exact error over docs/online examples when API mismatches occur.
- **Tauri requires `build.rs`** at the project root containing `fn main() { tauri_build::build() }`, plus `[build-dependencies] tauri-build = { version = "2", features = [] }` in Cargo.toml. Missing this causes `OUT_DIR env var is not set` errors.
- **Cargo package name should be lowercase** (`name = "reclaimer"` not `"Reclaimer"`) — capitalization mismatches caused silent build script issues on macOS's case-insensitive filesystem. The `[package.metadata.bundle]` display name can stay capitalized.
- **Don't have duplicate main.rs files** in different folders (e.g. a stray `src-tauri/main.rs` alongside `src/main.rs`) — caused major confusion during the Tauri migration.
- Tauri 2 splits functionality into separate plugin crates: `tauri-plugin-dialog` for file pickers, `tauri-plugin-opener` for opening files with OS default apps. Both need to be added as dependencies AND registered via `.plugin(...)` in `main.rs`.
- When extracting JPEGs, the end marker `FF D9` can appear mid-file, not just at the true end. Must search for the LAST occurrence (`.rposition()`), not the first, or output gets truncated early (visible as a partially-gray/corrupted image).
- Test image planting commands: when planting multiple fake files into `test.img` via `dd`, leave enough byte distance between them or they overwrite each other. Calculate: `next_offset > previous_offset + previous_file_size`.

## Design system reference

- Background: `#0A0E14` (near-black, not pure black)
- Panel: `#10141C`
- Accent (primary, all interactive/highlight elements): `#4A9EFF`
- Success/green (for "recovered" status): `#4ADE80`
- Text primary: `#E8EDF4`
- All borders: `rgba(255,255,255, 0.04–0.1)` depending on emphasis
- Philosophy: ONE accent color only (restraint = trust, per color psychology research done for this project). No multi-color rainbow coding of file types — all badges/thumbnails use the same blue.

## User's working style / preferences (from custom instructions)

- Casual, friendly tone, like a knowledgeable friend, not corporate
- No em dashes/en dashes in writing
- Explain Rust concepts as code is introduced — the user is learning, not just shipping
- When code has bugs, give the corrected snippet directly, explain what was wrong and why
- Match response length to complexity, no padding
- Be honest/direct about mistakes or bad approaches, including the user's own pasted code
- Prefers being asked clarifying questions (via structured options) before big new features/phases, but proceeds with stated assumptions for smaller things

## Git / GitHub

- Repo: `github.com/dincertekin/reclaimer` (public, MIT license)
- Uses SSH for auth (`git@github.com:dincertekin/reclaimer.git`), already set up
- Commit style: Conventional Commits (`feat:`, `fix:`, `chore:`, `docs:`)

## Immediate next task when resuming

Wire `ui/app.js` to call the real Tauri commands instead of fake data:

1. Replace `fakeScan()` with `await window.__TAURI__.core.invoke("scan_image", { imagePath })`
2. Listen for `scan-progress` events to update the progress bar live: `window.__TAURI__.event.listen("scan-progress", (event) => {...})`
3. Wire the "Open" button to `tauri-plugin-dialog`'s file picker instead of the hardcoded `"test.img"` path
