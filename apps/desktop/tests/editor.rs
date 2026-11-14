//! Editing in the window, driven headless with pointer and key events.

use circuit_model::builder::pin;
use circuit_model::{GridPoint, ParamValue, Schematic, SchematicBuilder, is_orthogonal};
use eframe::egui::{self, Event, Modifiers, PointerButton, Pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use opencircuit_desktop::{DesktopApp, Mode};

const COMMAND: &str = include_str!("../../../examples/direct-starter/command.ocircuit");

fn project(build: impl FnOnce(&mut SchematicBuilder)) -> String {
    let mut builder = SchematicBuilder::new("editing");
    build(&mut builder);
    circuit_project::to_string(&builder.finish()).unwrap()
}

fn harness(text: &str) -> Harness<'static, DesktopApp> {
    let mut app = DesktopApp::default();
    app.open_text(text, "editing.ocircuit");
    let mut harness = Harness::new_ui_state(|ui, app: &mut DesktopApp| app.show(ui), app);
    harness.set_size(egui::Vec2::new(1200.0, 800.0));
    harness.run();
    harness
}

fn document(harness: &Harness<'_, DesktopApp>) -> Schematic {
    harness.state().session().unwrap().document().clone()
}

fn click_label(harness: &mut Harness<'_, DesktopApp>, label: &str) {
    harness.get_by_label(label).click();
    harness.run();
}

/// Press and release the primary button over a grid point of the canvas.
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

fn edit_mode(text: &str) -> Harness<'static, DesktopApp> {
    let mut harness = harness(text);
    click_label(&mut harness, "Edit");
    harness
}

#[test]
fn edit_mode_shows_the_palette_instead_of_the_operator() {
    let harness = edit_mode(COMMAND);

    assert_eq!(harness.state().mode(), Mode::Edit);
    assert!(harness.query_by_label("Wire").is_some());
    assert!(harness.query_by_label("Operator").is_none());
    assert!(harness.query_by_label("Play").is_none());
}

#[test]
fn symbol_chosen_in_the_palette_is_placed_where_the_canvas_is_clicked() {
    let mut harness = edit_mode(&project(|_| {}));

    click_label(&mut harness, "pushbutton_no · contact");
    click_grid(&mut harness, 10, 10);

    let doc = document(&harness);
    assert_eq!(doc.devices.len(), 1);
    assert_eq!(doc.devices[0].reference.0, "S1");
    assert_eq!(doc.symbols[0].position, GridPoint::new(10, 10));
}

#[test]
fn undo_and_redo_work_from_the_toolbar_and_the_keyboard() {
    let mut harness = edit_mode(&project(|_| {}));
    click_label(&mut harness, "pushbutton_no · contact");
    click_grid(&mut harness, 10, 10);
    assert_eq!(document(&harness).devices.len(), 1);

    click_label(&mut harness, "Undo");
    assert_eq!(document(&harness).devices.len(), 0);

    harness.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, egui::Key::Z);
    harness.run();
    assert_eq!(document(&harness).devices.len(), 1);

    harness.key_press_modifiers(Modifiers::COMMAND, egui::Key::Z);
    harness.run();
    assert_eq!(document(&harness).devices.len(), 0);
}

#[test]
fn two_pins_are_wired_with_the_wire_tool() {
    let mut harness = edit_mode(&project(|b| {
        let first = b.device("S1", "pushbutton_no", &[]).unwrap();
        b.symbol(first, "contact", None, GridPoint::new(0, 0))
            .unwrap();
        let second = b.device("S2", "pushbutton_no", &[]).unwrap();
        b.symbol(second, "contact", None, GridPoint::new(6, 8))
            .unwrap();
    }));

    click_label(&mut harness, "Wire");
    click_grid(&mut harness, 0, 2);
    click_grid(&mut harness, 6, 6);

    let doc = document(&harness);
    assert_eq!(doc.wires.len(), 1);
    let catalog = harness.state().session().unwrap().catalog();
    let path = circuit_model::LayoutIndex::new(&doc, catalog)
        .wire_polyline(&doc.wires[0])
        .unwrap();
    assert!(is_orthogonal(&path));
}

#[test]
fn inspector_refuses_an_invalid_designator_and_keeps_the_document() {
    let mut harness = edit_mode(&project(|b| {
        let device = b.device("S1", "pushbutton_no", &[]).unwrap();
        b.symbol(device, "contact", None, GridPoint::new(0, 0))
            .unwrap();
    }));
    click_grid(&mut harness, 0, 0);

    let field = harness.get_by_label("Reference");
    field.focus();
    field.type_text(" x");
    harness.run();
    harness.key_press(egui::Key::Enter);
    harness.run();

    assert!(
        harness
            .query_by_label_contains("not a valid reference designator")
            .is_some()
    );
    assert_eq!(document(&harness).devices[0].reference.0, "S1");
}

#[test]
fn parameter_typed_in_the_inspector_is_set_on_the_device() {
    let mut harness = edit_mode(&project(|b| {
        let device = b.device("K1", "contactor.generic", &[]).unwrap();
        b.symbol(device, "coil", None, GridPoint::new(0, 0))
            .unwrap();
    }));
    click_grid(&mut harness, 0, 0);

    let field = harness.get_by_label("rated_voltage_mv");
    field.focus();
    field.type_text("24000");
    harness.run();
    harness.key_press(egui::Key::Enter);
    harness.run();

    assert_eq!(
        document(&harness).devices[0].params.get("rated_voltage_mv"),
        Some(&ParamValue::Int(24_000))
    );
}

#[test]
fn delete_key_removes_the_selected_symbol_and_its_wires() {
    let mut harness = edit_mode(&project(|b| {
        let first = b.device("S1", "pushbutton_no", &[]).unwrap();
        let a = b
            .symbol(first, "contact", None, GridPoint::new(0, 0))
            .unwrap();
        let second = b.device("S2", "pushbutton_no", &[]).unwrap();
        let c = b
            .symbol(second, "contact", None, GridPoint::new(0, 8))
            .unwrap();
        b.wire(pin(a, "p2"), pin(c, "p1")).unwrap();
    }));
    click_grid(&mut harness, 0, 0);

    harness.key_press(egui::Key::Delete);
    harness.run();

    let doc = document(&harness);
    assert_eq!((doc.devices.len(), doc.wires.len()), (1, 0));
}

#[test]
fn editing_then_simulating_starts_from_tick_zero_on_the_edited_circuit() {
    let mut harness = harness(COMMAND);
    click_label(&mut harness, "Step");
    assert_eq!(harness.state().session().unwrap().next_tick(), 1);

    click_label(&mut harness, "Edit");
    click_label(&mut harness, "lamp.generic · lamp");
    click_grid(&mut harness, 12, 4);
    click_label(&mut harness, "Simulate");

    let session = harness.state().session().unwrap();
    assert_eq!(session.next_tick(), 0);
    assert_eq!(session.document().devices.len(), 6);
    assert!(session.history().can_undo());
}

#[test]
fn new_project_has_room_to_draw_left_of_and_above_the_origin() {
    let mut harness = edit_mode(&project(|_| {}));

    click_label(&mut harness, "pushbutton_no · contact");
    click_grid(&mut harness, -6, -6);

    assert_eq!(
        document(&harness).symbols[0].position,
        GridPoint::new(-6, -6)
    );
}
