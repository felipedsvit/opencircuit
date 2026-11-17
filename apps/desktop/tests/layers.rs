//! Layers in the window (ADR-0015): the `layer` command, the Layers section
//! of the inspector, hidden elements that cannot be picked and locked ones
//! that refuse a change.

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

fn document(harness: &Harness<'_, DesktopApp>) -> circuit_model::Schematic {
    harness
        .state()
        .session()
        .expect("a project is open")
        .document()
        .clone()
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

fn selection(harness: &Harness<'_, DesktopApp>) -> Vec<Subject> {
    harness.state().selection().iter().cloned().collect()
}

/// Open the Layers section, which is inside the Project tree section.
fn open_layers(harness: &mut Harness<'_, DesktopApp>) {
    for label in ["Project tree", "Layers"] {
        harness.get_by_label(label).scroll_to_me();
        harness.run();
        harness.get_by_label(label).click();
        harness.run();
    }
}

/// A project with a note and a layer `Power` that holds it.
fn with_note_on_a_layer() -> Harness<'static, DesktopApp> {
    let mut harness = harness();
    command(&mut harness, "text 10,10 Secret");
    command(&mut harness, "layer new Power");
    command(&mut harness, "select");
    click_grid(&mut harness, 11, 10);
    assert!(matches!(selection(&harness).as_slice(), [Subject::Text(_)]));
    command(&mut harness, "layer assign Power");
    harness
}

#[test]
fn the_layer_command_makes_a_layer_lists_it_and_undo_takes_it_away() {
    let mut harness = harness();

    command(&mut harness, "layer new Power");
    assert_eq!(document(&harness).layers[0].name, "Power");
    command(&mut harness, "layer");
    assert!(
        harness
            .state()
            .command_log()
            .iter()
            .any(|line| line.contains("Power")),
        "{:?}",
        harness.state().command_log()
    );
    command(&mut harness, "undo");
    assert!(document(&harness).layers.is_empty());
}

#[test]
fn the_layers_section_hides_and_locks_a_layer_and_each_switch_is_one_undo() {
    let mut harness = harness();
    command(&mut harness, "layer new Power");

    open_layers(&mut harness);
    harness.get_by_label("Show Power").click();
    harness.run();
    assert!(!document(&harness).layers[0].visible);
    harness.get_by_label("Lock Power").click();
    harness.run();
    assert!(document(&harness).layers[0].locked);

    command(&mut harness, "undo");
    assert!(!document(&harness).layers[0].locked);
    command(&mut harness, "undo");
    assert!(document(&harness).layers[0].visible);
}

#[test]
fn a_note_on_a_hidden_layer_cannot_be_clicked_and_comes_back_when_the_layer_is_shown() {
    let mut harness = with_note_on_a_layer();
    command(&mut harness, "layer hide Power");
    // A click on empty canvas drops the selection.
    click_grid(&mut harness, 40, 30);

    click_grid(&mut harness, 11, 10);
    assert!(selection(&harness).is_empty(), "{:?}", selection(&harness));

    command(&mut harness, "layer show Power");
    click_grid(&mut harness, 11, 10);
    assert!(matches!(selection(&harness).as_slice(), [Subject::Text(_)]));
}

#[test]
fn a_locked_note_refuses_a_move_and_the_reason_is_in_the_command_log() {
    let mut harness = with_note_on_a_layer();
    command(&mut harness, "layer lock Power");
    let before = document(&harness).texts[0].position;

    command(&mut harness, "move 2,0");

    assert_eq!(document(&harness).texts[0].position, before);
    assert!(
        harness
            .state()
            .command_log()
            .iter()
            .any(|line| line.contains("locked")),
        "{:?}",
        harness.state().command_log()
    );
    command(&mut harness, "layer unlock Power");
    command(&mut harness, "move 2,0");
    assert_ne!(document(&harness).texts[0].position, before);
}

#[test]
fn putting_the_selection_on_a_layer_from_the_inspector_and_back() {
    let mut harness = harness();
    command(&mut harness, "text 10,10 Note");
    command(&mut harness, "layer new Power");
    command(&mut harness, "select");
    click_grid(&mut harness, 11, 10);

    open_layers(&mut harness);
    harness
        .get_by_label("Put selection on Power")
        .scroll_to_me();
    harness.run();
    harness.get_by_label("Put selection on Power").click();
    harness.run();
    assert_eq!(
        document(&harness).texts[0].layer,
        Some(document(&harness).layers[0].id)
    );

    harness
        .get_by_label("Put selection on the default layer")
        .click();
    harness.run();
    assert_eq!(document(&harness).texts[0].layer, None);
}
