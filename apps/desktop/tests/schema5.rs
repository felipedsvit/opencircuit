//! M9.3 (#118), the documentation data of schema version 5 (ADR-0018) in
//! the window: the client, the data of a wire, shapes, comments, groups,
//! mirrored symbols, datasheets, sheet kinds with device views and PLC
//! modules. Everything lives in closed sections and commands, so a fresh
//! window is as before.

use eframe::egui::{self, Key};
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

#[allow(dead_code)]
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

/// Open a section inside the Project tree section.
fn open_in_project_tree(harness: &mut Harness<'_, DesktopApp>, section: &str) {
    for label in ["Project tree", section] {
        harness.get_by_label(label).scroll_to_me();
        harness.run();
        harness.get_by_label(label).click();
        harness.run();
    }
}

#[test]
fn the_client_is_typed_in_the_project_data_section() {
    let mut harness = harness();
    assert!(harness.query_by_label("Client").is_none());
    open_in_project_tree(&mut harness, "Project data");
    let field = harness.get_by_label("Client");
    field.focus();
    field.type_text("ACME");
    harness.run();
    harness.key_press(Key::Enter);
    harness.run();
    assert_eq!(document(&harness).project.client.as_deref(), Some("ACME"));
}

fn starter() -> Harness<'static, DesktopApp> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/direct-starter/starter.ocircuit");
    let mut harness = Harness::new_ui_state(
        |ui, app: &mut DesktopApp| app.show(ui),
        DesktopApp::new(Some(&path)),
    );
    harness.set_size(egui::Vec2::new(1300.0, 900.0));
    harness.run();
    harness.state_mut().set_mode(Mode::Edit);
    harness.run();
    harness
}

#[test]
fn the_data_of_a_wire_is_typed_in_its_closed_section() {
    let mut harness = starter();
    let wire = document(&harness).wires[0].id;
    harness
        .state_mut()
        .select(circuit_model::Subject::Wire(wire));
    harness.run();
    assert!(harness.query_by_label("Colour").is_none());
    harness.get_by_label("Wire data").scroll_to_me();
    harness.run();
    harness.get_by_label("Wire data").click();
    harness.run();
    for (label, text) in [("Colour", "BU"), ("Cross-section", "1.5 mm²")] {
        harness.get_by_label(label).scroll_to_me();
        harness.run();
        let field = harness.get_by_label(label);
        field.focus();
        field.type_text(text);
        harness.run();
        harness.key_press(Key::Enter);
        harness.run();
    }
    let spec = document(&harness).wires[0].spec.clone().expect("set");
    assert_eq!(spec.colour.as_deref(), Some("BU"));
    assert_eq!(spec.cross_section.as_deref(), Some("1.5 mm²"));
    assert_eq!(spec.kind, None);
}
