//! The frame of a sheet: border, zone marks and title block (ADR-0011).
//!
//! Everything is drawn in the [`StyleKey::Frame`] style and tagged with the
//! sheet as its subject, so that the scene of one printed page can keep its
//! own frame and drop the others.

use circuit_model::{Schematic, Sheet, Subject};

use crate::scene::{Anchor, Scene, ScenePoint, SceneRect, Shape, StyleKey};

/// Width of the band between the paper edge and the border, where the zone
/// numbers and letters go, in subunits.
const BAND: i64 = 12;
/// Size of the title block, in subunits.
const BLOCK_WIDTH: i64 = 300;
const BLOCK_HEIGHT: i64 = 64;
/// Height of one row of the title block.
const ROW: i64 = BLOCK_HEIGHT / 4;
/// Left of the second column of the title block.
const SECOND_COLUMN: i64 = 150;
/// Gap between a text and the line before it.
const PAD: i64 = 4;
/// Most characters of a text in the wide row and in a narrow cell, which the
/// width of one glyph of the monospaced PDF font (4.8 subunits) allows.
const WIDE_CHARS: usize = 58;
const NARROW_CHARS: usize = 28;

/// The frame of a sheet on the canvas, in subunits; `None` when it leaves
/// the coordinate range.
pub(crate) fn frame_rect(sheet: &Sheet) -> Option<SceneRect> {
    let frame = sheet.frame()?;
    Some(SceneRect::from_corners(
        ScenePoint::from_grid(frame.min),
        ScenePoint::from_grid(frame.max),
    ))
}

/// Draw the frame of every sheet of the document, in sheet-number order.
pub(crate) fn draw_sheets(scene: &mut Scene, doc: &Schematic) {
    let mut sheets: Vec<&Sheet> = doc.sheets.iter().collect();
    sheets.sort_by_key(|sheet| (sheet.number, sheet.id));
    let total = sheets.len();
    for sheet in sheets {
        draw_sheet(scene, doc, sheet, total);
    }
}

