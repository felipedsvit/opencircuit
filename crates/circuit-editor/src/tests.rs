//! Tests of the editor, driven by grid points as a front end would.

use std::collections::BTreeSet;

use circuit_commands::{History, SupplyPreset, ops};
use circuit_model::{
    ComponentCatalog, DeviceId, DiagnosticCode, GridPoint, InputAction, LayoutIndex, ModelRef,
    ParamValue, QuarterTurn, Schematic, Subject, SymbolId, SymbolKey, is_orthogonal,
};
use circuit_sim::{SimConfig, Simulator, compile};
use proptest::prelude::*;

use crate::{Editor, Hit, Key, Modifiers, Tool, hit_test};

fn point(x: i32, y: i32) -> GridPoint {
    GridPoint::new(x, y)
}

fn model(id: &str) -> ModelRef {
    ModelRef {
        id: id.to_owned(),
        version: 1,
    }
}

fn place_tool(model_id: &str, symbol: &str) -> Tool {
    Tool::Place {
        model: model(model_id),
        symbol: SymbolKey(symbol.to_owned()),
        rotation: QuarterTurn::Deg0,
    }
}

/// A document under edit: the document, its history and an editor.
struct Bench {
    doc: Schematic,
    history: History,
    editor: Editor,
    catalog: ComponentCatalog,
}

impl Bench {
    fn new() -> Self {
        Self {
            doc: Schematic::new("editor"),
            history: History::new(),
            editor: Editor::new(),
            catalog: ComponentCatalog::builtin(),
        }
    }

    fn apply(&mut self, outcome: crate::Outcome) {
        assert_eq!(outcome.error, None);
        if let Some(tx) = outcome.commit {
            self.history.execute(&mut self.doc, tx).unwrap();
        }
        self.editor.retain_existing(&self.doc);
    }

    fn click_with(&mut self, at: GridPoint, modifiers: Modifiers) {
        let down = self
            .editor
            .pointer_down(&self.doc, &self.catalog, at, modifiers);
        self.apply(down);
        let up = self.editor.pointer_up(&self.doc, &self.catalog, at);
        self.apply(up);
    }

    fn click(&mut self, at: GridPoint) {
        self.click_with(at, Modifiers::default());
    }

    fn drag(&mut self, from: GridPoint, to: GridPoint) {
        let down = self
            .editor
            .pointer_down(&self.doc, &self.catalog, from, Modifiers::default());
        self.apply(down);
        self.editor.pointer_move(&self.doc, &self.catalog, to);
        let up = self.editor.pointer_up(&self.doc, &self.catalog, to);
        self.apply(up);
    }

    fn key(&mut self, key: Key) {
        let outcome = self.editor.key(&self.doc, &self.catalog, key);
        self.apply(outcome);
    }

    /// Place a device with the place tool and return its symbol.
    fn place(&mut self, model_id: &str, symbol: &str, at: GridPoint) -> SymbolId {
        self.editor.set_tool(place_tool(model_id, symbol));
        self.click(at);
        self.editor.set_tool(Tool::Select);
        self.doc.symbols.iter().map(|s| s.id).max().unwrap()
    }

    fn device(&self, reference: &str) -> DeviceId {
        self.doc
            .devices
            .iter()
            .find(|d| d.reference.0 == reference)
            .unwrap()
            .id
    }

    fn position(&self, symbol: SymbolId) -> GridPoint {
        self.doc
            .symbols
            .iter()
            .find(|s| s.id == symbol)
            .unwrap()
            .position
    }

    fn wires_are_orthogonal(&self) -> bool {
        let layout = LayoutIndex::new(&self.doc, &self.catalog);
        self.doc
            .wires
            .iter()
            .all(|w| layout.wire_polyline(w).is_some_and(|p| is_orthogonal(&p)))
    }

