//! The inspector: a summary of the selection and sections that fold.

use circuit_model::GridPoint;
use eframe::egui::{self, Event, Modifiers, PointerButton, Pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use opencircuit_desktop::DesktopApp;

fn harness() -> Harness<'static, DesktopApp> {
    let mut harness = Harness::new_ui_state(
        |ui, app: &mut DesktopApp| app.show(ui),
        DesktopApp::default(),
    );
    harness.set_size(egui::Vec2::new(1300.0, 900.0));
    harness.run();
    harness.get_by_label("New").click();
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

#[test]
fn the_summary_says_what_is_selected() {
    let mut harness = harness();
    assert!(harness.query_by_label("Nothing selected").is_some());

    click(&mut harness, "lamp.generic · lamp");
    click_grid(&mut harness, 0, 0);

    assert!(harness.query_by_label("H1 · Indicator lamp").is_some());
    assert!(harness.query_by_label("Nothing selected").is_none());
}

#[test]
fn several_selected_elements_are_counted() {
    let mut harness = harness();
    click(&mut harness, "lamp.generic · lamp");
    click_grid(&mut harness, 0, 0);
    click_grid(&mut harness, 10, 0);
    harness.key_press_modifiers(Modifiers::COMMAND, egui::Key::A);
    harness.run();

    assert!(harness.query_by_label("2 elements selected").is_some());
}

#[test]
fn a_section_folds_and_unfolds() {
    let mut harness = harness();
    assert!(harness.query_by_label("Add 24 V DC").is_some());

    click(&mut harness, "Supplies");
    assert!(harness.query_by_label("Add 24 V DC").is_none());

    click(&mut harness, "Supplies");
    assert!(harness.query_by_label("Add 24 V DC").is_some());
}
