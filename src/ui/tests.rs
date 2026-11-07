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

