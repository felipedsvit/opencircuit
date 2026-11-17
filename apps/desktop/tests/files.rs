//! New, save, reopen and the question about unsaved changes. These flows
//! never open a native file dialog: they use the path field.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use circuit_model::GridPoint;
use eframe::egui::{self, Event, Modifiers, PointerButton, Pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use opencircuit_desktop::{DesktopApp, Mode};

const COMMAND: &str = include_str!("../../../examples/direct-starter/command.ocircuit");

/// A path in the temporary directory, removed when dropped.
struct TempPath(PathBuf);

impl TempPath {
    fn new() -> Self {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        Self(std::env::temp_dir().join(format!(
            "opencircuit-files-{}-{}.ocircuit",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        )))
    }

    fn with_command() -> Self {
        let path = Self::new();
        std::fs::write(&path.0, COMMAND).unwrap();
        path
    }
}

impl Drop for TempPath {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn harness(app: DesktopApp) -> Harness<'static, DesktopApp> {
    let mut harness = Harness::new_ui_state(|ui, app: &mut DesktopApp| app.show(ui), app);
    harness.set_size(egui::Vec2::new(1300.0, 800.0));
    harness.run();
    harness
}

fn opened(path: &TempPath) -> Harness<'static, DesktopApp> {
    harness(DesktopApp::new(Some(&path.0)))
}

/// Click the widget named `label`, scrolling it into view first: the
/// palette lists more parts than fit the window.
fn click_label(harness: &mut Harness<'_, DesktopApp>, label: &str) {
    harness.get_by_label(label).scroll_to_me();
    harness.run();
    harness.get_by_label(label).click();
    harness.run();
}

fn click_grid(harness: &mut Harness<'_, DesktopApp>, x: i32, y: i32) {
    let [sx, sy] = harness
        .state()
        .grid_to_screen(GridPoint::new(x, y))
        .unwrap();
    let pos = Pos2::new(sx, sy);
    harness.hover_at(pos);
    harness.run();
    for pressed in [true, false] {
        harness.event(Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
        harness.run();
    }
}

/// Place a lamp in edit mode: an unsaved change.
fn add_lamp(harness: &mut Harness<'_, DesktopApp>) {
    if harness.state().mode() != Mode::Edit {
        click_label(harness, "Edit");
    }
    click_label(harness, "lamp.generic · lamp");
    click_grid(harness, 12, 4);
    assert!(harness.state().is_dirty());
}

fn devices(harness: &Harness<'_, DesktopApp>) -> usize {
    harness.state().session().unwrap().document().devices.len()
}

fn type_path(harness: &mut Harness<'_, DesktopApp>, path: &std::path::Path) {
    let field = harness.get_by_label("Path");
    field.focus();
    field.type_text(&path.display().to_string());
    harness.run();
}

#[test]
fn saved_project_is_canonical_and_reopens_identical() {
    let file = TempPath::with_command();
    let mut harness = opened(&file);
    add_lamp(&mut harness);

    click_label(&mut harness, "Save");

    let session = harness.state().session().unwrap();
    let on_disk = std::fs::read_to_string(&file.0).unwrap();
    assert_eq!(
        on_disk,
        circuit_project::to_string(session.document()).unwrap()
    );
    assert!(!harness.state().is_dirty());
    let saved = session.document().clone();

    harness.state_mut().request_open(file.0.clone());
    harness.run();
    assert_eq!(harness.state().session().unwrap().document(), &saved);
}

#[test]
fn unsaved_changes_are_marked_until_saved() {
    let file = TempPath::with_command();
    let mut harness = opened(&file);
    add_lamp(&mut harness);
    assert!(harness.query_by_label_contains("(unsaved)").is_some());

    click_label(&mut harness, "Save");

    assert!(harness.query_by_label_contains("(unsaved)").is_none());
}

#[test]
fn new_project_with_unsaved_changes_asks_and_cancel_keeps_everything() {
    let file = TempPath::with_command();
    let mut harness = opened(&file);
    add_lamp(&mut harness);
    let count = devices(&harness);

    click_label(&mut harness, "New");
    assert!(harness.query_by_label("Unsaved changes").is_some());
    click_label(&mut harness, "Cancel");

    assert!(!harness.state().is_asking());
    assert_eq!(devices(&harness), count);
    assert!(harness.state().is_dirty());
}

#[test]
fn discarding_unsaved_changes_starts_a_new_project_in_edit_mode() {
    let file = TempPath::with_command();
    let mut harness = opened(&file);
    add_lamp(&mut harness);

    click_label(&mut harness, "New");
    click_label(&mut harness, "Discard changes");

    let session = harness.state().session().unwrap();
    assert_eq!(session.document().project.name, "Untitled");
    assert_eq!(session.document().devices.len(), 0);
    assert_eq!(harness.state().mode(), Mode::Edit);
    assert_eq!(std::fs::read_to_string(&file.0).unwrap(), COMMAND);
}

#[test]
fn saving_from_the_question_writes_the_file_then_continues() {
    let file = TempPath::with_command();
    let mut harness = opened(&file);
    add_lamp(&mut harness);
    let edited = harness.state().session().unwrap().document().clone();

    click_label(&mut harness, "New");
    click_label(&mut harness, "Save and continue");

    let saved = circuit_project::from_str(&std::fs::read_to_string(&file.0).unwrap()).unwrap();
    assert_eq!(saved, edited);
    assert_eq!(devices(&harness), 0);
}

#[test]
fn closing_with_unsaved_changes_asks_and_discard_allows_it() {
    let file = TempPath::with_command();
    let mut harness = opened(&file);
    add_lamp(&mut harness);

    assert!(!harness.state_mut().request_close());
    harness.run();
    assert!(harness.query_by_label("Unsaved changes").is_some());
    click_label(&mut harness, "Discard changes");

    assert!(harness.state_mut().request_close());
}

#[test]
fn closing_without_changes_does_not_ask() {
    let file = TempPath::with_command();
    let mut harness = opened(&file);

    assert!(harness.state_mut().request_close());
    assert!(!harness.state().is_asking());
}

#[test]
fn new_project_is_saved_where_the_path_field_says() {
    let file = TempPath::new();
    let mut harness = harness(DesktopApp::default());
    click_label(&mut harness, "New");
    add_lamp(&mut harness);

    type_path(&mut harness, &file.0);
    click_label(&mut harness, "Save here");

    let text = std::fs::read_to_string(&file.0).unwrap();
    let saved = circuit_project::from_str(&text).unwrap();
    assert_eq!(saved.devices.len(), 1);
    assert_eq!(
        harness.state().session().unwrap().path(),
        Some(file.0.as_path())
    );
    assert!(!harness.state().is_dirty());
}

#[test]
fn failed_save_shows_the_error_and_keeps_the_changes_unsaved() {
    let missing = std::env::temp_dir()
        .join(format!("opencircuit-no-dir-{}", std::process::id()))
        .join("x.ocircuit");
    let mut harness = harness(DesktopApp::default());
    click_label(&mut harness, "New");
    add_lamp(&mut harness);

    type_path(&mut harness, &missing);
    click_label(&mut harness, "Save here");

    assert!(harness.state().error().is_some());
    assert!(harness.state().is_dirty());
}
