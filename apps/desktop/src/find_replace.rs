//! Find and replace (M9.2, issue #118): a window that replaces a text in the
//! designators, the text notes and the text parameters of the project, in
//! one step that one undo takes back. The replacement is built by
//! `circuit_commands::find::replace`, which refuses a result that is not
//! valid.

use circuit_commands::Transaction;
use circuit_commands::find::{Fields, replace};
use circuit_model::{ComponentCatalog, Schematic};
use eframe::egui::{self, Ui};

use crate::i18n::{tr, trf};
use crate::inspector::Request;

/// The window between frames.
#[derive(Debug)]
pub struct FindReplace {
    open: bool,
    find: String,
    with: String,
    fields: Fields,
}

impl Default for FindReplace {
    fn default() -> Self {
        Self {
            open: false,
            find: String::new(),
            with: String::new(),
            fields: Fields::ALL,
        }
    }
}

impl FindReplace {
    /// Open the window, keeping what was typed last.
    pub fn open(&mut self) {
        self.open = true;
    }

    /// Whether the window is open.
    #[must_use]
    pub const fn is_open(&self) -> bool {
        self.open
    }

    /// Draw the window while it is open; the replacement when the person
    /// asks for it.
    pub fn show(
        &mut self,
        ui: &Ui,
        doc: &Schematic,
        catalog: &ComponentCatalog,
    ) -> Option<Request> {
        if !self.open {
            return None;
        }
        let mut open = true;
        let mut request = None;
        egui::Window::new(tr("Find and replace"))
            .id(egui::Id::unique("Find and replace"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ui.ctx(), |ui| {
                egui::Grid::new("find and replace fields")
                    .num_columns(2)
                    .show(ui, |ui| {
                        let label = ui.label(tr("Find"));
                        ui.text_edit_singleline(&mut self.find)
                            .labelled_by(label.id);
                        ui.end_row();
                        let label = ui.label(tr("Replace with"));
                        ui.text_edit_singleline(&mut self.with)
                            .labelled_by(label.id);
                        ui.end_row();
                    });
                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.fields.designators, tr("Designators"));
                    ui.checkbox(&mut self.fields.notes, tr("Text notes"));
                    ui.checkbox(&mut self.fields.parameters, tr("Parameters"));
                });
                ui.weak(tr("Matches the text exactly, with its case."));
                let preview: Result<Transaction, _> =
                    replace(doc, catalog, self.fields, &self.find, &self.with);
                let count = preview.as_ref().map_or(0, |tx| tx.edits.len());
                match &preview {
                    Ok(_) => {
                        ui.label(trf("{0} changes", &[&count]));
                    }
                    Err(error) => {
                        ui.colored_label(ui.visuals().error_fg_color, error.to_string());
                    }
                }
                if ui
                    .add_enabled(count > 0, egui::Button::new(tr("Replace all")))
                    .clicked()
                {
                    request = Some(preview);
                }
            });
        if !open {
            self.open = false;
        }
        request
    }
}
