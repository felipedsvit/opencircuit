//! The palette: the editing tools and every symbol the catalog offers, in
//! the newest version of each model.

use circuit_editor::{Editor, Tool};
use circuit_model::{ComponentCatalog, ModelRef, QuarterTurn};
use eframe::egui::{self, Ui};

/// Draw the palette; choosing an entry changes the tool of the editor.
pub fn show(ui: &mut Ui, catalog: &ComponentCatalog, editor: &mut Editor) {
    ui.strong("Tools");
    let tool = editor.tool().clone();
    if ui
        .selectable_label(tool == Tool::Select, "Select")
        .clicked()
    {
        editor.set_tool(Tool::Select);
    }
    if ui.selectable_label(tool == Tool::Wire, "Wire").clicked() {
        editor.set_tool(Tool::Wire);
    }
    ui.separator();
    ui.strong("Components");
    egui::ScrollArea::vertical().show(ui, |ui| {
        for model in catalog.latest() {
            for symbol in model.symbols() {
                let current = matches!(
                    &tool,
                    Tool::Place { model: m, symbol: s, .. }
                        if m.id == model.id() && m.version == model.version() && s == symbol.key()
                );
                let label = format!("{} · {}", model.id(), symbol.key().0);
                let provenance = model.provenance();
                let hover = format!("{} · {}", provenance.origin, provenance.license);
                if ui
                    .selectable_label(current, label)
                    .on_hover_text(hover)
                    .clicked()
                {
                    editor.set_tool(Tool::Place {
                        model: ModelRef {
                            id: model.id().to_owned(),
                            version: model.version(),
                        },
                        symbol: symbol.key().clone(),
                        rotation: QuarterTurn::Deg0,
                    });
                }
            }
        }
    });
}
