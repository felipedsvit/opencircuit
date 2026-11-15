//! The exit criterion of M3, through the window: open the star-delta
//! starter, start it from the operator panel and watch it go from star,
//! through open windings, to delta without the star and delta contactors
//! ever picked up together; then export the drawing as SVG and PDF.

use std::path::PathBuf;

use circuit_model::{MotorState, WindingConnection};
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use opencircuit_desktop::DesktopApp;

const STAR_DELTA: &str = include_str!("../../../examples/star-delta/star-delta.ocircuit");
const STAR_DELTA_SVG: &str = include_str!("../../../examples/star-delta/star-delta.svg");
const STAR_DELTA_PDF: &[u8] = include_bytes!("../../../examples/star-delta/star-delta.pdf");
const SHEETS_PROJECT: &str = include_str!("../../../examples/sheets/starter-sheets.ocircuit");
const SHEETS_PDF: &[u8] = include_bytes!("../../../examples/sheets/starter-sheets.pdf");

/// A path in the temporary directory, removed when dropped.
struct TempPath(PathBuf);

impl TempPath {
    fn new(extension: &str) -> Self {
        Self(std::env::temp_dir().join(format!(
            "opencircuit-m3-exit-{}.{extension}",
            std::process::id()
        )))
    }
}

impl Drop for TempPath {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn opened() -> Harness<'static, DesktopApp> {
    let mut app = DesktopApp::default();
    app.open_text(STAR_DELTA, "star-delta.ocircuit");
    let mut harness = Harness::new_ui_state(|ui, app: &mut DesktopApp| app.show(ui), app);
    harness.set_size(egui::Vec2::new(1400.0, 900.0));
    harness.run();
    harness
}

fn click(harness: &mut Harness<'_, DesktopApp>, label: &str) {
    harness.get_by_label(label).click();
    harness.run();
}

fn active(harness: &Harness<'_, DesktopApp>, reference: &str) -> bool {
    let session = harness.state().session().unwrap();
    let device = session
        .document()
        .devices
        .iter()
        .find(|d| d.reference.0 == reference)
        .unwrap();
    session
        .snapshot()
        .is_some_and(|s| s.active_devices.contains(&device.id))
}

/// What the motor does: its state and, while it runs, its winding.
fn motor(harness: &Harness<'_, DesktopApp>) -> (MotorState, Option<WindingConnection>) {
    let snapshot = harness.state().session().unwrap().snapshot().unwrap();
    let view = snapshot.motors.first().unwrap();
    (view.state, view.winding)
}

#[test]
fn star_delta_starts_in_star_then_runs_in_delta_and_exports() {
    let mut harness = opened();
    assert!(harness.state().session().unwrap().diagnostics().is_empty());

    // Hold S1 for a few ticks: KM3 (star) picks up, then KM1.
    click(&mut harness, "S1: press");
    let mut seen = Vec::new();
    for tick in 0..20 {
        if tick == 4 {
            click(&mut harness, "S1: release");
        }
        click(&mut harness, "Step");

        assert!(
            !(active(&harness, "KM2") && active(&harness, "KM3")),
            "step {tick}: star and delta together"
        );
        let now = motor(&harness);
        if seen.last() != Some(&now) {
            seen.push(now);
        }
    }

    assert_eq!(
        seen,
        [
            (MotorState::Stopped, None),
            (MotorState::Forward, Some(WindingConnection::Star)),
            (MotorState::Stopped, None),
            (MotorState::Forward, Some(WindingConnection::Delta)),
        ]
    );
    assert!(active(&harness, "KM1") && active(&harness, "KM2"));

    // The exports: at rest they are the committed drawings; with the
    // simulation state the SVG shows the running tick.
    let session = harness.state().session().unwrap();
    let svg = TempPath::new("svg");
    let pdf = TempPath::new("pdf");
    session.export(&svg.0, false).unwrap();
    session.export(&pdf.0, false).unwrap();

    assert_eq!(std::fs::read_to_string(&svg.0).unwrap(), STAR_DELTA_SVG);
    assert_eq!(std::fs::read(&pdf.0).unwrap(), STAR_DELTA_PDF);
    assert_ne!(session.svg(true), STAR_DELTA_SVG);
}

#[test]
fn the_pdf_of_a_project_with_sheets_is_the_committed_multi_page_one() {
    let mut app = DesktopApp::default();
    app.open_text(SHEETS_PROJECT, "starter-sheets.ocircuit");

    let pdf = app.session().unwrap().pdf(false);

    assert_eq!(pdf, SHEETS_PDF);
    assert!(String::from_utf8_lossy(&pdf).contains("/Count 2"));
}
