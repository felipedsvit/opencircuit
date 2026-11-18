//! Version history (M9.1, #118), through the window: every save keeps a
//! version, the File menu lists them, and one opens as a new, unsaved
//! project without touching the file.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use eframe::egui::{self, Key};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use opencircuit_desktop::DesktopApp;
use opencircuit_desktop::versions::VersionStore;

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "opencircuit-desktop-versions-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("scratch");
        let example = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/direct-starter/starter.ocircuit");
        fs::copy(example, dir.join("starter.ocircuit")).expect("copy");
        Self(dir)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn window(scratch: &Scratch) -> Harness<'static, DesktopApp> {
    let mut app = DesktopApp::new(Some(&scratch.0.join("starter.ocircuit")));
    app.enable_version_history(VersionStore::at(scratch.0.join("versions")));
    let mut harness = Harness::new_ui_state(|ui, app: &mut DesktopApp| app.show(ui), app);
    harness.set_size(egui::Vec2::new(1300.0, 900.0));
    harness.run();
    harness
}

/// Type a line in the command line and send it.
fn command(harness: &mut Harness<'_, DesktopApp>, line: &str) {
    let field = harness.get_by_label("Command");
    field.focus();
    field.type_text(line);
    harness.run();
    harness.key_press(Key::Enter);
    harness.run();
}

/// Place a lamp, which changes the project.
fn place(harness: &mut Harness<'_, DesktopApp>, at: &str) {
    command(harness, "mode edit");
    command(harness, &format!("place lamp.generic {at}"));
}

fn devices(harness: &Harness<'_, DesktopApp>) -> usize {
    harness
        .state()
        .session()
        .expect("a project is open")
        .document()
        .devices
        .len()
}

#[test]
fn every_save_is_a_version_and_one_opens_as_a_new_unsaved_project() {
    let scratch = Scratch::new();
    let mut harness = window(&scratch);
    let original = devices(&harness);

    place(&mut harness, "40,40");
    assert!(harness.state_mut().save());
    place(&mut harness, "50,40");
    assert!(harness.state_mut().save());
    // Saving again without a change adds no version.
    assert!(harness.state_mut().save());
    harness.run();

    harness.get_by_label("File").click();
    harness.run();
    harness.get_by_label("Version history…").click();
    harness.run();
    assert_eq!(harness.state().versions_listed(), Some(2));

    // The older version: one lamp, not two.
    harness.get_by_label("Open version 2").click();
    harness.run();

    let app = harness.state();
    assert_eq!(app.versions_listed(), None, "the window closes");
    assert_eq!(devices(&harness), original + 1);
    let session = harness.state().session().expect("a project is open");
    assert_eq!(session.path(), None, "a new project, not the file");
    assert!(harness.state().is_dirty());
    let on_disk = fs::read_to_string(scratch.0.join("starter.ocircuit")).expect("file");
    assert_eq!(
        circuit_project::from_str(&on_disk)
            .expect("project")
            .devices
            .len(),
        original + 2,
        "the file is untouched"
    );
}

#[test]
fn opening_a_version_over_unsaved_changes_asks_first() {
    let scratch = Scratch::new();
    let mut harness = window(&scratch);
    place(&mut harness, "40,40");
    assert!(harness.state_mut().save());
    place(&mut harness, "50,40");

    harness.state_mut().show_version_history();
    harness.run();
    harness.get_by_label("Open version 1").click();
    harness.run();

    assert!(harness.state().is_asking());
    harness.get_by_label("Cancel").click();
    harness.run();
    assert!(harness.state().session().expect("open").path().is_some());
}

#[test]
fn without_a_store_the_menu_has_no_version_history() {
    let scratch = Scratch::new();
    let app = DesktopApp::new(Some(&scratch.0.join("starter.ocircuit")));
    let mut harness = Harness::new_ui_state(|ui, app: &mut DesktopApp| app.show(ui), app);
    harness.set_size(egui::Vec2::new(1300.0, 900.0));
    harness.run();

    harness.get_by_label("File").click();
    harness.run();

    assert!(harness.query_by_label("Version history…").is_none());
}
