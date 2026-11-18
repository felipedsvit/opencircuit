//! Documentation data of schema version 5 (ADR-0018): the client, what a
//! wire is made of, shapes and QR codes, comments, groups, device views and
//! PLC modules.
//!
//! None of it has electrical meaning. The compiler and the simulator read
//! none of these types; a project with them and the same project without
//! them compile to the same circuit.

use serde::{Deserialize, Serialize};

use crate::diagnostics::Subject;
use crate::geometry::{GridPoint, GridRect};
use crate::ids::{CommentId, DeviceId, GroupId, LayerId, PlcId, ShapeId, ViewId};

/// Most characters of the client.
pub const MAX_CLIENT_CHARS: usize = 64;
/// Most characters of each field of a wire specification.
pub const MAX_SPEC_CHARS: usize = 32;
/// Most characters of a comment.
pub const MAX_COMMENT_CHARS: usize = 200;
/// Most characters of the author of a comment.
pub const MAX_AUTHOR_CHARS: usize = 32;
/// Most characters of the name of a group.
pub const MAX_GROUP_NAME_CHARS: usize = 32;
/// Most members of a group.
pub const MAX_GROUP_MEMBERS: usize = 1_024;
/// Most points of a polyline.
pub const MAX_POLYLINE_POINTS: usize = 64;
/// Largest radius of a circle or an arc, in grid units.
pub const MAX_RADIUS: u32 = 1_000;
/// Most characters of the revision mark of a cloud.
pub const MAX_REVISION_CHARS: usize = 8;
/// Smallest and largest side of a QR code, in grid units.
pub const QR_SIDE: (u32, u32) = (2, 100);
/// Largest side of a device view, in grid units.
pub const MAX_VIEW_SIDE: u32 = 200;
/// Most channels of a PLC module.
pub const MAX_PLC_CHANNELS: usize = 256;
/// Most characters of a channel address.
pub const MAX_ADDRESS_CHARS: usize = 16;
/// Most characters of the description of a PLC module.
pub const MAX_PLC_DESCRIPTION_CHARS: usize = 64;
/// Most characters of the label of a channel.
pub const MAX_CHANNEL_LABEL_CHARS: usize = 32;

/// Whether `text` is 1 to `max_chars` characters and at most
/// [`crate::limits::MAX_TEXT_BYTES`] bytes, with no control character.
#[must_use]
pub fn is_valid_text(text: &str, max_chars: usize) -> bool {
    !text.is_empty()
        && text.chars().count() <= max_chars
        && text.len() <= crate::limits::MAX_TEXT_BYTES
        && !text.chars().any(char::is_control)
}

/// Whether `link` may name a datasheet: an `http://`, `https://` or `file:`
/// URL, or a relative path, with no space or control character; at most
/// [`crate::limits::MAX_TEXT_BYTES`] bytes.
#[must_use]
pub fn is_valid_datasheet(link: &str) -> bool {
    if link.is_empty()
        || link.len() > crate::limits::MAX_TEXT_BYTES
        || link.chars().any(|c| c.is_control() || c.is_whitespace())
    {
        return false;
    }
    let lower = link.to_ascii_lowercase();
    match lower.split_once(':') {
        // A scheme: only these three.
        Some((scheme, _))
            if scheme
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.')
                && scheme.len() > 1 =>
        {
            lower.starts_with("http://")
                || lower.starts_with("https://")
                || lower.starts_with("file:")
        }
        // No scheme, or a drive letter: a path, which must be relative.
        _ => !link.starts_with('/') && !link.starts_with('\\') && !link.contains(':'),
    }
}

/// What a wire is made of (ADR-0018). Every field is optional text.
#[derive(Clone, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WireSpec {
    /// The colour of the insulation, such as `BU` or `black`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub colour: Option<String>,
    /// The cross-section, such as `1.5 mm²` or `16 AWG`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cross_section: Option<String>,
    /// The type of the conductor, such as `H07V-K`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// The material of the conductor, such as `Cu`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub material: Option<String>,
    /// The insulation, such as `PVC`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub insulation: Option<String>,
}

