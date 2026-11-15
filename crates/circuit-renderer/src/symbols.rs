//! Artwork of each symbol kind.
//!
//! The drawings are original to this project: plain lines, rectangles and
//! circles that follow common ladder-diagram conventions. No third-party
//! symbol file is copied or traced.
//!
//! Each drawing is in local subunits around the symbol origin, unrotated and
//! top to bottom, with its leads ending exactly on the terminal offsets of the
//! catalog. It stays inside the bounds of its kind, except for texts.

use circuit_model::{MotorState, QuarterTurn, SymbolKind, catalog::POLE_PITCH};

use crate::scene::{Anchor, SUBUNITS_PER_GRID, ScenePoint, SceneRect, Shape, StyleKey};

/// Where the leads of a two-terminal element meet its body.
const BODY: i64 = 8;
/// Where the terminals are: two grid units above and below the origin.
const LEAD: i64 = 2 * SUBUNITS_PER_GRID;

/// The operator part drawn next to a contact.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Actuator {
    /// Driven by a coil, a timer or a thermal relay: no operator part.
    None,
    /// Momentary pushbutton.
    Push,
    /// Maintained switch.
    Latch,
}

/// The state a drawing shows.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct Look {
    /// The contact conducts.
    pub(crate) closed: bool,
    /// The load is under power.
    pub(crate) energized: bool,
    /// The contactor or relay is picked up, or the timer output is high.
    pub(crate) active: bool,
    /// The operator holds the button or has closed the switch.
    pub(crate) actuated: bool,
    /// The thermal relay is tripped.
    pub(crate) tripped: bool,
    /// What the motor does, when a simulation runs.
    pub(crate) motor: Option<MotorState>,
}

const fn point(x: i64, y: i64) -> ScenePoint {
    ScenePoint::new(x, y)
}

fn line(from: (i64, i64), to: (i64, i64)) -> Shape {
    Shape::Line {
        from: point(from.0, from.1),
        to: point(to.0, to.1),
    }
}

fn polyline(points: &[(i64, i64)]) -> Shape {
    Shape::Polyline(points.iter().map(|&(x, y)| point(x, y)).collect())
}

fn rect(left: i64, top: i64, right: i64, bottom: i64) -> Shape {
    Shape::Rect(SceneRect::from_corners(
        point(left, top),
        point(right, bottom),
    ))
}

fn text(at: (i64, i64), content: &str) -> Shape {
    Shape::Text {
        at: point(at.0, at.1),
        text: content.to_owned(),
        anchor: Anchor::Center,
    }
}

/// Horizontal offset, in subunits, of pole `index` (0-based) of `poles`.
/// Mirrors the catalog: poles one pitch apart, centred on the origin.
fn pole_x(index: u8, poles: u8) -> i64 {
    let steps = i64::from(index)
        .saturating_mul(2)
        .saturating_add(1)
        .saturating_sub(i64::from(poles));
    let half_pitch = i64::from(POLE_PITCH)
        .saturating_mul(SUBUNITS_PER_GRID)
        .checked_div(2)
        .unwrap_or(0);
    steps.saturating_mul(half_pitch)
}

fn shifted(x: i64, (a, b): (i64, i64)) -> (i64, i64) {
    (a.saturating_add(x), b)
}

/// The two leads of a vertical element at `x`, from the terminals to the
/// body.
fn leads(x: i64, out: &mut Vec<(Shape, StyleKey)>) {
    let above = LEAD.saturating_neg();
    let body_top = BODY.saturating_neg();
    out.push((line((x, above), (x, body_top)), StyleKey::Symbol));
    out.push((line((x, BODY), (x, LEAD)), StyleKey::Symbol));
}

/// A normally open or normally closed contact at `x`. The blade pivots on
/// the lower lead; closed, it reaches the upper lead (or the stub of a
/// normally closed contact); open, it swings to the left.
fn contact(x: i64, normally_closed: bool, closed: bool, out: &mut Vec<(Shape, StyleKey)>) {
    leads(x, out);
    let body_top = BODY.saturating_neg();
    let stub_end = shifted(x, (6, body_top));
    if normally_closed {
        out.push((line((x, body_top), stub_end), StyleKey::Symbol));
    }
    let blade = if closed {
        let tip = if normally_closed {
            stub_end
        } else {
            (x, body_top)
        };
        (line((x, BODY), tip), StyleKey::ContactClosed)
    } else {
        (line((x, BODY), shifted(x, (-8, -6))), StyleKey::Symbol)
    };
    out.push(blade);
}

