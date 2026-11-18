//! M9.2 (#118): Modify > Route the selected wires.

use circuit_model::{GridPoint, QuarterTurn, SchematicBuilder};
use eframe::egui::{self, Event, Key, Modifiers, PointerButton, Pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use opencircuit_desktop::DesktopApp;
use opencircuit_desktop::app::Mode;

#[test]
fn the_selected_wire_goes_around_the_lamp_and_one_undo_brings_it_back() {
    let mut b = SchematicBuilder::new("route");
    let left = b.junction(GridPoint::new(0, 0)).expect("id");
    let right = b.junction(GridPoint::new(20, 0)).expect("id");
    let lamp = b.device("H1", "lamp.generic", &[]).expect("id");
    b.symbol_turned(
        lamp,
        "lamp",
        None,
        GridPoint::new(10, 0),
        QuarterTurn::Deg90,
    )
    .expect("id");
    b.wire(left, right).expect("id");
    let text = circuit_project::to_string(&b.finish()).expect("text");
    let mut app = DesktopApp::default();
    app.open_text(&text, "route.ocircuit");
    app.set_mode(Mode::Edit);
    let mut harness = Harness::new_ui_state(|ui, app: &mut DesktopApp| app.show(ui), app);
    harness.set_size(egui::Vec2::new(1300.0, 900.0));
    harness.run();

    // Select the wire by a click on it, away from the lamp.
    let [x, y] = harness
        .state()
        .grid_to_screen(GridPoint::new(2, 0))
        .expect("drawn");
    let pos = Pos2::new(x, y);
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
    assert_eq!(harness.state().selection().len(), 1);

    harness.get_by_label("Modify").click();
    harness.run();
    harness.get_by_label("Route the selected wires").click();
    harness.run();
    let routed = |harness: &Harness<'_, DesktopApp>| {
        !harness.state().session().expect("open").document().wires[0]
            .route
            .is_empty()
    };
    assert!(routed(&harness), "{:?}", harness.state().error());

    harness.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    harness.run();
    assert!(!routed(&harness));
}