impl WireSpec {
    /// The names of the fields, in report order.
    pub const FIELDS: [&'static str; 5] =
        ["colour", "cross_section", "kind", "material", "insulation"];

    /// The fields, in the order of [`FIELDS`](Self::FIELDS).
    #[must_use]
    pub const fn values(&self) -> [&Option<String>; 5] {
        [
            &self.colour,
            &self.cross_section,
            &self.kind,
            &self.material,
            &self.insulation,
        ]
    }

    /// The field called `name`, to change it.
    pub fn field_mut(&mut self, name: &str) -> Option<&mut Option<String>> {
        match name {
            "colour" => Some(&mut self.colour),
            "cross_section" => Some(&mut self.cross_section),
            "kind" => Some(&mut self.kind),
            "material" => Some(&mut self.material),
            "insulation" => Some(&mut self.insulation),
            _ => None,
        }
    }

    /// Whether no field is set.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.values().iter().all(|value| value.is_none())
    }

    /// Whether every set field is acceptable.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.values()
            .iter()
            .filter_map(|value| value.as_deref())
            .all(|value| is_valid_text(value, MAX_SPEC_CHARS))
    }
}

/// The geometry of a shape (ADR-0018). Everything is on the grid.
#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ShapeKind {
    /// Straight segments through 2 to [`MAX_POLYLINE_POINTS`] points, back to
    /// the first when closed.
    Polyline {
        /// The points, in order.
        points: Vec<GridPoint>,
        /// Whether the last point joins the first.
        closed: bool,
    },
    /// A rectangle given by two opposite corners.
    Rectangle {
        /// One corner.
        from: GridPoint,
        /// The opposite corner.
        to: GridPoint,
    },
    /// A circle.
    Circle {
        /// The centre.
        center: GridPoint,
        /// The radius, 1 to [`MAX_RADIUS`] grid units.
        radius: u32,
    },
    /// An arc of a circle, counter-clockwise as seen on the sheet from
    /// `start` degrees (0 is to the right) through `sweep` degrees.
    Arc {
        /// The centre.
        center: GridPoint,
        /// The radius, 1 to [`MAX_RADIUS`] grid units.
        radius: u32,
        /// Where the arc starts, 0 to 359 degrees.
        start: u16,
        /// How far it goes, 1 to 360 degrees.
        sweep: u16,
    },
    /// A revision cloud around a rectangle, with an optional revision mark.
    Cloud {
        /// One corner.
        from: GridPoint,
        /// The opposite corner.
        to: GridPoint,
        /// The revision mark, at most [`MAX_REVISION_CHARS`] characters.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        revision: Option<String>,
    },
    /// A QR code: its top-left corner, its side and the text it encodes.
    Qr {
        /// The top-left corner.
        position: GridPoint,
        /// The side, in grid units, within [`QR_SIDE`].
        size: u32,
        /// What the code says, 1 to 256 bytes.
        text: String,
    },
}

