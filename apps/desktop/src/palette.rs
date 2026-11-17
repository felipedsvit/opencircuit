//! The palette: the editing tools and the parts of the catalog, in one tab
//! for each kind of part, each part drawn as it will look on the sheet.

use circuit_editor::{Editor, Tool};
use circuit_model::{
    Category, ComponentCatalog, ComponentModel, DeviceClass, ModelRef, QuarterTurn,
};
use circuit_renderer::{Scene, ScenePoint, Shape, symbol_preview};
use eframe::egui::{self, Align2, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Ui};

use crate::i18n::{tr, trf};
use crate::icons::{self, Icon};
use crate::theme::{self, Tokens};

/// The keyboard of the editor, listed in the Help menu.
pub(crate) const KEYS: &[(&str, &str)] = &[
    ("W", "wire tool"),
    ("Space", "turn the next bend the other way"),
    ("Esc", "cancel, then select tool, then clear"),
    ("Arrows", "move the selection (Shift: 5)"),
    ("R", "rotate"),
    ("Del", "delete"),
    ("Ctrl+A", "select all"),
    ("Ctrl+C / Ctrl+V", "copy / paste"),
    ("Ctrl+D", "duplicate"),
    ("Ctrl+Z / Ctrl+Y", "undo / redo"),
    ("+ / -", "zoom"),
    ("0", "fit the drawing"),
    ("F", "fit the selection"),
    ("T", "trace the net of the selection"),
    ("Ctrl+K", "command palette: run a command, find an element"),
    ("Ctrl+H", "find and replace"),
    ("F2", "edit the label of the selection"),
    ("G", "type the point of the next click"),
    ("Alt+Left", "previous view"),
];

/// Width and height of the cell of one symbol, in points.
const CELL: egui::Vec2 = egui::vec2(70.0, 62.0);
/// Height of the drawing in a cell, in points.
const DRAWING_HEIGHT: f32 = 40.0;
/// Most points one subunit of a drawing takes: small symbols are not blown
/// up beyond what a sheet shows at the default zoom.
const MAX_SCALE: f32 = 0.9;

/// What the palette remembers between frames.
#[derive(Debug)]
pub struct PaletteState {
    /// The tab that is open.
    tab: Category,
    /// What the user is looking for; all tabs are searched when it is not
    /// empty.
    search: String,
    /// The parts chosen most recently, newest first, kept in this window only.
    recent: Vec<(ModelRef, circuit_model::SymbolKey)>,
}

/// Most parts the recent strip remembers.
const MAX_RECENT: usize = 8;

impl Default for PaletteState {
    fn default() -> Self {
        Self {
            tab: Category::Controls,
            search: String::new(),
            recent: Vec::new(),
        }
    }
}

impl PaletteState {
    /// The tab that is open.
    #[must_use]
    pub const fn tab(&self) -> Category {
        self.tab
    }
}

const fn tab_icon(category: Category) -> Icon {
    match category {
        Category::Supplies => Icon::CategorySupplies,
        Category::Protection => Icon::CategoryProtection,
        Category::Controls => Icon::CategoryControls,
        Category::Motors => Icon::CategoryMotors,
        Category::Contacts => Icon::CategoryContacts,
        Category::Automation => Icon::CategoryAutomation,
        Category::Actuators => Icon::CategoryActuators,
        Category::Lines => Icon::CategoryLines,
    }
}

/// Whether `model` or one of its symbols matches what is searched for.
fn matches(model: &ComponentModel, needle: &str) -> bool {
    let has = |text: &str| text.to_lowercase().contains(needle);
    has(model.id())
        || has(model.title())
        || model.symbols().iter().any(|symbol| has(&symbol.key().0))
}

/// Draw the palette; choosing a part changes the tool of the editor.
pub fn show(
    ui: &mut Ui,
    catalog: &ComponentCatalog,
    editor: &mut Editor,
    state: &mut PaletteState,
) {
    let tool = editor.tool().clone();
    ui.horizontal(|ui| {
        if icons::button_hint(
            ui,
            Icon::Select,
            tr("Select"),
            tr("Select and move (Esc)"),
            tool == Tool::Select,
        )
        .clicked()
        {
            editor.set_tool(Tool::Select);
        }
        if icons::button_hint(
            ui,
            Icon::Wire,
            tr("Wire"),
            tr("Draw a wire (W)"),
            tool == Tool::Wire,
        )
        .clicked()
        {
            editor.set_tool(Tool::Wire);
        }
    });
    ui.separator();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        for category in Category::ALL {
            let count = catalog
                .latest()
                .filter(|model| model.category() == category)
                .count();
            let label = tr(category.label());
            let hint = trf("{0} ({1})", &[&label, &count]);
            let open = state.search.is_empty() && state.tab == category;
            if icons::button_hint(ui, tab_icon(category), label, &hint, open).clicked() {
                state.tab = category;
                state.search.clear();
            }
        }
    });
    ui.horizontal(|ui| {
        let name = ui.label(tr("Search"));
        ui.add(
            egui::TextEdit::singleline(&mut state.search)
                .hint_text(tr("title, model or symbol"))
                .desired_width(f32::INFINITY),
        )
        .labelled_by(name.id);
    });
    let needle = state.search.trim().to_lowercase();
    let mut models: Vec<&ComponentModel> = catalog
        .latest()
        .filter(|model| {
            if needle.is_empty() {
                model.category() == state.tab
            } else {
                matches(model, &needle)
            }
        })
        .collect();
    models.sort_by(|a, b| (a.title(), a.id()).cmp(&(b.title(), b.id())));
    recent_strip(ui, catalog, editor, state);
    let place = if needle.is_empty() {
        tr(state.tab.label())
    } else {
        tr("Search results")
    };
    ui.strong(trf("{0}: {1} parts", &[&place, &models.len()]));
    if models.is_empty() {
        ui.label(tr("Nothing here."));
        return;
    }
    egui::ScrollArea::vertical().show(ui, |ui| {
        for model in models {
            part(ui, model, editor, &tool, &mut state.recent);
        }
    });
}