/// A changeover contact: the blade pivots on the `com` lead below; at rest
/// it rests on the stub of the `nc` lead, which comes down 2 grid units to
/// the right; picked up, it reaches the `no` lead straight above. The blade
/// always conducts on one side.
fn changeover(no_side: bool, out: &mut Vec<(Shape, StyleKey)>) {
    leads(0, out);
    let body_top = BODY.saturating_neg();
    let nc_x = LEAD;
    out.push((
        line((nc_x, LEAD.saturating_neg()), (nc_x, body_top)),
        StyleKey::Symbol,
    ));
    out.push((line((nc_x, body_top), (6, body_top)), StyleKey::Symbol));
    let tip = if no_side {
        (0, body_top)
    } else {
        (6, body_top)
    };
    out.push((line((0, BODY), tip), StyleKey::ContactClosed));
}

/// The operator part of a contact whose blade is at `x`: a push rod for a
/// pushbutton, a bracketed rod for a maintained switch.
fn actuator(x: i64, kind: Actuator, actuated: bool, out: &mut Vec<(Shape, StyleKey)>) {
    let style = if actuated {
        StyleKey::Actuated
    } else {
        StyleKey::Symbol
    };
    match kind {
        Actuator::None => {}
        Actuator::Push => {
            out.push((line(shifted(x, (-4, 0)), shifted(x, (-14, 0))), style));
            out.push((line(shifted(x, (-14, -5)), shifted(x, (-14, 5))), style));
        }
        Actuator::Latch => {
            out.push((line(shifted(x, (-4, 0)), shifted(x, (-14, 0))), style));
            out.push((
                polyline(&[
                    shifted(x, (-11, -5)),
                    shifted(x, (-14, -5)),
                    shifted(x, (-14, 5)),
                    shifted(x, (-11, 5)),
                ]),
                style,
            ));
        }
    }
}

/// The drawing of one symbol in local subunits.
pub(crate) fn art(kind: SymbolKind, operator: Actuator, look: Look) -> Vec<(Shape, StyleKey)> {
    let mut out = Vec::new();
    let body_top = BODY.saturating_neg();
    match kind {
        SymbolKind::ContactNo | SymbolKind::ContactNc => {
            contact(0, kind == SymbolKind::ContactNc, look.closed, &mut out);
            actuator(0, operator, look.actuated, &mut out);
        }
        SymbolKind::Changeover => changeover(look.closed, &mut out),
        SymbolKind::InterlockLink => {
            // A dashed bar with two triangles meeting in the middle: the two
            // contactors push against one another.
            for (from, to) in [(-20, -14), (-11, -6), (6, 11), (14, 20)] {
                out.push((line((from, 0), (to, 0)), StyleKey::Linkage));
            }
            out.push((
                polyline(&[(-5, -5), (0, 0), (-5, 5), (-5, -5)]),
                StyleKey::Symbol,
            ));
            out.push((
                polyline(&[(5, -5), (0, 0), (5, 5), (5, -5)]),
                StyleKey::Symbol,
            ));
        }
        SymbolKind::PowerContacts { poles } => {
            for index in 0..poles {
                contact(pole_x(index, poles), false, look.closed, &mut out);
            }
            let first = pole_x(0, poles);
            let last = pole_x(poles.saturating_sub(1), poles);
            let link_y = if look.closed { 0 } else { 1 };
            let link_start = if look.closed {
                first
            } else {
                first.saturating_sub(4)
            };
            out.push((
                line((link_start, link_y), (last, link_y)),
                StyleKey::Linkage,
            ));
            actuator(first, operator, look.actuated, &mut out);
        }
        SymbolKind::Coil => {
            leads(0, &mut out);
            let style = if look.energized {
                StyleKey::Energized
            } else {
                StyleKey::Symbol
            };
            out.push((rect(-10, body_top, 10, BODY), style));
            if look.active {
                out.push((line((-10, body_top), (10, BODY)), StyleKey::Active));
            }
        }
        SymbolKind::FuseLink => {
            out.push((
                line((0, LEAD.saturating_neg()), (0, LEAD)),
                StyleKey::Symbol,
            ));
            out.push((rect(-4, -12, 4, 12), StyleKey::Symbol));
        }
        SymbolKind::ThermalElement { poles } => {
            let style = if look.tripped {
                StyleKey::Tripped
            } else {
                StyleKey::Symbol
            };
            for index in 0..poles {
                let x = pole_x(index, poles);
                leads(x, &mut out);
                out.push((
                    rect(x.saturating_sub(5), body_top, x.saturating_add(5), BODY),
                    style,
                ));
                out.push((
                    polyline(&[
                        (x, body_top),
                        (x, -3),
                        shifted(x, (3, -3)),
                        shifted(x, (3, 3)),
                        (x, 3),
                        (x, BODY),
                    ]),
                    style,
                ));
            }
        }
        SymbolKind::Lamp => {
            leads(0, &mut out);
            let style = if look.energized {
                StyleKey::Energized
            } else {
                StyleKey::Symbol
            };
            out.push((
                Shape::Circle {
                    center: point(0, 0),
                    radius: BODY,
                },
                style,
            ));
            out.push((line((-6, -6), (6, 6)), style));
            out.push((line((-6, 6), (6, -6)), style));
        }
        SymbolKind::Buzzer => {
            let style = if look.energized {
                StyleKey::Energized
            } else {
                StyleKey::Symbol
            };
            out.push((line((0, LEAD.saturating_neg()), (0, -4)), StyleKey::Symbol));
            out.push((line((0, 4), (0, LEAD)), StyleKey::Symbol));
            out.push((
                polyline(&[
                    (-8, 4),
                    (-7, 0),
                    (-4, -3),
                    (0, -4),
                    (4, -3),
                    (7, 0),
                    (8, 4),
                    (-8, 4),
                ]),
                style,
            ));
        }
        SymbolKind::Motor => {
            let style = look.motor.map_or(StyleKey::Symbol, StyleKey::Motor);
            let pitch = i64::from(POLE_PITCH).saturating_mul(SUBUNITS_PER_GRID);
            let above = LEAD.saturating_neg();
            out.push((
                polyline(&[
                    (pitch.saturating_neg(), above),
                    (pitch.saturating_neg(), 0),
                    (-15, 10),
                ]),
                StyleKey::Symbol,
            ));
            out.push((line((0, above), (0, 5)), StyleKey::Symbol));
            out.push((
                polyline(&[(pitch, above), (pitch, 0), (15, 10)]),
                StyleKey::Symbol,
            ));
            out.push((
                Shape::Circle {
                    center: point(0, 30),
                    radius: 25,
                },
                style,
            ));
            out.push((text((0, 26), "M"), StyleKey::Label));
            out.push((text((0, 40), "3~"), StyleKey::Label));
        }
        SymbolKind::SupplyTerminal => {
            out.push((line((-8, -12), (8, -12)), StyleKey::Symbol));
            out.push((line((0, -12), (0, 0)), StyleKey::Symbol));
        }
    }
    out
}