impl ShapeKind {
    /// The name of the kind, as edit files and reports write it.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Polyline { .. } => "polyline",
            Self::Rectangle { .. } => "rectangle",
            Self::Circle { .. } => "circle",
            Self::Arc { .. } => "arc",
            Self::Cloud { .. } => "cloud",
            Self::Qr { .. } => "qr",
        }
    }

    /// The smallest grid rectangle around the shape; `None` for a polyline
    /// with no point or when a coordinate would leave the range.
    #[must_use]
    pub fn bounds(&self) -> Option<GridRect> {
        let around = |center: GridPoint, radius: u32| {
            let radius = i32::try_from(radius).ok()?;
            Some(GridRect {
                min: GridPoint::new(center.x.checked_sub(radius)?, center.y.checked_sub(radius)?),
                max: GridPoint::new(center.x.checked_add(radius)?, center.y.checked_add(radius)?),
            })
        };
        match self {
            Self::Polyline { points, .. } => {
                let first = *points.first()?;
                Some(points.iter().fold(
                    GridRect {
                        min: first,
                        max: first,
                    },
                    |rect, point| GridRect {
                        min: GridPoint::new(rect.min.x.min(point.x), rect.min.y.min(point.y)),
                        max: GridPoint::new(rect.max.x.max(point.x), rect.max.y.max(point.y)),
                    },
                ))
            }
            Self::Rectangle { from, to } | Self::Cloud { from, to, .. } => Some(GridRect {
                min: GridPoint::new(from.x.min(to.x), from.y.min(to.y)),
                max: GridPoint::new(from.x.max(to.x), from.y.max(to.y)),
            }),
            Self::Circle { center, radius } | Self::Arc { center, radius, .. } => {
                around(*center, *radius)
            }
            Self::Qr { position, size, .. } => {
                let size = i32::try_from(*size).ok()?;
                Some(GridRect {
                    min: *position,
                    max: GridPoint::new(
                        position.x.checked_add(size)?,
                        position.y.checked_add(size)?,
                    ),
                })
            }
        }
    }

    /// What is wrong with the shape, if anything: a bound of ADR-0018.
    #[must_use]
    pub fn problem(&self) -> Option<String> {
        match self {
            Self::Polyline { points, .. } => (!(2..=MAX_POLYLINE_POINTS).contains(&points.len()))
                .then(|| format!("a polyline has 2 to {MAX_POLYLINE_POINTS} points")),
            Self::Rectangle { from, to } | Self::Cloud { from, to, .. }
                if from.x == to.x || from.y == to.y =>
            {
                Some("a rectangle or cloud needs two corners apart in x and y".to_owned())
            }
            Self::Cloud {
                revision: Some(revision),
                ..
            } if !is_valid_text(revision, MAX_REVISION_CHARS) => Some(format!(
                "a revision mark is 1 to {MAX_REVISION_CHARS} characters"
            )),
            Self::Circle { radius, .. } | Self::Arc { radius, .. }
                if !(1..=MAX_RADIUS).contains(radius) =>
            {
                Some(format!("a radius is 1 to {MAX_RADIUS} grid units"))
            }
            Self::Arc { start, sweep, .. } if *start >= 360 || !(1..=360).contains(sweep) => {
                Some("an arc starts at 0 to 359 degrees and sweeps 1 to 360".to_owned())
            }
            Self::Qr { size, text, .. } => {
                if !(QR_SIDE.0..=QR_SIDE.1).contains(size) {
                    Some(format!(
                        "a QR code is {} to {} grid units wide",
                        QR_SIDE.0, QR_SIDE.1
                    ))
                } else if text.is_empty() || text.len() > crate::limits::MAX_TEXT_BYTES {
                    Some("a QR code encodes 1 to 256 bytes".to_owned())
                } else {
                    None
                }
            }
            Self::Rectangle { .. }
            | Self::Cloud { .. }
            | Self::Circle { .. }
            | Self::Arc { .. } => None,
        }
        .or_else(|| {
            self.bounds()
                .is_none()
                .then(|| "the shape leaves the coordinate range".to_owned())
        })
    }

    /// The same shape moved by `dx`, `dy` grid units; `None` when a point
    /// would leave the range.
    #[must_use]
    pub fn moved(&self, dx: i32, dy: i32) -> Option<Self> {
        let shift = |point: GridPoint| {
            Some(GridPoint::new(
                point.x.checked_add(dx)?,
                point.y.checked_add(dy)?,
            ))
        };
        Some(match self {
            Self::Polyline { points, closed } => Self::Polyline {
                points: points.iter().map(|p| shift(*p)).collect::<Option<_>>()?,
                closed: *closed,
            },
            Self::Rectangle { from, to } => Self::Rectangle {
                from: shift(*from)?,
                to: shift(*to)?,
            },
            Self::Circle { center, radius } => Self::Circle {
                center: shift(*center)?,
                radius: *radius,
            },
            Self::Arc {
                center,
                radius,
                start,
                sweep,
            } => Self::Arc {
                center: shift(*center)?,
                radius: *radius,
                start: *start,
                sweep: *sweep,
            },
            Self::Cloud { from, to, revision } => Self::Cloud {
                from: shift(*from)?,
                to: shift(*to)?,
                revision: revision.clone(),
            },
            Self::Qr {
                position,
                size,
                text,
            } => Self::Qr {
                position: shift(*position)?,
                size: *size,
                text: text.clone(),
            },
        })
    }

    /// The point the shape is moved by: its first point, corner or centre.
    #[must_use]
    pub fn anchor(&self) -> Option<GridPoint> {
        match self {
            Self::Polyline { points, .. } => points.first().copied(),
            Self::Rectangle { from, .. } | Self::Cloud { from, .. } => Some(*from),
            Self::Circle { center, .. } | Self::Arc { center, .. } => Some(*center),
            Self::Qr { position, .. } => Some(*position),
        }
    }
}

