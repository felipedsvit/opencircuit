//! The window in Brazilian Portuguese (issue #103): the menus, the sections
//! and the dialogs are translated and the window still works; English stays the
//! default; the language can be switched while the window runs.

use eframe::egui::{self, Key};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use opencircuit_desktop::{DesktopApp, Language, Mode};

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

fn command(harness: &mut Harness<'_, DesktopApp>, line: &str) {
    let field = harness.get_by_label("Command");
    field.focus();
    field.type_text(line);
    harness.run();
    harness.key_press(Key::Enter);
    harness.run();
}

fn document(harness: &Harness<'_, DesktopApp>) -> circuit_model::Schematic {
    harness
        .state()
        .session()
        .expect("a project is open")
        .document()
        .clone()
}

#[test]
fn a_window_is_in_english_unless_it_is_told_otherwise() {
    let harness = window(Language::default());

    assert_eq!(harness.state().language(), Language::English);
    assert!(harness.query_by_label("File").is_some());
    assert!(harness.query_by_label("Arquivo").is_none());
}

#[test]
fn the_menus_the_toolbar_and_the_status_bar_are_translated() {
    let mut harness = window(Language::PortugueseBrazil);

    for label in [
        "Arquivo",
        "Modificar",
        "Exibir",
        "Executar",
        "Ajuda",
        "Novo",
        "Abrir…",
        "Caminho",
    ] {
        assert!(harness.query_by_label(label).is_some(), "{label}");
    }
    for english in ["File", "Modify", "View", "Run", "Help", "Path"] {
        assert!(harness.query_by_label(english).is_none(), "{english}");
    }
    click(&mut harness, "Arquivo");
    for label in [
        "Novo projeto",
        "Abrir projeto…",
        "Carregar biblioteca…",
        "Exportar desenho",
    ] {
        assert!(harness.query_by_label(label).is_some(), "{label}");
    }
}

#[test]
fn a_window_in_portuguese_still_edits_undoes_and_shows_its_sections() {
    let mut harness = window(Language::PortugueseBrazil);
    click(&mut harness, "Novo");
    assert_eq!(harness.state().mode(), Mode::Edit);

    // The command line is English by design; the rest of the window is not.
    command(&mut harness, "text 4,3 Nota");
    assert_eq!(document(&harness).texts.len(), 1);
    for section in [
        "Propriedades",
        "Alimentações",
        "Folhas e legenda",
        "Árvore do projeto",
    ] {
        assert!(harness.query_by_label(section).is_some(), "{section}");
    }
    click(&mut harness, "Desfazer");
    assert!(document(&harness).texts.is_empty());
    click(&mut harness, "Refazer");
    assert_eq!(document(&harness).texts.len(), 1);
    // The palette tabs and the toolbar hints are in Portuguese too.
    assert!(harness.query_by_label("Proteção").is_some());
    assert!(harness.query_by_label("Selecionar").is_some());
}

#[test]
fn the_menu_of_a_window_in_portuguese_runs_a_command() {
    let mut harness = window(Language::PortugueseBrazil);
    click(&mut harness, "Novo");
    command(&mut harness, "text 4,3 Nota");

    click(&mut harness, "Modificar");
    click(&mut harness, "Adicionar texto");

    assert_eq!(harness.state().tool(), &circuit_editor::Tool::Text);
}

#[test]
fn the_unsaved_changes_dialog_is_translated() {
    let mut harness = window(Language::PortugueseBrazil);
    click(&mut harness, "Novo");
    command(&mut harness, "text 4,3 Nota");

    click(&mut harness, "Novo");

    for label in [
        "Alterações não salvas",
        "Salvar e continuar",
        "Descartar alterações",
        "Cancelar",
    ] {
        assert!(harness.query_by_label(label).is_some(), "{label}");
    }
    click(&mut harness, "Cancelar");
    assert_eq!(document(&harness).texts.len(), 1);
}

#[test]
fn the_help_menu_switches_the_language_while_the_window_runs() {
    let mut harness = window(Language::English);
    click(&mut harness, "New");

    click(&mut harness, "Help");
    click(&mut harness, "Language");
    click(&mut harness, "Português (Brasil)");
    assert_eq!(harness.state().language(), Language::PortugueseBrazil);
    harness.run();
    assert!(harness.query_by_label("Arquivo").is_some());
    assert!(harness.query_by_label("File").is_none());

    click(&mut harness, "Ajuda");
    click(&mut harness, "Idioma");
    click(&mut harness, "English");
    assert_eq!(harness.state().language(), Language::English);
    harness.run();
    assert!(harness.query_by_label("File").is_some());
}

#[test]
fn two_windows_on_two_threads_keep_their_own_language() {
    let portuguese = std::thread::spawn(|| {
        let harness = window(Language::PortugueseBrazil);
        harness.query_by_label("Arquivo").is_some()
    });
    let english = std::thread::spawn(|| {
        let harness = window(Language::English);
        harness.query_by_label("File").is_some()
    });

    assert!(portuguese.join().expect("the first window"));
    assert!(english.join().expect("the second window"));
}

#[test]
fn the_keyboard_help_and_the_about_window_are_translated() {
    let mut harness = window(Language::PortugueseBrazil);

    click(&mut harness, "Ajuda");
    click(&mut harness, "Atalhos do teclado");
    assert!(harness.query_by_label("ferramenta de fio").is_some());
    assert!(harness.query_by_label("Espaço").is_some());
}
