//! M9.1 (#118): the autosave of the browser build, through the window with a
//! slot in memory in place of `localStorage`, and the backups a save keeps
//! next to the project when they are turned on.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use eframe::egui::{self, Key};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use opencircuit_desktop::browser_recovery::{MemorySlot, Slot as _};
use opencircuit_desktop::files::backup_path;
use opencircuit_desktop::{DesktopApp, Mode};

fn window(app: DesktopApp) -> Harness<'static, DesktopApp> {
    let mut harness = Harness::new_ui_state(|ui, app: &mut DesktopApp| app.show(ui), app);
    harness.set_size(egui::Vec2::new(1300.0, 900.0));
    harness.run();
    harness
}

/// Change the shown project through the command line.
fn edit(harness: &mut Harness<'_, DesktopApp>) {
    harness.state_mut().set_mode(Mode::Edit);
    harness.run();
    let field = harness.get_by_label("Command");
    field.focus();
    field.type_text("text 4,4 changed");
    harness.run();
    harness.key_press(Key::Enter);
    harness.run();
    assert!(harness.state().is_dirty());
}

#[test]
fn a_page_keeps_its_changes_and_the_next_page_offers_them() {
    let slot = MemorySlot::default();
    {
        let mut app = DesktopApp::default();
        app.enable_slot_autosave(Box::new(slot.clone()), 0.0);
        let mut harness = window(app);
        assert!(!harness.state().slot_recovery_offered());
        harness.get_by_label("New").click();
        harness.run();
        edit(&mut harness);
        harness.run();
        assert!(slot.get().is_some());
        assert!(harness.query_by_label("Recovery copy saved").is_some());
        // The page is closed without saving.
    }
    let mut app = DesktopApp::default();
    app.enable_slot_autosave(Box::new(slot.clone()), 0.0);
    let mut harness = window(app);
    assert!(harness.state().slot_recovery_offered());
    harness.get_by_label("Recover").click();
    harness.run();
    let app = harness.state();
    assert!(!app.slot_recovery_offered());
    assert!(app.is_dirty());
    assert!(
        app.session()
            .expect("recovered")
            .document()
            .texts
            .iter()
            .any(|note| note.text == "changed")
    );
}

#[test]
fn a_discarded_copy_is_gone_and_a_saved_project_keeps_none() {
    let slot = MemorySlot::default();
    {
        let mut app = DesktopApp::default();
        app.enable_slot_autosave(Box::new(slot.clone()), 0.0);
        let mut harness = window(app);
        harness.get_by_label("New").click();
        harness.run();
        edit(&mut harness);
        harness.run();
    }
    let mut app = DesktopApp::default();
    app.enable_slot_autosave(Box::new(slot.clone()), 0.0);
    let mut harness = window(app);
    harness.get_by_label("Discard").click();
    harness.run();
    assert_eq!(slot.get(), None);
    assert!(harness.state().session().is_none());
}

/// A scratch directory with a copy of the starter, removed when dropped.
struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "opencircuit-desktop-backups-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("scratch");
        fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../examples/direct-starter/starter.ocircuit"),
            dir.join("starter.ocircuit"),
        )
        .expect("copy");
        Self(dir)
    }

    fn project(&self) -> PathBuf {
        self.0.join("starter.ocircuit")
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn backups_are_off_in_a_fresh_window_and_kept_when_turned_on() {
    let scratch = Scratch::new();
    let original = fs::read_to_string(scratch.project()).expect("read");
    let mut harness = window(DesktopApp::new(Some(&scratch.project())));
    assert_eq!(harness.state().backups(), 0);
    edit(&mut harness);
    assert!(harness.state_mut().save());
    assert!(!backup_path(&scratch.project(), 1).exists());

    harness.state_mut().set_backups(3);
    edit(&mut harness);
    let before = fs::read_to_string(scratch.project()).expect("read");
    assert!(harness.state_mut().save());
    assert_eq!(
        fs::read_to_string(backup_path(&scratch.project(), 1)).expect("backup"),
        before
    );
    assert_ne!(before, original);
    edit(&mut harness);
    assert!(harness.state_mut().save());
    assert_eq!(
        fs::read_to_string(backup_path(&scratch.project(), 2)).expect("older"),
        before
    );
    assert!(harness.state().error().is_none());
}
