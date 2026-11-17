//! What M7 adds to the window (ADR-0014): renumbering, the project tab, the
//! new inspector sections and the problems actions. Each test opens a small
//! project and drives the window through its accessibility tree.

use circuit_model::{GridPoint, Schematic, SchematicBuilder};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use opencircuit_desktop::DesktopApp;

fn open(doc: &Schematic) -> Harness<'static, DesktopApp> {
    let text = circuit_project::to_string(doc).expect("the document is written");
    let mut app = DesktopApp::default();
    app.open_text(&text, "project.ocircuit");
    let mut harness = Harness::new_ui_state(|ui, app: &mut DesktopApp| app.show(ui), app);
    harness.set_size(eframe::egui::Vec2::new(1300.0, 900.0));
    harness.run();
    harness
}

fn click(harness: &mut Harness<'_, DesktopApp>, label: &str) {
    harness.get_by_label(label).click();
    harness.run();
}

fn document(harness: &Harness<'_, DesktopApp>) -> Schematic {
    harness
        .state()
        .session()
        .expect("a project is open")
        .document()
        .clone()
}

/// Pushbuttons `S7` below `S3`, so reading order wants them the other way.
fn out_of_order() -> Schematic {
    let mut b = SchematicBuilder::new("renumber");
    let below = b.device("S7", "pushbutton_no", &[]).unwrap();
    b.symbol(below, "contact", None, GridPoint::new(0, 20))
        .unwrap();
    let above = b.device("S3", "pushbutton_no", &[]).unwrap();
    b.symbol(above, "contact", None, GridPoint::new(0, 0))
        .unwrap();
    b.finish()
}

fn reference_at(doc: &Schematic, y: i32) -> String {
    let symbol = doc
        .symbols
        .iter()
        .find(|symbol| symbol.position.y == y)
        .expect("a symbol there");
    doc.devices
        .iter()
        .find(|device| device.id == symbol.device_id)
        .expect("its device")
        .reference
        .0
        .clone()
}

#[test]
fn renumbering_is_in_the_modify_menu_and_undone_in_one_step() {
    let mut harness = open(&out_of_order());
    assert_eq!(reference_at(&document(&harness), 0), "S3");
    click(&mut harness, "Run");
    click(&mut harness, "Drawing mode");

    click(&mut harness, "Modify");
    click(&mut harness, "Renumber designators");

    let doc = document(&harness);
    assert_eq!(reference_at(&doc, 0), "S1");
    assert_eq!(reference_at(&doc, 20), "S2");

    click(&mut harness, "Modify");
    click(&mut harness, "Undo last change");

    let doc = document(&harness);
    assert_eq!(reference_at(&doc, 0), "S3");
    assert_eq!(reference_at(&doc, 20), "S7");
}
