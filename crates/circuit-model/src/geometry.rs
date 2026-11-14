//! Logical sheet geometry.
//!
//! Coordinates are integer document units, never screen pixels, and rotation
//! is restricted to quarter turns. The core uses no floating point.

use serde::{Deserialize, Serialize};

/// A point on the logical document grid.
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
#[serde(deny_unknown_fields)]
pub struct GridPoint {
    /// Horizontal coordinate, growing to the right.
    pub x: i32,
    /// Vertical coordinate, growing downwards.
    pub y: i32,
}

impl GridPoint {
    /// Shorthand constructor.
    #[must_use]
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    /// Component-wise sum, or `None` when a coordinate overflows.
    #[must_use]
    pub const fn checked_add(self, other: Self) -> Option<Self> {
        let Some(x) = self.x.checked_add(other.x) else {
            return None;
        };
        let Some(y) = self.y.checked_add(other.y) else {
            return None;
        };
        Some(Self { x, y })
    }
}

/// An axis-aligned rectangle on the grid, bounds included.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct GridRect {
    /// Corner with the smallest coordinates.
    pub min: GridPoint,
    /// Corner with the largest coordinates.
    pub max: GridPoint,
}

impl GridRect {
    /// The rectangle spanned by two opposite corners, in any order.
    #[must_use]
    pub fn from_corners(a: GridPoint, b: GridPoint) -> Self {
        Self {
            min: GridPoint::new(a.x.min(b.x), a.y.min(b.y)),
            max: GridPoint::new(a.x.max(b.x), a.y.max(b.y)),
        }
    }

    /// Whether the point lies inside or on the border.
    #[must_use]
    pub const fn contains(&self, point: GridPoint) -> bool {
        self.min.x <= point.x
            && point.x <= self.max.x
            && self.min.y <= point.y
            && point.y <= self.max.y
    }

    /// Whether the interiors of the two rectangles overlap. Rectangles that
    /// only share a border do not.
    #[must_use]
    pub const fn overlaps(&self, other: &Self) -> bool {
        self.min.x < other.max.x
            && other.min.x < self.max.x
            && self.min.y < other.max.y
            && other.min.y < self.max.y
    }

    /// The rectangle rotated about the origin and then moved by `offset`, or
    /// `None` when a coordinate overflows.
    #[must_use]
    pub fn placed(&self, rotation: QuarterTurn, offset: GridPoint) -> Option<Self> {
        let a = rotation.rotate(self.min)?.checked_add(offset)?;
        let b = rotation.rotate(self.max)?.checked_add(offset)?;
        Some(Self::from_corners(a, b))
    }
}

/// Rotation of a symbol in right-angle steps.
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
#[serde(rename_all = "snake_case")]
pub enum QuarterTurn {
    /// No rotation.
    #[default]
    Deg0,
    /// One quarter turn.
    Deg90,
    /// Half a turn.
    Deg180,
    /// Three quarter turns.
    Deg270,
}

impl QuarterTurn {
    /// Every rotation, in increasing angle.
    pub const ALL: [Self; 4] = [Self::Deg0, Self::Deg90, Self::Deg180, Self::Deg270];

    /// Rotate an offset about the origin.
    ///
    /// The vertical axis grows downwards, so a quarter turn is clockwise on
    /// screen: the point above the origin, `(0, -1)`, moves to its right,
    /// `(1, 0)`. Returns `None` when a coordinate is `i32::MIN` and has no
    /// opposite.
    #[must_use]
    pub const fn rotate(self, offset: GridPoint) -> Option<GridPoint> {
        let GridPoint { x, y } = offset;
        match self {
            Self::Deg0 => Some(offset),
            Self::Deg90 => match y.checked_neg() {
                Some(neg_y) => Some(GridPoint::new(neg_y, x)),
                None => None,
            },
            Self::Deg180 => match (x.checked_neg(), y.checked_neg()) {
                (Some(neg_x), Some(neg_y)) => Some(GridPoint::new(neg_x, neg_y)),
                _ => None,
            },
            Self::Deg270 => match x.checked_neg() {
                Some(neg_x) => Some(GridPoint::new(y, neg_x)),
                None => None,
            },
        }
    }

    /// The next rotation, one quarter turn further.
    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::Deg0 => Self::Deg90,
            Self::Deg90 => Self::Deg180,
            Self::Deg180 => Self::Deg270,
            Self::Deg270 => Self::Deg0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quarter_turn_moves_the_point_above_the_origin_to_its_right() {
        assert_eq!(
            QuarterTurn::Deg90.rotate(GridPoint::new(0, -1)),
            Some(GridPoint::new(1, 0))
        );
    }

    #[test]
    fn four_quarter_turns_return_every_point_to_its_place() {
        for point in [
            GridPoint::new(0, 0),
            GridPoint::new(3, -2),
            GridPoint::new(-7, 5),
            GridPoint::new(i32::MAX, i32::MAX),
        ] {
            let mut turned = point;
            for _ in 0..4 {
                turned = QuarterTurn::Deg90.rotate(turned).unwrap();
            }

            assert_eq!(turned, point);
        }
    }

    #[test]
    fn each_rotation_equals_repeated_quarter_turns() {
        let point = GridPoint::new(3, -2);
        let mut expected = point;
        for rotation in QuarterTurn::ALL {
            assert_eq!(rotation.rotate(point), Some(expected), "{rotation:?}");
            expected = QuarterTurn::Deg90.rotate(expected).unwrap();
        }
    }

    #[test]
    fn rotating_a_coordinate_without_an_opposite_is_refused() {
        let extreme = GridPoint::new(i32::MIN, i32::MIN);

        assert_eq!(
            [
                QuarterTurn::Deg90.rotate(extreme),
                QuarterTurn::Deg180.rotate(extreme),
                QuarterTurn::Deg270.rotate(extreme),
            ],
            [None, None, None]
        );
    }

    #[test]
    fn next_cycles_through_every_rotation() {
        assert_eq!(QuarterTurn::Deg270.next(), QuarterTurn::Deg0);
        assert_eq!(
            QuarterTurn::ALL.map(QuarterTurn::next),
            [
                QuarterTurn::Deg90,
                QuarterTurn::Deg180,
                QuarterTurn::Deg270,
                QuarterTurn::Deg0,
            ]
        );
    }

    #[test]
    fn checked_add_refuses_overflow() {
        assert_eq!(
            GridPoint::new(i32::MAX, 0).checked_add(GridPoint::new(1, 0)),
            None
        );
    }

    #[test]
    fn rectangles_sharing_a_border_do_not_overlap() {
        let left = GridRect::from_corners(GridPoint::new(0, 0), GridPoint::new(2, 2));
        let right = GridRect::from_corners(GridPoint::new(2, 0), GridPoint::new(4, 2));

        assert!(!left.overlaps(&right));
    }

    #[test]
    fn rectangles_with_a_common_interior_overlap() {
        let a = GridRect::from_corners(GridPoint::new(0, 0), GridPoint::new(3, 3));
        let b = GridRect::from_corners(GridPoint::new(2, 2), GridPoint::new(5, 5));

        assert!(a.overlaps(&b));
    }

    #[test]
    fn placed_rectangle_is_rotated_then_moved() {
        let tall = GridRect::from_corners(GridPoint::new(-1, -2), GridPoint::new(1, 2));

        assert_eq!(
            tall.placed(QuarterTurn::Deg90, GridPoint::new(10, 0)),
            Some(GridRect::from_corners(
                GridPoint::new(8, -1),
                GridPoint::new(12, 1)
            ))
        );
    }
}