    fn set(&mut self, reference: &str, key: &str, value: ParamValue) {
        let device = self.device(reference);
        let tx = ops::set_param(&self.doc, &self.catalog, device, key, Some(value)).unwrap();
        self.history.execute(&mut self.doc, tx).unwrap();
    }
}

#[test]
fn pin_wins_over_the_body_of_its_symbol() {
    let mut bench = Bench::new();
    let contact = bench.place("pushbutton_no", "contact", point(0, 0));

    assert_eq!(
        hit_test(&bench.doc, &bench.catalog, point(0, -2)),
        Some(Hit::Pin(circuit_model::PinRef {
            symbol_id: contact,
            terminal: circuit_model::TerminalKey("p1".to_owned()),
        }))
    );
    assert_eq!(
        hit_test(&bench.doc, &bench.catalog, point(1, 0)),
        Some(Hit::Symbol(contact))
    );
}

#[test]
fn junction_wins_over_the_wire_it_sits_on_and_a_wire_over_empty_space() {
    let mut bench = Bench::new();
    bench.place("pushbutton_no", "contact", point(0, 0));
    bench.place("contactor.generic", "coil", point(0, 12));
    bench.editor.set_tool(Tool::Wire);
    bench.click(point(0, 2));
    bench.click(point(0, 10));
    bench.place("pushbutton_no", "contact", point(6, 6));
    bench.editor.set_tool(Tool::Wire);
    bench.click(point(6, 4));
    bench.click(point(0, 5));

    assert!(matches!(
        hit_test(&bench.doc, &bench.catalog, point(0, 5)),
        Some(Hit::Junction(_))
    ));
    assert!(matches!(
        hit_test(&bench.doc, &bench.catalog, point(0, 8)),
        Some(Hit::Wire { at, .. }) if at == point(0, 8)
    ));
    assert_eq!(hit_test(&bench.doc, &bench.catalog, point(30, 30)), None);
}

#[test]
fn click_selects_and_shift_click_toggles() {
    let mut bench = Bench::new();
    let a = bench.place("pushbutton_no", "contact", point(0, 0));
    let b = bench.place("pushbutton_no", "contact", point(8, 0));

    bench.click(point(0, 0));
    assert_eq!(
        bench.editor.selection(),
        &BTreeSet::from([Subject::Symbol(a)])
    );

    bench.click_with(point(8, 0), Modifiers { shift: true });
    assert_eq!(
        bench.editor.selection(),
        &BTreeSet::from([Subject::Symbol(a), Subject::Symbol(b)])
    );

    bench.click_with(point(0, 0), Modifiers { shift: true });
    assert_eq!(
        bench.editor.selection(),
        &BTreeSet::from([Subject::Symbol(b)])
    );
}

#[test]
fn click_on_empty_space_clears_the_selection() {
    let mut bench = Bench::new();
    bench.place("pushbutton_no", "contact", point(0, 0));
    bench.click(point(0, 0));

    bench.click(point(20, 20));

    assert!(bench.editor.selection().is_empty());
}

#[test]
fn dragging_previews_the_move_and_commits_it_once_on_release() {
    let mut bench = Bench::new();
    let contact = bench.place("pushbutton_no", "contact", point(0, 0));
    let revision = bench.history.revision();
    let down = bench.editor.pointer_down(
        &bench.doc,
        &bench.catalog,
        point(0, 0),
        Modifiers::default(),
    );
    bench.apply(down);
    for x in 1..=5 {
        bench
            .editor
            .pointer_move(&bench.doc, &bench.catalog, point(x, 3));
    }

    let preview = bench.editor.preview(&bench.doc, &bench.catalog);
    let ghost = preview.document.unwrap();
    assert_eq!(
        ghost
            .symbols
            .iter()
            .find(|s| s.id == contact)
            .unwrap()
            .position,
        point(5, 3)
    );
    assert_eq!(bench.position(contact), point(0, 0));
    assert_eq!(bench.history.revision(), revision);

    let up = bench
        .editor
        .pointer_up(&bench.doc, &bench.catalog, point(5, 3));
    bench.apply(up);

    assert_eq!(bench.position(contact), point(5, 3));
    assert_eq!(bench.history.revision(), revision + 1);
}

