//! M9.2 (#118), the workspace: shortcuts given other keys, the side panels
//! collapsed or swapped, the tool bar hidden, a theme, named workspaces, and
//! all of it kept between sessions. A fresh window is the window as before.

use circuit_editor::Tool;
use eframe::egui::{self, Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use opencircuit_desktop::DesktopApp;
use opencircuit_desktop::workspace::{Action, Chord, Keymap, Layout, Preferences, ThemeChoice};

const PART: &str = "lamp.generic · lamp";

fn window(app: DesktopApp) -> Harness<'static, DesktopApp> {
    let mut harness = Harness::new_ui_state(|ui, app: &mut DesktopApp| app.show(ui), app);
    harness.set_size(egui::Vec2::new(1300.0, 900.0));
    harness.run();
    harness.get_by_label("New").click();
    harness.run();
    harness
        .state_mut()
        .set_mode(opencircuit_desktop::Mode::Edit);
    harness.run();
    harness
}

#[test]
fn a_fresh_window_has_the_default_layout_and_keys() {
    let harness = window(DesktopApp::default());
    let app = harness.state();
    assert_eq!(app.layout(), Layout::default());
    assert_eq!(app.keymap(), &Keymap::default());
    assert!(app.side_panels_shown());
    assert!(app.workspace_names().is_empty());
    assert!(harness.query_by_label(PART).is_some());
}

#[test]
fn a_shortcut_given_other_keys_answers_to_them_only() {
    let mut harness = window(DesktopApp::default());
    let e = Chord::pressed(Key::E, Modifiers::NONE);
    assert_eq!(
        harness.state_mut().set_shortcut(Action::WireTool, e),
        Ok(())
    );

    harness.key_press(Key::W);
    harness.run();
    assert_eq!(harness.state().tool(), &Tool::Select);

    harness.key_press(Key::E);
    harness.run();
    assert_eq!(harness.state().tool(), &Tool::Wire);
}

#[test]
fn the_shortcut_window_takes_the_next_keys() {
    let mut harness = window(DesktopApp::default());
    harness.state_mut().open_shortcut_editor();
    harness.run();
    assert!(harness.state().shortcut_editor_open());

    // The button of the wire tool shows its keys; a click waits for new ones.
    harness.get_by_label("W").click();
    harness.run();
    harness.key_press(Key::E);
    harness.run();
    assert_eq!(harness.state().keymap().chord(Action::WireTool).text(), "E");
    // The key went to the window, not to the drawing.
    assert_eq!(harness.state().tool(), &Tool::Select);

    // Keys of another action are refused and say whose they are.
    harness.get_by_label("E").click();
    harness.run();
    harness.key_press(Key::R);
    harness.run();
    assert_eq!(harness.state().keymap().chord(Action::WireTool).text(), "E");
    assert!(harness.query_by_label_contains("already does").is_some());

    harness
        .get_by_label("All shortcuts to their defaults")
        .click();
    harness.run();
    assert_eq!(harness.state().keymap(), &Keymap::default());
}

#[test]
fn ctrl_b_collapses_and_brings_back_the_side_panels() {
    let mut harness = window(DesktopApp::default());
    harness.key_press_modifiers(Modifiers::COMMAND, Key::B);
    harness.run();
    assert!(!harness.state().side_panels_shown());
    assert!(harness.query_by_label(PART).is_none());

    harness.key_press_modifiers(Modifiers::COMMAND, Key::B);
    harness.run();
    assert!(harness.state().side_panels_shown());
    assert!(harness.query_by_label(PART).is_some());
}

#[test]
fn swapped_panels_put_the_palette_on_the_right() {
    let mut harness = window(DesktopApp::default());
    let left = harness.get_by_label(PART).rect().center().x;
    assert!(left < 650.0);

    let mut swapped = harness.state().layout();
    swapped.panels_swapped = true;
    harness.state_mut().apply_layout(&swapped);
    harness.run();
    harness.run();
    assert!(harness.get_by_label(PART).rect().center().x > 650.0);
}

#[test]
fn a_hidden_tool_bar_is_not_drawn() {
    let mut harness = window(DesktopApp::default());
    assert!(harness.query_by_label("Save here").is_some());
    let mut layout = harness.state().layout();
    layout.toolbar_hidden = true;
    harness.state_mut().apply_layout(&layout);
    harness.run();
    assert!(harness.query_by_label("Save here").is_none());
}

#[test]
fn a_named_workspace_brings_its_layout_back() {
    let mut harness = window(DesktopApp::default());
    let review = Layout {
        side_panels_hidden: true,
        rulers: true,
        theme: ThemeChoice::Light,
        ..Layout::default()
    };
    harness.state_mut().apply_layout(&review);
    assert!(harness.state_mut().save_workspace("Review"));
    harness.state_mut().apply_layout(&Layout::default());
    harness.run();
    assert!(harness.query_by_label(PART).is_some());

    assert!(harness.state_mut().open_workspace("Review"));
    harness.run();
    assert_eq!(harness.state().layout(), review);
    assert!(harness.query_by_label(PART).is_none());
    assert!(!harness.state_mut().open_workspace("Missing"));
    assert!(!harness.state_mut().save_workspace(" "));
}

#[test]
fn the_workspace_and_the_keys_are_kept_for_the_next_session() {
    let dir = std::env::temp_dir().join(format!(
        "opencircuit-desktop-workspace-{}",
        std::process::id()
    ));
    let file = dir.join("workspace.json");
    {
        let mut app = DesktopApp::default();
        app.enable_preferences(Preferences::at(file.clone()));
        let mut harness = window(app);
        let mut layout = harness.state().layout();
        layout.panels_swapped = true;
        layout.theme = ThemeChoice::Dark;
        harness.state_mut().apply_layout(&layout);
        let e = Chord::pressed(Key::E, Modifiers::NONE);
        assert_eq!(
            harness.state_mut().set_shortcut(Action::WireTool, e),
            Ok(())
        );
        assert!(harness.state_mut().save_workspace("Mine"));
        harness.run();
    }
    let mut app = DesktopApp::default();
    app.enable_preferences(Preferences::at(file));
    assert!(app.layout().panels_swapped);
    assert_eq!(app.layout().theme, ThemeChoice::Dark);
    assert_eq!(app.keymap().chord(Action::WireTool).text(), "E");
    assert_eq!(app.workspace_names(), ["Mine"]);
    let _ = std::fs::remove_dir_all(dir);
}
