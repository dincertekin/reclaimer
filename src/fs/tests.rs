//! Each filesystem is tested on volumes written by someone else — macOS
//! (the fixtures in testdata/), the `fatfs` crate — except NTFS, which no
//! tool here can format, so a small volume is built by hand.

use std::io::Write;

use super::*;
use crate::source::{Piece, Window};

fn read(name: &str) -> Vec<u8> {
    std::fs::read(format!("{}/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

/// The two photos stored in test.img.
fn photos() -> (Vec<u8>, Vec<u8>) {
    let image = read("test.img");
    (image[1024..1024 + 94_978].to_vec(), image[102_400..102_400 + 42_355].to_vec())
}

fn contents(image: &mut Vec<u8>, file: &DeletedFile) -> Vec<u8> {
    let mut w = Window::new(image);
    let mut out = Vec::new();
    for piece in &file.pieces {
        match piece {
            Piece::Disk { offset, len } => out.extend_from_slice(w.bytes(*offset, *len as usize).unwrap()),
            Piece::Zeros(len) => out.resize(out.len() + *len as usize, 0),
            Piece::Bytes(bytes) => out.extend(bytes),
        }
    }
    out
}

fn only_volume(image: &mut Vec<u8>) -> Volume {
    let mut volumes = analyze(image).unwrap();
    assert_eq!(volumes.len(), 1);
    volumes.remove(0)
}

fn file<'a>(volume: &'a Volume, folder: &str, name: &str) -> &'a DeletedFile {
    volume
        .files
        .iter()
        .find(|f| f.folder == folder && f.name == name)
        .unwrap_or_else(|| panic!("{folder}/{name} not found in {:?}", volume.files.iter().map(|f| (&f.folder, &f.name)).collect::<Vec<_>>()))
}

#[test]
fn fat12_and_fat16_recover_the_deleted_photo() {
    let (photo, _) = photos();
    for (fixture, kind) in [("testdata/fat12.img", "FAT12"), ("testdata/fat16.img", "FAT16")] {
        let mut image = read(fixture);
        let volume = only_volume(&mut image);
        assert_eq!(volume.filesystem, kind);

        // FAT keeps all but the first letter of a deleted 8.3 name.
        let f = file(&volume, "/DCIM", "_MG_0001.JPG");
        assert!(f.intact);
        assert_eq!(contents(&mut image, f), photo, "{kind}");
        let when = Timestamp { year: 2026, month: 11, day: 7, hour: 4, minute: 45 };
        assert_eq!(f.modified, Some(Modified::Local(when)));

        // The folder holding the other photo was overwritten by macOS right
        // after deletion (carving finds that photo), and notes.txt still
        // exists — so this is the only file.
        assert_eq!(volume.files.len(), 1, "{kind}");
        let allocation = volume.allocation.as_ref().unwrap();
        assert!(!allocation.contains(f.start().unwrap()));
    }
}

#[test]
fn exfat_recovers_full_names_and_deleted_folders() {
    let (first, second) = photos();
    let mut image = read("testdata/exfat.img");
    let volume = only_volume(&mut image);
    assert_eq!(volume.filesystem, "exFAT");
    assert_eq!(volume.files.len(), 2);

    let a = file(&volume, "/DCIM", "IMG_0001.JPG");
    let b = file(&volume, "/Holiday Photos", "Puppy and kitten.jpg");
    assert!(a.intact && b.intact);
    assert_eq!(contents(&mut image, a), first);
    assert_eq!(contents(&mut image, b), second);
    assert_eq!(a.modified, Some(Modified::Utc(1_794_019_500))); // 7 Nov 2026 02:45 UTC
}

#[test]
fn fat32_written_by_another_implementation() {
    let (first, second) = photos();
    let mut disk = std::io::Cursor::new(vec![0u8; 40 << 20]);
    let options = fatfs::FormatVolumeOptions::new().fat_type(fatfs::FatType::Fat32).bytes_per_cluster(512);
    fatfs::format_volume(&mut disk, options).unwrap();
    {
        let fs = fatfs::FileSystem::new(&mut disk, fatfs::FsOptions::new()).unwrap();
        {
            let root = fs.root_dir();
            root.create_dir("DCIM").unwrap();
            root.create_dir("Holiday Photos").unwrap();
            root.create_file("DCIM/IMG_0001.JPG").unwrap().write_all(&first).unwrap();
            root.create_file("Holiday Photos/Puppy and kitten.jpg").unwrap().write_all(&second).unwrap();
            root.create_file("notes.txt").unwrap().write_all(b"still here").unwrap();
            root.remove("DCIM/IMG_0001.JPG").unwrap();
            root.remove("Holiday Photos/Puppy and kitten.jpg").unwrap();
            root.remove("Holiday Photos").unwrap();
        }
        fs.unmount().unwrap();
    }
    let mut image = disk.into_inner();
    let volume = only_volume(&mut image);
    assert_eq!(volume.filesystem, "FAT32");

    let a = volume.files.iter().find(|f| f.folder == "/DCIM" && f.name.ends_with("MG_0001.JPG")).expect("photo in /DCIM");
    let b = file(&volume, "/Holiday Photos", "Puppy and kitten.jpg"); // long name survives
    assert!(a.intact && b.intact);
    assert_eq!(contents(&mut image, a), first);
    assert_eq!(contents(&mut image, b), second);
    assert_eq!(volume.files.len(), 2);
}

#[test]
fn ntfs_in_an_mbr_partition() {
    use ntfs::build::*;
    const DATA: u32 = 0x80;
    const DIR: u16 = 0x02;
    const IN_USE: u16 = 0x01;
    let (first, second) = photos();
    let modified = 1_794_019_500;

    let clusters = 160u64;
    let mut vol = vec![0u8; (clusters * CLUSTER) as usize];
    let at = |lcn: u64| (lcn * CLUSTER) as usize;
    vol[..512].copy_from_slice(&boot_sector(clusters, 4));

    let mut records: Vec<(usize, Vec<u8>)> = vec![
        (0, record(1, IN_USE, &[file_name(5, 5, "$MFT"), non_resident_data(&[(Some(4), 10)], 40 * 1024)])),
        (5, record(5, IN_USE | DIR, &[file_name(5, 5, ".")])),
        (6, record(6, IN_USE, &[file_name(5, 5, "$Bitmap"), non_resident_data(&[(Some(20), 1)], clusters / 8)])),
        // Deleted, fragmented in two runs.
        (30, record(2, 0, &[standard_information(modified), file_name(5, 5, "IMG_2001.JPG"), non_resident_data(&[(Some(32), 10), (Some(60), 14)], first.len() as u64)])),
        // Deleted, small enough to live inside its record.
        (31, record(2, 0, &[file_name(5, 5, "note.txt"), resident(DATA, b"hello ntfs")])),
        // A deleted folder (sequence bumped on deletion) and a file in it,
        // part of whose space has been reused.
        (32, record(3, DIR, &[file_name(5, 5, "Old Photos")])),
        (33, record(2, 0, &[file_name(32, 2, "kitten.jpg"), non_resident_data(&[(Some(100), 11)], second.len() as u64)])),
        // Still in use: not a lost file.
        (34, record(1, IN_USE, &[file_name(5, 5, "live.txt"), resident(DATA, b"still here")])),
        // Its folder's record is gone.
        (35, record(2, 0, &[file_name(77, 1, "orphan.txt"), resident(DATA, b"lost")])),
    ];
    for (index, bytes) in records.drain(..) {
        let offset = at(4) + index * RECORD;
        vol[offset..offset + RECORD].copy_from_slice(&bytes);
    }
    vol[at(32)..at(32) + 40_960].copy_from_slice(&first[..40_960]);
    vol[at(60)..at(60) + first.len() - 40_960].copy_from_slice(&first[40_960..]);
    vol[at(100)..at(100) + second.len()].copy_from_slice(&second);
    // In use: boot sector and MFT (0–13), the bitmap (20), and one cluster of
    // kitten.jpg's old space (105).
    for c in (0..14).chain([20, 105]) {
        vol[at(20) + c / 8] |= 1 << (c % 8);
    }

    // Put the volume in an MBR partition starting 1 MiB in.
    let mut image = vec![0u8; 1 << 20];
    image[446 + 4] = 0x07; // NTFS
    image[446 + 8..446 + 12].copy_from_slice(&2048u32.to_le_bytes());
    image[446 + 12..446 + 16].copy_from_slice(&((vol.len() / 512) as u32).to_le_bytes());
    image[510..512].copy_from_slice(&[0x55, 0xAA]);
    image.extend(vol);

    let volume = only_volume(&mut image);
    assert_eq!(volume.filesystem, "NTFS");
    assert_eq!(volume.files.len(), 4, "live.txt must not appear");

    let photo = file(&volume, "/", "IMG_2001.JPG");
    assert!(photo.intact);
    assert_eq!(contents(&mut image, photo), first);
    assert_eq!(photo.modified, Some(Modified::Utc(modified)));

    let note = file(&volume, "/", "note.txt");
    assert_eq!(contents(&mut image, note), b"hello ntfs");

    let kitten = file(&volume, "/Old Photos", "kitten.jpg");
    assert!(!kitten.intact, "cluster 105 was reused");
    assert_eq!(contents(&mut image, kitten), second);

    file(&volume, "/Unknown Folder", "orphan.txt");

    let allocation = volume.allocation.as_ref().unwrap();
    let base = 1 << 20;
    assert!(allocation.contains(base + 20 * CLUSTER));
    assert!(!allocation.contains(base + 32 * CLUSTER));
}

#[test]
fn an_image_without_a_filesystem_has_no_volumes() {
    let mut image = read("test.img");
    assert!(analyze(&mut image).unwrap().is_empty());
}

#[test]
fn timestamps() {
    let t = Timestamp { year: 2026, month: 11, day: 7, hour: 2, minute: 45 };
    assert_eq!(t.to_unix(), 1_794_019_500);
    assert_eq!(Timestamp::from_unix(1_794_019_500), t);
    assert_eq!(Timestamp::from_unix(951_782_400), Timestamp { year: 2000, month: 2, day: 29, hour: 0, minute: 0 });
    assert_eq!(Timestamp::from_dos(0x5D67, 0x05A7), Some(Timestamp { year: 2026, month: 11, day: 7, hour: 0, minute: 45 }));
    assert_eq!(Timestamp::from_dos(0, 0), None);
}