/// The sine of 0 to 90 whole degrees, times 10 000, rounded. A table rather
/// than floating point, so that every platform draws the same points.
const SINE: [i64; 91] = [
    0, 175, 349, 523, 698, 872, 1045, 1219, 1392, 1564, 1736, 1908, 2079, 2250, 2419, 2588, 2756,
    2924, 3090, 3256, 3420, 3584, 3746, 3907, 4067, 4226, 4384, 4540, 4695, 4848, 5000, 5150, 5299,
    5446, 5592, 5736, 5878, 6018, 6157, 6293, 6428, 6561, 6691, 6820, 6947, 7071, 7193, 7314, 7431,
    7547, 7660, 7771, 7880, 7986, 8090, 8192, 8290, 8387, 8480, 8572, 8660, 8746, 8829, 8910, 8988,
    9063, 9135, 9205, 9272, 9336, 9397, 9455, 9511, 9563, 9613, 9659, 9703, 9744, 9781, 9816, 9848,
    9877, 9903, 9925, 9945, 9962, 9976, 9986, 9994, 9998, 10000,
];

/// The sine and cosine of `degrees`, times 10 000.
fn sin_cos(degrees: u32) -> (i64, i64) {
    let degrees = degrees.checked_rem(360).unwrap_or(0);
    let quarter = degrees.checked_div(90).unwrap_or(0);
    let within = usize::try_from(degrees.checked_rem(90).unwrap_or(0)).unwrap_or(0);
    let rest = 90_usize.saturating_sub(within);
    let (a, b) = (
        SINE.get(within).copied().unwrap_or(0),
        SINE.get(rest).copied().unwrap_or(0),
    );
    match quarter {
        0 => (a, b),
        1 => (b, a.saturating_neg()),
        2 => (a.saturating_neg(), b.saturating_neg()),
        _ => (b.saturating_neg(), a),
    }
}

/// Points of an arc, `scale` units to a grid unit, one every 5 degrees and
/// at both ends: counter-clockwise as seen on the sheet, whose y grows
/// downwards.
fn arc_points(
    center: GridPoint,
    radius: u32,
    start: u32,
    sweep: u32,
    scale: i64,
) -> Vec<(i64, i64)> {
    let cx = i64::from(center.x).saturating_mul(scale);
    let cy = i64::from(center.y).saturating_mul(scale);
    let r = i64::from(radius).saturating_mul(scale);
    let sweep = sweep.min(360);
    let mut angles: Vec<u32> = (0..sweep)
        .step_by(5)
        .map(|a| start.saturating_add(a))
        .collect();
    angles.push(start.saturating_add(sweep));
    angles
        .into_iter()
        .map(|angle| {
            let (sin, cos) = sin_cos(angle);
            (
                cx.saturating_add(r.saturating_mul(cos).saturating_div(10_000)),
                cy.saturating_sub(r.saturating_mul(sin).saturating_div(10_000)),
            )
        })
        .collect()
}

/// Whether `p` is within `tolerance` of the segment from `a` to `b`. The
/// products are taken in `i128`, where coordinates of a document cannot
/// overflow; saturating arithmetic keeps a hostile value from panicking.
fn near_segment(a: (i64, i64), b: (i64, i64), p: (i64, i64), tolerance: i64) -> bool {
    let (ax, ay) = (i128::from(a.0), i128::from(a.1));
    let (bx, by) = (i128::from(b.0), i128::from(b.1));
    let (px, py) = (i128::from(p.0), i128::from(p.1));
    let (dx, dy) = (bx.saturating_sub(ax), by.saturating_sub(ay));
    let tolerance = i128::from(tolerance);
    let limit = tolerance.saturating_mul(tolerance);
    let square = |x: i128, y: i128| x.saturating_mul(x).saturating_add(y.saturating_mul(y));
    let length = square(dx, dy);
    let dot = px
        .saturating_sub(ax)
        .saturating_mul(dx)
        .saturating_add(py.saturating_sub(ay).saturating_mul(dy));
    if length == 0 || dot <= 0 {
        return square(px.saturating_sub(ax), py.saturating_sub(ay)) <= limit;
    }
    if dot >= length {
        return square(px.saturating_sub(bx), py.saturating_sub(by)) <= limit;
    }
    let cross = px
        .saturating_sub(ax)
        .saturating_mul(dy)
        .saturating_sub(py.saturating_sub(ay).saturating_mul(dx));
    cross.saturating_mul(cross) <= limit.saturating_mul(length)
}

