//! The neutral scene: drawing primitives in logical coordinates, each with a
//! semantic style and, when it represents one, the document element it
//! belongs to.
//!
//! Coordinates are integers in subunits of the document grid, so a scene is
//! exact and the same on every platform. A front end maps them to pixels and
//! each [`StyleKey`] to a colour and a stroke; nothing here knows about
//! either.

use circuit_model::{ConductorSet, GridPoint, MotorState, Subject};

/// Subunits per grid unit. Symbol artwork is drawn in subunits.
pub const SUBUNITS_PER_GRID: i64 = 10;

/// A point of the scene, in subunits; `y` grows downwards.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ScenePoint {
    /// Horizontal coordinate.
    pub x: i64,
    /// Vertical coordinate.
    pub y: i64,
}

impl ScenePoint {
    /// Shorthand constructor.
    #[must_use]
    pub const fn new(x: i64, y: i64) -> Self {
        Self { x, y }
    }

    /// The scene point of a grid point. Every `i32` grid coordinate fits.
    #[must_use]
    pub fn from_grid(point: GridPoint) -> Self {
        Self {
            x: i64::from(point.x).saturating_mul(SUBUNITS_PER_GRID),
            y: i64::from(point.y).saturating_mul(SUBUNITS_PER_GRID),
        }
    }

    /// Component-wise sum, or `None` on overflow.
    #[must_use]
    pub const fn checked_add(self, other: Self) -> Option<Self> {
        match (self.x.checked_add(other.x), self.y.checked_add(other.y)) {
            (Some(x), Some(y)) => Some(Self { x, y }),
            _ => None,
        }
    }
}

/// An axis-aligned rectangle of the scene, bounds included.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SceneRect {
    /// Corner with the smallest coordinates.
    pub min: ScenePoint,
    /// Corner with the largest coordinates.
    pub max: ScenePoint,
}

impl SceneRect {
    /// The rectangle spanned by two opposite corners, in any order.
    #[must_use]
    pub fn from_corners(a: ScenePoint, b: ScenePoint) -> Self {
        Self {
            min: ScenePoint::new(a.x.min(b.x), a.y.min(b.y)),
            max: ScenePoint::new(a.x.max(b.x), a.y.max(b.y)),
        }
    }

    /// The smallest rectangle holding every point, or `None` for no point.
    #[must_use]
    pub fn around(points: impl IntoIterator<Item = ScenePoint>) -> Option<Self> {
        points.into_iter().fold(None, |bounds, point| {
            Some(match bounds {
                None => Self::from_corners(point, point),
                Some(bounds) => bounds.union(&Self::from_corners(point, point)),
            })
        })
    }

    /// The smallest rectangle holding both.
    #[must_use]
    pub fn union(&self, other: &Self) -> Self {
        Self {
            min: ScenePoint::new(self.min.x.min(other.min.x), self.min.y.min(other.min.y)),
            max: ScenePoint::new(self.max.x.max(other.max.x), self.max.y.max(other.max.y)),
        }
    }

    /// Whether the point lies inside or on the border.
    #[must_use]
    pub const fn contains(&self, point: ScenePoint) -> bool {
        self.min.x <= point.x
            && point.x <= self.max.x
            && self.min.y <= point.y
            && point.y <= self.max.y
    }

    /// Whether the two rectangles have at least one point in common.
    #[must_use]
    pub const fn intersects(&self, other: &Self) -> bool {
        self.min.x <= other.max.x
            && other.min.x <= self.max.x
            && self.min.y <= other.max.y
            && other.min.y <= self.max.y
    }
}

/// Horizontal alignment of a text relative to its anchor point; the text is
/// centred vertically on it.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Anchor {
    /// The text starts at the point.
    Left,
    /// The text is centred on the point.
    Center,
}

/// A drawing primitive.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum Shape {
    /// A straight segment.
    Line {
        /// Start.
        from: ScenePoint,
        /// End.
        to: ScenePoint,
    },
    /// Connected segments through every point in order.
    Polyline(Vec<ScenePoint>),
    /// A rectangle outline.
    Rect(SceneRect),
    /// A circle outline.
    Circle {
        /// Centre.
        center: ScenePoint,
        /// Radius in subunits.
        radius: i64,
    },
    /// A dot: a filled circle, such as a junction.
    Dot {
        /// Centre.
        center: ScenePoint,
        /// Radius in subunits.
        radius: i64,
    },
    /// A filled rectangle, such as a module of a QR code (ADR-0018).
    Block(SceneRect),
    /// A line of text, always horizontal.
    Text {
        /// Anchor point.
        at: ScenePoint,
        /// The text.
        text: String,
        /// Alignment on the anchor.
        anchor: Anchor,
    },
}

impl Shape {
    /// The area the shape covers. A text covers its anchor point only: its
    /// size depends on the font, which the scene does not know.
    #[must_use]
    pub fn bounds(&self) -> Option<SceneRect> {
        match self {
            Self::Line { from, to } => Some(SceneRect::from_corners(*from, *to)),
            Self::Polyline(points) => SceneRect::around(points.iter().copied()),
            Self::Rect(rect) | Self::Block(rect) => Some(*rect),
            Self::Circle { center, radius } | Self::Dot { center, radius } => {
                let r = radius.saturating_abs();
                Some(SceneRect::from_corners(
                    ScenePoint::new(center.x.saturating_sub(r), center.y.saturating_sub(r)),
                    ScenePoint::new(center.x.saturating_add(r), center.y.saturating_add(r)),
                ))
            }
            Self::Text { at, .. } => Some(SceneRect::from_corners(*at, *at)),
        }
    }
}

