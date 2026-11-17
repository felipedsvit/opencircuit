//! The command line over the drawing: typing commands, the answers, the
//! history and the keys.

use circuit_editor::Tool;
use eframe::egui::{self, Key, Modifiers};
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

/// Type a line in the command line and send it.
fn command(harness: &mut Harness<'_, DesktopApp>, line: &str) {
    let field = harness.get_by_label("Command");
    field.focus();
    field.type_text(line);
    harness.run();
    harness.key_press(Key::Enter);
    harness.run();
}

fn devices(harness: &Harness<'_, DesktopApp>) -> Vec<String> {
    harness
        .state()
        .session()
        .expect("a project is open")
        .document()
        .devices
        .iter()
        .map(|device| device.reference.0.clone())
        .collect()
}

fn said(harness: &Harness<'_, DesktopApp>, text: &str) -> bool {
    harness
        .state()
        .command_log()
        .iter()
        .any(|line| line.contains(text))
}

#[test]
fn typing_a_tool_command_chooses_the_tool() {
    let mut harness = harness();

    command(&mut harness, "wire");
    assert_eq!(harness.state().tool(), &Tool::Wire);

    command(&mut harness, "select");
    assert_eq!(harness.state().tool(), &Tool::Select);
    assert!(said(&harness, "> wire"));
}

#[test]
fn place_with_a_point_places_the_part_and_undo_takes_it_back() {
    let mut harness = harness();

    command(&mut harness, "place lamp.generic 4,6");
    assert_eq!(devices(&harness), ["H1"]);
    assert!(said(&harness, "Place"));

    command(&mut harness, "u");
    assert!(devices(&harness).is_empty());
    command(&mut harness, "redo");
    assert_eq!(devices(&harness), ["H1"]);
}

#[test]
fn place_takes_a_standard_part_by_the_end_of_its_name() {
    let mut harness = harness();

    command(&mut harness, "p mcb_3p 0,0");

    assert_eq!(devices(&harness), ["Q1"]);
}

#[test]
fn a_command_that_the_editor_cannot_carry_out_says_why() {
    let mut harness = harness();

    command(&mut harness, "place nothing 1,1");
    command(&mut harness, "delete");
    command(&mut harness, "frobnicate");

    assert!(said(&harness, "error: no part `nothing`"));
    assert!(said(&harness, "error: nothing is selected"));
    assert!(said(&harness, "error: unknown command `frobnicate`"));
    assert!(devices(&harness).is_empty());
}

#[test]
fn help_and_parts_answer_in_the_log() {
    let mut harness = harness();

    command(&mut harness, "parts fuse");

    assert!(said(&harness, "fuse.generic"));
}

#[test]
fn a_command_that_edits_waits_for_the_drawing_mode() {
    let mut harness = harness();
    command(&mut harness, "mode simulate");
    assert_eq!(harness.state().mode(), Mode::Simulate);

    command(&mut harness, "place lamp.generic 1,1");

    assert!(said(&harness, "type `mode edit`"));
    assert!(devices(&harness).is_empty());
    command(&mut harness, "mode edit");
    assert_eq!(harness.state().mode(), Mode::Edit);
}

#[test]
fn the_up_arrow_brings_back_the_last_line() {
    let mut harness = harness();
    command(&mut harness, "zoom e");

    let field = harness.get_by_label("Command");
    field.focus();
    harness.run();
    harness.key_press(Key::ArrowUp);
    harness.run();

    assert_eq!(
        harness.get_by_label("Command").value().as_deref(),
        Some("zoom e")
    );
}

#[test]
fn tab_completes_a_command_and_ctrl_9_gives_the_keyboard_to_the_field() {
    let mut harness = harness();

    harness.key_press_modifiers(Modifiers::COMMAND, Key::Num9);
    harness.run();
    harness.run();
    harness.get_by_label("Command").type_text("zo");
    harness.run();
    harness.key_press(Key::Tab);
    harness.run();

    assert_eq!(
        harness.get_by_label("Command").value().as_deref(),
        Some("zoom ")
    );
}

#[test]
fn an_empty_line_repeats_the_last_command() {
    let mut harness = harness();
    command(&mut harness, "place lamp.generic 0,0");
    assert_eq!(devices(&harness).len(), 1);

    let field = harness.get_by_label("Command");
    field.focus();
    harness.run();
    harness.key_press(Key::Enter);
    harness.run();

    assert_eq!(devices(&harness).len(), 2);
}
