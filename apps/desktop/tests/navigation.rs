//! M9.2 (#118), navigation: previous view, a point typed for the next
//! click, wire points the wire tool snaps to, and the view options that are
//! off in a fresh window.

use std::path::Path;

use circuit_model::GridPoint;
use eframe::egui::{self, Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use opencircuit_desktop::DesktopApp;

fn starter() -> Harness<'static, DesktopApp> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/direct-starter/starter.ocircuit");
    let mut harness = Harness::new_ui_state(
        |ui, app: &mut DesktopApp| app.show(ui),
        DesktopApp::new(Some(&path)),
    );
    harness.set_size(egui::Vec2::new(1300.0, 900.0));
    harness.run();
    harness
}

fn new_project() -> Harness<'static, DesktopApp> {
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

fn screen_of(harness: &Harness<'_, DesktopApp>, point: GridPoint) -> [f32; 2] {
    harness.state().grid_to_screen(point).expect("drawn")
}

#[test]
fn alt_left_goes_back_to_the_view_before_a_zoom() {
    let mut harness = starter();
    let before = screen_of(&harness, GridPoint::new(0, 0));
    assert_eq!(harness.state().views_back(), 0);

    harness.key_press(Key::Plus);
    harness.run();
    harness.run();
    assert_ne!(screen_of(&harness, GridPoint::new(0, 0)), before);
    assert_eq!(harness.state().views_back(), 1);

    harness.key_press_modifiers(Modifiers::ALT, Key::ArrowLeft);
    harness.run();
    assert_eq!(screen_of(&harness, GridPoint::new(0, 0)), before);
    assert_eq!(harness.state().views_back(), 0);
}

#[test]
fn a_typed_point_places_a_part_there() {
    let mut harness = new_project();
    harness.get_by_label("lamp.generic · lamp").scroll_to_me();
    harness.run();
    harness.get_by_label("lamp.generic · lamp").click();
    harness.run();

    harness.key_press(Key::G);
    harness.run();
    assert!(harness.state().typing_point());
    harness.get_by_label("Point").type_text("12,-4");
    harness.run();
    harness.key_press(Key::Enter);
    harness.run();

    let app = harness.state();
    assert!(!app.typing_point());
    let doc = app.session().expect("open").document();
    assert_eq!(
        doc.symbols.iter().map(|s| s.position).collect::<Vec<_>>(),
        [GridPoint::new(12, -4)]
    );
}

#[test]
fn a_point_that_is_not_one_is_said_and_places_nothing() {
    let mut harness = new_project();
    harness.get_by_label("lamp.generic · lamp").scroll_to_me();
    harness.run();
    harness.get_by_label("lamp.generic · lamp").click();
    harness.run();
    harness.state_mut().type_point();
    harness.run();

    harness.get_by_label("Point").type_text("twelve");
    harness.run();
    harness.key_press(Key::Enter);
    harness.run();

    assert!(harness.state().error().is_some());
    assert!(
        harness
            .state()
            .session()
            .expect("open")
            .document()
            .symbols
            .is_empty()
    );
}

#[test]
fn the_new_view_options_are_off_in_a_fresh_window_and_listed_in_the_view_menu() {
    let mut harness = starter();

    harness.get_by_label("View").click();
    harness.run();

    for label in [
        "Snap to wire ends, middles and crossings",
        "Outline the element under the pointer",
        "Show rulers",
    ] {
        let node = harness.get_by_label(label);
        assert_eq!(
            node.accesskit_node().toggled(),
            Some(egui::accesskit::Toggled::False),
            "{label}"
        );
    }
    harness.get_by_label("Previous view");
}

#[test]
fn rulers_and_the_outline_draw_without_trouble() {
    let mut harness = starter();
    harness.get_by_label("View").click();
    harness.run();
    harness.get_by_label("Show rulers").click();
    harness.run();
    harness.get_by_label("View").click();
    harness.run();
    harness
        .get_by_label("Outline the element under the pointer")
        .click();
    harness.run();
    let [x, y] = screen_of(&harness, GridPoint::new(0, 0));
    harness.hover_at(egui::Pos2::new(x, y));
    harness.run();
    harness.state_mut().set_wire_point_snap(true);
    harness.run();
}
