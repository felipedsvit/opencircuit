//! What M7 adds to the window (ADR-0014): renumbering, the project tab, the
//! new inspector sections and the problems actions. Each test opens a small
//! project and drives the window through its accessibility tree.

use circuit_model::builder::pin;
use circuit_model::{GridPoint, Schematic, SchematicBuilder, Subject, SymbolId};
use eframe::egui::{self, Event, Modifiers, PointerButton, Pos2};
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

/// Open a project and switch to drawing mode, where the inspector is.
fn drawing(doc: &Schematic) -> Harness<'static, DesktopApp> {
    let mut harness = open(doc);
    click(&mut harness, "Run");
    click(&mut harness, "Drawing mode");
    harness
}

/// Click a widget of the inspector, scrolling to it first.
fn press(harness: &mut Harness<'_, DesktopApp>, label: &str) {
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

fn selected(harness: &Harness<'_, DesktopApp>) -> Vec<Subject> {
    harness.state().selection().iter().cloned().collect()
}

/// Type into a text field of the inspector and leave it.
fn type_into(harness: &mut Harness<'_, DesktopApp>, label: &str, text: &str) {
    harness.get_by_label(label).scroll_to_me();
    harness.run();
    harness.get_by_label(label).click();
    harness.run();
    harness.get_by_label(label).type_text(text);
    harness.run();
    harness.key_press(egui::Key::Enter);
    harness.run();
}

fn symbol_of(doc: &Schematic, reference: &str, key: &str) -> SymbolId {
    let device = doc
        .devices
        .iter()
        .find(|d| d.reference.0 == reference)
        .expect("device");
    doc.symbols
        .iter()
        .find(|s| s.device_id == device.id && s.symbol_key.0 == key)
        .expect("symbol")
        .id
}

/// S1 at (0, 0) and S2 at (0, 10), joined by a wire from S1's lower terminal
/// to S2's upper one.
fn two_buttons() -> Schematic {
    let mut b = SchematicBuilder::new("buttons");
    let s1 = b.device("S1", "pushbutton_no", &[]).unwrap();
    let s2 = b.device("S2", "pushbutton_no", &[]).unwrap();
    let first = b.symbol(s1, "contact", None, GridPoint::new(0, 0)).unwrap();
    let second = b
        .symbol(s2, "contact", None, GridPoint::new(0, 10))
        .unwrap();
    b.wire(pin(first, "p2"), pin(second, "p1")).unwrap();
    b.finish()
}

#[test]
fn the_project_tree_lists_the_sheets_and_a_click_on_a_device_selects_its_symbol() {
    let mut doc = SchematicBuilder::new("tree");
    let a = doc.device("S1", "pushbutton_no", &[]).unwrap();
    let b = doc.device("S2", "pushbutton_no", &[]).unwrap();
    doc.symbol(a, "contact", None, GridPoint::new(5, 0))
        .unwrap();
    doc.symbol(b, "contact", None, GridPoint::new(55, 0))
        .unwrap();
    let mut doc = doc.finish();
    for (id, number, x) in [(900, 1, -5), (901, 2, 45)] {
        doc.sheets.push(circuit_model::Sheet {
            id: circuit_model::SheetId(id),
            number,
            title: String::new(),
            origin: GridPoint::new(x, -5),
            size: circuit_model::SheetSize::Custom {
                width: 40,
                height: 20,
            },
            orientation: circuit_model::Orientation::Landscape,
            zones: circuit_model::Zones {
                columns: 4,
                rows: 2,
            },
        });
    }
    let mut harness = drawing(&doc);
    assert!(
        harness.query_by_label_contains("S2 ·").is_none(),
        "the tree is folded"
    );

    press(&mut harness, "Project tree");
    assert!(harness.query_by_label_contains("S1 ·").is_some());

    harness.get_by_label_contains("S2 ·").scroll_to_me();
    harness.run();
    harness.get_by_label_contains("S2 ·").click();
    harness.run();

    assert_eq!(
        selected(&harness),
        [Subject::Symbol(symbol_of(&doc, "S2", "contact"))]
    );
    // A sheet has a button that fits the view to it.
    let show = harness.get_all_by_label("Show sheet").next();
    assert!(show.is_some());
}

#[test]
fn the_connections_of_a_symbol_name_the_terminals_it_meets_and_a_click_goes_there() {
    let doc = two_buttons();
    let mut harness = drawing(&doc);
    click_grid(&mut harness, 0, 0);
    assert_eq!(
        selected(&harness),
        [Subject::Symbol(symbol_of(&doc, "S1", "contact"))]
    );

    press(&mut harness, "Connections");
    assert!(harness.query_by_label("not connected").is_some());
    harness.get_by_label("S2:13").click();
    harness.run();

    assert_eq!(
        selected(&harness),
        [Subject::Symbol(symbol_of(&doc, "S2", "contact"))]
    );
}

#[test]
fn a_coil_goes_to_its_contacts_and_a_contact_to_its_coil() {
    let mut b = SchematicBuilder::new("xref");
    let k1 = b.device("K1", "contactor.generic", &[]).unwrap();
    b.symbol(k1, "coil", None, GridPoint::new(0, 0)).unwrap();
    b.symbol(k1, "aux_no", Some(1), GridPoint::new(20, 0))
        .unwrap();
    let doc = b.finish();
    let mut harness = drawing(&doc);
    click_grid(&mut harness, 0, 0);

    press(&mut harness, "Cross-references");
    harness.get_by_label("13-14").click();
    harness.run();
    assert_eq!(
        selected(&harness),
        [Subject::Symbol(symbol_of(&doc, "K1", "aux_no"))]
    );

    press(&mut harness, "coil");
    assert_eq!(
        selected(&harness),
        [Subject::Symbol(symbol_of(&doc, "K1", "coil"))]
    );
}

#[test]
fn the_catalog_data_of_a_device_is_typed_into_the_inspector_and_undone_in_one_step() {
    let doc = two_buttons();
    let mut harness = drawing(&doc);
    click_grid(&mut harness, 0, 0);

    press(&mut harness, "Catalog data");
    type_into(&mut harness, "Part no", "3SB3000");

    let after = document(&harness);
    let info = after.devices[0].info.clone().expect("catalog data");
    assert_eq!(info.part_no.as_deref(), Some("3SB3000"));

    click(&mut harness, "Modify");
    click(&mut harness, "Undo last change");
    assert_eq!(document(&harness).devices[0].info, None);
}

#[test]
fn the_wire_numbering_rule_is_switched_on_and_a_wire_takes_its_number_or_its_own() {
    let doc = two_buttons();
    let mut harness = drawing(&doc);
    assert!(document(&harness).project.wire_numbering.is_none());

    press(&mut harness, "Number the wires");
    type_into(&mut harness, "Prefix", "W");
    let rule = document(&harness).project.wire_numbering.expect("a rule");
    assert_eq!(rule.prefix, "W");

    // The wire between the buttons is at x = 0, from y = 2 to y = 8.
    click_grid(&mut harness, 0, 5);
    assert!(harness.query_by_label("Number W1").is_some());
    type_into(&mut harness, "Own number", "L1");

    assert_eq!(document(&harness).wires[0].number.as_deref(), Some("L1"));
    assert!(harness.query_by_label("Number L1").is_some());
}

#[test]
fn the_view_menu_traces_the_net_of_the_selection_and_the_status_bar_counts_it() {
    let doc = two_buttons();
    let mut harness = drawing(&doc);
    // The wire between the buttons is at x = 0, from y = 2 to y = 8.
    click_grid(&mut harness, 0, 5);
    assert!(
        harness
            .query_by_label_contains("Net of the selection")
            .is_none()
    );

    click(&mut harness, "View");
    click(&mut harness, "Trace the net of the selection");

    assert!(
        harness
            .query_by_label("Net of the selection: 2 terminals, 1 wire")
            .is_some()
    );

    // Off again with the key, and nothing is traced.
    harness.key_press(egui::Key::T);
    harness.run();
    assert!(
        harness
            .query_by_label_contains("Net of the selection")
            .is_none()
    );
}

#[test]
fn tracing_does_nothing_without_a_selection() {
    let doc = two_buttons();
    let mut harness = drawing(&doc);

    harness.key_press(egui::Key::T);
    harness.run();

    assert!(
        harness
            .query_by_label_contains("Net of the selection")
            .is_none()
    );
}

#[test]
fn the_palette_remembers_the_parts_chosen_and_offers_them_again() {
    let mut harness = Harness::new_ui_state(
        |ui, app: &mut DesktopApp| app.show(ui),
        DesktopApp::default(),
    );
    harness.set_size(egui::Vec2::new(1300.0, 900.0));
    harness.run();
    click(&mut harness, "New");
    assert!(harness.query_by_label_contains("Recent ").is_none());

    press(&mut harness, "lamp.generic · lamp");
    press(&mut harness, "pushbutton_no · contact");

    // Newest first, both there, and the strip does not add a tab.
    assert!(
        harness
            .query_by_label("Recent lamp.generic · lamp")
            .is_some()
    );
    assert!(
        harness
            .query_by_label("Recent pushbutton_no · contact")
            .is_some()
    );
    // Choosing the older one from the strip chooses the part again.
    press(&mut harness, "Recent lamp.generic · lamp");
    click_grid(&mut harness, 0, 0);
    let doc = document(&harness);
    assert_eq!(doc.devices.len(), 1);
    assert_eq!(doc.devices[0].model.id, "lamp.generic");
}

#[test]
fn a_part_clicked_on_a_wire_in_the_window_is_put_in_series_with_it() {
    let doc = two_buttons();
    let mut harness = drawing(&doc);

    press(&mut harness, "pushbutton_no · contact");
    // The wire between the buttons is at x = 0, from y = 2 to y = 8.
    click_grid(&mut harness, 0, 5);

    let after = document(&harness);
    assert_eq!(after.symbols.len(), 3);
    assert_eq!(after.wires.len(), 2);
    assert_eq!(after.devices.len(), 3);

    click(&mut harness, "Modify");
    click(&mut harness, "Undo last change");
    let undone = document(&harness);
    assert_eq!((undone.symbols.len(), undone.wires.len()), (2, 1));
}