/// Rotate a local point about the origin, with the convention of
/// [`QuarterTurn::rotate`]: a quarter turn is clockwise on screen.
pub(crate) const fn rotate(rotation: QuarterTurn, p: ScenePoint) -> Option<ScenePoint> {
    let ScenePoint { x, y } = p;
    match rotation {
        QuarterTurn::Deg0 => Some(p),
        QuarterTurn::Deg90 => match y.checked_neg() {
            Some(neg_y) => Some(ScenePoint::new(neg_y, x)),
            None => None,
        },
        QuarterTurn::Deg180 => match (x.checked_neg(), y.checked_neg()) {
            (Some(neg_x), Some(neg_y)) => Some(ScenePoint::new(neg_x, neg_y)),
            _ => None,
        },
        QuarterTurn::Deg270 => match x.checked_neg() {
            Some(neg_x) => Some(ScenePoint::new(y, neg_x)),
            None => None,
        },
    }
}

/// A local shape rotated about the origin and moved to `origin`. Texts move
/// with their anchor and stay horizontal. `None` on overflow.
pub(crate) fn place(shape: &Shape, rotation: QuarterTurn, origin: ScenePoint) -> Option<Shape> {
    let map = |p: ScenePoint| rotate(rotation, p)?.checked_add(origin);
    Some(match shape {
        Shape::Line { from, to } => Shape::Line {
            from: map(*from)?,
            to: map(*to)?,
        },
        Shape::Polyline(points) => {
            Shape::Polyline(points.iter().map(|p| map(*p)).collect::<Option<Vec<_>>>()?)
        }
        Shape::Rect(rect) => Shape::Rect(SceneRect::from_corners(map(rect.min)?, map(rect.max)?)),
        Shape::Circle { center, radius } => Shape::Circle {
            center: map(*center)?,
            radius: *radius,
        },
        Shape::Dot { center, radius } => Shape::Dot {
            center: map(*center)?,
            radius: *radius,
        },
        Shape::Text { at, text, anchor } => Shape::Text {
            at: map(*at)?,
            text: text.clone(),
            anchor: *anchor,
        },
    })
}

#[cfg(test)]
mod tests {
    use circuit_model::catalog::MAX_POLES;
    use circuit_model::{GridPoint, MotorState};

    use super::*;

    fn every_kind() -> Vec<SymbolKind> {
        let mut kinds = vec![
            SymbolKind::Coil,
            SymbolKind::ContactNo,
            SymbolKind::ContactNc,
            SymbolKind::Changeover,
            SymbolKind::InterlockLink,
            SymbolKind::FuseLink,
            SymbolKind::Motor,
            SymbolKind::Lamp,
            SymbolKind::Buzzer,
            SymbolKind::SupplyTerminal,
        ];
        for poles in 1..=MAX_POLES {
            kinds.push(SymbolKind::PowerContacts { poles });
            kinds.push(SymbolKind::ThermalElement { poles });
        }
        kinds
    }

