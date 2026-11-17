//! The palette: tabs for the kinds of parts, a search over all of them, and
//! placing a part from the standard library or from a library loaded here.

use circuit_model::{Category, DiagnosticCode, GridPoint};
use eframe::egui::{self, Event, Modifiers, PointerButton, Pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use opencircuit_desktop::{DesktopApp, Mode};

const ACME: &str = include_str!("../../../examples/library/acme.oclib");

fn harness() -> Harness<'static, DesktopApp> {
    let mut harness = Harness::new_ui_state(
        |ui, app: &mut DesktopApp| app.show(ui),
        DesktopApp::default(),
    );
    harness.set_size(egui::Vec2::new(1300.0, 900.0));
    harness.run();
    harness.get_by_label("New").click();
    harness.run();
    assert_eq!(harness.state().mode(), Mode::Edit);
    harness
}

fn click(harness: &mut Harness<'_, DesktopApp>, label: &str) {
    harness.get_by_label(label).scroll_to_me();
    harness.run();
    harness.get_by_label(label).click();
    harness.run();
}

fn search(harness: &mut Harness<'_, DesktopApp>, text: &str) {
    let field = harness.get_by_label("Search");
    field.focus();
    field.type_text(text);
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

fn models(harness: &Harness<'_, DesktopApp>) -> Vec<String> {
    harness
        .state()
        .session()
        .expect("a project is open")
        .document()
        .devices
        .iter()
        .map(|device| device.model.id.clone())
        .collect()
}

#[test]
fn the_palette_opens_on_the_controls_tab_and_has_a_tab_for_each_kind_of_part() {
    let harness = harness();

    assert_eq!(harness.state().palette_tab(), Category::Controls);
    for category in Category::ALL {
        assert!(
            harness.query_by_label(category.label()).is_some(),
            "{category:?}"
        );
    }
    assert!(harness.query_by_label("pushbutton_no · contact").is_some());
    assert!(
        harness
            .query_by_label("std.fuse_cartridge · link")
            .is_none()
    );
}

#[test]
fn a_tab_lists_only_its_own_parts() {
    let mut harness = harness();

    click(&mut harness, Category::Protection.label());

    assert_eq!(harness.state().palette_tab(), Category::Protection);
    assert!(
        harness
            .query_by_label("std.fuse_cartridge · link")
            .is_some()
    );
    assert!(harness.query_by_label("fuse.generic · link").is_some());
    assert!(harness.query_by_label("pushbutton_no · contact").is_none());
}

#[test]
fn a_standard_part_is_placed_where_the_canvas_is_clicked() {
    let mut harness = harness();

    click(&mut harness, Category::Protection.label());
    click(&mut harness, "std.mcb_3p · main");
    click_grid(&mut harness, 0, 0);

    assert_eq!(models(&harness), ["std.mcb_3p"]);
    let doc = harness.state().session().expect("open").document();
    assert_eq!(doc.devices[0].reference.0, "Q1");
}

#[test]
fn the_search_looks_in_every_tab_and_clearing_it_goes_back_to_the_tab() {
    let mut harness = harness();

    search(&mut harness, "diode");

    assert!(harness.query_by_label("std.diode · body").is_some());
    assert!(harness.query_by_label("pushbutton_no · contact").is_none());
    assert!(harness.query_by_label_contains("Search results").is_some());

    click(&mut harness, Category::Controls.label());

    assert!(harness.query_by_label("pushbutton_no · contact").is_some());
}

#[test]
fn a_part_that_is_only_a_symbol_is_marked_and_reported_when_it_is_placed() {
    let mut harness = harness();

    search(&mut harness, "diode");
    assert!(harness.query_by_label("no sim").is_some());
    click(&mut harness, "std.diode · body");
    click_grid(&mut harness, 0, 0);

    assert_eq!(models(&harness), ["std.diode"]);
    let session = harness.state().session().expect("open");
    assert!(
        session
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code() == DiagnosticCode::NotSimulated)
    );
}

#[test]
fn a_library_loaded_in_the_window_is_listed_under_its_category() {
    let mut harness = harness();
    harness
        .state_mut()
        .add_library_text(ACME, "acme.oclib")
        .expect("the example library loads");
    harness.run();

    click(&mut harness, Category::Automation.label());

    assert!(harness.query_by_label("acme.relay_r2 · coil").is_some());
    assert!(harness.query_by_label("relay.generic · coil").is_some());
}

#[test]
fn a_search_with_no_match_says_so() {
    let mut harness = harness();

    search(&mut harness, "no such part anywhere");

    assert!(harness.query_by_label("Nothing here.").is_some());
}