impl ShapeKind {
    /// The lines of the shape, `scale` units to a grid unit: each a path
    /// through its points (a closed outline ends where it starts). A QR code
    /// has its border only; the modules are drawn by the renderer.
    #[must_use]
    pub fn paths(&self, scale: i64) -> Vec<Vec<(i64, i64)>> {
        let at = |point: GridPoint| {
            (
                i64::from(point.x).saturating_mul(scale),
                i64::from(point.y).saturating_mul(scale),
            )
        };
        let rectangle = |from: GridPoint, to: GridPoint| {
            let (a, b) = (at(from), at(to));
            vec![(a.0, a.1), (b.0, a.1), (b.0, b.1), (a.0, b.1), (a.0, a.1)]
        };
        match self {
            Self::Polyline { points, closed } => {
                let mut path: Vec<(i64, i64)> = points.iter().map(|p| at(*p)).collect();
                if let Some(first) = path.first().copied().filter(|_| *closed) {
                    path.push(first);
                }
                vec![path]
            }
            Self::Rectangle { from, to } => vec![rectangle(*from, *to)],
            Self::Circle { center, radius } => {
                vec![arc_points(
                    *center,
                    (*radius).min(MAX_RADIUS),
                    0,
                    360,
                    scale,
                )]
            }
            Self::Arc {
                center,
                radius,
                start,
                sweep,
            } => vec![arc_points(
                *center,
                (*radius).min(MAX_RADIUS),
                u32::from(*start).checked_rem(360).unwrap_or(0),
                u32::from(*sweep).clamp(1, 360),
                scale,
            )],
            Self::Cloud { from, to, .. } => vec![cloud(at(*from), at(*to), scale)],
            Self::Qr { position, size, .. } => {
                let side = i32::try_from((*size).min(QR_SIDE.1)).unwrap_or(0);
                let to = GridPoint::new(
                    position.x.saturating_add(side),
                    position.y.saturating_add(side),
                );
                vec![rectangle(*position, to)]
            }
        }
    }

    /// Whether `point` is on the shape: within one grid unit of a line, or
    /// inside a QR code.
    #[must_use]
    pub fn touches(&self, point: GridPoint) -> bool {
        if let Self::Qr { .. } = self {
            return self.bounds().is_some_and(|area| area.contains(point));
        }
        const SCALE: i64 = 10;
        let p = (
            i64::from(point.x).saturating_mul(SCALE),
            i64::from(point.y).saturating_mul(SCALE),
        );
        self.paths(SCALE).iter().any(|path| {
            path.windows(2).any(|pair| match pair {
                [a, b] => near_segment(*a, *b, p, SCALE),
                _ => false,
            }) || (path.len() == 1 && path.first().is_some_and(|a| near_segment(*a, *a, p, SCALE)))
        })
    }
}

/// A revision cloud around the rectangle from `a` to `b`: half circles of
/// about two grid units along each side, bulging outwards, clockwise.
fn cloud(a: (i64, i64), b: (i64, i64), scale: i64) -> Vec<(i64, i64)> {
    let (left, right) = (a.0.min(b.0), a.0.max(b.0));
    let (top, bottom) = (a.1.min(b.1), a.1.max(b.1));
    let sides = [
        ((left, top), (right, top)),
        ((right, top), (right, bottom)),
        ((right, bottom), (left, bottom)),
        ((left, bottom), (left, top)),
    ];
    let bump = scale.saturating_mul(2).max(1);
    let mut path = vec![(left, top)];
    for ((x0, y0), (x1, y1)) in sides {
        let (dx, dy) = (x1.saturating_sub(x0), y1.saturating_sub(y0));
        let length = dx.saturating_abs().max(dy.saturating_abs());
        // Bounded: a side of a valid cloud is at most the coordinate range.
        let count = length.checked_div(bump).unwrap_or(0).clamp(1, 10_000);
        let (ux, uy) = (dx.signum(), dy.signum());
        // The outward normal of a side walked clockwise is to its left.
        let (nx, ny) = (uy, ux.saturating_neg());
        let step_x = dx.checked_div(count).unwrap_or(0);
        let step_y = dy.checked_div(count).unwrap_or(0);
        for step in 0..count {
            let start = (
                x0.saturating_add(step_x.saturating_mul(step)),
                y0.saturating_add(step_y.saturating_mul(step)),
            );
            let end = if step.saturating_add(1) == count {
                (x1, y1)
            } else {
                (
                    start.0.saturating_add(step_x),
                    start.1.saturating_add(step_y),
                )
            };
            let half = end
                .0
                .saturating_sub(start.0)
                .saturating_abs()
                .max(end.1.saturating_sub(start.1).saturating_abs())
                .saturating_div(2);
            let middle = (
                start.0.saturating_add(end.0).saturating_div(2),
                start.1.saturating_add(end.1).saturating_div(2),
            );
            // A half circle from start to end through the outward bulge.
            for degrees in (0..=180).step_by(30) {
                let (sin, cos) = sin_cos(degrees);
                let along = half.saturating_mul(cos).saturating_div(10_000);
                let out = half.saturating_mul(sin).saturating_div(10_000);
                path.push((
                    middle
                        .0
                        .saturating_sub(ux.saturating_mul(along))
                        .saturating_add(nx.saturating_mul(out)),
                    middle
                        .1
                        .saturating_sub(uy.saturating_mul(along))
                        .saturating_add(ny.saturating_mul(out)),
                ));
            }
        }
    }
    path
}

