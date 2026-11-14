//! The window, driven headless through its accessibility tree.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use opencircuit_desktop::ViewerApp;

const COMMAND: &str = include_str!("../../../examples/direct-starter/command.ocircuit");
const SERIES_LOADS: &str =
    include_str!("../../../tests/fixtures/series-loads/series-loads.ocircuit");

fn harness(app: ViewerApp) -> Harness<'static, ViewerApp> {
    let mut harness = Harness::new_ui_state(|ui, app: &mut ViewerApp| app.show(ui), app);
    harness.set_size(eframe::egui::Vec2::new(1000.0, 700.0));
    harness.run();
    harness
}

#[test]
fn window_without_a_project_explains_how_to_open_one() {
    let harness = harness(ViewerApp::default());

    assert!(
        harness
            .query_by_label_contains("drop the file here")
            .is_some()
    );
}

#[test]
fn opened_project_shows_its_name_and_that_it_compiles() {
    let mut app = ViewerApp::default();
    app.open_text(COMMAND, "command.ocircuit");

    let harness = harness(app);

    assert!(
        harness
            .query_by_label("Direct-online starter - control circuit")
            .is_some()
    );
    assert!(
        harness
            .query_by_label_contains("the project compiles")
            .is_some()
    );
}

#[test]
fn project_that_does_not_compile_lists_its_diagnostics() {
    let mut app = ViewerApp::default();
    app.open_text(SERIES_LOADS, "series-loads.ocircuit");

    let harness = harness(app);

    assert!(harness.query_by_label_contains("OC-MOD-001").is_some());
}

#[test]
fn clicking_a_diagnostic_selects_its_subject() {
    let mut app = ViewerApp::default();
    app.open_text(SERIES_LOADS, "series-loads.ocircuit");
    let mut harness = harness(app);

    harness.get_by_label_contains("OC-MOD-001").click();
    harness.run();

    assert_eq!(harness.state().selection().len(), 1);
}

#[test]
fn invalid_project_text_shows_an_error_and_opens_nothing() {
    let mut app = ViewerApp::default();
    app.open_text("{", "broken.ocircuit");

    let harness = harness(app);

    assert!(harness.state().session().is_none());
    assert!(harness.query_by_label_contains("broken.ocircuit").is_some());
}
