//! The library of the left column (M9.5): at most four cells in a row, cells
//! that never grow past their full size and shrink in proportion with the
//! column, nothing past the edge of the column, and a column that stops a few
//! points past four full cells.

use eframe::egui::{self, Pos2, Rect};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use opencircuit_desktop::{DesktopApp, Language};

/// A cell at its full size: width, height and the gap between cells.
const FULL: (f32, f32) = (70.0, 62.0);
const GAP: f32 = 4.0;
/// What the column keeps between its edge and its content, each side.
const MARGIN: f32 = 8.0;

fn window() -> Harness<'static, DesktopApp> {
    let mut app = DesktopApp::default();
    app.set_language(Language::English);
    let mut harness = Harness::new_ui_state(|ui, app: &mut DesktopApp| app.show(ui), app);
    harness.set_size(egui::Vec2::new(1300.0, 900.0));
    harness.run();
    harness.get_by_label("New").click();
    harness.run();
    harness
}

/// The right edge of the column, from its search field.
fn column_right(harness: &Harness<'_, DesktopApp>) -> f32 {
    let field = harness
        .get_all_by_label("Search")
        .map(|node| node.rect())
        .max_by(|a, b| a.width().total_cmp(&b.width()))
        .expect("the palette has a search field");
    field.max.x + MARGIN
}

/// Drag the right edge of the column to `to`.
fn resize_column(harness: &mut Harness<'_, DesktopApp>, to: f32) {
    let edge = column_right(harness);
    let y = 500.0;
    harness.hover_at(Pos2::new(edge - 1.0, y));
    harness.run();
    harness.drag_at(Pos2::new(edge - 1.0, y));
    harness.step();
    for step in 1..=10 {
        let x = edge + (to - edge) * step as f32 / 10.0;
        harness.hover_at(Pos2::new(x - 1.0, y));
        harness.step();
    }
    harness.drop_at(Pos2::new(to - 1.0, y));
    harness.run();
}

/// The cells of the library that are drawn, as rectangles.
fn cells(harness: &Harness<'_, DesktopApp>) -> Vec<Rect> {
    let right = column_right(harness);
    harness
        .root()
        .children_recursive()
        .filter(|node| {
            node.accesskit_node().role() == egui::accesskit::Role::Button
                && node
                    .accesskit_node()
                    .label()
                    .is_some_and(|label| label.contains(" · ") && label.contains('.'))
        })
        .map(|node| node.rect())
        .filter(|rect| rect.min.x < right && rect.min.y > 150.0)
        .collect()
}

/// The cells grouped by row.
fn rows(cells: &[Rect]) -> Vec<Vec<Rect>> {
    let mut rows: Vec<Vec<Rect>> = Vec::new();
    for cell in cells {
        match rows.iter_mut().find(|row| {
            row.first()
                .is_some_and(|first| (first.min.y - cell.min.y).abs() < 1.0)
        }) {
            Some(row) => row.push(*cell),
            None => rows.push(vec![*cell]),
        }
    }
    for row in &mut rows {
        row.sort_by(|a, b| a.min.x.total_cmp(&b.min.x));
    }
    rows
}

#[test]
fn the_library_follows_the_width_of_the_column() {
    let mut harness = window();

    for target in [150.0, 180.0, 220.0, 260.0, 290.0, 310.0] {
        resize_column(&mut harness, target);
        let right = column_right(&harness);
        let found = cells(&harness);
        assert!(!found.is_empty(), "no cell at {target}");

        let grouped = rows(&found);
        assert_eq!(
            grouped.iter().map(Vec::len).max(),
            Some(4),
            "four cells do not share a row at {target}: the cells wrapped instead of shrinking"
        );
        for row in grouped {
            assert!(row.len() <= 4, "{} cells in a row at {target}", row.len());
            for cell in &row {
                assert!(
                    cell.max.x <= right - MARGIN + 0.6,
                    "a cell ends at {} past the column at {right} (target {target})",
                    cell.max.x
                );
                assert!(
                    cell.width() <= FULL.0 + 0.1,
                    "a cell grew to {}",
                    cell.width()
                );
                assert!(cell.width() >= 24.0, "a cell shrank to {}", cell.width());
                assert!(
                    (cell.height() / cell.width() - FULL.1 / FULL.0).abs() < 0.02,
                    "a cell lost its proportions: {}x{}",
                    cell.width(),
                    cell.height()
                );
            }
            for pair in row.windows(2) {
                let gap = pair[1].min.x - pair[0].max.x;
                assert!((gap - GAP).abs() < 0.6, "a gap of {gap} at {target}");
            }
        }
    }
}

#[test]
fn the_column_stops_a_few_points_past_four_full_cells() {
    let mut harness = window();

    resize_column(&mut harness, 700.0);

    let right = column_right(&harness);
    let found = cells(&harness);
    let widest = rows(&found)
        .into_iter()
        .max_by_key(Vec::len)
        .expect("a row of cells");
    assert_eq!(
        widest.len(),
        4,
        "four cells share a row at the widest column"
    );
    for cell in &widest {
        assert!(
            (cell.width() - FULL.0).abs() < 0.6,
            "a cell is {} wide, not {} (it is never blown up)",
            cell.width(),
            FULL.0
        );
    }
    let slack = right - MARGIN - widest.last().expect("a cell").max.x;
    assert!(
        (0.0..=8.0).contains(&slack),
        "the widest column leaves {slack} points past four full cells"
    );
}

#[test]
fn the_column_keeps_a_least_width_where_four_cells_and_eight_icons_still_fit_a_row() {
    let mut harness = window();

    resize_column(&mut harness, 20.0);

    let right = column_right(&harness);
    assert!(
        (170.0..=190.0).contains(&(right - 8.0)),
        "the column shrank to {}",
        right - 8.0
    );
    let found = cells(&harness);
    assert!(!found.is_empty());
    for cell in &found {
        assert!(cell.max.x <= right - MARGIN + 0.6);
        assert!(cell.width() >= 35.9, "a cell shrank to {}", cell.width());
    }
    // A part with four symbols keeps them in one row: the cells shrink, they
    // do not break into another line.
    let widest = rows(&found).into_iter().map(|row| row.len()).max();
    assert_eq!(widest, Some(4));
    let contactor = found
        .iter()
        .filter(|cell| (cell.min.y - found[1].min.y).abs() < 1.0)
        .count();
    assert!(contactor <= 4);

    // The eight category icons share one row, inside the column.
    let icons: Vec<Rect> = [
        "Supplies and sources",
        "Protection",
        "Controls and actuation",
        "Motors and power",
        "Contacts and terminals",
        "Automation and logic",
        "Mechanical actuators",
        "Lines and connections",
    ]
    .into_iter()
    .map(|label| harness.get_by_label(label).rect())
    .collect();
    for icon in &icons {
        assert!(
            (icon.min.y - icons[0].min.y).abs() < 0.5,
            "an icon wrapped: {icon:?}"
        );
        assert!(icon.max.x <= right - MARGIN + 0.6, "{icon:?}");
        assert!(icon.width() >= 17.9, "{icon:?}");
    }
}

#[test]
fn the_category_icons_keep_their_size_when_there_is_room() {
    let mut harness = window();

    let icon = harness.get_by_label("Protection").rect();
    assert!((icon.width() - 26.0).abs() < 0.1, "{icon:?}");

    resize_column(&mut harness, 190.0);
    let smaller = harness.get_by_label("Protection").rect();
    assert!(smaller.width() < icon.width(), "{smaller:?}");
}
