//! M9.2 (#118): a macro records the edits of the window as an edit file and
//! plays them back in one undoable step.

use eframe::egui::{self, Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use opencircuit_desktop::DesktopApp;

fn new_project() -> Harness<'static, DesktopApp> {
    let mut harness = Harness::new_ui_state(
        |ui, app: &mut DesktopApp| app.show(ui),
        DesktopApp::default(),
    );
    harness.set_size(egui::Vec2::new(1300.0, 900.0));
    harness.run();
    harness.get_by_label("New").click();
    harness.run();
    harness
}

fn command(harness: &mut Harness<'_, DesktopApp>, line: &str) {
    let field = harness.get_by_label("Command");
    field.focus();
    field.type_text(line);
    harness.run();
    harness.key_press(Key::Enter);
    harness.run();
}

fn designators(harness: &Harness<'_, DesktopApp>) -> Vec<String> {
    let mut names: Vec<String> = harness
        .state()
        .session()
        .expect("open")
        .document()
        .devices
        .iter()
        .map(|device| device.reference.0.clone())
        .collect();
    names.sort();
    names
}

#[test]
fn the_modify_menu_records_and_the_status_bar_counts_the_steps() {
    let mut harness = new_project();

    harness.get_by_label("Modify").click();
    harness.run();
    harness.get_by_label("Record macro").click();
    harness.run();
    assert_eq!(harness.state().macro_steps(), Some((0, 0)));

    command(&mut harness, "place lamp.generic 0,0");
    command(&mut harness, "place lamp.generic 10,0");
    assert_eq!(harness.state().macro_steps(), Some((2, 0)));
    harness.get_by_label("Recording macro: 2 steps");

    harness.get_by_label("Modify").click();
    harness.run();
    harness.get_by_label("Stop recording and save the macro…");
}

#[test]
fn a_recorded_macro_plays_back_on_a_new_project_in_one_undo() {
    let mut harness = new_project();
    harness.state_mut().start_macro();
    command(&mut harness, "place lamp.generic 0,0");
    command(&mut harness, "place lamp.generic 10,0");
    command(&mut harness, "undo");
    command(&mut harness, "place lamp.generic 20,0");
    let file = harness.state_mut().finish_macro().expect("recorded");
    assert_eq!(harness.state().macro_steps(), None);
    assert_eq!(designators(&harness), ["H1", "H2"]);

    let mut other = new_project();
    command(&mut other, "mode edit");
    other.state_mut().play_macro_text(&file);
    other.run();
    assert_eq!(other.state().error(), None);
    assert_eq!(designators(&other), ["H1", "H2"]);
    let positions: Vec<i32> = {
        let doc = other.state().session().expect("open").document();
        let mut x: Vec<i32> = doc.symbols.iter().map(|s| s.position.x).collect();
        x.sort_unstable();
        x
    };
    assert_eq!(positions, [0, 20]);

    other.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    other.run();
    assert!(designators(&other).is_empty(), "one undo takes it all back");
}

#[test]
fn a_file_that_is_not_a_macro_is_said_and_changes_nothing() {
    let mut harness = new_project();

    harness.state_mut().play_macro_text("not json");

    assert!(harness.state().error().is_some());
    assert!(designators(&harness).is_empty());
}