fn draw_sheet(scene: &mut Scene, doc: &Schematic, sheet: &Sheet, total: usize) {
    let Some(outer) = frame_rect(sheet) else {
        return;
    };
    let subject = Some(Subject::Sheet(sheet.id));
    let line = |scene: &mut Scene, from: ScenePoint, to: ScenePoint| {
        scene.push(Shape::Line { from, to }, StyleKey::Frame, subject.clone());
    };
    let text = |scene: &mut Scene, at: ScenePoint, text: String, anchor: Anchor| {
        scene.push(
            Shape::Text { at, text, anchor },
            StyleKey::Frame,
            subject.clone(),
        );
    };
    let point = ScenePoint::new;
    let inner = SceneRect::from_corners(
        point(
            outer.min.x.saturating_add(BAND),
            outer.min.y.saturating_add(BAND),
        ),
        point(
            outer.max.x.saturating_sub(BAND),
            outer.max.y.saturating_sub(BAND),
        ),
    );
    scene.push(Shape::Rect(outer), StyleKey::Frame, subject.clone());
    scene.push(Shape::Rect(inner), StyleKey::Frame, subject.clone());

    // Zone marks: a tick across the band at every boundary between zones,
    // the number of the column on the top and bottom bands and the letter of
    // the row on the left and right ones, each centred on its zone.
    let (width, height) = (
        outer.max.x.saturating_sub(outer.min.x),
        outer.max.y.saturating_sub(outer.min.y),
    );
    let columns = i64::from(sheet.zones.columns);
    let rows = i64::from(sheet.zones.rows);
    let half = BAND / 2;
    for column in 0..columns {
        let edge = |index: i64| {
            outer.min.x.saturating_add(
                width
                    .saturating_mul(index)
                    .checked_div(columns)
                    .unwrap_or(0),
            )
        };
        if column > 0 {
            let x = edge(column);
            line(scene, point(x, outer.min.y), point(x, inner.min.y));
            line(scene, point(x, inner.max.y), point(x, outer.max.y));
        }
        let middle = edge(column)
            .saturating_add(edge(column.saturating_add(1)))
            .checked_div(2)
            .unwrap_or(0);
        let number = column.saturating_add(1).to_string();
        text(
            scene,
            point(middle, outer.min.y.saturating_add(half)),
            number.clone(),
            Anchor::Center,
        );
        text(
            scene,
            point(middle, outer.max.y.saturating_sub(half)),
            number,
            Anchor::Center,
        );
    }
    for row in 0..rows {
        let edge = |index: i64| {
            outer
                .min
                .y
                .saturating_add(height.saturating_mul(index).checked_div(rows).unwrap_or(0))
        };
        if row > 0 {
            let y = edge(row);
            line(scene, point(outer.min.x, y), point(inner.min.x, y));
            line(scene, point(inner.max.x, y), point(outer.max.x, y));
        }
        let middle = edge(row)
            .saturating_add(edge(row.saturating_add(1)))
            .checked_div(2)
            .unwrap_or(0);
        let letter = u8::try_from(row)
            .ok()
            .map_or('?', |row| char::from(b'A'.saturating_add(row)));
        text(
            scene,
            point(outer.min.x.saturating_add(half), middle),
            letter.to_string(),
            Anchor::Center,
        );
        text(
            scene,
            point(outer.max.x.saturating_sub(half), middle),
            letter.to_string(),
            Anchor::Center,
        );
    }

    // The title block, in the bottom right corner inside the border, when
    // the sheet is large enough to hold it.
    let room = |side: i64, wanted: i64| side >= wanted.saturating_mul(2);
    if !room(inner.max.x.saturating_sub(inner.min.x), BLOCK_WIDTH)
        || !room(inner.max.y.saturating_sub(inner.min.y), BLOCK_HEIGHT)
    {
        return;
    }
    let left = inner.max.x.saturating_sub(BLOCK_WIDTH);
    let top = inner.max.y.saturating_sub(BLOCK_HEIGHT);
    scene.push(
        Shape::Rect(SceneRect::from_corners(point(left, top), inner.max)),
        StyleKey::Frame,
        subject.clone(),
    );
    for row in 1..4 {
        let y = top.saturating_add(ROW.saturating_mul(row));
        line(scene, point(left, y), point(inner.max.x, y));
    }
    line(
        scene,
        point(left.saturating_add(SECOND_COLUMN), top.saturating_add(ROW)),
        point(left.saturating_add(SECOND_COLUMN), inner.max.y),
    );

    let meta = &doc.project;
    let or_blank = |value: &Option<String>| value.as_deref().unwrap_or("").to_owned();
    let cells = [
        (0, 0, WIDE_CHARS, sheet.title.clone()),
        (1, 0, NARROW_CHARS, meta.name.clone()),
        (1, 1, NARROW_CHARS, or_blank(&meta.company)),
        (
            2,
            0,
            NARROW_CHARS,
            format!("Author {}", or_blank(&meta.author)),
        ),
        (2, 1, NARROW_CHARS, format!("Date {}", or_blank(&meta.date))),
        (
            3,
            0,
            NARROW_CHARS,
            format!("Rev {}", or_blank(&meta.revision)),
        ),
        (
            3,
            1,
            NARROW_CHARS,
            format!("Sheet {}/{}", sheet.number, total),
        ),
    ];
    for (row, column, limit, value) in cells {
        let x = left
            .saturating_add(PAD)
            .saturating_add(SECOND_COLUMN.saturating_mul(column));
        let y = top
            .saturating_add(ROW.saturating_mul(row))
            .saturating_add(ROW / 2);
        text(scene, point(x, y), fit(&value, limit), Anchor::Left);
    }
}

/// `value` cut to `limit` characters, with `…` in place of the last one when
/// it was longer, and its control characters and line breaks as spaces.
fn fit(value: &str, limit: usize) -> String {
    let clean: String = value
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    if clean.chars().count() <= limit {
        return clean;
    }
    clean
        .chars()
        .take(limit.saturating_sub(1))
        .chain(std::iter::once('\u{2026}'))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_texts_are_cut_with_an_ellipsis_and_controls_become_spaces() {
        assert_eq!(fit("short", 10), "short");
        assert_eq!(fit("abcdefghij", 10), "abcdefghij");
        assert_eq!(fit("abcdefghijk", 10), "abcdefghi\u{2026}");
        assert_eq!(fit("a\nb\tc", 10), "a b c");
        assert_eq!(fit("", 10), "");
        assert_eq!(fit("abc", 0), "\u{2026}");
    }
}