#[test]
fn box_selection_selects_what_lies_entirely_inside() {
    let mut bench = Bench::new();
    let inside = bench.place("pushbutton_no", "contact", point(0, 0));
    bench.place("pushbutton_no", "contact", point(20, 0));

    bench.drag(point(-5, -5), point(5, 5));

    assert_eq!(
        bench.editor.selection(),
        &BTreeSet::from([Subject::Symbol(inside)])
    );
}

#[test]
fn box_selection_shows_its_rectangle_while_dragging() {
    let mut bench = Bench::new();
    let down = bench.editor.pointer_down(
        &bench.doc,
        &bench.catalog,
        point(1, 1),
        Modifiers::default(),
    );
    bench.apply(down);
    bench
        .editor
        .pointer_move(&bench.doc, &bench.catalog, point(4, -2));

    assert_eq!(
        bench
            .editor
            .preview(&bench.doc, &bench.catalog)
            .selection_box,
        Some(circuit_model::GridRect::from_corners(
            point(1, 1),
            point(4, -2)
        ))
    );
}

#[test]
fn place_tool_commits_selects_and_previews_the_next_device() {
    let mut bench = Bench::new();
    bench
        .editor
        .set_tool(place_tool("contactor.generic", "coil"));

    bench.click(point(0, 0));
    let placed = bench.doc.symbols[0].id;
    bench
        .editor
        .pointer_move(&bench.doc, &bench.catalog, point(8, 0));

    assert_eq!(
        bench.editor.selection(),
        &BTreeSet::from([Subject::Symbol(placed)])
    );
    let ghost = bench
        .editor
        .preview(&bench.doc, &bench.catalog)
        .document
        .unwrap();
    assert_eq!(ghost.symbols.len(), 2);
    assert_eq!(bench.doc.symbols.len(), 1);
}

#[test]
fn rotate_key_turns_the_next_placement() {
    let mut bench = Bench::new();
    bench
        .editor
        .set_tool(place_tool("pushbutton_no", "contact"));

    bench.key(Key::Rotate);
    bench.click(point(0, 0));

    assert_eq!(bench.doc.symbols[0].rotation, QuarterTurn::Deg90);
}

#[test]
fn wire_between_two_pins_is_orthogonal() {
    let mut bench = Bench::new();
    bench.place("pushbutton_no", "contact", point(0, 0));
    bench.place("pushbutton_no", "contact", point(6, 8));
    bench.editor.set_tool(Tool::Wire);

    bench.click(point(0, 2));
    bench.click(point(6, 6));

    assert_eq!(bench.doc.wires.len(), 1);
    assert!(bench.wires_are_orthogonal());
}

#[test]
fn clicks_on_empty_space_add_orthogonal_bends() {
    let mut bench = Bench::new();
    bench.place("pushbutton_no", "contact", point(0, 0));
    bench.place("pushbutton_no", "contact", point(10, 0));
    bench.editor.set_tool(Tool::Wire);

    bench.click(point(0, 2));
    bench.click(point(3, 7));
    bench
        .editor
        .pointer_move(&bench.doc, &bench.catalog, point(10, 4));
    let guide = bench.editor.preview(&bench.doc, &bench.catalog).guide;
    bench.click(point(10, 2));

    assert!(is_orthogonal(&guide));
    assert!(bench.doc.wires[0].route.contains(&point(3, 7)));
    assert!(bench.wires_are_orthogonal());
}

