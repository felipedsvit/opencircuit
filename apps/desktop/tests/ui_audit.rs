//! The interface audit of M9.5: what the window remembers while it runs does
//! not depend on the language it speaks, and nothing is pushed out of a small
//! window by the longer words of Portuguese.

use eframe::egui::{self, Id};
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
    harness.get_by_label(label).scroll_to_me();
    harness.run();
    harness.get_by_label(label).click();
    harness.run();
}

/// The command line is English by design, its label included (ADR-0017): a
/// window in Portuguese is still driven by the word `Command`.
#[test]
fn the_command_line_prompt_stays_in_english_in_a_window_in_portuguese() {
    let mut harness = window(Language::PortugueseBrazil);
    click(&mut harness, "Novo");

    assert!(harness.query_by_label("Command").is_some());
    assert!(harness.query_by_label("Comando").is_none());

    let field = harness.get_by_label("Command");
    field.focus();
    field.type_text("zoom e");
    harness.run();
    assert_eq!(
        harness.get_by_label("Command").value().as_deref(),
        Some("zoom e")
    );
}

#[test]
fn an_open_section_stays_open_when_the_language_changes() {
    let mut harness = window(Language::English);
    click(&mut harness, "New");
    click(&mut harness, "Project tree");
    assert!(harness.query_by_label("Layers").is_some());

    harness.state_mut().set_language(Language::PortugueseBrazil);
    harness.run();

    assert!(harness.query_by_label("Project tree").is_none());
    assert!(
        harness.query_by_label("Camadas").is_some(),
        "the section was closed by the change of language"
    );
}

#[test]
fn a_window_keeps_its_place_in_every_language() {
    for language in [Language::English, Language::PortugueseBrazil] {
        let mut harness = window(language);
        click(
            &mut harness,
            if language == Language::English {
                "Help"
            } else {
                "Ajuda"
            },
        );
        click(
            &mut harness,
            if language == Language::English {
                "About OpenCircuit"
            } else {
                "Sobre o OpenCircuit"
            },
        );

        let placed = harness
            .ctx
            .memory(|memory| memory.area_rect(Id::unique("About OpenCircuit")));
        assert!(
            placed.is_some(),
            "the About window of {language:?} is not under its own identifier"
        );
    }
}

/// Portuguese is longer than English; neither may push a control past the
/// edge of a small window. What is below the bottom edge is in a scroll area
/// and is not looked at.
#[test]
fn no_control_is_pushed_past_the_side_of_a_small_window() {
    for language in [Language::English, Language::PortugueseBrazil] {
        for (width, height) in [(800.0, 600.0), (1024.0, 700.0), (1366.0, 768.0)] {
            let mut app = DesktopApp::default();
            app.set_language(language);
            let mut harness = Harness::new_ui_state(|ui, app: &mut DesktopApp| app.show(ui), app);
            harness.set_size(egui::Vec2::new(width, height));
            harness.run();
            click(
                &mut harness,
                if language == Language::English {
                    "New"
                } else {
                    "Novo"
                },
            );

            let mut outside = Vec::new();
            for node in harness.root().children_recursive() {
                let Some(label) = node.accesskit_node().label() else {
                    continue;
                };
                let rect = node.rect();
                let on_screen = rect.min.y >= 0.0 && rect.min.y < height;
                if on_screen && (rect.max.x > width + 0.5 || rect.min.x < -0.5) {
                    outside.push(format!("{label:?} {rect:?}"));
                }
            }
            assert!(
                outside.is_empty(),
                "{language:?} at {width}x{height}: {outside:?}"
            );
        }
    }
}
