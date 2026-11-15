//! The window, driven headless through its accessibility tree.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use opencircuit_desktop::DesktopApp;

const COMMAND: &str = include_str!("../../../examples/direct-starter/command.ocircuit");
const SERIES_LOADS: &str =
    include_str!("../../../tests/fixtures/series-loads/series-loads.ocircuit");

fn harness(app: DesktopApp) -> Harness<'static, DesktopApp> {
    let mut harness = Harness::new_ui_state(|ui, app: &mut DesktopApp| app.show(ui), app);
    harness.set_size(eframe::egui::Vec2::new(1000.0, 700.0));
    harness.run();
    harness
}

#[test]
fn window_without_a_project_explains_how_to_open_one() {
    let harness = harness(DesktopApp::default());

    assert!(
        harness
            .query_by_label_contains("drop the file here")
            .is_some()
    );
}

#[test]
fn opened_project_shows_its_name_and_that_it_compiles() {
    let mut app = DesktopApp::default();
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
    let mut app = DesktopApp::default();
    app.open_text(SERIES_LOADS, "series-loads.ocircuit");

    let harness = harness(app);

    assert!(harness.query_by_label_contains("OC-MOD-001").is_some());
}

#[test]
fn clicking_a_diagnostic_selects_its_subject() {
    let mut app = DesktopApp::default();
    app.open_text(SERIES_LOADS, "series-loads.ocircuit");
    let mut harness = harness(app);

    harness.get_by_label_contains("OC-MOD-001").click();
    harness.run();

    assert_eq!(harness.state().selection().len(), 1);
}

const STARTER: &str = include_str!("../../../examples/direct-starter/starter.ocircuit");
const REVERSING: &str = include_str!("../../../examples/forward-reverse/reversing.ocircuit");

fn opened(text: &str, origin: &str) -> Harness<'static, DesktopApp> {
    let mut app = DesktopApp::default();
    app.open_text(text, origin);
    harness(app)
}

/// Whether the device with this designator is picked up in the last solved
/// tick.
fn active(harness: &Harness<'_, DesktopApp>, reference: &str) -> bool {
    let Some(session) = harness.state().session() else {
        return false;
    };
    let Some(device) = session
        .document()
        .devices
        .iter()
        .find(|d| d.reference.0 == reference)
    else {
        return false;
    };
    session
        .snapshot()
        .is_some_and(|s| s.active_devices.contains(&device.id))
}

fn click(harness: &mut Harness<'_, DesktopApp>, label: &str) {
    harness.get_by_label(label).click();
    harness.run();
}

#[test]
fn pressing_start_in_the_operator_panel_picks_up_the_contactor() {
    let mut harness = opened(STARTER, "starter.ocircuit");

    click(&mut harness, "S1: press");
    click(&mut harness, "Step");
    click(&mut harness, "Step");

    assert!(active(&harness, "K1"));
}

#[test]
fn pending_action_is_marked_until_the_next_tick() {
    let mut harness = opened(STARTER, "starter.ocircuit");

    click(&mut harness, "S1: press");

    assert!(harness.query_by_label("S1: release (next tick)").is_some());
}

#[test]
fn play_runs_the_simulation_and_pause_stops_it() {
    let mut harness = opened(STARTER, "starter.ocircuit");

    harness.get_by_label("Play").click();
    harness.run_steps(5);
    let running_tick = harness.state().session().unwrap().next_tick();
    harness.get_by_label("Pause").click();
    harness.run();
    let paused_tick = harness.state().session().unwrap().next_tick();
    harness.run_steps(5);

    assert!(running_tick > 0);
    assert_eq!(harness.state().session().unwrap().next_tick(), paused_tick);
}

#[test]
fn simultaneous_start_latches_and_reset_fault_clears_it() {
    let mut harness = opened(REVERSING, "reversing.ocircuit");

    click(&mut harness, "S1: press");
    click(&mut harness, "S3: press");
    click(&mut harness, "Step");
    click(&mut harness, "Step");

    assert!(harness.state().session().unwrap().is_faulted());
    assert!(harness.query_by_label_contains("Latched").is_some());
    // Once in the runtime faults, once in the events.
    assert!(
        harness
            .query_by_label_contains("fault OC-ELEC-001 started")
            .is_some()
    );
    assert!(harness.query_all_by_label_contains("OC-ELEC-001").count() >= 2);

    click(&mut harness, "Reset fault");

    assert!(!harness.state().session().unwrap().is_faulted());
}

#[test]
fn project_that_does_not_compile_has_no_simulation_controls() {
    let harness = opened(SERIES_LOADS, "series-loads.ocircuit");

    assert!(harness.query_by_label("Play").is_none());
}

#[test]
fn invalid_project_text_shows_an_error_and_opens_nothing() {
    let mut app = DesktopApp::default();
    app.open_text("{", "broken.ocircuit");

    let harness = harness(app);

    assert!(harness.state().session().is_none());
    assert!(harness.query_by_label_contains("broken.ocircuit").is_some());
}

#[test]
fn toolbar_offers_svg_export() {
    let mut app = DesktopApp::default();
    app.open_text(COMMAND, "command.ocircuit");

    let harness = harness(app);

    assert!(harness.query_by_label("SVG…").is_some());
}
