//! A project open in two windows (M9.1, #118): the second window says so in
//! the status bar until the first one lets the project go.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use opencircuit_desktop::DesktopApp;
use opencircuit_desktop::project_lock::ProjectLocks;

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "opencircuit-desktop-locks-{}-{}",
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
            examples.join("direct-starter/command.ocircuit"),
            dir.join("command.ocircuit"),
        )
        .expect("copy");
        Self(dir)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// A window on `project`, with locks in the scratch directory, trying a
/// held lock again at every frame.
fn window(scratch: &Scratch, project: &str) -> Harness<'static, DesktopApp> {
    let mut app = DesktopApp::new(Some(&scratch.0.join(project)));
    app.enable_project_locks(ProjectLocks::at(scratch.0.join("locks")), 0.0);
    let mut harness = Harness::new_ui_state(|ui, app: &mut DesktopApp| app.show(ui), app);
    harness.set_size(egui::Vec2::new(1300.0, 900.0));
    harness.run();
    harness
}

#[test]
fn the_second_window_on_a_project_is_warned_until_the_first_closes() {
    let scratch = Scratch::new();
    let first = window(&scratch, "starter.ocircuit");
    assert!(!first.state().open_elsewhere());

    let mut second = window(&scratch, "starter.ocircuit");
    assert!(second.state().open_elsewhere());
    second.get_by_label("Open in another window");

    drop(first);
    second.run();
    assert!(!second.state().open_elsewhere());
    assert!(second.query_by_label("Open in another window").is_none());
}

#[test]
fn another_project_is_not_a_conflict_and_opening_it_releases_the_first() {
    let scratch = Scratch::new();
    let mut first = window(&scratch, "starter.ocircuit");
    let other = window(&scratch, "command.ocircuit");
    assert!(!other.state().open_elsewhere());

    // The first window moves to another project: the starter is free.
    first
        .state_mut()
        .open_path(&scratch.0.join("command.ocircuit"));
    first.run();
    assert!(first.state().open_elsewhere(), "the other window has it");
    let third = window(&scratch, "starter.ocircuit");
    assert!(!third.state().open_elsewhere());
}

#[test]
fn a_window_without_locks_never_warns() {
    let scratch = Scratch::new();
    let _held = window(&scratch, "starter.ocircuit");
    let app = DesktopApp::new(Some(&scratch.0.join("starter.ocircuit")));
    let mut harness = Harness::new_ui_state(|ui, app: &mut DesktopApp| app.show(ui), app);
    harness.run();

    assert!(!harness.state().open_elsewhere());
}
