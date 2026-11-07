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
/// `RECLAIMER_README_SHOT=.github/assets/screenshot.png cargo test readme_screenshot -- --ignored`
#[test]
#[ignore]
fn readme_screenshot() {
    use crate::scanner::samples;
    let Some(target) = std::env::var_os("RECLAIMER_README_SHOT") else { return };
    let temp = std::env::temp_dir().join(format!("reclaimer-readme-{}", std::process::id()));
    std::fs::create_dir_all(&temp).unwrap();

    // A demo card: the exFAT fixture (two deleted photos, found by name)
    // plus a few files only carving can find.
    let mut image = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/exfat.img")).unwrap();
    for file in [samples::pdf(), samples::mp4(), samples::zip(), samples::sqlite()] {
        image.resize(image.len().next_multiple_of(512) + 4096, 0);
        image.extend(file);
    }
    let image_path = temp.join("demo.img");
    std::fs::write(&image_path, image).unwrap();

    // 2× scale: the window is 1080×720 points, rendered at 2160×1440.
    let mut harness = Harness::builder()
        .with_size(vec2(1080.0, 720.0))
        .with_pixels_per_point(2.0)
        .build_eframe(|cc| ReclaimerApp::new(cc, Some(image_path)));
    harness.state_mut().output_dir = temp.join("out");
    let ctx = harness.ctx.clone();
    harness.state_mut().start_scan(ctx);
    step_until(&mut harness, |app| app.phase == Phase::Done);
    let home = std::env::home_dir().unwrap();
    harness.state_mut().scan_folder = Some(home.join("Documents/Reclaimer/demo.img")); // as a user would see it
    harness.state_mut().selected = Some(0);
    step_until(&mut harness, |app| app.thumbs.idle());

    harness.render().unwrap().save(target).unwrap();
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
