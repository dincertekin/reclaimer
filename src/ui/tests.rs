//! Drives the real app headlessly: choose → scan → browse.
//! Set RECLAIMER_SHOTS=<dir> to also save a screenshot of each step.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use eframe::egui::vec2;
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;

use super::types::Phase;
use super::ReclaimerApp;

fn step_until(harness: &mut Harness<'_, ReclaimerApp>, done: impl Fn(&ReclaimerApp) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !done(harness.state()) {
        let app = harness.state();
        assert!(Instant::now() < deadline, "timed out: phase {:?}, error {:?}, found {}, progress {:?}", app.phase, app.error, app.found.len(), app.progress);
        harness.step();
        std::thread::sleep(Duration::from_millis(20));
    }
    harness.run_steps(3);
}

fn shot(harness: &mut Harness<'_, ReclaimerApp>, name: &str) {
    if let Some(dir) = std::env::var_os("RECLAIMER_SHOTS") {
        let image = harness.render().expect("render");
        image.save(PathBuf::from(dir).join(format!("{name}.png"))).unwrap();
    }
}

#[test]
fn scan_an_image_and_browse_the_results() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let output = std::env::temp_dir().join(format!("reclaimer-ui-{}", std::process::id()));

    let mut harness = Harness::builder()
        // 1× scale: at 2× kittest clicks in physical pixels and misses.
        .with_size(vec2(1080.0, 720.0))
        .build_eframe(|cc| ReclaimerApp::new(cc, Some(root.join("test.img"))));
    harness.state_mut().output_dir = output.clone();
    step_until(&mut harness, |app| app.drives_listed.is_some());
    shot(&mut harness, "1-choose");

    harness.get_by_label("Scan").click();
    step_until(&mut harness, |app| app.phase == Phase::Done);
    assert_eq!(harness.state().found.len(), 2);
    assert!(harness.state().found.iter().all(|f| f.complete && f.path.exists()));
    step_until(&mut harness, |app| app.thumbs.idle());
    shot(&mut harness, "2-results");

    harness.get_by_label("recovered_1.jpg").click();
    harness.run_steps(2);
    assert_eq!(harness.state().selected, Some(0));
    step_until(&mut harness, |app| app.thumbs.idle());
    shot(&mut harness, "3-inspector");

    std::fs::remove_dir_all(output).unwrap();
}


/// Regenerates the README screenshot at Retina scale:
/// Regenerates the README screenshots at Retina scale (1080×720 points):
/// `RECLAIMER_README_SHOTS=.github/assets cargo test readme_screenshots -- --ignored`
#[test]
#[ignore]
fn readme_screenshots() {
    use crate::phone::{Phone, PhoneKind, PhoneStatus};
    use crate::scanner::samples;
    use crate::source::drives::Drive;
    use super::types::Source;
    let Some(dir) = std::env::var_os("RECLAIMER_README_SHOTS").map(PathBuf::from) else { return };
    let temp = std::env::temp_dir().join(format!("reclaimer-readme-{}", std::process::id()));
    std::fs::create_dir_all(&temp).unwrap();
    let harness = || Harness::builder().with_size(vec2(1080.0, 720.0)).with_pixels_per_point(2.0);

    // 1. Choose: demo devices, the SD card selected.
    let mut choose = harness().build_eframe(|cc| ReclaimerApp::new(cc, None));
    step_until(&mut choose, |app| app.drives_listed.is_some());
    let card = Drive {
        path: "/dev/rdisk4".into(), name: "SD Card".into(), model: "SD Card Reader".into(),
        size: 64_000_000_000, removable: true, encrypted: false, mount_points: Vec::new(),
    };
    let app = choose.state_mut();
    app.drives = vec![card.clone()];
    app.backups.clear();
    app.backups_locked = false;
    app.phones = vec![
        Phone { kind: PhoneKind::IPhone, id: "demo-iphone".into(), name: "My iPhone".into(), status: PhoneStatus::Ready },
        Phone { kind: PhoneKind::Android, id: "demo-android".into(), name: "Pixel 9".into(), status: PhoneStatus::Ready },
    ];
    app.source = Some(Source::Drive(card));
    app.drives_listed = Some(std::time::Instant::now()); // keep the demo devices on screen
    choose.run_steps(3);
    choose.render().unwrap().save(dir.join("screenshot-choose.png")).unwrap();

    // 2. Results: the exFAT fixture (two deleted photos, found by name) plus
    // a few files only carving can find.
    let mut image = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/exfat.img")).unwrap();
    for file in [samples::pdf(), samples::mp4(), samples::zip(), samples::sqlite()] {
        image.resize(image.len().next_multiple_of(512) + 4096, 0);
        image.extend(file);
    }
    let image_path = temp.join("demo.img");
    std::fs::write(&image_path, image).unwrap();
    let mut results = harness().build_eframe(|cc| ReclaimerApp::new(cc, Some(image_path)));
    results.state_mut().output_dir = temp.join("out");
    let ctx = results.ctx.clone();
    results.state_mut().start_scan(ctx);
    step_until(&mut results, |app| app.phase == Phase::Done);
    let home = std::env::home_dir().unwrap();
    results.state_mut().scan_folder = Some(home.join("Documents/Reclaimer/demo.img")); // as a user would see it
    results.state_mut().selected = Some(0);
    step_until(&mut results, |app| app.thumbs.idle());
    results.render().unwrap().save(dir.join("screenshot-results.png")).unwrap();

    std::fs::remove_dir_all(temp).unwrap();
}

