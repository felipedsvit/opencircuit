//! The update notice in the window (ADR-0020): the chip in the status bar, the
//! dialog that offers a version, the Help entry and the Updates window. The
//! windows here are built without the update check, so nothing goes to the
//! network; a version is offered by hand, as a check would have found it.

#![cfg(not(target_arch = "wasm32"))]

use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use opencircuit_desktop::{DesktopApp, Language};

fn window(language: Language, offered: Option<&str>) -> Harness<'static, DesktopApp> {
    let mut app = DesktopApp::default();
    app.set_language(language);
    if let Some(version) = offered {
        app.offer_update(version);
    }
    let mut harness = Harness::new_ui_state(|ui, app: &mut DesktopApp| app.show(ui), app);
    harness.set_size(egui::Vec2::new(1300.0, 900.0));
    harness.run();
    harness
}

fn click(harness: &mut Harness<'_, DesktopApp>, label: &str) {
    harness.get_by_label(label).scroll_to_me();
    harness.run();
    harness.get_by_label(label).click();
    harness.run();
}

#[test]
fn a_fresh_window_shows_no_update_and_help_has_the_entry() {
    let mut harness = window(Language::default(), None);

    assert!(harness.query_by_label("Version 0.0.3 available").is_none());
    click(&mut harness, "Help");
    assert!(harness.query_by_label("Updates...").is_some());
}

#[test]
fn an_offered_version_shows_a_chip_that_opens_the_dialog_with_its_choices() {
    let mut harness = window(Language::default(), Some("0.0.3"));

    click(&mut harness, "Version 0.0.3 available");

    assert!(harness.query_by_label("Download 0.0.3").is_some());
    assert!(harness.query_by_label("Remind me in 5 days").is_some());
    assert!(harness.query_by_label("Skip this version").is_some());
}

#[test]
fn skipping_a_version_hides_its_chip() {
    let mut harness = window(Language::default(), Some("0.0.3"));
    click(&mut harness, "Version 0.0.3 available");

    click(&mut harness, "Skip this version");

    assert!(harness.query_by_label("Version 0.0.3 available").is_none());
}

#[test]
fn the_updates_window_offers_frequency_and_keeps_automatic_install_off() {
    let mut harness = window(Language::default(), None);
    click(&mut harness, "Help");
    click(&mut harness, "Updates...");

    for label in ["Every 3 days", "Every 5 days", "Never", "Check now"] {
        assert!(harness.query_by_label(label).is_some(), "{label}");
    }
    let install = harness.get_by_label("Install automatically");
    assert!(
        install.accesskit_node().is_disabled(),
        "automatic install waits for signed releases"
    );
}

#[test]
fn never_hides_the_chip_of_an_offered_version() {
    let mut harness = window(Language::default(), Some("0.0.3"));
    click(&mut harness, "Help");
    click(&mut harness, "Updates...");

    click(&mut harness, "Never");

    assert!(harness.query_by_label("Version 0.0.3 available").is_none());
}

#[test]
fn in_portuguese_the_chip_and_the_menu_entry_are_translated() {
    let mut harness = window(Language::PortugueseBrazil, Some("0.0.3"));
    assert!(harness.query_by_label("Versão 0.0.3 disponível").is_some());

    click(&mut harness, "Ajuda");
    assert!(harness.query_by_label("Atualizações...").is_some());
}