    fn every_look() -> Vec<Look> {
        let mut looks = Vec::new();
        for bits in 0_u8..32 {
            looks.push(Look {
                closed: bits & 1 != 0,
                energized: bits & 2 != 0,
                active: bits & 4 != 0,
                actuated: bits & 8 != 0,
                tripped: bits & 16 != 0,
                motor: Some(MotorState::Forward),
            });
        }
        looks.push(Look::default());
        looks
    }

    fn endpoints(shape: &Shape) -> Vec<ScenePoint> {
        match shape {
            Shape::Line { from, to } => vec![*from, *to],
            Shape::Polyline(points) => points.clone(),
            _ => Vec::new(),
        }
    }

    #[test]
    fn every_terminal_is_reached_by_a_lead() {
        for kind in every_kind() {
            for operator in [Actuator::None, Actuator::Push, Actuator::Latch] {
                for look in every_look() {
                    let drawn: Vec<ScenePoint> = art(kind, operator, look)
                        .iter()
                        .flat_map(|(shape, _)| endpoints(shape))
                        .collect();
                    for terminal in kind.terminals() {
                        let expected = ScenePoint::from_grid(terminal.offset);

                        assert!(
                            drawn.contains(&expected),
                            "{kind:?} {}: no lead ends at {expected:?}",
                            terminal.key.0
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn artwork_stays_inside_the_symbol_bounds() {
        for kind in every_kind() {
            let bounds = kind.bounds();
            let area = SceneRect::from_corners(
                ScenePoint::from_grid(bounds.min),
                ScenePoint::from_grid(bounds.max),
            );
            for operator in [Actuator::None, Actuator::Push, Actuator::Latch] {
                for look in every_look() {
                    for (shape, _) in art(kind, operator, look) {
                        if matches!(shape, Shape::Text { .. }) {
                            continue;
                        }
                        let covered = shape.bounds().unwrap();

                        assert!(
                            area.contains(covered.min) && area.contains(covered.max),
                            "{kind:?}: {shape:?} leaves {area:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn subunit_pole_offsets_match_the_catalog() {
        for poles in 1..=MAX_POLES {
            let kind = SymbolKind::PowerContacts { poles };
            let catalog: Vec<i64> = kind
                .terminals()
                .iter()
                .step_by(2)
                .map(|t| ScenePoint::from_grid(t.offset).x)
                .collect();
            let drawn: Vec<i64> = (0..poles).map(|index| pole_x(index, poles)).collect();

            assert_eq!(drawn, catalog, "{poles} poles");
        }
    }

    #[test]
    fn closed_contact_blade_uses_the_closed_style() {
        let drawn = art(
            SymbolKind::ContactNo,
            Actuator::None,
            Look {
                closed: true,
                ..Look::default()
            },
        );

        assert!(
            drawn
                .iter()
                .any(|(_, style)| *style == StyleKey::ContactClosed)
        );
    }

    #[test]
    fn open_contact_has_no_closed_blade() {
        let drawn = art(SymbolKind::ContactNc, Actuator::None, Look::default());

        assert!(
            !drawn
                .iter()
                .any(|(_, style)| *style == StyleKey::ContactClosed)
        );
    }

    #[test]
    fn local_rotation_agrees_with_the_document_rotation() {
        for rotation in QuarterTurn::ALL {
            for (x, y) in [(0, -2), (3, 1), (-4, -2), (7, 0)] {
                let grid = rotation.rotate(GridPoint::new(x, y)).unwrap();

                assert_eq!(
                    rotate(rotation, ScenePoint::from_grid(GridPoint::new(x, y))),
                    Some(ScenePoint::from_grid(grid)),
                    "{rotation:?} ({x}, {y})"
                );
            }
        }
    }

    #[test]
    fn placing_beyond_the_coordinate_range_is_refused() {
        let shape = line((0, 0), (0, 20));

        assert_eq!(
            place(&shape, QuarterTurn::Deg0, ScenePoint::new(0, i64::MAX)),
            None
        );
    }

    #[test]
    fn placed_text_stays_horizontal_and_moves_with_its_anchor() {
        let shape = text((10, 0), "M");

        assert_eq!(
            place(&shape, QuarterTurn::Deg90, ScenePoint::new(100, 100)),
            Some(Shape::Text {
                at: ScenePoint::new(100, 110),
                text: "M".to_owned(),
                anchor: Anchor::Center,
            })
        );
    }
}
