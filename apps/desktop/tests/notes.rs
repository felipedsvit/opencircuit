//! Text notes in the window (ADR-0015): the Modify menu, the command line and
//! the inspector. Each test drives the window through its accessibility tree.

use circuit_editor::Tool;
use circuit_model::{GridPoint, Subject};
use eframe::egui::{self, Event, Key, Modifiers, PointerButton, Pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use opencircuit_desktop::{DesktopApp, Mode};

fn harness() -> Harness<'static, DesktopApp> {
    let mut harness = Harness::new_ui_state(
        |ui, app: &mut DesktopApp| app.show(ui),
        DesktopApp::default(),
    );
    harness.set_size(egui::Vec2::new(1300.0, 900.0));
    harness.run();
    harness.get_by_label("New").click();
    harness.run();
    assert_eq!(harness.state().mode(), Mode::Edit);
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

fn texts(harness: &Harness<'_, DesktopApp>) -> Vec<(GridPoint, String)> {
    harness
        .state()
        .session()
        .expect("a project is open")
        .document()
        .texts
        .iter()
        .map(|note| (note.position, note.text.clone()))
        .collect()
}

fn click_grid(harness: &mut Harness<'_, DesktopApp>, x: i32, y: i32) {
    let [sx, sy] = harness
        .state()
        .grid_to_screen(GridPoint::new(x, y))
        .expect("the canvas has been drawn");
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

#[test]
fn the_text_command_adds_a_note_and_one_undo_takes_it_away() {
    let mut harness = harness();

    command(&mut harness, "text 6,4 Stop: normally closed");

    assert_eq!(
        texts(&harness),
        [(GridPoint::new(6, 4), "Stop: normally closed".to_owned())]
    );
    command(&mut harness, "undo");
    assert!(texts(&harness).is_empty());
}

#[test]
fn text_alone_chooses_the_tool_and_a_click_adds_a_note_to_edit_in_the_inspector() {
    let mut harness = harness();

    command(&mut harness, "text");
    assert_eq!(harness.state().tool(), &Tool::Text);
    click_grid(&mut harness, 10, 10);
    let notes = texts(&harness);
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].0, GridPoint::new(10, 10));

    // The new note is selected: the inspector offers its words.
    assert!(matches!(
        harness.state().selection().iter().next(),
        Some(Subject::Text(_))
    ));
    harness.get_by_label("Words").scroll_to_me();
    harness.run();
    harness.get_by_label("Words").click();
    harness.run();
    harness.get_by_label("Words").type_text("Panel 1");
    harness.run();
    harness.key_press(Key::Enter);
    harness.run();

    assert!(
        texts(&harness)
            .iter()
            .any(|(_, text)| text.contains("Panel 1")),
        "{:?}",
        texts(&harness)
    );
}

#[test]
fn add_text_is_in_the_modify_menu_and_chooses_the_tool() {
    let mut harness = harness();

    harness.get_by_label("Modify").click();
    harness.run();
    harness.get_by_label("Add text").click();
    harness.run();

    assert_eq!(harness.state().tool(), &Tool::Text);
}

#[test]
fn a_refused_text_says_why_and_changes_nothing() {
    let mut harness = harness();

    command(&mut harness, &format!("text 0,0 {}", "x".repeat(121)));

    assert!(texts(&harness).is_empty());
    assert!(
        harness
            .state()
            .command_log()
            .iter()
            .any(|line| line.contains("characters")),
        "{:?}",
        harness.state().command_log()
    );
}
