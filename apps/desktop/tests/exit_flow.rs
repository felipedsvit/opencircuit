//! The exit criterion of M2b, through the window only: draw a direct-online
//! starter from nothing, save it, reopen it and simulate it, with undo and
//! redo along the way and no partially applied operation.

use std::path::PathBuf;

use circuit_model::{DiagnosticCode, GridPoint, Schematic};
use eframe::egui::{self, Event, Modifiers, PointerButton, Pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use opencircuit_desktop::{DesktopApp, Mode};

/// A path in the temporary directory, removed when dropped.
struct TempPath(PathBuf);

impl Drop for TempPath {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn harness() -> Harness<'static, DesktopApp> {
    let mut harness = Harness::new_ui_state(
        |ui, app: &mut DesktopApp| app.show(ui),
        DesktopApp::default(),
    );
    harness.set_size(egui::Vec2::new(1400.0, 1100.0));
    harness.run();
    harness
}

fn click_label(harness: &mut Harness<'_, DesktopApp>, label: &str) {
    harness.get_by_label(label).click();
    harness.run();
}

/// Where the drawing sits on the sheet. A new project shows the grid origin
/// in the middle of the canvas; the circuit, 22 units tall, is centred on
/// it.
const ORIGIN: (i32, i32) = (0, -11);

fn screen(harness: &Harness<'_, DesktopApp>, x: i32, y: i32) -> Pos2 {
    let [sx, sy] = harness
        .state()
        .grid_to_screen(GridPoint::new(x + ORIGIN.0, y + ORIGIN.1))
        .unwrap();
    Pos2::new(sx, sy)
}

fn button(harness: &mut Harness<'_, DesktopApp>, pos: Pos2, pressed: bool) {
    harness.event(Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    });
    harness.run();
}

fn click_grid(harness: &mut Harness<'_, DesktopApp>, x: i32, y: i32) {
    let pos = screen(harness, x, y);
    harness.hover_at(pos);
    harness.run();
    button(harness, pos, true);
    button(harness, pos, false);
}

fn drag_grid(harness: &mut Harness<'_, DesktopApp>, from: (i32, i32), to: (i32, i32)) {
    let start = screen(harness, from.0, from.1);
    let end = screen(harness, to.0, to.1);
    harness.hover_at(start);
    harness.run();
    button(harness, start, true);
    harness.hover_at(end);
    harness.run();
    button(harness, end, false);
}

/// Choose an option of a combo box of the inspector.
fn choose(harness: &mut Harness<'_, DesktopApp>, combo: &str, option: &str) {
    click_label(harness, combo);
    click_label(harness, option);
}

/// Type into a text field of the inspector and leave it with Enter.
fn type_into(harness: &mut Harness<'_, DesktopApp>, field: &str, text: &str) {
    let node = harness.get_by_label(field);
    node.focus();
    node.type_text(text);
    harness.run();
    harness.key_press(egui::Key::Enter);
    harness.run();
}

fn document(harness: &Harness<'_, DesktopApp>) -> Schematic {
    harness.state().session().unwrap().document().clone()
}

/// No error a transaction must never leave behind.
fn intact(doc: &Schematic) -> bool {
    doc.validate().iter().all(|d| {
        !matches!(
            d.code(),
            DiagnosticCode::DuplicateId
                | DiagnosticCode::DanglingReference
                | DiagnosticCode::IdOutOfRange
                | DiagnosticCode::SelfLoopWire
        )
    })
}

fn place(harness: &mut Harness<'_, DesktopApp>, palette: &str, x: i32, y: i32) {
    click_label(harness, palette);
    click_grid(harness, x, y);
    assert!(intact(&document(harness)));
}

/// Draw the control circuit of a direct-online starter with the window.
fn draw(harness: &mut Harness<'_, DesktopApp>) {
    click_label(harness, "New");
    assert_eq!(harness.state().mode(), Mode::Edit);
    click_label(harness, "Add 24 V DC");
    let supply = document(harness).supplies[0].id;
    let supply_option = format!("Supply {supply} (DC, 24000 mV)");

    // Supply taps: each placed tap is selected, so the inspector shows it.
    for (y, conductor) in [(0, "dc_pos"), (22, "dc_neg")] {
        place(harness, "supply.terminal · terminal", 0, y);
        choose(harness, "supply", &supply_option);
        choose(harness, "conductor", conductor);
    }
    place(harness, "pushbutton_no · contact", -4, 11);
    place(harness, "pushbutton_nc · contact", 0, 4);
    place(harness, "contactor.generic · coil", 0, 18);
    type_into(harness, "rated_voltage_mv", "24000");
    choose(harness, "connection", "dc");
    // The seal-in contact lands beside the coil; move it into the rung.
    click_label(harness, "Add aux_no to K1");
    click_label(harness, "Select");
    drag_grid(harness, (6, 18), (4, 11));

    click_label(harness, "Wire");
    for (from, to) in [
        ((0, 0), (0, 2)),
        ((0, 6), (0, 16)),
        ((0, 20), (0, 22)),
        ((-4, 9), (0, 8)),
        ((-4, 13), (0, 14)),
        ((4, 9), (0, 8)),
        ((4, 13), (0, 14)),
    ] {
        click_grid(harness, from.0, from.1);
        click_grid(harness, to.0, to.1);
        assert!(intact(&document(harness)));
    }
    click_label(harness, "Select");
}

#[test]
fn starter_drawn_saved_reopened_and_simulated_through_the_window() {
    let file = TempPath(std::env::temp_dir().join(format!(
        "opencircuit-exit-flow-{}.ocircuit",
        std::process::id()
    )));
    let mut harness = harness();

    draw(&mut harness);
    let drawn = document(&harness);
    assert_eq!(
        (
            drawn.devices.len(),
            drawn.symbols.len(),
            drawn.junctions.len()
        ),
        (5, 6, 2)
    );
    assert!(
        harness.state().session().unwrap().diagnostics().is_empty(),
        "{:?}",
        harness.state().session().unwrap().diagnostics()
    );

    // Undo and redo the last wire: no partial state either way.
    click_label(&mut harness, "Undo");
    let undone = document(&harness);
    assert_eq!(undone.wires.len(), drawn.wires.len() - 1);
    assert!(intact(&undone));
    click_label(&mut harness, "Redo");
    assert_eq!(document(&harness), drawn);

    // Save where the path field says, then reopen from disk.
    let field = harness.get_by_label("Path");
    field.focus();
    field.type_text(&file.0.display().to_string());
    harness.run();
    click_label(&mut harness, "Save here");
    assert!(!harness.state().is_dirty());
    harness.state_mut().request_open(file.0.clone());
    harness.run();
    assert_eq!(document(&harness), drawn);

    // Simulate: S1 starts K1 and the seal-in keeps it after S1 is released.
    click_label(&mut harness, "Simulate");
    click_label(&mut harness, "S1: press");
    click_label(&mut harness, "Step");
    click_label(&mut harness, "Step");
    click_label(&mut harness, "S1: release");
    click_label(&mut harness, "Step");
    click_label(&mut harness, "Step");

    let session = harness.state().session().unwrap();
    let k1 = session
        .document()
        .devices
        .iter()
        .find(|d| d.reference.0 == "K1")
        .unwrap()
        .id;
    assert!(session.snapshot().unwrap().active_devices.contains(&k1));

    // S2 stops it.
    click_label(&mut harness, "S2: press");
    click_label(&mut harness, "Step");
    click_label(&mut harness, "Step");
    let session = harness.state().session().unwrap();
    assert!(!session.snapshot().unwrap().active_devices.contains(&k1));
}
