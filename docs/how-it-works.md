# How Reclaimer recovers files

When you delete a file, the data usually isn't erased. The computer just
forgets where it was and marks the space as free. Until something new is
written there, the file can be brought back. Reclaimer looks in three ways,
depending on what you scan.

- [Drives, SD cards, USB sticks and disk images](#drives-sd-cards-usb-sticks-and-disk-images)
- [Phones](#phones)
- [What can't be recovered](#what-cant-be-recovered)
- [Permissions on each platform](#permissions-on-each-platform)

## Drives, SD cards, USB sticks and disk images

A scan has two stages.

### 1. Ask the file system

Deleting a file rarely erases its record; the file system just marks it as
unused. Reclaimer reads the partition table (MBR, with logical partitions, or
GPT, or none on cards formatted whole), then each volume's records:

| File system | Where it's used | What a deleted file leaves behind |
|-------------|-----------------|-----------------------------------|
| FAT12 / FAT16 / FAT32 | SD cards, USB sticks, cameras | Name (FAT loses the first letter of a short 8.3 name), folder, size, date |
| exFAT | SD cards of 64 GB and more, USB drives | Full name, folder, size, date, and whether the data was in one piece |
| NTFS | Windows disks | Full name, folder, dates, and an exact map of every fragment |

Files come back with their real names, saved into their original folders.
If some of a file's space has since been given to another file, it's marked
**Partial**: it may open with pieces missing.

### 2. Search every block

Then every block is searched for files the file system has forgotten: on a
reformatted card, or when a folder's own record was overwritten. This is
called *carving*. Each file type starts with fixed bytes (its "magic
number"); finding the start is easy, finding the **end** is what matters.
Reclaimer never guesses. Each format's carver walks the file's own structure
until it ends:

| Format | Starts with | Ends where |
|--------|-------------|------------|
| JPEG | `FF D8 FF` | `FF D9`, reached by hopping segment to segment (skipping EXIF thumbnails) |
| PNG | `89 50 4E 47 0D 0A 1A 0A` | after the `IEND` chunk |
| GIF | `GIF87a` / `GIF89a` | the `3B` trailer, after walking all blocks |
| MP4 · MOV · HEIC · AVIF · 3GP | `ftyp` box at offset 4 | after the last top-level box |
| PDF | `%PDF-` | the last `%%EOF` (or the exact length in linearized PDFs) |
| ZIP · DOCX · XLSX · PPTX | `PK 03 04` | the end-of-central-directory record that points back to this file |
| SQLite | `SQLite format 3` | page size × page count from the header |

Files already found by name aren't reported twice, and space that belongs to
files that still exist is skipped, so the results show only what was lost.

Drives are only ever opened **read-only**, and unreadable (bad) sectors are
skipped instead of stopping the scan. Reclaimer refuses to save recovered
files onto the drive being scanned, because that could overwrite the very
files you want back.

## Phones

Neither iPhones nor modern Android phones let a computer read their raw
storage, so Reclaimer copies everything the phone hands over, its trash
included.

**iPhone on a cable.** Through the system's iPhone connection service
(usbmuxd) and Apple's file service (AFC), Reclaimer copies the whole camera
roll. Photos in **Recently Deleted** are still on the phone until they're
purged, so they come back too; the Photos library database (including its
write-ahead log, where the newest changes are) says which ones they are.

**Android on a cable.** Over USB file transfer (MTP), Reclaimer copies
photos, videos, audio and documents from shared storage: camera, screenshots,
downloads, WhatsApp media. App-private data and thumbnail caches are
skipped. Since Android 11, the gallery's trash keeps files for 30 days as
hidden `.trashed-<time>-<name>` files; Reclaimer brings them back under their
real names, along with Samsung Gallery's `.Trash` folder.

**iPhone backups on this computer.** A Finder or iTunes backup holds the
Camera Roll (Recently Deleted included), Messages attachments and WhatsApp
media. Its files are stored under hashed names; Reclaimer reads the backup's
index (`Manifest.db`) to give each one its real name and folder.

## What can't be recovered

- **Photos removed from an iPhone's Recently Deleted.** Every file on an
  iPhone has its own encryption key, and iOS destroys that key when a file is
  deleted for good. No tool can read the data afterwards. An older backup, or
  iCloud within its 30 days, may still have a copy.
- **Files deleted from a phone's internal storage after the trash empties.**
  Modern Android encrypts each file too.
- **Files on SSDs, after a while.** SSDs erase freed space in the background
  (TRIM), often within minutes. SD cards and USB sticks usually don't.
- **The internal SSD of Apple silicon and T2 Macs.** It's always encrypted
  by the hardware, so raw reads return noise. Reclaimer shows it, disabled.
- **Overwritten files.** Once new data lands on a deleted file's space, that
  part is gone. Stop using a card or drive as soon as you notice a loss.

## Permissions on each platform

| Platform | Drives | Phones |
|----------|--------|--------|
| macOS | The system password prompt appears when you scan a drive | iPhone: tap **Trust**. Backups need **Full Disk Access** (the app offers a shortcut). Android: macOS's camera service is asked to step aside; quit Photos, Image Capture and Android File Transfer if they're open |
| Windows | Run Reclaimer as administrator | iPhone: install **Apple Devices** (or iTunes) from the Microsoft Store. Android: works with Windows' own MTP support |
| Linux | Run with `sudo`, or join the `disk` group | iPhone: install `usbmuxd`. Android: allow USB access with a udev rule: `SUBSYSTEM=="usb", MODE="0666"` in `/etc/udev/rules.d/99-mtp.rules`, then `sudo udevadm control --reload-rules` |

iPhone backups live in `~/Library/Application Support/MobileSync/Backup`
(macOS) and `%APPDATA%\Apple Computer\MobileSync\Backup` or
`%USERPROFILE%\Apple\MobileSync\Backup` (Windows).
