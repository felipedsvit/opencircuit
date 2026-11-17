//! Report tables as PDF pages (ADR-0014).
//!
//! A [`Table`] is set in the embedded monospaced font on A4 landscape pages
//! (84 by 59 grid units, as a sheet): the title, a row of headings that is
//! repeated on every page, a rule, one line per row and a footer with the page
//! number. The engine of the PDF writer draws lines and text only, so there
//! is no filled cell and no wrapping; a cell longer than [`MAX_CELL_CHARS`]
//! characters is cut and ends in `~`, and a character outside printable ASCII
//! is written as `?`, because the embedded font is not meant for more. A
//! table too wide for the paper makes its pages wider.

use circuit_model::Table;
use circuit_renderer::{Anchor, Scene, SceneItem, ScenePoint, SceneRect, Shape, StyleKey};

use crate::{Theme, pdf_pages};

/// Longest cell, in characters.
pub const MAX_CELL_CHARS: usize = 40;

/// Width of an A4 landscape page, in points.
const PAGE_WIDTH: i64 = 840;
/// Height of the page, in points.
const PAGE_HEIGHT: i64 = 590;
/// Space left on the four sides.
const MARGIN: i64 = 30;
/// Height of one line of the table.
const ROW_HEIGHT: i64 = 14;
/// Space between two columns.
const COLUMN_GAP: i64 = 10;
/// Where the headings are, below the top of the page: the title is above.
const HEADINGS_AT: i64 = 56;
/// Width of one glyph of the font, in tenths of a point.
const GLYPH_TENTHS: i64 = 48;
/// Space kept for the footer.
const FOOTER: i64 = 40;

/// The cell as it is set: printable ASCII only, and at most
/// [`MAX_CELL_CHARS`] characters.
fn clean(cell: &str) -> String {
    let mut text: String = cell
        .chars()
        .map(|c| if (' '..='~').contains(&c) { c } else { '?' })
        .collect();
    if text.chars().count() > MAX_CELL_CHARS {
        text = text
            .chars()
            .take(MAX_CELL_CHARS.saturating_sub(1))
            .collect();
        text.push('~');
    }
    text
}

/// The width of a text of `chars` characters, in points, rounded up.
fn width_of(chars: usize) -> i64 {
    i64::try_from(chars)
        .unwrap_or(i64::MAX)
        .saturating_mul(GLYPH_TENTHS)
        .saturating_add(9)
        .checked_div(10)
        .unwrap_or(0)
}

fn text(x: i64, y: i64, text: String) -> SceneItem {
    SceneItem {
        shape: Shape::Text {
            at: ScenePoint::new(x, y),
            text,
            anchor: Anchor::Left,
        },
        style: StyleKey::Label,
        subject: None,
    }
}

fn rule(from: i64, to: i64, y: i64) -> SceneItem {
    SceneItem {
        shape: Shape::Line {
            from: ScenePoint::new(from, y),
            to: ScenePoint::new(to, y),
        },
        style: StyleKey::Frame,
        subject: None,
    }
}

/// The pages of a table, one scene each.
fn pages_of(table: &Table) -> Vec<Scene> {
    let headings: Vec<String> = table.columns.iter().map(|c| clean(c)).collect();
    let rows: Vec<Vec<String>> = table
        .rows
        .iter()
        .map(|row| row.iter().map(|cell| clean(cell)).collect())
        .collect();

    // Column positions: the widest cell of each column.
    let mut x = MARGIN;
    let mut columns: Vec<i64> = Vec::new();
    for (index, heading) in headings.iter().enumerate() {
        columns.push(x);
        let widest = std::iter::once(heading)
            .chain(rows.iter().filter_map(|row| row.get(index)))
            .map(|cell| cell.chars().count())
            .max()
            .unwrap_or(0);
        x = x
            .saturating_add(width_of(widest))
            .saturating_add(COLUMN_GAP);
    }
    let right = x.saturating_sub(COLUMN_GAP);
    let width = PAGE_WIDTH.max(right.saturating_add(MARGIN));

    let rows_per_page = usize::try_from(
        PAGE_HEIGHT
            .saturating_sub(HEADINGS_AT)
            .saturating_sub(ROW_HEIGHT)
            .saturating_sub(FOOTER)
            .checked_div(ROW_HEIGHT)
            .unwrap_or(1),
    )
    .unwrap_or(1)
    .max(1);
    let chunks: Vec<&[Vec<String>]> = if rows.is_empty() {
        vec![&[]]
    } else {
        rows.chunks(rows_per_page).collect()
    };
    let count = chunks.len();

    let page = SceneRect::from_corners(ScenePoint::new(0, 0), ScenePoint::new(width, PAGE_HEIGHT));
    let mut scenes = Vec::new();
    for (number, chunk) in (1_usize..).zip(chunks) {
        let mut items = vec![text(MARGIN, MARGIN, clean(&table.title))];
        for (heading, at) in headings.iter().zip(&columns) {
            items.push(text(*at, HEADINGS_AT, heading.clone()));
        }
        let under = HEADINGS_AT.saturating_add(ROW_HEIGHT.checked_div(2).unwrap_or(0));
        items.push(rule(MARGIN, right, under));
        let mut y = HEADINGS_AT.saturating_add(ROW_HEIGHT);
        for row in chunk {
            for (cell, at) in row.iter().zip(&columns) {
                if !cell.is_empty() {
                    items.push(text(*at, y, cell.clone()));
                }
            }
            y = y.saturating_add(ROW_HEIGHT);
        }
        if rows.is_empty() {
            items.push(text(MARGIN, y, "(no rows)".to_owned()));
        }
        items.push(text(
            MARGIN,
            PAGE_HEIGHT.saturating_sub(MARGIN),
            format!("{}  page {number} of {count}", clean(&table.title)),
        ));
        scenes.push(Scene {
            items,
            bounds: Some(page),
            page: Some(page),
        });
    }
    scenes
}

