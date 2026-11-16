//! The frame of the window: menus, status bar, sheet tabs and the problems
//! list, driven headless through its accessibility tree.

use circuit_model::GridPoint;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use opencircuit_desktop::{DesktopApp, Mode};

const COMMAND: &str = include_str!("../../../examples/direct-starter/command.ocircuit");
const SHEETS: &str = include_str!("../../../examples/sheets/starter-sheets.ocircuit");

fn harness(app: DesktopApp) -> Harness<'static, DesktopApp> {
    let mut harness = Harness::new_ui_state(|ui, app: &mut DesktopApp| app.show(ui), app);
    harness.set_size(eframe::egui::Vec2::new(1200.0, 900.0));
    harness.run();
    harness
}

fn opened(text: &str, name: &str) -> Harness<'static, DesktopApp> {
    let mut app = DesktopApp::default();
    app.open_text(text, name);
    harness(app)
}

fn click(harness: &mut Harness<'_, DesktopApp>, label: &str) {
    harness.get_by_label(label).click();
    harness.run();
}

#[test]
fn the_menus_are_there_and_open_on_a_click() {
    let mut harness = opened(COMMAND, "command.ocircuit");

    for menu in ["File", "Modify", "View", "Run", "Help"] {
        assert!(harness.query_by_label(menu).is_some(), "{menu}");
    }
    assert!(harness.query_by_label("New project").is_none());

    click(&mut harness, "File");

    assert!(harness.query_by_label("New project").is_some());
    assert!(harness.query_by_label("Load library…").is_some());
}

#[test]
fn the_help_menu_opens_the_keys_and_the_about_window() {
    let mut harness = opened(COMMAND, "command.ocircuit");

    click(&mut harness, "Help");
    click(&mut harness, "Keyboard shortcuts");
    assert!(harness.query_by_label("Keyboard shortcuts").is_some());
    assert!(harness.query_by_label_contains("select all").is_some());

    click(&mut harness, "Help");
    click(&mut harness, "About OpenCircuit");
    assert!(
        harness
            .query_by_label_contains("MIT or Apache-2.0")
            .is_some()
    );
}

#[test]
fn the_run_menu_switches_the_mode() {
    let mut harness = opened(COMMAND, "command.ocircuit");
    assert_eq!(harness.state().mode(), Mode::Simulate);

    click(&mut harness, "Run");
    click(&mut harness, "Drawing mode");

    assert_eq!(harness.state().mode(), Mode::Edit);
}

#[test]
fn the_grid_and_snap_switches_of_the_status_bar_toggle_the_view() {
    let mut harness = opened(COMMAND, "command.ocircuit");
    assert!(harness.state().grid_shown() && harness.state().snapping());

    click(&mut harness, "Grid");
    assert!(!harness.state().grid_shown());
    click(&mut harness, "Grid");
    assert!(harness.state().grid_shown());

    click(&mut harness, "Snap");
    assert!(!harness.state().snapping());
}

#[test]
fn the_view_menu_toggles_the_same_switches() {
    let mut harness = opened(COMMAND, "command.ocircuit");

    click(&mut harness, "View");
    click(&mut harness, "Show the grid");

    assert!(!harness.state().grid_shown());
}

#[test]
fn the_problems_badge_hides_and_shows_the_list() {
    let mut harness = opened(COMMAND, "command.ocircuit");
    assert!(
        harness
            .query_by_label_contains("the project compiles")
            .is_some()
    );

    click(&mut harness, "Problems");
    assert!(!harness.state().problems_shown());
    assert!(
        harness
            .query_by_label_contains("the project compiles")
            .is_none()
    );

    click(&mut harness, "Problems");
    assert!(
        harness
            .query_by_label_contains("the project compiles")
            .is_some()
    );
}

#[test]
fn the_status_bar_shows_the_grid_point_under_the_pointer() {
    let mut harness = opened(COMMAND, "command.ocircuit");
    assert_eq!(harness.state().cursor(), None);
    let [x, y] = harness
        .state()
        .grid_to_screen(GridPoint::new(3, 4))
        .expect("the canvas has been drawn");

    harness.hover_at(eframe::egui::Pos2::new(x, y));
    harness.run();

    assert_eq!(harness.state().cursor(), Some(GridPoint::new(3, 4)));
    assert!(harness.query_by_label_contains("3, 4").is_some());
}

#[test]
fn a_project_with_sheets_has_a_tab_for_each_and_a_click_frames_it() {
    let mut harness = opened(SHEETS, "starter-sheets.ocircuit");

    assert!(harness.query_by_label("1: Circuito de comando").is_some());
    assert!(harness.query_by_label("2: Circuito de força").is_some());
    let before = harness.state().grid_to_screen(GridPoint::new(0, 0));

    harness.get_by_label("2: Circuito de força").click();
    harness.run();
    harness.run();

    assert_ne!(harness.state().grid_to_screen(GridPoint::new(0, 0)), before);
}

#[test]
fn the_zoom_buttons_over_the_drawing_zoom_it() {
    let mut harness = opened(COMMAND, "command.ocircuit");
    let at = |harness: &Harness<'_, DesktopApp>, x: i32| {
        harness
            .state()
            .grid_to_screen(GridPoint::new(x, 0))
            .expect("the canvas has been drawn")[0]
    };
    let before = at(&harness, 10) - at(&harness, 0);

    click(&mut harness, "Zoom in");
    harness.run();

    assert!(at(&harness, 10) - at(&harness, 0) > before);
}

#[test]
fn a_window_without_a_project_has_the_menus_but_nothing_to_save() {
    let harness = harness(DesktopApp::default());

    assert!(harness.query_by_label("File").is_some());
    assert!(harness.query_by_label("Zoom in").is_none());
    assert_eq!(harness.state().cursor(), None);
}
