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
    ("M", "mirror"),
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

/// Width and height of the cell of one symbol at its full size, in points:
/// the most a cell is ever drawn.
const CELL: egui::Vec2 = egui::vec2(70.0, 62.0);
/// Height of the drawing in a cell at its full size, in points.
const DRAWING_HEIGHT: f32 = 40.0;
/// Most cells in a row.
const MAX_COLUMNS: usize = 4;
/// Space between cells, in points.
const GAP: f32 = 4.0;
/// Narrowest a cell is drawn, in points (about half its full size): the
/// column cannot be made narrower than four of them, so a row never loses a
/// cell to a line break, it shrinks.
const MIN_CELL_WIDTH: f32 = 36.0;
/// Space between the edge of the palette and its content, on each side, in
/// points: the inner margin of a side panel.
const PANEL_MARGIN: f32 = 8.0;
/// Points the widest palette has over what four full cells need, so that
/// they fit with a little air and no more.
const PANEL_SLACK: f32 = 6.0;
/// Side of a category icon at its full size, and the narrowest it is drawn, in
/// points.
const ICON: f32 = 26.0;
const MIN_ICON: f32 = 18.0;
/// Space between category icons, in points.
const ICON_GAP: f32 = 2.0;
/// How many category icons share a row.
const ICONS: usize = Category::ALL.len();

/// The width the content of the narrowest column has: four cells at their
/// narrowest and the eight category icons at their smallest, whichever needs
/// more.
const MIN_CONTENT: f32 = {
    let cells = MAX_COLUMNS as f32 * MIN_CELL_WIDTH + (MAX_COLUMNS - 1) as f32 * GAP;
    let icons = ICONS as f32 * MIN_ICON + (ICONS - 1) as f32 * ICON_GAP;
    if cells > icons { cells } else { icons }
};
/// The narrowest the palette column goes, in points: what its content needs
/// to keep four cells in a row and eight icons in a row.
pub(crate) const MIN_WIDTH: f32 = MIN_CONTENT + 2.0 * PANEL_MARGIN;
/// The widest the palette column goes, in points: four cells at their full
/// size, their gaps, the margins and a little air.
pub(crate) const MAX_WIDTH: f32 =
    MAX_COLUMNS as f32 * CELL.x + (MAX_COLUMNS - 1) as f32 * GAP + 2.0 * PANEL_MARGIN + PANEL_SLACK;

/// The side of a category icon in a row `width` points wide: full size when
/// the eight fit, smaller in proportion when they do not.
fn icon_size(width: f32) -> f32 {
    if !width.is_finite() {
        return ICON;
    }
    ((width - ICON_GAP * (ICONS - 1) as f32) / ICONS as f32).clamp(MIN_ICON, ICON)
}

/// The width of each of `columns` cells in a row `width` points wide.
fn cell_width(width: f32, columns: usize) -> f32 {
    (width - GAP * columns.saturating_sub(1) as f32) / columns.max(1) as f32
}