/// Put a chosen part first in the recent list, once, and keep the newest.
fn remember(
    recent: &mut Vec<(ModelRef, circuit_model::SymbolKey)>,
    model: &ModelRef,
    symbol: &circuit_model::SymbolKey,
) {
    recent.retain(|(m, s)| !(m == model && s == symbol));
    recent.insert(0, (model.clone(), symbol.clone()));
    recent.truncate(MAX_RECENT);
}

/// The parts chosen most recently, as one small button each under the search
/// field; nothing at all until a part has been chosen.
fn recent_strip(
    ui: &mut Ui,
    catalog: &ComponentCatalog,
    editor: &mut Editor,
    state: &mut PaletteState,
) {
    let shown: Vec<(ModelRef, circuit_model::SymbolKey, String)> = state
        .recent
        .iter()
        .filter_map(|(model, symbol)| {
            let found = catalog.model(model)?;
            found.symbol(symbol)?;
            Some((model.clone(), symbol.clone(), found.title().to_owned()))
        })
        .collect();
    if shown.is_empty() {
        return;
    }
    let tokens = Tokens::of(ui);
    ui.horizontal_wrapped(|ui| {
        ui.label(
            egui::RichText::new(tr("Recent"))
                .small()
                .color(tokens.text_dim),
        );
        for (model, symbol, title) in shown {
            let label = trf("Recent {0} · {1}", &[&model.id, &symbol.0]);
            let response = ui.small_button(title);
            let enabled = ui.is_enabled();
            response.widget_info(|| {
                egui::WidgetInfo::labeled(egui::Role::Button, enabled, label.as_str())
            });
            if response.clicked() {
                remember(&mut state.recent, &model, &symbol);
                editor.set_tool(Tool::Place {
                    model,
                    symbol,
                    rotation: QuarterTurn::Deg0,
                });
            }
        }
    });
}

/// One model: its title and one cell for each of its symbols.
fn part(
    ui: &mut Ui,
    model: &ComponentModel,
    editor: &mut Editor,
    tool: &Tool,
    recent: &mut Vec<(ModelRef, circuit_model::SymbolKey)>,
) {
    let tokens = Tokens::of(ui);
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(model.title()).strong());
        if model.class() == DeviceClass::Symbolic {
            ui.label(egui::RichText::new(tr("no sim")).small().color(tokens.warn))
                .on_hover_text(tr(
                    "Drawn and wired only: it has no behaviour in the simulation.",
                ));
        }
    });
    let provenance = model.provenance();
    let mut hover = format!(
        "{}\n{} · {}",
        model.title(),
        provenance.origin,
        provenance.license
    );
    if let Some(note) = model.note() {
        hover.push('\n');
        hover.push_str(note);
    }
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
        for symbol in model.symbols() {
            let current = matches!(
                tool,
                Tool::Place { model: m, symbol: s, .. }
                    if m.id == model.id() && m.version == model.version() && s == symbol.key()
            );
            let label = format!("{} · {}", model.id(), symbol.key().0);
            let scene = symbol_preview(model, symbol.key());
            let clicked = cell(ui, &label, &symbol.key().0, scene.as_ref(), current)
                .on_hover_text(&hover)
                .clicked();
            if clicked {
                let chosen = ModelRef {
                    id: model.id().to_owned(),
                    version: model.version(),
                };
                remember(recent, &chosen, symbol.key());
                editor.set_tool(Tool::Place {
                    model: chosen,
                    symbol: symbol.key().clone(),
                    rotation: QuarterTurn::Deg0,
                });
            }
        }
    });
}