#[test]
fn wire_ending_on_another_wire_makes_a_t_junction() {
    let mut bench = Bench::new();
    bench.place("pushbutton_no", "contact", point(0, 0));
    bench.place("contactor.generic", "coil", point(0, 12));
    bench.place("pushbutton_no", "contact", point(-6, 6));
    bench.editor.set_tool(Tool::Wire);
    bench.click(point(0, 2));
    bench.click(point(0, 10));

    bench.click(point(-6, 4));
    bench.click(point(0, 5));

    assert_eq!(bench.doc.junctions.len(), 1);
    assert_eq!(bench.doc.wires.len(), 3);
    assert_eq!(bench.doc.validate(), []);
}

#[test]
fn escape_cancels_the_wire_then_leaves_the_tool_then_clears_the_selection() {
    let mut bench = Bench::new();
    let contact = bench.place("pushbutton_no", "contact", point(0, 0));
    bench.click(point(0, 0));
    bench.editor.set_tool(Tool::Wire);
    bench.click(point(0, 2));
    assert!(bench.editor.is_busy());

    bench.key(Key::Escape);
    assert!(!bench.editor.is_busy());
    assert_eq!(bench.editor.tool(), &Tool::Wire);

    bench.key(Key::Escape);
    assert_eq!(bench.editor.tool(), &Tool::Select);
    assert_eq!(
        bench.editor.selection(),
        &BTreeSet::from([Subject::Symbol(contact)])
    );

    bench.key(Key::Escape);
    assert!(bench.editor.selection().is_empty());
    assert_eq!(bench.doc.wires.len(), 0);
}

#[test]
fn delete_key_removes_the_selection_and_the_selection_follows() {
    let mut bench = Bench::new();
    bench.place("pushbutton_no", "contact", point(0, 0));
    bench.click(point(0, 0));

    bench.key(Key::Delete);

    assert!(bench.doc.symbols.is_empty());
    assert!(bench.editor.selection().is_empty());
}

#[test]
fn undo_brings_back_what_was_deleted_without_selecting_ghosts() {
    let mut bench = Bench::new();
    bench.place("pushbutton_no", "contact", point(0, 0));
    bench.click(point(0, 0));
    bench.key(Key::Delete);

    bench.history.undo(&mut bench.doc).unwrap();
    bench.editor.retain_existing(&bench.doc);

    assert_eq!(bench.doc.symbols.len(), 1);
    assert!(bench.editor.selection().is_empty());
}

#[test]
fn rotate_key_turns_the_selected_symbols() {
    let mut bench = Bench::new();
    let contact = bench.place("pushbutton_no", "contact", point(0, 0));
    bench.click(point(0, 0));

    bench.key(Key::Rotate);

    assert_eq!(
        bench
            .doc
            .symbols
            .iter()
            .find(|s| s.id == contact)
            .unwrap()
            .rotation,
        QuarterTurn::Deg90
    );
}

