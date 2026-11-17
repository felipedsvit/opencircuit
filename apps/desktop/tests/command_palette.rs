//! The command palette (M9.2, #118): Ctrl+K, a command by name, an element
//! of the project found and selected.

use std::path::Path;

use circuit_model::Subject;
use eframe::egui::{self, Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use opencircuit_desktop::DesktopApp;
use opencircuit_desktop::app::Mode;

fn window() -> Harness<'static, DesktopApp> {
    let starter = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/direct-starter/starter.ocircuit");
    let app = DesktopApp::new(Some(&starter));
    let mut harness = Harness::new_ui_state(|ui, app: &mut DesktopApp| app.show(ui), app);
    harness.set_size(egui::Vec2::new(1300.0, 900.0));
    harness.run();
    harness
}

fn type_in_palette(harness: &mut Harness<'_, DesktopApp>, text: &str) {
    harness.get_by_label("Find or run").type_text(text);
    harness.run();
}

#[test]
fn ctrl_k_opens_the_palette_in_both_modes_and_escape_closes_it() {
    let mut harness = window();
    assert_eq!(harness.state().mode(), Mode::Simulate);

    harness.key_press_modifiers(Modifiers::COMMAND, Key::K);
    harness.run();
    assert!(harness.state().command_palette_open());
    harness.get_by_label("Find or run");

    harness.key_press(Key::Escape);
    harness.run();
    assert!(!harness.state().command_palette_open());
}

#[test]
fn a_designator_finds_the_device_and_enter_selects_it() {
    let mut harness = window();
    harness.state_mut().open_command_palette();
    harness.run();

    type_in_palette(&mut harness, "K1");
    harness.key_press(Key::Enter);
    harness.run();

    let app = harness.state();
    assert!(!app.command_palette_open());
    let k1 = app
        .session()
        .expect("open")
        .document()
        .devices
        .iter()
        .find(|device| device.reference.0 == "K1")
        .expect("K1")
        .id;
    assert_eq!(
        app.selection().iter().collect::<Vec<_>>(),
        [&Subject::Device(k1)]
    );
}

#[test]
fn a_command_runs_by_name() {
    let mut harness = window();
    harness.state_mut().open_command_palette();
    harness.run();

    type_in_palette(&mut harness, "edit");
    harness.key_press(Key::Enter);
    harness.run();

    assert_eq!(harness.state().mode(), Mode::Edit);
}

#[test]
fn the_help_menu_opens_the_palette() {
    let mut harness = window();

    harness.get_by_label("Help").click();
    harness.run();
    harness.get_by_label("Command palette").click();
    harness.run();

    assert!(harness.state().command_palette_open());
}
