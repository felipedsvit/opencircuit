//! The drawing: the box that selects what it contains or what it touches,
//! the menu of a secondary click and the cross-shaped cursor.

use circuit_model::{GridPoint, QuarterTurn};
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

fn at(harness: &Harness<'_, DesktopApp>, x: i32, y: i32) -> Pos2 {
    let [sx, sy] = harness
        .state()
        .grid_to_screen(GridPoint::new(x, y))
        .expect("the canvas has been drawn");
    Pos2::new(sx, sy)
}

fn button(harness: &mut Harness<'_, DesktopApp>, pos: Pos2, which: PointerButton, pressed: bool) {
    harness.event(Event::PointerButton {
        pos,
        button: which,
        pressed,
        modifiers: Modifiers::NONE,
    });
    harness.run();
}

fn click_grid(harness: &mut Harness<'_, DesktopApp>, x: i32, y: i32) {
    let pos = at(harness, x, y);
    harness.hover_at(pos);
    harness.run();
    button(harness, pos, PointerButton::Primary, true);
    button(harness, pos, PointerButton::Primary, false);
}

fn right_click_grid(harness: &mut Harness<'_, DesktopApp>, x: i32, y: i32) {
    let pos = at(harness, x, y);
    harness.hover_at(pos);
    harness.run();
    button(harness, pos, PointerButton::Secondary, true);
    button(harness, pos, PointerButton::Secondary, false);
}

fn drag_grid(harness: &mut Harness<'_, DesktopApp>, from: (i32, i32), to: (i32, i32)) {
    let start = at(harness, from.0, from.1);
    let end = at(harness, to.0, to.1);
    harness.hover_at(start);
    harness.run();
    button(harness, start, PointerButton::Primary, true);
    harness.hover_at(end);
    harness.run();
    button(harness, end, PointerButton::Primary, false);
}

/// A new project with one lamp at the origin, nothing selected.
fn with_a_lamp() -> Harness<'static, DesktopApp> {
    let mut harness = harness();
    click(&mut harness, "lamp.generic · lamp");
    click_grid(&mut harness, 0, 0);
    click(&mut harness, "Select");
    harness.key_press(egui::Key::Escape);
    harness.run();
    assert!(harness.state().selection().is_empty());
    harness
}

#[test]
fn a_box_dragged_to_the_right_needs_the_whole_symbol_inside() {
    let mut harness = with_a_lamp();

    drag_grid(&mut harness, (-6, -6), (1, 1));
    assert!(harness.state().selection().is_empty());

    drag_grid(&mut harness, (-6, -6), (6, 6));
    assert_eq!(harness.state().selection().len(), 1);
}

#[test]
fn a_box_dragged_to_the_left_takes_a_symbol_it_only_touches() {
    let mut harness = with_a_lamp();

    drag_grid(&mut harness, (6, 6), (1, 1));

    assert_eq!(harness.state().selection().len(), 1);
}

#[test]
fn a_secondary_click_on_a_symbol_selects_it_and_opens_a_menu() {
    let mut harness = with_a_lamp();
    assert!(!harness.state().context_menu_open());

    right_click_grid(&mut harness, 0, 0);

    assert!(harness.state().context_menu_open());
    assert_eq!(harness.state().selection().len(), 1);
    assert!(harness.query_by_label("Rotate selection").is_some());
}

#[test]
fn the_menu_rotates_and_deletes_what_was_clicked() {
    let mut harness = with_a_lamp();

    right_click_grid(&mut harness, 0, 0);
    click(&mut harness, "Rotate selection");
    assert!(!harness.state().context_menu_open());
    let rotation = harness.state().session().expect("open").document().symbols[0].rotation;
    assert_eq!(rotation, QuarterTurn::Deg90);

    right_click_grid(&mut harness, 0, 0);
    click(&mut harness, "Delete selection");
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
fn a_click_elsewhere_closes_the_menu() {
    let mut harness = with_a_lamp();
    right_click_grid(&mut harness, 0, 0);
    assert!(harness.state().context_menu_open());

    click_grid(&mut harness, 12, 12);

    assert!(!harness.state().context_menu_open());
}

#[test]
fn the_view_menu_turns_the_cross_shaped_cursor_off_and_on() {
    let mut harness = harness();
    assert!(harness.state().crosshair_shown());

    click(&mut harness, "View");
    click(&mut harness, "Cross-shaped cursor");
    assert!(!harness.state().crosshair_shown());

    click(&mut harness, "View");
    click(&mut harness, "Cross-shaped cursor");
    assert!(harness.state().crosshair_shown());
}