/// Draw the control circuit of a direct-online starter with the editor, as a
/// user would: place each part, wire it, branch the seal-in contact and the
/// start button off the trunk.
fn draw_starter_control() -> Bench {
    let mut bench = Bench::new();
    let tx = ops::add_supply(&bench.doc, SupplyPreset::Dc24V).unwrap();
    bench.history.execute(&mut bench.doc, tx).unwrap();
    let supply = ParamValue::Int(i64::try_from(bench.doc.supplies[0].id.0).unwrap());

    let p24 = bench.place("supply.terminal", "terminal", point(0, 0));
    let p0 = bench.place("supply.terminal", "terminal", point(0, 22));
    bench.place("pushbutton_no", "contact", point(-4, 11));
    bench.place("pushbutton_nc", "contact", point(0, 4));
    bench.place("contactor.generic", "coil", point(0, 18));
    let k1 = bench.device("K1");
    let seal = ops::add_symbol(
        &bench.doc,
        &bench.catalog,
        k1,
        &SymbolKey("aux_no".to_owned()),
        point(4, 11),
        QuarterTurn::Deg0,
    )
    .unwrap();
    bench.history.execute(&mut bench.doc, seal).unwrap();

    for (symbol, conductor) in [(p24, "dc_pos"), (p0, "dc_neg")] {
        let device = bench
            .doc
            .symbols
            .iter()
            .find(|s| s.id == symbol)
            .unwrap()
            .device_id;
        let reference = bench
            .doc
            .devices
            .iter()
            .find(|d| d.id == device)
            .unwrap()
            .reference
            .0
            .clone();
        bench.set(&reference, "supply", supply.clone());
        bench.set(
            &reference,
            "conductor",
            ParamValue::Text(conductor.to_owned()),
        );
    }
    bench.set("K1", "rated_voltage_mv", ParamValue::Int(24_000));
    bench.set("K1", "connection", ParamValue::Text("dc".to_owned()));

    bench.editor.set_tool(Tool::Wire);
    for (from, to) in [
        // Trunk: P24, S2, K1 coil, P0.
        (point(0, 0), point(0, 2)),
        (point(0, 6), point(0, 16)),
        (point(0, 20), point(0, 22)),
        // S1 in parallel with the seal-in contact, between y = 8 and y = 14.
        (point(-4, 9), point(0, 8)),
        (point(-4, 13), point(0, 14)),
        (point(4, 9), point(0, 8)),
        (point(4, 13), point(0, 14)),
    ] {
        bench.click(from);
        bench.click(to);
    }
    bench
}

#[test]
fn starter_control_drawn_with_the_editor_compiles_cleanly_and_is_orthogonal() {
    let bench = draw_starter_control();

    let output = compile(&bench.doc, &bench.catalog);

    assert_eq!(output.diagnostics, []);
    assert!(output.circuit.is_some());
    assert!(bench.wires_are_orthogonal());
    assert_eq!(bench.doc.junctions.len(), 2);
}

#[test]
fn starter_control_drawn_with_the_editor_starts_and_holds_the_contactor() {
    let bench = draw_starter_control();
    let circuit = compile(&bench.doc, &bench.catalog).circuit.unwrap();
    let mut sim = Simulator::new(circuit, SimConfig::default()).unwrap();
    let (s1, k1) = (bench.device("S1"), bench.device("K1"));

    sim.step_n(3, &[InputAction::Press(s1)]).unwrap();
    sim.step_n(3, &[InputAction::Release(s1)]).unwrap();

    assert!(sim.snapshot().unwrap().active_devices.contains(&k1));
}

/// One random front-end event.
#[derive(Clone, Debug)]
enum Event {
    Tool(prop::sample::Index, prop::sample::Index),
    Down(i8, i8, bool),
    Move(i8, i8),
    Up(i8, i8),
    Key(prop::sample::Index),
    Undo,
    Redo,
}

fn event() -> impl Strategy<Value = Event> {
    use prop::sample::Index;
    let coordinate = || -12_i8..12;
    prop_oneof![
        1 => (any::<Index>(), any::<Index>()).prop_map(|(t, m)| Event::Tool(t, m)),
        4 => (coordinate(), coordinate(), any::<bool>()).prop_map(|(x, y, s)| Event::Down(x, y, s)),
        3 => (coordinate(), coordinate()).prop_map(|(x, y)| Event::Move(x, y)),
        3 => (coordinate(), coordinate()).prop_map(|(x, y)| Event::Up(x, y)),
        1 => any::<Index>().prop_map(Event::Key),
        1 => Just(Event::Undo),
        1 => Just(Event::Redo),
    ]
}

