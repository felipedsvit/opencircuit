//! M9.2 (#118): find and replace, the catalog data of several devices, and
//! a label typed over the drawing (F2).

use circuit_model::GridPoint;
use eframe::egui::{self, Event, Key, Modifiers, PointerButton, Pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use opencircuit_desktop::DesktopApp;

/// A new project with two lamps, H1 at (0, 0) and H2 at (10, 0).
fn two_lamps() -> Harness<'static, DesktopApp> {
    let mut harness = Harness::new_ui_state(
        |ui, app: &mut DesktopApp| app.show(ui),
        DesktopApp::default(),
    );
    harness.set_size(egui::Vec2::new(1300.0, 900.0));
    harness.run();
    harness.get_by_label("New").click();
    harness.run();
    click(&mut harness, "lamp.generic · lamp");
    click_grid(&mut harness, 0, 0);
    click_grid(&mut harness, 10, 0);
    harness.key_press(Key::Escape);
    harness.run();
    harness
}

fn click(harness: &mut Harness<'_, DesktopApp>, label: &str) {
    harness.get_by_label(label).scroll_to_me();
    harness.run();
    harness.get_by_label(label).click();
    harness.run();
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

/// Type into the field with this label.
fn type_into(harness: &mut Harness<'_, DesktopApp>, label: &str, text: &str) {
    harness.get_by_label(label).focus();
    harness.run();
    harness.get_by_label(label).type_text(text);
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
fn ctrl_h_replaces_designators_in_one_undoable_step() {
    let mut harness = two_lamps();
    assert_eq!(designators(&harness), ["H1", "H2"]);

    harness.key_press_modifiers(Modifiers::COMMAND, Key::H);
    harness.run();
    assert!(harness.state().find_replace_open());
    type_into(&mut harness, "Find", "H");
    type_into(&mut harness, "Replace with", "P");
    assert!(harness.query_by_label("2 changes").is_some());
    click(&mut harness, "Replace all");

    assert_eq!(designators(&harness), ["P1", "P2"]);
    harness.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    harness.run();
    assert_eq!(designators(&harness), ["H1", "H2"]);
}

#[test]
fn a_replacement_that_would_repeat_a_designator_is_refused() {
    let mut harness = two_lamps();
    harness.state_mut().open_find_replace();
    harness.run();

    type_into(&mut harness, "Find", "2");
    type_into(&mut harness, "Replace with", "1");

    assert!(harness.query_by_label("2 changes").is_none());
    click(&mut harness, "Replace all");
    assert_eq!(designators(&harness), ["H1", "H2"]);
}

#[test]
fn f2_types_a_new_designator_over_the_symbol() {
    let mut harness = two_lamps();
    click_grid(&mut harness, 0, 0);

    harness.key_press(Key::F2);
    harness.run();
    assert!(harness.state().label_editing());
    // The field holds "H1", with the cursor at its end.
    for _ in 0..2 {
        harness.key_press(Key::Backspace);
        harness.run();
    }
    harness.get_by_label("Label").type_text("H9");
    harness.run();
    harness.key_press(Key::Enter);
    harness.run();

    assert!(!harness.state().label_editing());
    assert_eq!(designators(&harness), ["H2", "H9"]);
}

#[test]
fn escape_drops_the_label_being_typed() {
    let mut harness = two_lamps();
    click_grid(&mut harness, 0, 0);
    harness.state_mut().edit_label();
    harness.run();

    harness.get_by_label("Label").type_text("X");
    harness.run();
    harness.key_press(Key::Escape);
    harness.run();

    assert!(!harness.state().label_editing());
    assert_eq!(designators(&harness), ["H1", "H2"]);
}

#[test]
fn the_catalog_data_of_several_devices_is_set_at_once() {
    let mut harness = two_lamps();
    harness.key_press_modifiers(Modifiers::COMMAND, Key::A);
    harness.run();

    click(&mut harness, "Catalog data of 2 devices");
    type_into(&mut harness, "Manufacturer", "Acme");
    harness.key_press(Key::Enter);
    harness.run();

    let doc = harness.state().session().expect("open").document();
    assert!(doc.devices.iter().all(|device| {
        device
            .info
            .as_ref()
            .and_then(|info| info.manufacturer.as_deref())
            == Some("Acme")
    }));
}