/// What an item means. The theme of a front end decides how each one looks.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum StyleKey {
    /// A wire with the conductors present on it; empty when dead or when no
    /// simulation is running.
    Wire {
        /// Conductors present.
        conductors: ConductorSet,
    },
    /// A junction with the conductors present on it.
    Junction {
        /// Conductors present.
        conductors: ConductorSet,
    },
    /// Outline of a symbol at rest.
    Symbol,
    /// A load under power: an energized coil, a lit lamp, a sounding buzzer.
    Energized,
    /// A contactor or relay that is picked up, or a timer whose output is
    /// high. Differs from [`StyleKey::Energized`] during the actuation delay.
    Active,
    /// The moving part of a contact that conducts.
    ContactClosed,
    /// The mechanical link between the poles of a multi-pole contact.
    Linkage,
    /// The actuator of a pushbutton or switch that the operator holds or has
    /// closed.
    Actuated,
    /// A thermal overload relay that is tripped.
    Tripped,
    /// A motor and what it is doing.
    Motor(MotorState),
    /// An element involved in an active fault.
    Fault,
    /// An element the user selected.
    Selected,
    /// An element of the net that is being traced (ADR-0014): every terminal,
    /// junction and wire joined to the selection.
    Traced,
    /// A designator or another text.
    Label,
    /// The frame of a sheet: border, zone marks and title block.
    Frame,
    /// A symbol whose device, model or symbol key does not resolve against
    /// the catalog.
    Unknown,
}

/// One primitive of the scene.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct SceneItem {
    /// What to draw.
    pub shape: Shape,
    /// What it means.
    pub style: StyleKey,
    /// The document element it represents, for selection and navigation.
    pub subject: Option<Subject>,
}

/// Everything to draw, in drawing order: wires, junctions, symbols, labels,
/// then the fault and selection overlays.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Scene {
    /// The items, back to front.
    pub items: Vec<SceneItem>,
    /// The area covered by the items, or `None` for an empty scene.
    pub bounds: Option<SceneRect>,
    /// The exact area of one printed page, when the scene is one sheet:
    /// exporters write a page of this size instead of one of the bounds plus
    /// a margin.
    pub page: Option<SceneRect>,
}

impl Scene {
    /// Add an item and grow the bounds.
    pub(crate) fn push(&mut self, shape: Shape, style: StyleKey, subject: Option<Subject>) {
        if let Some(area) = shape.bounds() {
            self.bounds = Some(match self.bounds {
                Some(bounds) => bounds.union(&area),
                None => area,
            });
        }
        self.items.push(SceneItem {
            shape,
            style,
            subject,
        });
    }

    /// The area covered by the items of one subject, if it has any.
    #[must_use]
    pub fn subject_bounds(&self, subject: &Subject) -> Option<SceneRect> {
        SceneRect::around(
            self.items
                .iter()
                .filter(|item| item.subject.as_ref() == Some(subject))
                .filter_map(|item| item.shape.bounds())
                .flat_map(|rect| [rect.min, rect.max]),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_points_scale_by_the_subunit() {
        assert_eq!(
            ScenePoint::from_grid(GridPoint::new(-3, 7)),
            ScenePoint::new(-30, 70)
        );
    }

    #[test]
    fn extreme_grid_points_fit_without_overflow() {
        assert_eq!(
            ScenePoint::from_grid(GridPoint::new(i32::MIN, i32::MAX)),
            ScenePoint::new(i64::from(i32::MIN) * 10, i64::from(i32::MAX) * 10)
        );
    }

    #[test]
    fn rectangle_around_no_point_is_none() {
        assert_eq!(SceneRect::around([]), None);
    }

    #[test]
    fn rectangle_around_points_holds_every_point() {
        assert_eq!(
            SceneRect::around([ScenePoint::new(3, -1), ScenePoint::new(-2, 4)]),
            Some(SceneRect::from_corners(
                ScenePoint::new(-2, -1),
                ScenePoint::new(3, 4)
            ))
        );
    }

    #[test]
    fn circle_bounds_extend_by_the_radius() {
        let circle = Shape::Circle {
            center: ScenePoint::new(10, 10),
            radius: 5,
        };

        assert_eq!(
            circle.bounds(),
            Some(SceneRect::from_corners(
                ScenePoint::new(5, 5),
                ScenePoint::new(15, 15)
            ))
        );
    }

    #[test]
    fn pushing_items_grows_the_scene_bounds() {
        let mut scene = Scene::default();
        scene.push(
            Shape::Line {
                from: ScenePoint::new(0, 0),
                to: ScenePoint::new(10, 0),
            },
            StyleKey::Symbol,
            None,
        );
        scene.push(
            Shape::Line {
                from: ScenePoint::new(0, -5),
                to: ScenePoint::new(0, 20),
            },
            StyleKey::Symbol,
            None,
        );

        assert_eq!(
            scene.bounds,
            Some(SceneRect::from_corners(
                ScenePoint::new(0, -5),
                ScenePoint::new(10, 20)
            ))
        );
    }

    #[test]
    fn subject_bounds_cover_only_the_items_of_that_subject() {
        let mut scene = Scene::default();
        let wire = Subject::Wire(circuit_model::WireId(1));
        scene.push(
            Shape::Line {
                from: ScenePoint::new(0, 0),
                to: ScenePoint::new(10, 0),
            },
            StyleKey::Symbol,
            Some(wire.clone()),
        );
        scene.push(
            Shape::Line {
                from: ScenePoint::new(100, 100),
                to: ScenePoint::new(200, 100),
            },
            StyleKey::Symbol,
            None,
        );

        assert_eq!(
            scene.subject_bounds(&wire),
            Some(SceneRect::from_corners(
                ScenePoint::new(0, 0),
                ScenePoint::new(10, 0)
            ))
        );
    }
}