fn integrity(doc: &Schematic) -> Vec<DiagnosticCode> {
    doc.validate()
        .into_iter()
        .map(|d| d.code())
        .filter(|code| {
            matches!(
                code,
                DiagnosticCode::IdOutOfRange
                    | DiagnosticCode::DuplicateId
                    | DiagnosticCode::DanglingReference
                    | DiagnosticCode::SelfLoopWire
            )
        })
        .collect()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn random_events_never_break_the_document_or_select_ghosts(
        events in prop::collection::vec(event(), 1..80)
    ) {
        let catalog = ComponentCatalog::builtin();
        let mut doc = Schematic::new("random");
        let mut history = History::new();
        let mut editor = Editor::new();
        let models = catalog.models();

        for event in &events {
            let outcome = match event {
                Event::Tool(tool, choice) => {
                    let model = choice.get(models);
                    let tools = [
                        Tool::Select,
                        Tool::Wire,
                        Tool::Place {
                            model: ModelRef { id: model.id().to_owned(), version: model.version() },
                            symbol: model.symbols().first().map(|s| s.key().clone()).unwrap_or(SymbolKey(String::new())),
                            rotation: QuarterTurn::Deg0,
                        },
                    ];
                    editor.set_tool(tool.get(tools.as_slice()).clone());
                    None
                }
                Event::Down(x, y, shift) => Some(editor.pointer_down(
                    &doc, &catalog, point((*x).into(), (*y).into()), Modifiers { shift: *shift },
                )),
                Event::Move(x, y) => {
                    editor.pointer_move(&doc, &catalog, point((*x).into(), (*y).into()));
                    None
                }
                Event::Up(x, y) => Some(editor.pointer_up(&doc, &catalog, point((*x).into(), (*y).into()))),
                Event::Key(key) => Some(editor.key(&doc, &catalog, *key.get([Key::Delete, Key::Rotate, Key::Escape].as_slice()))),
                Event::Undo => {
                    prop_assert!(history.undo(&mut doc).is_ok());
                    None
                }
                Event::Redo => {
                    prop_assert!(history.redo(&mut doc).is_ok());
                    None
                }
            };
            if let Some(tx) = outcome.and_then(|o| o.commit) {
                let before = doc.clone();
                if history.execute(&mut doc, tx).is_err() {
                    prop_assert_eq!(&doc, &before);
                }
            }
            editor.retain_existing(&doc);
            let _ = editor.preview(&doc, &catalog);

            prop_assert_eq!(integrity(&doc), Vec::<DiagnosticCode>::new());
            for subject in editor.selection() {
                let exists = match subject {
                    Subject::Symbol(id) => doc.symbols.iter().any(|s| s.id == *id),
                    Subject::Junction(id) => doc.junctions.iter().any(|j| j.id == *id),
                    Subject::Wire(id) => doc.wires.iter().any(|w| w.id == *id),
                    _ => true,
                };
                prop_assert!(exists, "{subject} is selected but gone");
            }
        }
    }
}

#[test]
fn drag_stops_when_its_selection_disappears() {
    let mut bench = Bench::new();
    let contact = bench.place("pushbutton_no", "contact", point(0, 0));
    let down = bench.editor.pointer_down(
        &bench.doc,
        &bench.catalog,
        point(0, 0),
        Modifiers::default(),
    );
    bench.apply(down);
    bench
        .editor
        .pointer_move(&bench.doc, &bench.catalog, point(3, 0));
    assert!(bench.editor.is_busy());

    let tx = ops::delete(&bench.doc, &BTreeSet::from([Subject::Symbol(contact)])).unwrap();
    bench.history.execute(&mut bench.doc, tx).unwrap();
    bench.editor.retain_existing(&bench.doc);

    assert!(!bench.editor.is_busy());
    assert_eq!(
        bench.editor.preview(&bench.doc, &bench.catalog),
        crate::Preview::default()
    );
}

#[test]
fn drag_survives_events_that_change_nothing() {
    let mut bench = Bench::new();
    bench.place("pushbutton_no", "contact", point(0, 0));
    let down = bench.editor.pointer_down(
        &bench.doc,
        &bench.catalog,
        point(0, 0),
        Modifiers::default(),
    );
    bench.apply(down);
    bench
        .editor
        .pointer_move(&bench.doc, &bench.catalog, point(3, 0));

    bench.editor.retain_existing(&bench.doc);

    assert!(bench.editor.is_busy());
}
