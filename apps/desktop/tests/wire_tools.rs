//! M9.2 (#118): the wire tools in the context menu of the drawing.

use circuit_model::{GridPoint, SchematicBuilder};
use eframe::egui::{self, Event, Modifiers, PointerButton, Pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use opencircuit_desktop::DesktopApp;
use opencircuit_desktop::app::Mode;

/// A wire from (0, 0) to (10, 0), its ends free, in edit mode.
fn one_wire() -> Harness<'static, DesktopApp> {
    let mut b = SchematicBuilder::new("wire");
    let left = b.junction(GridPoint::new(0, 0)).expect("id");
    let right = b.junction(GridPoint::new(10, 0)).expect("id");
    b.wire(left, right).expect("id");
    let text = circuit_project::to_string(&b.finish()).expect("text");
    let mut app = DesktopApp::default();
    app.open_text(&text, "wire.ocircuit");
    app.set_mode(Mode::Edit);
    let mut harness = Harness::new_ui_state(|ui, app: &mut DesktopApp| app.show(ui), app);
    harness.set_size(egui::Vec2::new(1300.0, 900.0));
    harness.run();
    harness
}

fn right_click(harness: &mut Harness<'_, DesktopApp>, x: i32, y: i32) {
    let [sx, sy] = harness
        .state()
        .grid_to_screen(GridPoint::new(x, y))
        .expect("drawn");
    let pos = Pos2::new(sx, sy);
    harness.hover_at(pos);
    harness.run();
    for pressed in [true, false] {
        harness.event(Event::PointerButton {
            pos,
            button: PointerButton::Secondary,
            pressed,
            modifiers: Modifiers::NONE,
        });
        harness.run();
    }
}

fn ends(harness: &Harness<'_, DesktopApp>) -> Vec<GridPoint> {
    let mut points: Vec<GridPoint> = harness
        .state()
        .session()
        .expect("open")
        .document()
        .junctions
        .iter()
        .map(|junction| junction.position)
        .collect();
    points.sort();
    points
}

#[test]
fn the_context_menu_trims_a_wire_at_the_point_clicked() {
    let mut harness = one_wire();

    right_click(&mut harness, 8, 0);
    harness.get_by_label("Trim wire here").click();
    harness.run();

    assert_eq!(ends(&harness), [GridPoint::new(0, 0), GridPoint::new(8, 0)]);
}

#[test]
fn a_free_end_with_nothing_ahead_offers_no_extension() {
    let mut harness = one_wire();

    right_click(&mut harness, 10, 0);

    assert!(harness.query_by_label("Extend wire end").is_none());
    assert!(harness.query_by_label("Trim wire here").is_none());
}
