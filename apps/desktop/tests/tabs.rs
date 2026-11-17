//! M9.2 (#118), several projects open in tabs: each tab keeps its project,
//! its edits and its autosave; closing a tab or the window asks about the
//! unsaved changes of every tab. A window with one project has no tab bar
//! beyond the one it always had.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use eframe::egui::{self, Key};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use opencircuit_desktop::DesktopApp;
use opencircuit_desktop::recovery::RecoveryStore;

/// A scratch directory with two projects, removed when dropped.
struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "opencircuit-desktop-tabs-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("scratch");
        let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
        fs::copy(
            examples.join("direct-starter/starter.ocircuit"),
            dir.join("starter.ocircuit"),
        )
        .expect("copy");
        fs::copy(
            examples.join("forward-reverse/reversing.ocircuit"),
            dir.join("reversing.ocircuit"),
        )
        .expect("copy");
        Self(dir)
    }

    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }

    /// The copies in the recovery folder, without locks and descriptions.
    fn copies(&self) -> usize {
        fs::read_dir(self.0.join("recovery"))
            .map(|listing| {
                listing
                    .filter_map(Result::ok)
                    .filter(|entry| entry.file_name().to_string_lossy().ends_with(".ocircuit"))
                    .count()
            })
            .unwrap_or_default()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn window(app: DesktopApp) -> Harness<'static, DesktopApp> {
    let mut harness = Harness::new_ui_state(|ui, app: &mut DesktopApp| app.show(ui), app);
    harness.set_size(egui::Vec2::new(1300.0, 900.0));
    harness.run();
    harness
}

fn name(harness: &Harness<'_, DesktopApp>) -> String {
    harness
        .state()
        .session()
        .expect("a project is open")
        .document()
        .project
        .name
        .clone()
}

/// Change the shown project through the command line.
fn edit(harness: &mut Harness<'_, DesktopApp>) {
    harness
        .state_mut()
        .set_mode(opencircuit_desktop::Mode::Edit);
    harness.run();
    let field = harness.get_by_label("Command");
    field.focus();
    field.type_text("place lamp.generic 40,40");
    harness.run();
    harness.key_press(Key::Enter);
    harness.run();
    assert!(harness.state().is_dirty());
}

#[test]
fn one_project_has_no_tab_buttons() {
    let scratch = Scratch::new();
    let harness = window(DesktopApp::new(Some(&scratch.path("starter.ocircuit"))));
    assert_eq!(harness.state().tab_count(), 1);
    assert!(harness.query_by_label_contains("Close ").is_none());
}

#[test]
fn a_project_opened_in_a_new_tab_leaves_the_first_open() {
    let scratch = Scratch::new();
    let mut harness = window(DesktopApp::new(Some(&scratch.path("starter.ocircuit"))));
    let first = name(&harness);
    harness
        .state_mut()
        .open_in_new_tab(&scratch.path("reversing.ocircuit"));
    harness.run();
    let second = name(&harness);
    assert_ne!(first, second);
    assert_eq!(harness.state().tab_count(), 2);
    assert_eq!(harness.state().active_tab(), 1);
    assert_eq!(harness.state().tab_names(), [first.clone(), second.clone()]);

    // A click on the other tab shows its project.
    harness.get_by_label(&first).click();
    harness.run();
    assert_eq!(name(&harness), first);
    assert_eq!(harness.state().active_tab(), 0);

    // Opening a project that is open already shows its tab.
    harness
        .state_mut()
        .open_in_new_tab(&scratch.path("reversing.ocircuit"));
    harness.run();
    assert_eq!(harness.state().tab_count(), 2);
    assert_eq!(name(&harness), second);
}

#[test]
fn each_tab_keeps_its_own_edits_and_undo() {
    let scratch = Scratch::new();
    let mut harness = window(DesktopApp::new(Some(&scratch.path("starter.ocircuit"))));
    harness.state_mut().new_tab();
    harness.run();
    assert_eq!(harness.state().tab_count(), 2);
    edit(&mut harness);
    let placed = harness
        .state()
        .session()
        .expect("open")
        .document()
        .devices
        .len();
    assert_eq!(placed, 1);

    harness.state_mut().switch_tab(0);
    harness.run();
    assert!(!harness.state().is_dirty());
    assert!(harness.state().any_dirty());
    assert!(harness.query_by_label_contains("(unsaved)").is_some());

    harness.state_mut().switch_tab(1);
    harness.run();
    assert!(harness.state().is_dirty());
}

#[test]
fn closing_a_tab_with_changes_asks_and_shows_the_next() {
    let scratch = Scratch::new();
    let mut harness = window(DesktopApp::new(Some(&scratch.path("starter.ocircuit"))));
    let first = name(&harness);
    harness.state_mut().new_tab();
    harness.run();
    edit(&mut harness);

    harness.state_mut().close_tab();
    harness.run();
    assert!(harness.state().is_asking());
    harness.get_by_label("Cancel").click();
    harness.run();
    assert_eq!(harness.state().tab_count(), 2);

    harness.state_mut().close_tab();
    harness.run();
    harness.get_by_label("Discard changes").click();
    harness.run();
    assert_eq!(harness.state().tab_count(), 1);
    assert_eq!(name(&harness), first);

    // The last tab closes to an empty window.
    harness.state_mut().close_tab();
    harness.run();
    assert_eq!(harness.state().tab_count(), 0);
    assert!(harness.state().session().is_none());
}

#[test]
fn closing_the_window_asks_about_every_tab_with_changes() {
    let scratch = Scratch::new();
    let mut harness = window(DesktopApp::new(Some(&scratch.path("starter.ocircuit"))));
    edit(&mut harness);
    harness
        .state_mut()
        .open_in_new_tab(&scratch.path("reversing.ocircuit"));
    harness.run();
    edit(&mut harness);
    harness.state_mut().new_tab();
    harness.run();
    assert_eq!(harness.state().tab_count(), 3);
    assert!(!harness.state().is_dirty());

    // The tab shown has no changes: a tab that has them is shown and asked
    // about.
    assert!(!harness.state_mut().request_close());
    harness.run();
    assert!(harness.state().is_asking());
    assert!(harness.state().is_dirty());
    harness.get_by_label("Discard changes").click();
    harness.run();

    // The other one is asked about next.
    assert!(harness.state().is_asking());
    assert!(harness.state().is_dirty());
    harness.get_by_label("Discard changes").click();
    harness.run();
    // Both answered: the window closes.
    assert!(!harness.state().is_asking());
    assert!(harness.state_mut().request_close());
}

#[test]
fn every_tab_with_changes_has_its_own_recovery_copy() {
    let scratch = Scratch::new();
    let mut app = DesktopApp::new(Some(&scratch.path("starter.ocircuit")));
    app.enable_recovery(RecoveryStore::at(scratch.path("recovery")), 0.0);
    let mut harness = window(app);
    edit(&mut harness);
    harness
        .state_mut()
        .open_in_new_tab(&scratch.path("reversing.ocircuit"));
    harness.run();
    edit(&mut harness);
    harness.run();
    harness.run();
    assert_eq!(scratch.copies(), 2);

    // A tab closed with its changes dropped takes its copy with it.
    harness.state_mut().close_tab();
    harness.run();
    harness.get_by_label("Discard changes").click();
    harness.run();
    harness.run();
    assert_eq!(scratch.copies(), 1);
}
