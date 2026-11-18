//! M9.4 (#118): the symbol sets that ship with OpenCircuit, and the symbol
//! editor that writes an extension library.

use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use opencircuit_desktop::DesktopApp;

fn window() -> Harness<'static, DesktopApp> {
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

fn has_model(harness: &Harness<'_, DesktopApp>, id: &str) -> bool {
    harness
        .state()
        .session()
        .expect("open")
        .catalog()
        .latest()
        .any(|model| model.id() == id)
}

#[test]
fn the_symbol_sets_load_from_the_file_menu_once() {
    let mut harness = window();
    assert!(!has_model(&harness, "iec.relay"));
    harness.get_by_label("File").click();
    harness.run();
    harness.get_by_label("Symbol sets").click();
    harness.run();
    harness.get_by_label("IEC style").click();
    harness.run();
    assert!(has_model(&harness, "iec.relay"));
    assert!(harness.state().error().is_none());

    assert_eq!(harness.state_mut().load_symbol_set("ansi"), Ok(()));
    assert_eq!(harness.state_mut().load_symbol_set("iec"), Ok(()));
    assert_eq!(harness.state().libraries().len(), 2);
    assert!(has_model(&harness, "ansi.lamp"));
    assert!(harness.state_mut().load_symbol_set("din").is_err());
}

#[test]
fn a_part_drawn_in_the_symbol_editor_goes_into_the_palette() {
    let mut harness = window();
    harness.get_by_label("File").click();
    harness.run();
    harness.get_by_label("Symbol editor…").click();
    harness.run();
    assert!(harness.state().symbol_editor_open());
    assert!(harness.query_by_label("The library is valid.").is_some());

    // A drawing that misses a terminal is refused, and says why.
    *harness.state_mut().symbol_drawing_mut().expect("open") = "circle 0 0 10\n".to_owned();
    harness.run();
    assert!(harness.query_by_label("The library is valid.").is_none());
    assert!(harness.state_mut().load_symbol_draft().is_err());

    *harness.state_mut().symbol_drawing_mut().expect("open") =
        "line 0 -20 0 -10\nline 0 10 0 20\ncircle 0 0 10\ntext 0 0 R\n".to_owned();
    harness.run();
    harness.get_by_label("Load into this window").click();
    harness.run();
    assert!(has_model(&harness, "mine.lamp"));
    assert!(
        harness
            .query_by_label_contains("is in the palette")
            .is_some()
    );

    // Loading it again replaces the library of the same namespace.
    harness.get_by_label("Load into this window").click();
    harness.run();
    assert_eq!(harness.state().libraries().len(), 1);
}