/// A drawn shape: a line, a rectangle, a circle, an arc, a revision cloud or
/// a QR code (ADR-0018). It is documentation only.
#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Shape {
    /// Stable identifier, from the document counter.
    pub id: ShapeId,
    /// The geometry.
    pub kind: ShapeKind,
    /// The layer the shape is on (ADR-0015); absent: the default layer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layer: Option<LayerId>,
}

/// A comment about one element (ADR-0018). It is not drawn.
#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Comment {
    /// Stable identifier, from the document counter.
    pub id: CommentId,
    /// The element the comment is about.
    pub subject: Subject,
    /// The comment, 1 to [`MAX_COMMENT_CHARS`] characters.
    pub text: String,
    /// Who wrote it, at most [`MAX_AUTHOR_CHARS`] characters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
}

/// Elements that are selected together (ADR-0018).
#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Group {
    /// Stable identifier, from the document counter.
    pub id: GroupId,
    /// The name, 1 to [`MAX_GROUP_NAME_CHARS`] characters.
    pub name: String,
    /// The members: symbols, wires, junctions, texts and shapes, sorted.
    pub members: Vec<Subject>,
}

impl Group {
    /// Whether `subject` is a kind of element a group may hold.
    #[must_use]
    pub const fn may_hold(subject: &Subject) -> bool {
        matches!(
            subject,
            Subject::Symbol(_)
                | Subject::Wire(_)
                | Subject::Junction(_)
                | Subject::Text(_)
                | Subject::Shape(_)
        )
    }
}

/// A device of the schematic shown again: its footprint on a panel layout
/// or its glyph on a single-line diagram (ADR-0018). It has no terminals.
#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceView {
    /// Stable identifier, from the document counter.
    pub id: ViewId,
    /// The device shown.
    pub device_id: DeviceId,
    /// The top-left corner.
    pub position: GridPoint,
    /// The width, 1 to [`MAX_VIEW_SIDE`] grid units.
    pub width: u32,
    /// The height, 1 to [`MAX_VIEW_SIDE`] grid units.
    pub height: u32,
    /// The layer the view is on (ADR-0015); absent: the default layer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layer: Option<LayerId>,
}

impl DeviceView {
    /// The rectangle of the view; `None` when it leaves the range.
    #[must_use]
    pub fn bounds(&self) -> Option<GridRect> {
        let width = i32::try_from(self.width).ok()?;
        let height = i32::try_from(self.height).ok()?;
        Some(GridRect {
            min: self.position,
            max: GridPoint::new(
                self.position.x.checked_add(width)?,
                self.position.y.checked_add(height)?,
            ),
        })
    }

    /// Whether the sides are within the bounds of ADR-0018.
    #[must_use]
    pub fn has_valid_size(&self) -> bool {
        (1..=MAX_VIEW_SIDE).contains(&self.width) && (1..=MAX_VIEW_SIDE).contains(&self.height)
    }
}

/// Which way a PLC channel carries a signal.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlcDirection {
    /// The PLC reads the device.
    Input,
    /// The PLC drives the device.
    Output,
}

impl PlcDirection {
    /// The name, as edit files and reports write it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Input => "input",
            Self::Output => "output",
        }
    }
}

