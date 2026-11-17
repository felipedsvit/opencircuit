//! Autosave and recovery (M9.1, #118), through the window: a copy of a
//! project with unsaved changes is kept in the recovery folder, removed when
//! the project is saved or closed on purpose, and offered again by the next
//! window when its window ended without closing.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use eframe::egui::{self, Key};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use opencircuit_desktop::DesktopApp;
use opencircuit_desktop::recovery::RecoveryStore;

/// A scratch directory with a recovery folder and a project, removed when
/// dropped.
struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "opencircuit-desktop-recovery-{}-{}",
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

    fn project(&self) -> PathBuf {
        self.0.join("starter.ocircuit")
    }

    fn store(&self) -> RecoveryStore {
        RecoveryStore::at(self.0.join("recovery"))
    }

    /// The copies and descriptions in the recovery folder; the lock a
    /// running window holds there is left out.
    fn copies(&self) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(self.0.join("recovery"))
            .map(|listing| {
                listing
                    .filter_map(Result::ok)
                    .map(|entry| entry.file_name().to_string_lossy().into_owned())
                    .filter(|name| !name.ends_with(".lock"))
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// A window on the project with autosave on, writing at every frame.
fn window(scratch: &Scratch) -> Harness<'static, DesktopApp> {
    let mut app = DesktopApp::new(Some(&scratch.project()));
    app.enable_recovery(scratch.store(), 0.0);
    let mut harness = Harness::new_ui_state(|ui, app: &mut DesktopApp| app.show(ui), app);
    harness.set_size(egui::Vec2::new(1300.0, 900.0));
    harness.run();
    harness
}

/// Change the project through the command line.
fn edit(harness: &mut Harness<'_, DesktopApp>) {
    harness.get_by_label("Edit").click();
    harness.run();
    let field = harness.get_by_label("Command");
    field.focus();
    field.type_text("place lamp.generic 40,40");
    harness.run();
    harness.key_press(Key::Enter);
    harness.run();
    harness.run();
}

/// A copy left by a window that ended long ago, as `Autosave` writes it.
fn leave_copy(scratch: &Scratch, name: &str, original: Option<&Path>) {
    let dir = scratch.0.join("recovery");
    fs::create_dir_all(&dir).expect("folder");
    let text = fs::read_to_string(scratch.project())
        .expect("project")
        .replace(
            "\"name\": \"Direct-online starter\"",
            &format!("\"name\": \"{name}\""),
        );
    fs::write(dir.join("window-1-1.ocircuit"), text).expect("copy");
    let meta = serde_json::json!({
        "format": 1,
        "original": original,
        "name": name,
        "saved_at": 1,
        "heartbeat": 1,
    });
    fs::write(dir.join("window-1-1.json"), meta.to_string()).expect("meta");
}

#[test]
fn a_copy_is_kept_while_there_are_unsaved_changes_and_removed_by_saving() {
    let scratch = Scratch::new();
    let mut harness = window(&scratch);
    assert!(scratch.copies().is_empty(), "nothing to keep yet");

    edit(&mut harness);

    assert!(harness.state().is_dirty());
    assert!(harness.state().recovery_current());
    assert_eq!(scratch.copies().len(), 2, "{:?}", scratch.copies());
    harness.get_by_label("Recovery copy saved");

    assert!(harness.state_mut().save());
    harness.run();

    assert!(scratch.copies().is_empty());
}

#[test]
fn a_copy_left_by_a_window_that_ended_is_recovered_as_unsaved_changes() {
    let scratch = Scratch::new();
    let project = scratch.project();
    leave_copy(&scratch, "Pump before the crash", Some(&project));

    let mut harness = window(&scratch);
    assert_eq!(harness.state().recovery_offered(), 1);
    harness.get_by_label("Recover unsaved work");

    harness
        .get_by_label("Recover Pump before the crash")
        .click();
    harness.run();

    let app = harness.state();
    assert_eq!(app.recovery_offered(), 0);
    let session = app.session().expect("a project is open");
    assert_eq!(session.document().project.name, "Pump before the crash");
    assert_eq!(session.path(), Some(project.as_path()));
    assert!(app.is_dirty(), "a recovered project is not saved yet");
    // The old copy is gone; this window keeps its own.
    harness.run();
    assert!(
        !scratch
            .copies()
            .iter()
            .any(|name| name.starts_with("window-1-1")),
        "{:?}",
        scratch.copies()
    );
}

#[test]
fn a_discarded_copy_is_removed_and_the_project_stays_as_it_was() {
    let scratch = Scratch::new();
    leave_copy(&scratch, "Unwanted", None);

    let mut harness = window(&scratch);
    harness.get_by_label("Discard Unwanted").click();
    harness.run();

    assert_eq!(harness.state().recovery_offered(), 0);
    assert!(scratch.copies().is_empty());
    let session = harness.state().session().expect("a project is open");
    assert_eq!(session.document().project.name, "Direct-online starter");
    assert!(!harness.state().is_dirty());
}

#[test]
fn later_keeps_the_copy_for_the_next_window() {
    let scratch = Scratch::new();
    leave_copy(&scratch, "Not now", None);

    let mut harness = window(&scratch);
    harness.get_by_label("Later").click();
    harness.run();

    assert_eq!(harness.state().recovery_offered(), 0);
    assert_eq!(scratch.copies().len(), 2);
    assert_eq!(window(&scratch).state().recovery_offered(), 1);
}

#[test]
fn discarding_the_changes_on_close_removes_the_copy() {
    let scratch = Scratch::new();
    let mut harness = window(&scratch);
    edit(&mut harness);
    assert_eq!(scratch.copies().len(), 2);

    assert!(!harness.state_mut().request_close());
    harness.run();
    harness.get_by_label("Discard changes").click();
    harness.run();

    assert!(scratch.copies().is_empty());
}

#[test]
fn a_window_without_recovery_writes_nothing_and_offers_nothing() {
    let scratch = Scratch::new();
    leave_copy(&scratch, "Someone else's", None);
    let app = DesktopApp::new(Some(&scratch.project()));
    let mut harness = Harness::new_ui_state(|ui, app: &mut DesktopApp| app.show(ui), app);
    harness.run();

    assert_eq!(harness.state().recovery_offered(), 0);
    assert!(harness.query_by_label("Recover unsaved work").is_none());
}
