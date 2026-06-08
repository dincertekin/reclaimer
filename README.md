# Reclaimer

A cross-platform file recovery tool written in Rust. Reclaimer scans raw disk images and physical drives to find and recover deleted files, even after the recycle bin has been emptied.

> This project is a work in progress. Currently in early development.

---

## Goals

- Recover deleted photos and videos from Windows NTFS drives
- Scan raw disk images (`.img`, `.dd`) for forensic practice
- Parse the NTFS Master File Table (MFT) to find deleted file entries
- Use file signature scanning (carving) to recover files even when metadata is gone
- Provide a simple GUI for non-technical users

## Planned Features

- [x] Raw disk image reading (sector by sector)
- [ ] NTFS MFT parsing
- [ ] File signature carving (JPEG, PNG, MP4, and more)
- [ ] Deleted file listing with metadata (name, size, date)
- [ ] File recovery to a chosen output folder
- [ ] Simple GUI built with egui
- [ ] Physical drive scanning on Windows (`\\.\C:`)

---

## How It Works

When a file is deleted, the operating system marks its space as available but does not immediately overwrite the data. Reclaimer works in two ways:

1. **MFT Parsing.** On NTFS drives, file metadata is stored in a structure called the Master File Table. Deleted entries are marked but often still readable. Reclaimer reads the MFT directly to find these entries.

2. **File Carving.** Even when MFT entries are gone, file data may still exist on disk. Reclaimer scans raw bytes for known file signatures (magic bytes) to locate recoverable files.

---

## Supported File Systems

| File System | Status  |
| ----------- | ------- |
| NTFS        | Planned |
| FAT32/exFAT | Future  |
| ext4        | Future  |

---

## Getting Started

### Requirements

- Rust (install via [rustup](https://rustup.rs))
- Windows (for physical drive scanning)
- A raw disk image for testing (`.img` or `.dd`)

### Build

```bash
git clone https://github.com/dincertekin/reclaimer
cd reclaimer
cargo build
```

### Run

```bash
cargo run
```

---

## Project Structure

```
src/
├── main.rs        # Entry point
├── disk/          # Raw disk and image reading
├── scanner/       # File signature carving
├── ntfs/          # NTFS MFT parsing
└── ui/            # GUI (egui)
```

---

## Learning Goals

This project is also a personal exercise in learning Rust. The code prioritizes clarity and correctness over speed. Comments and documentation are written to explain not just what the code does, but why.

---

## Disclaimer

Reclaimer is intended for legitimate data recovery and forensic learning purposes only. Always recover files to a separate drive, never to the source drive, to avoid overwriting data you are trying to recover.

---

## License

MIT License, see [LICENSE](./LICENSE) for details.