/// One channel of a PLC module.
#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlcChannel {
    /// The address, such as `I0.0`, 1 to [`MAX_ADDRESS_CHARS`] characters.
    pub address: String,
    /// Input or output.
    pub direction: PlcDirection,
    /// The device the channel reads or drives.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device: Option<DeviceId>,
    /// What the signal is, at most [`MAX_CHANNEL_LABEL_CHARS`] characters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// A PLC module with its channels (ADR-0018). Documentation only: a PLC
/// program runs outside OpenCircuit.
#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlcModule {
    /// Stable identifier, from the document counter.
    pub id: PlcId,
    /// The name, which follows the rule of a reference designator.
    pub name: String,
    /// What the module is, at most [`MAX_PLC_DESCRIPTION_CHARS`] characters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Where the box listing the channels is drawn; absent: not drawn.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<GridPoint>,
    /// The channels, at most [`MAX_PLC_CHANNELS`].
    pub channels: Vec<PlcChannel>,
}

impl PlcModule {
    /// What is wrong with the module, if anything: a bound of ADR-0018.
    #[must_use]
    pub fn problem(&self) -> Option<String> {
        if !crate::schematic::ReferenceDesignator(self.name.clone()).is_valid() {
            return Some(format!(
                "PLC module name `{}` must be an uppercase letter followed by letters, digits or underscores",
                self.name
            ));
        }
        if self
            .description
            .as_deref()
            .is_some_and(|text| !is_valid_text(text, MAX_PLC_DESCRIPTION_CHARS))
        {
            return Some(format!(
                "a PLC description is 1 to {MAX_PLC_DESCRIPTION_CHARS} characters"
            ));
        }
        if self.channels.len() > MAX_PLC_CHANNELS {
            return Some(format!(
                "a PLC module has at most {MAX_PLC_CHANNELS} channels"
            ));
        }
        self.channels.iter().find_map(|channel| {
            if !is_valid_text(&channel.address, MAX_ADDRESS_CHARS)
                || channel.address.chars().any(char::is_whitespace)
            {
                Some(format!(
                    "a channel address is 1 to {MAX_ADDRESS_CHARS} characters with no space"
                ))
            } else if channel
                .label
                .as_deref()
                .is_some_and(|text| !is_valid_text(text, MAX_CHANNEL_LABEL_CHARS))
            {
                Some(format!(
                    "a channel label is 1 to {MAX_CHANNEL_LABEL_CHARS} characters"
                ))
            } else {
                None
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn datasheet_links_are_web_or_file_urls_or_relative_paths() {
        for good in [
            "https://example.com/k1.pdf",
            "http://example.com/a?b=c",
            "file:///home/me/k1.pdf",
            "docs/k1.pdf",
            "k1.pdf",
        ] {
            assert!(is_valid_datasheet(good), "{good}");
        }
        for bad in [
            "",
            "javascript:alert(1)",
            "cmd:run",
            "/etc/passwd",
            "C:\\data\\k1.pdf",
            "\\\\server\\share",
            "docs/my file.pdf",
            "https://example.com/\n",
        ] {
            assert!(!is_valid_datasheet(bad), "{bad}");
        }
    }

    #[test]
    fn shape_bounds_and_problems() {
        let rect = ShapeKind::Rectangle {
            from: GridPoint::new(4, 1),
            to: GridPoint::new(0, 3),
        };
        assert_eq!(
            rect.bounds(),
            Some(GridRect {
                min: GridPoint::new(0, 1),
                max: GridPoint::new(4, 3)
            })
        );
        assert_eq!(rect.problem(), None);
        let flat = ShapeKind::Rectangle {
            from: GridPoint::new(0, 1),
            to: GridPoint::new(4, 1),
        };
        assert!(flat.problem().is_some());
        let line = ShapeKind::Polyline {
            points: vec![GridPoint::new(0, 0)],
            closed: false,
        };
        assert!(line.problem().is_some());
        let arc = ShapeKind::Arc {
            center: GridPoint::new(0, 0),
            radius: 3,
            start: 360,
            sweep: 90,
        };
        assert!(arc.problem().is_some());
        let qr = ShapeKind::Qr {
            position: GridPoint::new(0, 0),
            size: 1,
            text: "x".to_owned(),
        };
        assert!(qr.problem().is_some());
        let far = ShapeKind::Circle {
            center: GridPoint::new(i32::MAX, 0),
            radius: 5,
        };
        assert!(far.bounds().is_none());
        assert!(far.problem().is_some());
    }

    #[test]
    fn the_sine_table_gives_the_four_quarters() {
        assert_eq!(sin_cos(0), (0, 10_000));
        assert_eq!(sin_cos(90), (10_000, 0));
        assert_eq!(sin_cos(180), (0, -10_000));
        assert_eq!(sin_cos(270), (-10_000, 0));
        assert_eq!(sin_cos(30), (5_000, 8_660));
        assert_eq!(sin_cos(390), sin_cos(30));
    }

    #[test]
    fn an_arc_goes_counter_clockwise_on_the_sheet() {
        let arc = ShapeKind::Arc {
            center: GridPoint::new(0, 0),
            radius: 2,
            start: 0,
            sweep: 90,
        };
        let path = arc.paths(10).remove(0);
        assert_eq!(path.first(), Some(&(20, 0)));
        // Up the sheet is towards smaller y.
        assert_eq!(path.last(), Some(&(0, -20)));
        assert!(arc.touches(GridPoint::new(0, -2)));
        assert!(!arc.touches(GridPoint::new(0, 2)));
    }

    #[test]
    fn shapes_are_touched_near_their_lines_and_qr_codes_inside() {
        let rect = ShapeKind::Rectangle {
            from: GridPoint::new(0, 0),
            to: GridPoint::new(10, 6),
        };
        assert!(rect.touches(GridPoint::new(5, 0)));
        assert!(rect.touches(GridPoint::new(10, 3)));
        assert!(!rect.touches(GridPoint::new(5, 3)));
        let line = ShapeKind::Polyline {
            points: vec![GridPoint::new(0, 0), GridPoint::new(10, 5)],
            closed: false,
        };
        assert!(line.touches(GridPoint::new(4, 2)));
        assert!(!line.touches(GridPoint::new(4, 6)));
        let circle = ShapeKind::Circle {
            center: GridPoint::new(0, 0),
            radius: 5,
        };
        assert!(circle.touches(GridPoint::new(0, 5)));
        assert!(!circle.touches(GridPoint::new(0, 0)));
        let qr = ShapeKind::Qr {
            position: GridPoint::new(0, 0),
            size: 4,
            text: "x".to_owned(),
        };
        assert!(qr.touches(GridPoint::new(2, 2)));
        assert!(!qr.touches(GridPoint::new(5, 2)));
        let cloud = ShapeKind::Cloud {
            from: GridPoint::new(0, 0),
            to: GridPoint::new(8, 4),
            revision: None,
        };
        let path = cloud.paths(10).remove(0);
        // The bumps bulge outside the rectangle, never inside.
        assert!(path.iter().any(|(_, y)| *y < 0));
        assert!(cloud.touches(GridPoint::new(4, 0)));
        assert!(!cloud.touches(GridPoint::new(4, 2)));
    }

    #[test]
    fn a_moved_shape_keeps_its_size() {
        let circle = ShapeKind::Circle {
            center: GridPoint::new(1, 2),
            radius: 4,
        };
        assert_eq!(
            circle.moved(2, -1),
            Some(ShapeKind::Circle {
                center: GridPoint::new(3, 1),
                radius: 4
            })
        );
        assert_eq!(circle.moved(i32::MAX, 0), None);
    }

    #[test]
    fn plc_module_bounds() {
        let mut module = PlcModule {
            id: PlcId(1),
            name: "A1".to_owned(),
            description: None,
            position: None,
            channels: vec![PlcChannel {
                address: "I0.0".to_owned(),
                direction: PlcDirection::Input,
                device: None,
                label: None,
            }],
        };
        assert_eq!(module.problem(), None);
        module.name = "a1".to_owned();
        assert!(module.problem().is_some());
        module.name = "A1".to_owned();
        if let Some(channel) = module.channels.first_mut() {
            channel.address = "I0 .0".to_owned();
        }
        assert!(module.problem().is_some());
    }

    #[test]
    fn wire_spec_fields_are_named_and_bounded() {
        let mut spec = WireSpec::default();
        assert!(spec.is_empty() && spec.is_valid());
        for name in WireSpec::FIELDS {
            *spec.field_mut(name).unwrap() = Some(name.to_owned());
        }
        assert!(!spec.is_empty() && spec.is_valid());
        spec.colour = Some("x".repeat(MAX_SPEC_CHARS + 1));
        assert!(!spec.is_valid());
        assert!(spec.field_mut("other").is_none());
    }
}