/// The tables as a PDF document: the pages of the first table, then those of
/// the next. The bytes depend only on the tables and the theme.
#[must_use]
pub fn table_pdf(tables: &[Table], theme: Theme) -> Vec<u8> {
    let scenes: Vec<Scene> = tables.iter().flat_map(pages_of).collect();
    pdf_pages(&scenes, theme)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(rows: usize) -> Table {
        let mut table = Table::new("Parts", &["Ref", "Note"]);
        for index in 0..rows {
            table.push(vec![format!("K{index}"), "coil".to_owned()]);
        }
        table
    }

    fn count(bytes: &[u8], needle: &str) -> usize {
        String::from_utf8_lossy(bytes).matches(needle).count()
    }

    #[test]
    fn a_table_is_a_pdf_with_a_page_per_screenful_of_rows() {
        let one = table_pdf(&[table(10)], Theme::Print);
        let three = table_pdf(&[table(100)], Theme::Print);

        assert!(one.starts_with(b"%PDF-"));
        assert_eq!(count(&one, "/Count 1"), 1);
        // 35 rows fit on a page: 100 rows take three pages.
        assert_eq!(count(&three, "/Count 3"), 1);
    }

    #[test]
    fn every_table_starts_on_its_own_page() {
        let both = table_pdf(&[table(3), table(4)], Theme::Print);

        assert_eq!(count(&both, "/Count 2"), 1);
    }

    #[test]
    fn an_empty_table_still_has_a_page_with_its_headings() {
        let scenes = pages_of(&Table::new("Empty", &["A", "B"]));

        assert_eq!(scenes.len(), 1);
        let texts: Vec<&str> = scenes[0]
            .items
            .iter()
            .filter_map(|item| match &item.shape {
                Shape::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(
            texts,
            ["Empty", "A", "B", "(no rows)", "Empty  page 1 of 1"]
        );
    }

    #[test]
    fn headings_are_repeated_on_every_page() {
        let scenes = pages_of(&table(100));

        assert_eq!(scenes.len(), 3);
        for scene in &scenes {
            let has = |wanted: &str| {
                scene
                    .items
                    .iter()
                    .any(|item| matches!(&item.shape, Shape::Text { text, .. } if text == wanted))
            };
            assert!(has("Ref") && has("Note"));
        }
    }

    #[test]
    fn a_long_cell_is_cut_and_a_character_the_font_may_lack_is_a_question_mark() {
        assert_eq!(clean(&"x".repeat(60)), format!("{}~", "x".repeat(39)));
        assert_eq!(clean(&"x".repeat(40)), "x".repeat(40));
        assert_eq!(clean("caf\u{e9} \u{2014} ok"), "caf? ? ok");
    }

    #[test]
    fn a_table_wider_than_the_paper_makes_a_wider_page() {
        let wide = Table::new("Wide", &["aaaaaaaaaa"; 30]);

        let scenes = pages_of(&wide);

        let page = scenes[0].page.expect("a page");
        assert!(page.max.x > PAGE_WIDTH);
    }

    #[test]
    fn the_same_tables_give_the_same_bytes() {
        assert_eq!(
            table_pdf(&[table(50)], Theme::Print),
            table_pdf(&[table(50)], Theme::Print)
        );
    }

    #[test]
    fn no_table_is_still_a_document() {
        assert!(table_pdf(&[], Theme::Print).starts_with(b"%PDF-"));
    }
}
