//! Extension libraries (ADR-0008): third-party component models given as
//! data, and the artwork a library may draw for its symbols.
//!
//! A library reuses the closed behaviour of the core: its models name an
//! existing class and existing symbol kinds, and are checked like the
//! built-in ones. Nothing in a library is executed. This module holds the
//! types and the checks; reading the JSON text is `circuit-project`'s job.

use crate::catalog::{ComponentModel, Provenance, SymbolKind};
use crate::limits::MAX_TEXT_BYTES;

/// Most models one library may define.
pub const MAX_LIBRARY_MODELS: usize = 256;
/// Most symbols one library model may define.
pub const MAX_LIBRARY_SYMBOLS: usize = 32;
/// Most primitives in one artwork list, and most points in one polyline.
pub const MAX_ART_PRIMITIVES: usize = 256;
/// Most bytes of library text.
pub const MAX_LIBRARY_BYTES: usize = 1024 * 1024;
/// Scene subunits per grid unit, as in the renderer.
const SUBUNITS_PER_GRID: i32 = 10;

/// A point of an artwork, in scene subunits (a tenth of a grid unit)
/// relative to the symbol origin, unrotated; `y` grows downwards.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ArtPoint {
    /// Horizontal coordinate.
    pub x: i32,
    /// Vertical coordinate.
    pub y: i32,
}

/// One drawing primitive of an artwork.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum ArtPrimitive {
    /// A straight segment.
    Line {
        /// Start.
        from: ArtPoint,
        /// End.
        to: ArtPoint,
    },
    /// Connected segments through every point in order.
    Polyline(Vec<ArtPoint>),
    /// A rectangle outline between two opposite corners.
    Rect {
        /// One corner.
        from: ArtPoint,
        /// The opposite corner.
        to: ArtPoint,
    },
    /// A circle outline.
    Circle {
        /// Centre.
        center: ArtPoint,
        /// Radius in subunits.
        radius: i32,
    },
    /// A filled circle.
    Dot {
        /// Centre.
        center: ArtPoint,
        /// Radius in subunits.
        radius: i32,
    },
    /// A line of text, centred on its anchor.
    Text {
        /// Anchor point.
        at: ArtPoint,
        /// The text.
        text: String,
    },
}

/// The drawing of a library symbol: what is drawn at rest and, optionally,
/// what is drawn instead while the contact is closed or the actuator held.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Artwork {
    /// Primitives drawn at rest.
    pub rest: Vec<ArtPrimitive>,
    /// Primitives drawn while operated; `None` draws `rest` always.
    pub operated: Option<Vec<ArtPrimitive>>,
}

/// A loaded extension library.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Library {
    /// Namespace of the library: every model ID starts with it and a dot.
    pub id: String,
    /// Version of the library.
    pub version: u32,
    /// Human-readable name.
    pub name: String,
    /// Origin and licence, shown to the user and given to every model.
    pub provenance: Provenance,
    /// The models.
    pub models: Vec<ComponentModel>,
}

/// Whether `id` is a valid library namespace: a lowercase letter then one
/// to 31 lowercase letters, digits or underscores.
#[must_use]
pub fn is_valid_library_id(id: &str) -> bool {
    let mut chars = id.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && (2..=32).contains(&id.len())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// Whether `name`, the part of a model ID after the namespace, is valid:
/// one to 64 lowercase letters, digits, underscores or dots.
#[must_use]
pub fn is_valid_model_name(name: &str) -> bool {
    (1..=64).contains(&name.len())
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '.')
}

/// Check an artwork against the symbol kind it draws: within the limits,
/// inside the kind's bounds, and with a segment ending on every terminal in
/// each list, so that wires meet the drawing.
///
/// # Errors
///
/// Returns what is wrong, in words.
pub fn check_artwork(kind: SymbolKind, artwork: &Artwork) -> Result<(), String> {
    check_list(kind, &artwork.rest, "rest")?;
    if let Some(operated) = &artwork.operated {
        check_list(kind, operated, "operated")?;
    }
    Ok(())
}

fn scaled(value: i32) -> i32 {
    value.saturating_mul(SUBUNITS_PER_GRID)
}

