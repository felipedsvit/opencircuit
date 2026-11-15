//! Drawing sheets: frames placed over the canvas (ADR-0011).
//!
//! A sheet is a rectangle on the single canvas of a document, with a paper
//! size, an orientation, zones that a cross-reference can name, and a title.
//! It has no electrical meaning: wires still connect whatever they reach.
//! The sheet of a symbol is derived from where it is, never stored.

use serde::{Deserialize, Serialize};

use crate::geometry::{GridPoint, GridRect};
use crate::ids::SheetId;
use crate::schematic::Schematic;

/// Largest number of zone columns or rows of a sheet.
pub const MAX_ZONES: u8 = 26;

/// Largest side of a custom sheet, in grid units (about 3.5 m).
pub const MAX_SHEET_SIDE: u32 = 1_000;

/// The paper a sheet is printed on.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum SheetSize {
    /// ISO A4, 297 by 210 mm.
    A4,
    /// ISO A3, 420 by 297 mm.
    A3,
    /// ISO A2, 594 by 420 mm.
    A2,
    /// Any size, in grid units, landscape as given.
    Custom {
        /// Width before orientation, in grid units.
        width: u32,
        /// Height before orientation, in grid units.
        height: u32,
    },
}

impl SheetSize {
    /// Width and height in landscape, in grid units: the largest whole
    /// number of grid units (10 pt, about 3.53 mm) that fits the paper.
    #[must_use]
    pub const fn landscape(self) -> (u32, u32) {
        match self {
            Self::A4 => (84, 59),
            Self::A3 => (119, 84),
            Self::A2 => (168, 119),
            Self::Custom { width, height } => (width, height),
        }
    }
}

/// Which way the paper lies.
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
#[serde(rename_all = "snake_case")]
pub enum Orientation {
    /// Wider than tall.
    #[default]
    Landscape,
    /// Taller than wide.
    Portrait,
}

/// The grid of zones along the frame: columns numbered from 1, left to
/// right, and rows lettered from A, top to bottom.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Zones {
    /// Number of columns, 1 to [`MAX_ZONES`].
    pub columns: u8,
    /// Number of rows, 1 to [`MAX_ZONES`].
    pub rows: u8,
}

/// A zone of a sheet, such as column 4, row B.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Zone {
    /// Column, from 1.
    pub column: u8,
    /// Row, from 0 for A.
    pub row: u8,
}

impl std::fmt::Display for Zone {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let letter = char::from(b'A'.saturating_add(self.row));
        write!(f, "{}{letter}", self.column)
    }
}

/// One drawing sheet.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Sheet {
    /// Stable identifier, from the document counter.
    pub id: SheetId,
    /// Number printed in the title block and used by cross-references;
    /// unique in the document, from 1.
    pub number: u32,
    /// Title printed in the title block.
    pub title: String,
    /// Top-left corner of the frame on the canvas.
    pub origin: GridPoint,
    /// Paper size.
    pub size: SheetSize,
    /// Orientation of the paper.
    pub orientation: Orientation,
    /// Zones along the frame.
    pub zones: Zones,
}

impl Sheet {
    /// Width and height of the frame, in grid units.
    #[must_use]
    pub const fn extent(&self) -> (u32, u32) {
        let (long, short) = self.size.landscape();
        match self.orientation {
            Orientation::Landscape => (long, short),
            Orientation::Portrait => (short, long),
        }
    }

    /// The frame on the canvas; `None` when it would leave the coordinate
    /// range.
    #[must_use]
    pub fn frame(&self) -> Option<GridRect> {
        let (width, height) = self.extent();
        let max = GridPoint::new(
            self.origin.x.checked_add(i32::try_from(width).ok()?)?,
            self.origin.y.checked_add(i32::try_from(height).ok()?)?,
        );
        Some(GridRect::from_corners(self.origin, max))
    }

    /// The zone of a point inside the frame; `None` outside it.
    #[must_use]
    pub fn zone_of(&self, point: GridPoint) -> Option<Zone> {
        let frame = self.frame()?;
        let inside = frame.min.x <= point.x
            && point.x < frame.max.x
            && frame.min.y <= point.y
            && point.y < frame.max.y;
        if !inside || self.zones.columns == 0 || self.zones.rows == 0 {
            return None;
        }
        let (width, height) = self.extent();
        let along = |offset: i32, length: u32, count: u8| -> Option<u8> {
            let offset = u64::try_from(offset).ok()?;
            let index = offset
                .checked_mul(u64::from(count))?
                .checked_div(u64::from(length))?;
            u8::try_from(index).ok().filter(|index| *index < count)
        };
        let column = along(point.x.checked_sub(frame.min.x)?, width, self.zones.columns)?;
        let row = along(point.y.checked_sub(frame.min.y)?, height, self.zones.rows)?;
        Some(Zone {
            column: column.checked_add(1)?,
            row,
        })
    }
}