/// The cells of one row of the library, for a row `width` points wide: how
/// many, and how wide each is (the height follows). A cell is never wider than
/// [`CELL`], never more than four share a row, and a row takes as many as keep
/// each at [`MIN_CELL_WIDTH`] or more; narrower than that, one is left out and
/// the others grow back.
pub(crate) fn grid(width: f32) -> (usize, f32) {
    if !width.is_finite() || width <= 0.0 {
        return (1, MIN_CELL_WIDTH);
    }
    let mut columns = MAX_COLUMNS;
    while columns > 1 && cell_width(width, columns) < MIN_CELL_WIDTH {
        columns -= 1;
    }
    (columns, cell_width(width, columns).clamp(24.0, CELL.x))
}
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
    // One row of eight icons, as large as the column allows.
    let icon_side = icon_size(ui.available_width());
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(ICON_GAP, ICON_GAP);
        for category in Category::ALL {
            let count = catalog
                .latest()
                .filter(|model| model.category() == category)
                .count();
            let label = tr(category.label());
            let hint = trf("{0} ({1})", &[&label, &count]);
            let open = state.search.is_empty() && state.tab == category;
            if icons::button_sized(ui, tab_icon(category), label, &hint, open, icon_side).clicked()
            {
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
    ui.horizontal_wrapped(|ui| {
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
    // Rows of at most four cells that share the width of the column; the
    // cells shrink, in proportion, when the column does.
    let (columns, width) = grid(ui.available_width());
    let size = egui::vec2(width, width * CELL.y / CELL.x);
    let symbols = model.symbols();
    ui.spacing_mut().item_spacing = egui::vec2(GAP, GAP);
    for row in symbols.chunks(columns) {
        ui.horizontal(|ui| {
            for symbol in row {
                let current = matches!(
                    tool,
                    Tool::Place { model: m, symbol: s, .. }
                        if m.id == model.id() && m.version == model.version() && s == symbol.key()
                );
                let label = format!("{} · {}", model.id(), symbol.key().0);
                let scene = symbol_preview(model, symbol.key());
                let clicked = cell(ui, &label, &symbol.key().0, scene.as_ref(), current, size)
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
}

/// The button of one symbol: its drawing, with its key under it.
fn cell(
    ui: &mut Ui,
    label: &str,
    caption: &str,
    scene: Option<&Scene>,
    selected: bool,
    size: egui::Vec2,
) -> egui::Response {
    let scale = size.x / CELL.x;
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
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
        rect.min + egui::vec2(4.0, 3.0) * scale,
        egui::vec2(size.x - 8.0 * scale, DRAWING_HEIGHT * scale),
    );
    if let Some(scene) = scene {
        paint_preview(ui, area, scene);
    }
    // The caption shrinks with the cell, but stays readable and as long as
    // the cell has room for.
    let characters = (12.0 * scale).round().clamp(6.0, 12.0) as usize;
    ui.painter().text(
        Pos2::new(rect.center().x, rect.max.y - 8.0 * scale),
        Align2::CENTER_CENTER,
        shorten(caption, characters),
        FontId::proportional((10.0 * scale).max(8.0)),
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
    paint_scaled(ui, area, scene, MAX_SCALE);
}

/// Paint `scene`, centred in `area` and scaled to fit it, at most
/// `max_scale` points to a subunit (the symbol editor draws larger).
pub(crate) fn paint_scaled(ui: &Ui, area: Rect, scene: &Scene, max_scale: f32) {
    let Some(bounds) = scene.bounds else {
        return;
    };
    let dark = ui.visuals().dark_mode;
    let painter = ui.painter().with_clip_rect(area.expand(2.0));
    let width = to_points(bounds.max.x.saturating_sub(bounds.min.x)).max(1.0);
    let height = to_points(bounds.max.y.saturating_sub(bounds.min.y)).max(1.0);
    let scale = (area.width() / width)
        .min(area.height() / height)
        .min(max_scale);
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
            Shape::Block(rect) => {
                painter.rect_filled(
                    Rect::from_two_pos(to_screen(rect.min), to_screen(rect.max)),
                    0.0,
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

#[cfg(test)]
mod grid_tests {
    use super::*;

    #[test]
    fn a_cell_is_never_wider_than_its_full_size_and_never_four_share_more_than_a_row() {
        for tenths in 0_i16..4000 {
            let width = f32::from(tenths) / 10.0;
            let (columns, cell) = grid(width);

            assert!((1..=MAX_COLUMNS).contains(&columns), "{columns} at {width}");
            assert!(cell <= CELL.x, "{cell} at {width}");
            assert!(cell >= 24.0, "{cell} at {width}");
        }
    }

    #[test]
    fn four_full_cells_fit_the_widest_column_and_the_cells_are_not_blown_up() {
        let content = MAX_WIDTH - 2.0 * PANEL_MARGIN;

        assert_eq!(grid(content), (4, CELL.x));
        assert_eq!(grid(content + 500.0), (4, CELL.x));
        // The column stops a few points past what four cells need, no more.
        let needed = 4.0 * CELL.x + 3.0 * GAP;
        assert!((0.0..=8.0).contains(&(content - needed)));
    }

    #[test]
    fn cells_shrink_with_the_column_and_a_column_is_dropped_only_when_they_get_too_small() {
        let mut last = grid(300.0);
        for width in (60..300).rev() {
            let now = grid(width as f32);
            // Narrower never gives more columns, and, with the same number of
            // columns, never a wider cell.
            assert!(now.0 <= last.0, "{width}: {now:?} after {last:?}");
            if now.0 == last.0 {
                assert!(now.1 <= last.1 + 0.001, "{width}: {now:?} after {last:?}");
            }
            // The row always fits.
            let used = now.1 * now.0 as f32 + GAP * (now.0 - 1) as f32;
            assert!(
                used <= width as f32 + 0.01 || now.0 == 1,
                "{width}: {now:?}"
            );
            last = now;
        }
    }

    #[test]
    fn the_narrowest_column_still_has_four_cells_in_a_row_and_eight_icons() {
        let content = MIN_WIDTH - 2.0 * PANEL_MARGIN;

        let (columns, cell) = grid(content);
        assert_eq!(columns, 4, "a cell is lost to a line break");
        assert!(cell >= MIN_CELL_WIDTH - 0.01, "{cell}");
        let icon = icon_size(content);
        assert!(icon >= MIN_ICON - 0.01);
        let used = ICONS as f32 * icon + (ICONS - 1) as f32 * ICON_GAP;
        assert!(used <= content + 0.01, "the icons need {used} of {content}");
    }

    #[test]
    fn category_icons_are_never_larger_than_before_and_shrink_with_the_column() {
        assert_eq!(icon_size(500.0), ICON);
        // At the default width of the column all eight fit at full size.
        assert_eq!(icon_size(260.0 - 2.0 * PANEL_MARGIN), ICON);
        let mut last = ICON;
        for width in (100..300).rev() {
            let now = icon_size(width as f32);
            assert!(now <= last + 0.001 && (MIN_ICON..=ICON).contains(&now));
            last = now;
        }
        assert_eq!(icon_size(f32::NAN), ICON);
    }

    #[test]
    fn a_width_that_is_not_a_number_gives_one_narrow_column() {
        assert_eq!(grid(f32::NAN).0, 1);
        assert_eq!(grid(f32::INFINITY).0, 1);
        assert_eq!(grid(-4.0).0, 1);
    }
}
