//! M9.2 (#118), the File menu: new from template, recent projects and a
//! saved copy.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use eframe::egui::{self, Key};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use opencircuit_desktop::DesktopApp;
use opencircuit_desktop::recent::RecentProjects;

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "opencircuit-desktop-files-m9-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("scratch");
        let dir = fs::canonicalize(dir).expect("canonical");
        let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
        for (from, to) in [
            ("direct-starter/starter.ocircuit", "starter.ocircuit"),
            ("direct-starter/command.ocircuit", "command.ocircuit"),
        ] {
            fs::copy(examples.join(from), dir.join(to)).expect("copy");
        }
        Self(dir)
    }

    fn recent(&self) -> RecentProjects {
        RecentProjects::at(self.0.join("data/recent.json"))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn harness(app: DesktopApp) -> Harness<'static, DesktopApp> {
    let mut harness = Harness::new_ui_state(|ui, app: &mut DesktopApp| app.show(ui), app);
    harness.set_size(egui::Vec2::new(1300.0, 900.0));
    harness.run();
    harness
}

/// Place a lamp through the command line: the project has unsaved changes.
fn change(harness: &mut Harness<'_, DesktopApp>) {
    for line in ["mode edit", "place lamp.generic 40,40"] {
        let field = harness.get_by_label("Command");
        field.focus();
        field.type_text(line);
        harness.run();
        harness.key_press(Key::Enter);
        harness.run();
    }
    assert!(harness.state().is_dirty());
}

#[test]
fn a_template_opens_as_a_new_project_without_a_file() {
    let mut harness = harness(DesktopApp::new(None));

    harness.get_by_label("File").click();
    harness.run();
    harness.get_by_label("New from template").click();
    harness.run();
    harness.get_by_label("Star-delta starter").click();
    harness.run();

    let app = harness.state();
    let session = app.session().expect("a project is open");
    assert_eq!(session.document().project.name, "Star-delta starter");
    assert_eq!(session.path(), None);
    assert!(app.error().is_none(), "{:?}", app.error());
}

#[test]
fn a_template_over_unsaved_changes_asks_first() {
    let scratch = Scratch::new();
    let mut harness = harness(DesktopApp::new(Some(&scratch.0.join("starter.ocircuit"))));
    change(&mut harness);

    harness.state_mut().new_from_template(0);
    harness.run();

    assert!(harness.state().is_asking());
}

#[test]
fn opened_and_saved_projects_are_listed_newest_first_and_open_again() {
    let scratch = Scratch::new();
    let mut app = DesktopApp::new(None);
    app.enable_recent_projects(scratch.recent());
    let mut harness = harness(app);

    harness
        .state_mut()
        .open_path(&scratch.0.join("starter.ocircuit"));
    harness
        .state_mut()
        .open_path(&scratch.0.join("command.ocircuit"));
    assert!(
        harness
            .state_mut()
            .save_to(scratch.0.join("saved.ocircuit"))
    );
    harness.run();
    assert_eq!(
        harness.state().recent_projects(),
        [
            scratch.0.join("saved.ocircuit"),
            scratch.0.join("command.ocircuit"),
            scratch.0.join("starter.ocircuit"),
        ]
    );

    harness.get_by_label("File").click();
    harness.run();
    harness.get_by_label("Open recent").click();
    harness.run();
    harness.get_by_label("starter.ocircuit").click();
    harness.run();

    let session = harness.state().session().expect("open");
    assert_eq!(
        session.path(),
        Some(scratch.0.join("starter.ocircuit").as_path())
    );
    assert_eq!(
        harness.state().recent_projects().first(),
        Some(&scratch.0.join("starter.ocircuit"))
    );
}

#[test]
fn a_recent_project_that_is_gone_is_taken_off_the_list() {
    let scratch = Scratch::new();
    let gone = scratch.0.join("command.ocircuit");
    scratch.recent().record(&gone).expect("list");
    fs::remove_file(&gone).expect("remove");
    let mut app = DesktopApp::new(Some(&scratch.0.join("starter.ocircuit")));
    app.enable_recent_projects(scratch.recent());
    let mut harness = harness(app);

    harness.get_by_label("File").click();
    harness.run();
    harness.get_by_label("Open recent").click();
    harness.run();
    harness.get_by_label("command.ocircuit").click();
    harness.run();

    assert!(harness.state().error().is_some());
    assert!(!harness.state().recent_projects().contains(&gone));
}

#[test]
fn a_copy_keeps_the_file_and_the_unsaved_changes_of_the_project() {
    let scratch = Scratch::new();
    let original = scratch.0.join("starter.ocircuit");
    let mut harness = harness(DesktopApp::new(Some(&original)));
    change(&mut harness);
    let copy = scratch.0.join("copy.ocircuit");

    assert!(harness.state_mut().save_copy_to(&copy));

    let app = harness.state();
    assert_eq!(
        app.session().expect("open").path(),
        Some(original.as_path())
    );
    assert!(app.is_dirty(), "the changes are still unsaved");
    let written = fs::read_to_string(&copy).expect("copy written");
    assert_eq!(
        circuit_project::from_str(&written)
            .expect("a project")
            .project
            .name,
        "Direct-online starter"
    );
}

#[test]
fn without_a_list_the_menu_has_no_recent_projects() {
    let mut harness = harness(DesktopApp::new(None));

    harness.get_by_label("File").click();
    harness.run();

    assert!(harness.query_by_label("Open recent").is_none());
    harness.get_by_label("New from template");
    harness.get_by_label("Save a copy…");
}