/// The sheet whose frame holds `point`, if any. Frames do not overlap in a
/// valid document; if they do, the lowest sheet number wins.
#[must_use]
pub fn sheet_at(doc: &Schematic, point: GridPoint) -> Option<&Sheet> {
    doc.sheets
        .iter()
        .filter(|sheet| sheet.zone_of(point).is_some())
        .min_by_key(|sheet| sheet.number)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a4(origin: GridPoint) -> Sheet {
        Sheet {
            id: SheetId(1),
            number: 1,
            title: "Control".to_owned(),
            origin,
            size: SheetSize::A4,
            orientation: Orientation::Landscape,
            zones: Zones {
                columns: 8,
                rows: 6,
            },
        }
    }

    #[test]
    fn paper_sizes_fit_whole_grid_units_and_turn_with_the_orientation() {
        let mut sheet = a4(GridPoint::new(0, 0));
        assert_eq!(sheet.extent(), (84, 59));
        sheet.orientation = Orientation::Portrait;
        assert_eq!(sheet.extent(), (59, 84));
        sheet.size = SheetSize::A3;
        assert_eq!(sheet.extent(), (84, 119));
    }

    #[test]
    fn each_paper_size_is_the_largest_whole_number_of_grid_units_that_fits() {
        // One grid unit is 10 pt; 1 mm is 72 / 25.4 pt.
        let points = |mm: u32| f64::from(mm) * 72.0 / 25.4;
        for (size, (long_mm, short_mm)) in [
            (SheetSize::A4, (297, 210)),
            (SheetSize::A3, (420, 297)),
            (SheetSize::A2, (594, 420)),
        ] {
            let (long, short) = size.landscape();
            for (units, mm) in [(long, long_mm), (short, short_mm)] {
                assert!(f64::from(units * 10) <= points(mm), "{size:?}");
                assert!(f64::from((units + 1) * 10) > points(mm), "{size:?}");
            }
        }
    }

    #[test]
    fn zones_are_numbered_from_the_top_left_corner() {
        let sheet = a4(GridPoint::new(-10, 5));
        assert_eq!(
            sheet.zone_of(GridPoint::new(-10, 5)),
            Some(Zone { column: 1, row: 0 })
        );
        // 84 / 8 = 10.5 grid units per column; 59 / 6 per row.
        assert_eq!(
            sheet.zone_of(GridPoint::new(-10 + 11, 5 + 10)),
            Some(Zone { column: 2, row: 1 })
        );
        assert_eq!(
            sheet.zone_of(GridPoint::new(-10 + 83, 5 + 58)),
            Some(Zone { column: 8, row: 5 })
        );
        assert_eq!(sheet.zone_of(GridPoint::new(-10 + 84, 5)), None);
        assert_eq!(sheet.zone_of(GridPoint::new(-11, 5)), None);
        assert_eq!(Zone { column: 4, row: 1 }.to_string(), "4B");
    }

    #[test]
    fn a_frame_beyond_the_coordinate_range_has_no_zones() {
        let sheet = a4(GridPoint::new(i32::MAX - 10, 0));
        assert_eq!(sheet.frame(), None);
        assert_eq!(sheet.zone_of(GridPoint::new(i32::MAX - 5, 1)), None);
    }

    #[test]
    fn the_sheet_of_a_point_is_the_frame_that_holds_it() {
        let mut doc = Schematic::new("sheets");
        let mut second = a4(GridPoint::new(100, 0));
        second.id = SheetId(2);
        second.number = 2;
        doc.sheets = vec![second, a4(GridPoint::new(0, 0))];
        doc.next_id = 3;

        assert_eq!(
            sheet_at(&doc, GridPoint::new(5, 5)).map(|s| s.number),
            Some(1)
        );
        assert_eq!(
            sheet_at(&doc, GridPoint::new(105, 5)).map(|s| s.number),
            Some(2)
        );
        assert_eq!(sheet_at(&doc, GridPoint::new(90, 5)), None);
    }
}
