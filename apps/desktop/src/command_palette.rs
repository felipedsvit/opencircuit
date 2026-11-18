//! The command palette (M9.2, issue #118): Ctrl+K opens a field over the
//! drawing that runs a command of the window by name, or finds an element of
//! the project (`circuit_commands::find`) and goes to it.

use circuit_commands::find::{Found, find};
use circuit_model::{ComponentCatalog, Schematic, Subject};
use eframe::egui::{self, Ui};

use crate::i18n::tr;

/// Most elements of the project listed at once.
const MAX_FOUND: usize = 30;

/// A command of the window the palette runs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Command {
    New,
    Open,
    Save,
    SaveAs,
    SaveCopy,
    Undo,
    Redo,
    Fit,
    Edit,
    Simulate,
    ExportSvg,
    ExportPdf,
    Library,
    Keys,
}

/// Every command, with the text the menus already use for it.
pub(crate) const COMMANDS: [(Command, &str); 14] = [
    (Command::New, "New project"),
    (Command::Open, "Open project…"),
    (Command::Save, "Save project"),
    (Command::SaveAs, "Save project as…"),
    (Command::SaveCopy, "Save a copy…"),
    (Command::Undo, "Undo last change"),
    (Command::Redo, "Redo change"),
    (Command::Fit, "Fit"),
    (Command::Edit, "Edit"),
    (Command::Simulate, "Simulate"),
    (Command::ExportSvg, "Export drawing as SVG…"),
    (Command::ExportPdf, "Export drawing as PDF…"),
    (Command::Library, "Load library…"),
    (Command::Keys, "Keyboard shortcuts"),
];

/// What the person chose in the palette.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Choice {
    /// Run a command.
    Command(Command),
    /// Go to an element of the project.
    Element(Subject),
}

/// The palette between frames.
#[derive(Debug, Default)]
pub struct PaletteState {
    open: bool,
    /// The field has not been focused since the palette opened.
    focus_pending: bool,
    query: String,
    /// The highlighted line, from 0.
    chosen: usize,
}

/// One line of the list.
enum Line {
    Command(Command, String),
    Element(Found),
}

impl PaletteState {
    /// Open the palette with an empty field.
    pub fn open(&mut self) {
        self.open = true;
        self.focus_pending = true;
        self.query.clear();
        self.chosen = 0;
    }

    /// Whether the palette is open.
    #[must_use]
    pub const fn is_open(&self) -> bool {
        self.open
    }

    fn lines(&self, doc: Option<&Schematic>, catalog: Option<&ComponentCatalog>) -> Vec<Line> {
        let needle = self.query.trim().to_lowercase();
        let commands = COMMANDS
            .iter()
            .map(|(command, key)| (*command, tr(key).to_owned()))
            .filter(|(_, label)| label.to_lowercase().contains(&needle))
            .map(|(command, label)| Line::Command(command, label));
        let found = match (doc, catalog) {
            (Some(doc), Some(catalog)) => find(doc, catalog, &self.query, MAX_FOUND),
            _ => Vec::new(),
        };
        commands
            .chain(
                found
                    .into_iter()
                    // A layer is chosen in the project tree, not gone to.
                    .filter(|found| !matches!(found.subject, Subject::Layer(_)))
                    .map(Line::Element),
            )
            .collect()
    }

    /// Draw the palette while it is open, and return what was chosen.
    pub fn show(
        &mut self,
        ui: &Ui,
        doc: Option<&Schematic>,
        catalog: Option<&ComponentCatalog>,
    ) -> Option<Choice> {
        if !self.open {
            return None;
        }
        let lines = self.lines(doc, catalog);
        let last = lines.len().saturating_sub(1);
        let (down, up, enter, escape) = ui.input_mut(|input| {
            (
                input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown),
                input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp),
                input.consume_key(egui::Modifiers::NONE, egui::Key::Enter),
                input.consume_key(egui::Modifiers::NONE, egui::Key::Escape),
            )
        });
        if down {
            self.chosen = self.chosen.saturating_add(1).min(last);
        }
        if up {
            self.chosen = self.chosen.saturating_sub(1);
        }
        self.chosen = self.chosen.min(last);
        let mut choice = None;
        let mut open = true;
        egui::Window::new(tr("Command palette"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_TOP, [0.0, 60.0])
            .default_width(420.0)
            .show(ui.ctx(), |ui| {
                let prompt = ui.label(tr("Find or run"));
                let field = ui
                    .add(
                        egui::TextEdit::singleline(&mut self.query)
                            .id_salt("command palette input")
                            .hint_text(tr("A command, a designator, a wire number, a text…"))
                            .desired_width(f32::INFINITY),
                    )
                    .labelled_by(prompt.id);
                if std::mem::take(&mut self.focus_pending) {
                    field.request_focus();
                }
                if field.changed() {
                    self.chosen = 0;
                }
                if lines.is_empty() {
                    ui.weak(tr("Nothing found"));
                }
                egui::ScrollArea::vertical()
                    .max_height(320.0)
                    .show(ui, |ui| {
                        for (index, line) in lines.iter().enumerate() {
                            let (text, picked) = match line {
                                Line::Command(command, label) => {
                                    (label.clone(), Choice::Command(*command))
                                }
                                Line::Element(found) => {
                                    let mut text = format!("{}  {}", found.label, found.detail);
                                    text.truncate(text.trim_end().len());
                                    (
                                        format!("{} · {text}", tr(kind_label(found.kind))),
                                        Choice::Element(found.subject.clone()),
                                    )
                                }
                            };
                            let response = ui.selectable_label(index == self.chosen, text);
                            if index == self.chosen && (down || up) {
                                response.scroll_to_me(None);
                            }
                            if response.clicked() {
                                choice = Some(picked);
                            }
                        }
                    });
            });
        if enter && choice.is_none() {
            choice = lines.get(self.chosen).map(|line| match line {
                Line::Command(command, _) => Choice::Command(*command),
                Line::Element(found) => Choice::Element(found.subject.clone()),
            });
        }
        if choice.is_some() || escape || !open {
            self.open = false;
        }
        choice
    }
}

/// The names of the kinds of element, to translate; the last one names an
/// element of no other kind.
pub(crate) const KINDS: [(&str, &str); 5] = [
    ("device", "Device"),
    ("wire", "Wire"),
    ("sheet", "Sheet"),
    ("text", "Text"),
    ("", "Element"),
];

/// The name of a kind of element, to translate.
fn kind_label(kind: &str) -> &'static str {
    KINDS
        .iter()
        .find(|(name, _)| *name == kind)
        .map_or("Element", |(_, label)| label)
}
