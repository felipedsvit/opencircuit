//! M9.2 (#118): the print preview shows the pages of the PDF, one per sheet.

use std::path::Path;

use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use opencircuit_desktop::DesktopApp;

fn window(example: &str) -> Harness<'static, DesktopApp> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples")
        .join(example);
    let mut harness = Harness::new_ui_state(
        |ui, app: &mut DesktopApp| app.show(ui),
        DesktopApp::new(Some(&path)),
    );
    harness.set_size(egui::Vec2::new(1300.0, 900.0));
    harness.run();
    harness
}

#[test]
fn the_preview_has_a_page_per_sheet_and_turns_them() {
    let mut harness = window("sheets/starter-sheets.ocircuit");

    harness.get_by_label("File").click();
    harness.run();
    harness.get_by_label("Print preview…").click();
    harness.run();
    assert_eq!(harness.state().print_preview_page(), Some((0, 2)));
    harness.get_by_label("Page 1 of 2");

    harness.get_by_label("Next page").click();
    harness.run();
    assert_eq!(harness.state().print_preview_page(), Some((1, 2)));
    harness.get_by_label("Page 2 of 2");

    harness.get_by_label("Previous page").click();
    harness.run();
    assert_eq!(harness.state().print_preview_page(), Some((0, 2)));
}

#[test]
fn a_drawing_without_sheets_is_one_page() {
    let mut harness = window("direct-starter/starter.ocircuit");
    harness.state_mut().open_print_preview();
    harness.run();

    assert_eq!(harness.state().print_preview_page(), Some((0, 1)));
}
