//! The dialogs of the window (M9.5): one look for the questions and the
//! errors, the main button first on Windows and last elsewhere with the
//! keyboard on it, Escape that dismisses, and an error that can be cleared.

use eframe::egui::{self, Key};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use opencircuit_desktop::{DesktopApp, Language};

fn window(language: Language) -> Harness<'static, DesktopApp> {
    let mut app = DesktopApp::default();
    app.set_language(language);
    let mut harness = Harness::new_ui_state(|ui, app: &mut DesktopApp| app.show(ui), app);
    harness.set_size(egui::Vec2::new(1300.0, 900.0));
    harness.run();
    harness
}

fn click(harness: &mut Harness<'_, DesktopApp>, label: &str) {
    harness.get_by_label(label).click();
    harness.run();
}

fn command(harness: &mut Harness<'_, DesktopApp>, line: &str) {
    let field = harness.get_by_label("Command");
    field.focus();
    field.type_text(line);
    harness.run();
    harness.key_press(Key::Enter);
    harness.run();
}

/// A project with an unsaved change, asked to start a new one.
fn asking(language: Language) -> Harness<'static, DesktopApp> {
    let new = match language {
        Language::English => "New",
        Language::PortugueseBrazil => "Novo",
    };
    let mut harness = window(language);
    click(&mut harness, new);
    command(&mut harness, "text 4,3 Note");
    click(&mut harness, new);
    assert!(harness.state().is_asking());
    harness
}

#[test]
fn the_question_about_unsaved_changes_has_three_buttons_in_the_order_of_the_system() {
    let harness = asking(Language::English);

    let save = harness.get_by_label("Save and continue").rect();
    let discard = harness.get_by_label("Discard changes").rect();
    let cancel = harness.get_by_label("Cancel").rect();

    // They share a row.
    assert!((save.center().y - cancel.center().y).abs() < 1.0);
    assert!((save.center().y - discard.center().y).abs() < 1.0);
    if cfg!(target_os = "windows") {
        assert!(save.max.x <= discard.min.x && discard.max.x <= cancel.min.x);
    } else {
        assert!(cancel.max.x <= discard.min.x && discard.max.x <= save.min.x);
    }
    // And they are a size a thumb or a pointer hits: not the thin buttons of
    // a form.
    for button in [save, discard, cancel] {
        assert!(button.height() >= 28.0, "{button:?}");
        assert!(button.width() >= 88.0, "{button:?}");
    }
}

#[test]
fn the_dialog_stays_inside_the_window_and_the_main_button_has_the_keyboard() {
    let harness = asking(Language::English);

    let save = harness.get_by_label("Save and continue");
    assert!(
        save.accesskit_node().is_focused(),
        "Enter would not answer the main button"
    );
    let window = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1300.0, 900.0));
    for label in [
        "Save and continue",
        "Discard changes",
        "Cancel",
        "Unsaved changes",
    ] {
        let rect = harness.get_by_label(label).rect();
        assert!(window.contains_rect(rect), "{label}: {rect:?}");
    }
}

#[test]
fn escape_dismisses_the_question_and_keeps_the_project() {
    let mut harness = asking(Language::English);

    harness.key_press(Key::Escape);
    harness.run();

    assert!(!harness.state().is_asking());
    assert!(harness.state().is_dirty());
    assert!(harness.query_by_label("Unsaved changes").is_none());
}

#[test]
fn the_question_is_in_portuguese_with_the_same_buttons() {
    let harness = asking(Language::PortugueseBrazil);

    for label in [
        "Alterações não salvas",
        "Salvar e continuar",
        "Descartar alterações",
        "Cancelar",
    ] {
        assert!(harness.query_by_label(label).is_some(), "{label}");
    }
}

#[test]
fn an_error_is_a_banner_that_can_be_dismissed() {
    let mut harness = window(Language::English);
    click(&mut harness, "New");
    harness.state_mut().type_point();
    harness.run();
    harness.get_by_label("Point").type_text("twelve");
    harness.run();
    harness.key_press(Key::Enter);
    harness.run();

    assert!(harness.state().error().is_some());
    assert!(
        harness
            .query_by_label("Type a point as x,y or @dx,dy.")
            .is_some()
    );
    click(&mut harness, "Dismiss");

    assert!(harness.state().error().is_none());
    assert!(
        harness
            .query_by_label("Type a point as x,y or @dx,dy.")
            .is_none()
    );
}