/// Runs a full scan of a fixture through the app; returns the files found
/// and the folder they were saved in.
fn scan_fixture(name: &str) -> (Vec<super::types::FoundFile>, PathBuf, PathBuf) {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let output = std::env::temp_dir().join(format!("reclaimer-fs-{}-{name}", std::process::id()));
    let mut harness = Harness::builder()
        .with_size(vec2(1080.0, 720.0))
        .build_eframe(|cc| ReclaimerApp::new(cc, Some(root.join("testdata").join(name))));
    harness.state_mut().output_dir = output.clone();
    let ctx = harness.ctx.clone();
    harness.state_mut().start_scan(ctx);
    step_until(&mut harness, |app| app.phase == Phase::Done);
    let app = harness.state();
    assert_eq!(app.error, None);
    (app.found.clone(), app.scan_folder.clone().unwrap(), output)
}

#[test]
fn filesystem_scan_keeps_names_and_folders() {
    let (found, folder, output) = scan_fixture("exfat.img");
    let names: Vec<_> = found.iter().map(|f| (f.folder.clone(), f.filename.as_str())).collect();
    assert_eq!(names, [
        (Some("/DCIM".into()), "IMG_0001.JPG"),
        (Some("/Holiday Photos".into()), "Puppy and kitten.jpg"),
    ], "carving must not add the same photos again");
    assert!(folder.join("DCIM/IMG_0001.JPG").exists());
    assert!(folder.join("Holiday Photos/Puppy and kitten.jpg").exists());
    assert!(found.iter().all(|f| f.complete && f.modified.is_some()));
    std::fs::remove_dir_all(output).unwrap();
}

#[test]
fn carving_finds_what_the_filesystem_forgot() {
    // On this FAT12 card the second photo's folder was overwritten, so only
    // carving can find it — and it must not re-find the first photo.
    let (found, folder, output) = scan_fixture("fat12.img");
    let names: Vec<_> = found.iter().map(|f| (f.folder.is_some(), f.filename.as_str(), f.size)).collect();
    assert_eq!(names, [(true, "_MG_0001.JPG", 94_978), (false, "recovered_1.jpg", 42_355)]);
    assert!(folder.join("DCIM/_MG_0001.JPG").exists() && folder.join("recovered_1.jpg").exists());
    std::fs::remove_dir_all(output).unwrap();
}

fn fixture_backup(encrypted: bool) -> crate::phone::iphone::Backup {
    let folder = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/iphone-backups"));
    crate::phone::iphone::find_backups(&[folder]).unwrap().into_iter().find(|b| b.encrypted == encrypted).unwrap()
}

fn scan_backup(backup: crate::phone::iphone::Backup) -> Harness<'static, ReclaimerApp> {
    let mut harness = Harness::builder().with_size(vec2(1080.0, 720.0)).build_eframe(|cc| ReclaimerApp::new(cc, None));
    let output = std::env::temp_dir().join(format!("reclaimer-backup-{}-{}", std::process::id(), backup.encrypted));
    harness.state_mut().output_dir = output;
    harness.state_mut().source = Some(super::types::Source::Backup(backup));
    let ctx = harness.ctx.clone();
    harness.state_mut().start_scan(ctx);
    step_until(&mut harness, |app| app.phase != Phase::Scanning);
    harness
}

#[test]
fn iphone_backup_recovers_camera_roll_and_attachments() {
    use super::types::Filter;
    let mut harness = scan_backup(fixture_backup(false));
    let app = harness.state();
    assert_eq!(app.error, None);
    let mut found: Vec<_> = app.found.iter().map(|f| (f.folder.clone().unwrap(), f.filename.clone(), f.recently_deleted, f.complete)).collect();
    found.sort();
    assert_eq!(found, [
        ("/Camera Roll/100APPLE".into(), "IMG_0001.JPG".into(), false, true),
        ("/Camera Roll/100APPLE".into(), "IMG_0002.JPG".into(), true, true),
        ("/Messages".into(), "Menu.pdf".into(), false, true),
        ("/WhatsApp".into(), "9f8e2d1c.jpg".into(), false, true),
    ]);
    let folder = app.scan_folder.clone().unwrap();
    assert!(folder.join("Camera Roll/100APPLE/IMG_0002.JPG").exists());
    assert!(folder.join("WhatsApp/9f8e2d1c.jpg").exists());

    harness.state_mut().filter = Filter::RecentlyDeleted;
    let visible = harness.state().visible_files();
    assert_eq!(visible.len(), 1);
    assert_eq!(harness.state().found[visible[0]].filename, "IMG_0002.JPG");
    std::fs::remove_dir_all(folder.parent().unwrap()).unwrap();
}