fn check_list(kind: SymbolKind, list: &[ArtPrimitive], name: &str) -> Result<(), String> {
    if list.is_empty() {
        return Err(format!("the {name} artwork is empty"));
    }
    if list.len() > MAX_ART_PRIMITIVES {
        return Err(format!(
            "the {name} artwork has more than {MAX_ART_PRIMITIVES} primitives"
        ));
    }
    let bounds = kind.bounds();
    let (left, top) = (scaled(bounds.min.x), scaled(bounds.min.y));
    let (right, bottom) = (scaled(bounds.max.x), scaled(bounds.max.y));
    let inside = |p: &ArtPoint| (left..=right).contains(&p.x) && (top..=bottom).contains(&p.y);
    let circle_inside = |center: &ArtPoint, radius: i32| {
        radius > 0
            && inside(&ArtPoint {
                x: center.x.saturating_sub(radius),
                y: center.y.saturating_sub(radius),
            })
            && inside(&ArtPoint {
                x: center.x.saturating_add(radius),
                y: center.y.saturating_add(radius),
            })
    };
    let mut ends: Vec<ArtPoint> = Vec::new();
    for primitive in list {
        let fits = match primitive {
            ArtPrimitive::Line { from, to } | ArtPrimitive::Rect { from, to } => {
                inside(from) && inside(to)
            }
            ArtPrimitive::Polyline(points) => {
                (2..=MAX_ART_PRIMITIVES).contains(&points.len()) && points.iter().all(inside)
            }
            ArtPrimitive::Circle { center, radius } | ArtPrimitive::Dot { center, radius } => {
                circle_inside(center, *radius)
            }
            ArtPrimitive::Text { at, text } => {
                inside(at)
                    && !text.is_empty()
                    && text.len() <= MAX_TEXT_BYTES
                    && !text.chars().any(char::is_control)
            }
        };
        if !fits {
            return Err(format!(
                "a primitive of the {name} artwork leaves the symbol area or is malformed"
            ));
        }
        match primitive {
            ArtPrimitive::Line { from, to } => ends.extend([*from, *to]),
            ArtPrimitive::Polyline(points) => ends.extend(points.iter().copied()),
            _ => {}
        }
    }
    for terminal in kind.terminals() {
        let target = ArtPoint {
            x: scaled(terminal.offset.x),
            y: scaled(terminal.offset.y),
        };
        if !ends.contains(&target) {
            return Err(format!(
                "no line of the {name} artwork reaches terminal `{}`",
                terminal.key.0
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(x: i32, y: i32) -> ArtPoint {
        ArtPoint { x, y }
    }

    /// A contact drawn as one straight line from terminal to terminal.
    fn straight() -> Vec<ArtPrimitive> {
        vec![ArtPrimitive::Line {
            from: point(0, -20),
            to: point(0, 20),
        }]
    }

    #[test]
    fn library_ids_are_short_lowercase_words() {
        for valid in ["acme", "a1", "vendor_x"] {
            assert!(is_valid_library_id(valid), "{valid}");
        }
        for invalid in ["", "a", "Acme", "1acme", "ac-me", "acme.x", &"a".repeat(33)] {
            assert!(!is_valid_library_id(invalid), "{invalid}");
        }
    }

    #[test]
    fn artwork_reaching_both_terminals_inside_the_bounds_is_accepted() {
        let artwork = Artwork {
            rest: straight(),
            operated: None,
        };

        assert_eq!(check_artwork(SymbolKind::ContactNo, &artwork), Ok(()));
    }

    #[test]
    fn artwork_that_misses_a_terminal_or_leaves_the_bounds_is_refused() {
        let short = Artwork {
            rest: vec![ArtPrimitive::Line {
                from: point(0, -20),
                to: point(0, 10),
            }],
            operated: None,
        };
        let outside = Artwork {
            rest: vec![
                ArtPrimitive::Line {
                    from: point(0, -20),
                    to: point(0, 20),
                },
                ArtPrimitive::Circle {
                    center: point(0, 0),
                    radius: 30,
                },
            ],
            operated: None,
        };
        let empty_operated = Artwork {
            rest: straight(),
            operated: Some(Vec::new()),
        };

        for artwork in [short, outside, empty_operated] {
            assert!(
                check_artwork(SymbolKind::ContactNo, &artwork).is_err(),
                "{artwork:?}"
            );
        }
    }

    #[test]
    fn artwork_text_must_be_short_and_printable() {
        let with_text = |text: &str| Artwork {
            rest: vec![
                ArtPrimitive::Line {
                    from: point(0, -20),
                    to: point(0, 20),
                },
                ArtPrimitive::Text {
                    at: point(0, 0),
                    text: text.to_owned(),
                },
            ],
            operated: None,
        };

        assert!(check_artwork(SymbolKind::ContactNo, &with_text("R")).is_ok());
        assert!(check_artwork(SymbolKind::ContactNo, &with_text("a\u{7}")).is_err());
        assert!(check_artwork(SymbolKind::ContactNo, &with_text(&"x".repeat(300))).is_err());
    }
}