/// The button of one symbol: its drawing, with its key under it.
fn cell(
    ui: &mut Ui,
    label: &str,
    caption: &str,
    scene: Option<&Scene>,
    selected: bool,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(CELL, Sense::click());
    let enabled = ui.is_enabled();
    response
        .widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, enabled, selected, label));
    if !ui.is_rect_visible(rect) {
        return response;
    }
    let tokens = Tokens::of(ui);
    let fill = if selected {
        tokens.tab_active
    } else if response.hovered() {
        tokens.control_hover
    } else {
        tokens.control
    };
    let painter = ui.painter();
    painter.rect_filled(rect, 4.0, fill);
    if selected {
        painter.rect_stroke(
            rect,
            4.0,
            Stroke::new(1.0, tokens.accent),
            StrokeKind::Inside,
        );
    }
    let area = Rect::from_min_size(
        rect.min + egui::vec2(4.0, 3.0),
        egui::vec2(CELL.x - 8.0, DRAWING_HEIGHT),
    );
    if let Some(scene) = scene {
        paint_preview(ui, area, scene);
    }
    ui.painter().text(
        Pos2::new(rect.center().x, rect.max.y - 8.0),
        Align2::CENTER_CENTER,
        shorten(caption, 12),
        FontId::proportional(10.0),
        tokens.text_dim,
    );
    response
}

/// At most `most` characters of `text`, with an ellipsis when it is cut.
fn shorten(text: &str, most: usize) -> String {
    if text.chars().count() <= most {
        text.to_owned()
    } else {
        let kept: String = text.chars().take(most.saturating_sub(1)).collect();
        format!("{kept}…")
    }
}

/// Paint `scene`, centred in `area` and scaled to fit it.
fn paint_preview(ui: &Ui, area: Rect, scene: &Scene) {
    let Some(bounds) = scene.bounds else {
        return;
    };
    let dark = ui.visuals().dark_mode;
    let painter = ui.painter().with_clip_rect(area.expand(2.0));
    let width = to_points(bounds.max.x.saturating_sub(bounds.min.x)).max(1.0);
    let height = to_points(bounds.max.y.saturating_sub(bounds.min.y)).max(1.0);
    let scale = (area.width() / width)
        .min(area.height() / height)
        .min(MAX_SCALE);
    let middle = ScenePoint::new(
        bounds.min.x.saturating_add(bounds.max.x) / 2,
        bounds.min.y.saturating_add(bounds.max.y) / 2,
    );
    let to_screen = |point: ScenePoint| {
        area.center()
            + egui::vec2(
                to_points(point.x.saturating_sub(middle.x)) * scale,
                to_points(point.y.saturating_sub(middle.y)) * scale,
            )
    };
    for item in &scene.items {
        let look = theme::look(item.style, dark);
        let stroke = Stroke::new(look.width.min(1.4), look.color);
        match &item.shape {
            Shape::Line { from, to } => {
                painter.line_segment([to_screen(*from), to_screen(*to)], stroke);
            }
            Shape::Polyline(points) => {
                painter.line(points.iter().map(|p| to_screen(*p)).collect(), stroke);
            }
            Shape::Rect(rect) => {
                painter.rect_stroke(
                    Rect::from_two_pos(to_screen(rect.min), to_screen(rect.max)),
                    0.0,
                    stroke,
                    StrokeKind::Middle,
                );
            }
            Shape::Circle { center, radius } => {
                painter.circle_stroke(to_screen(*center), to_points(*radius) * scale, stroke);
            }
            Shape::Dot { center, radius } => {
                painter.circle_filled(
                    to_screen(*center),
                    (to_points(*radius) * scale).max(1.2),
                    look.color,
                );
            }
            Shape::Text { at, text, .. } => {
                painter.text(
                    to_screen(*at),
                    Align2::CENTER_CENTER,
                    text,
                    FontId::proportional(8.5),
                    look.color,
                );
            }
        }
    }
}

/// A length in subunits as points of an unscaled drawing.
#[allow(clippy::cast_precision_loss)]
fn to_points(subunits: i64) -> f32 {
    subunits as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_caption_is_cut_with_an_ellipsis_and_never_in_the_middle_of_a_character() {
        assert_eq!(shorten("coil", 12), "coil");
        assert_eq!(shorten("abcdefghijklmnop", 12), "abcdefghijk…");
        assert_eq!(shorten("ééééééééééééé", 12), "ééééééééééé…");
        assert_eq!(shorten("", 12), "");
    }

    #[test]
    fn the_search_looks_at_the_title_the_id_and_the_symbols() {
        let catalog = ComponentCatalog::builtin();
        let find = |needle: &str| -> Vec<String> {
            catalog
                .latest()
                .filter(|model| matches(model, needle))
                .map(|model| model.id().to_owned())
                .collect()
        };

        assert_eq!(find("emergency"), ["estop.latching"]);
        assert!(find("aux_no").contains(&"contactor.generic".to_owned()));
        assert!(find("motor.3ph").len() >= 2);
        assert!(find("zzz").is_empty());
    }

    #[test]
    fn every_category_has_an_icon_and_the_default_tab_has_parts() {
        let catalog = ComponentCatalog::builtin();
        for category in Category::ALL {
            let _ = tab_icon(category);
        }

        assert!(
            catalog
                .latest()
                .any(|model| model.category() == PaletteState::default().tab())
        );
    }
}