#[test]
fn encrypted_backup_explains_itself() {
    let harness = scan_backup(fixture_backup(true));
    let app = harness.state();
    assert_eq!(app.phase, Phase::Choose, "nothing found: back to Choose with the message");
    assert!(app.error.as_deref().unwrap().contains("encrypted"));
}


#[test]
fn phone_files_land_in_their_folders() {
    use super::scan::ToFolder;
    use super::types::ScanMsg;
    use crate::phone::{PhoneFile, Sink};
    use std::sync::atomic::AtomicBool;

    let folder = std::env::temp_dir().join(format!("reclaimer-phone-{}", std::process::id()));
    let (tx, rx) = std::sync::mpsc::channel();
    let ctx = eframe::egui::Context::default();
    let cancel = AtomicBool::new(false);
    let mut sink = ToFolder { folder: folder.clone(), tx: &tx, ctx: &ctx, cancel: &cancel, total: 1, done: 0 };

    let photo = read_photo();
    let files = [
        PhoneFile { name: "IMG_0001.JPG".into(), folder: "/Camera Roll/100APPLE".into(), size: photo.len() as u64, modified: None, recently_deleted: false },
        PhoneFile { name: "IMG_0001.JPG".into(), folder: "/Camera Roll/100APPLE".into(), size: photo.len() as u64, modified: None, recently_deleted: true },
        PhoneFile { name: "a:b.pdf".into(), folder: "/Internal shared storage/Download".into(), size: 3, modified: None, recently_deleted: false },
    ];
    sink.listed(files.iter().map(|f| f.size).sum());
    for (file, bytes) in files.iter().zip([photo.clone(), photo.clone(), b"%PD".to_vec()]) {
        let path = sink.destination(file).unwrap();
        std::fs::write(&path, bytes).unwrap();
        assert!(sink.saved(file, path));
    }

    let found: Vec<_> = rx.try_iter().filter_map(|m| match m { ScanMsg::Found(f) => Some(f), _ => None }).collect();
    let summary: Vec<_> = found.iter().map(|f| (f.path.strip_prefix(&folder).unwrap().to_path_buf(), f.recently_deleted, f.complete)).collect();
    assert_eq!(summary, [
        (PathBuf::from("Camera Roll/100APPLE/IMG_0001.JPG"), false, true),
        (PathBuf::from("Camera Roll/100APPLE/IMG_0001 2.JPG"), true, true), // same name: kept apart
        (PathBuf::from("Internal shared storage/Download/a_b.pdf"), false, false), // unsafe name cleaned; not a real PDF
    ]);
    std::fs::remove_dir_all(folder).unwrap();
}

fn read_photo() -> Vec<u8> {
    let image = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/test.img")).unwrap();
    image[1024..1024 + 94_978].to_vec()
}


#[test]
fn results_sort_by_date_name_kind_and_size() {
    use super::types::{Category, FoundFile, Sort, SortKey};
    use crate::fs::Timestamp;

    let mut harness = Harness::builder().with_size(vec2(1080.0, 720.0)).build_eframe(|cc| ReclaimerApp::new(cc, None));
    let file = |name: &str, kind: &str, size: u64, day: Option<u8>| FoundFile {
        kind: kind.into(),
        extension: name.rsplit('.').next().unwrap().into(),
        offset: None,
        size,
        filename: name.into(),
        path: PathBuf::new(),
        category: Category::Photos,
        complete: true,
        folder: None,
        modified: day.map(|day| Timestamp { year: 2026, month: 11, day, hour: 0, minute: 0 }),
        recently_deleted: false,
    };
    harness.state_mut().found = vec![
        file("IMG_10.jpg", "JPEG", 300, Some(2)),
        file("IMG_2.jpg", "JPEG", 100, Some(5)),
        file("clip.mov", "MOV", 900, None), // found by content: no date
        file("a.pdf", "PDF", 50, Some(1)),
    ];
    let order = |h: &mut Harness<'_, ReclaimerApp>, sort: Sort| {
        h.state_mut().sort = sort;
        let app = h.state();
        app.visible_files().iter().map(|&i| app.found[i].filename.clone()).collect::<Vec<_>>()
    };

    assert_eq!(order(&mut harness, Sort::default()), ["IMG_2.jpg", "IMG_10.jpg", "a.pdf", "clip.mov"], "newest first, undated last");
    assert_eq!(order(&mut harness, Sort { key: SortKey::Date, descending: false }), ["a.pdf", "IMG_10.jpg", "IMG_2.jpg", "clip.mov"], "undated stay last");
    assert_eq!(order(&mut harness, Sort::by(SortKey::Name)), ["a.pdf", "clip.mov", "IMG_2.jpg", "IMG_10.jpg"], "natural order");
    assert_eq!(order(&mut harness, Sort::by(SortKey::Kind)), ["IMG_2.jpg", "IMG_10.jpg", "clip.mov", "a.pdf"]);
    assert_eq!(order(&mut harness, Sort::by(SortKey::Size)), ["clip.mov", "IMG_10.jpg", "IMG_2.jpg", "a.pdf"], "largest first");
}

