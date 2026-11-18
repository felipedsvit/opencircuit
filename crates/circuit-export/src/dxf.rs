//! DXF written directly from the scene (M9.4): AutoCAD R12 ASCII
//! (`AC1009`), the version every CAD program reads.
//!
//! One scene subunit is one point (1/72 inch); coordinates are written in
//! millimetres with four decimals, computed with integers, and `y` grows
//! upwards as DXF wants it. Each item goes on a layer named after what it
//! is (`WIRES`, `SYMBOLS`, `LABELS`, `FRAME`, …) with the nearest colour of
//! the AutoCAD index to its look. The output is deterministic: the same
//! scene and theme give the same bytes.

use std::fmt::Write as _;

use circuit_renderer::{Anchor, Scene, ScenePoint, Shape, StyleKey};

use crate::theme::{Rgb, Theme};

/// Height of a text, in scene subunits, as in the SVG export.
const FONT_SIZE: i64 = 8;

/// The layers, in the order of the layer table.
const LAYERS: [&str; 6] = ["WIRES", "SYMBOLS", "LABELS", "FRAME", "STATE", "MARKS"];

/// The layer of an item.
const fn layer(style: StyleKey) -> &'static str {
    match style {
        StyleKey::Wire { .. } | StyleKey::Junction { .. } => "WIRES",
        StyleKey::Symbol | StyleKey::Linkage | StyleKey::Unknown => "SYMBOLS",
        StyleKey::Label => "LABELS",
        StyleKey::Frame => "FRAME",
        StyleKey::Energized
        | StyleKey::Active
        | StyleKey::ContactClosed
        | StyleKey::Actuated
        | StyleKey::Tripped
        | StyleKey::Motor(_) => "STATE",
        StyleKey::Fault | StyleKey::Selected | StyleKey::Traced => "MARKS",
    }
}

/// The nearest colour of the AutoCAD colour index: 7 (black on white, white
/// on black) for black and greys darker than mid, 8 for grey, then red,
/// yellow, green, cyan, blue and magenta by the strongest channels.
fn aci(Rgb(red, green, blue): Rgb) -> u8 {
    let (r, g, b) = (u16::from(red), u16::from(green), u16::from(blue));
    let high = r.max(g).max(b);
    let low = r.min(g).min(b);
    if high.saturating_sub(low) < 48 {
        return if high < 96 { 7 } else { 8 };
    }
    let strong = |value: u16| value.saturating_mul(2) > high;
    match (strong(r), strong(g), strong(b)) {
        (true, true, false) if r > g.saturating_add(g / 4) => 30,
        (true, true, false) => 2,
        (true, false, false) => 1,
        (false, true, false) => 3,
        (false, true, true) => 4,
        (false, false, true) => 5,
        (true, false, true) => 6,
        _ => 7,
    }
}

/// Subunits (points) as millimetres with four decimals: `value * 25.4 / 72`,
/// rounded half away from zero, with integers only.
fn mm(value: i64) -> String {
    let scaled = i128::from(value).saturating_mul(254_000);
    let half = if scaled < 0 { -36 } else { 36 };
    let tenths = scaled.saturating_add(half).checked_div(72).unwrap_or(0);
    let sign = if tenths < 0 { "-" } else { "" };
    let magnitude = tenths.unsigned_abs();
    format!(
        "{sign}{}.{:04}",
        magnitude.checked_div(10_000).unwrap_or(0),
        magnitude.checked_rem(10_000).unwrap_or(0)
    )
}

/// One group code and its value, each on its own line.
fn pair(out: &mut String, code: u16, value: impl std::fmt::Display) {
    let _ = writeln!(out, "{code:>3}\n{value}");
}

/// A point as group codes `base` (x), `base + 10` (y) and `base + 20` (z).
fn point(out: &mut String, base: u16, at: ScenePoint) {
    pair(out, base, mm(at.x));
    pair(out, base.saturating_add(10), mm(at.y.saturating_neg()));
    pair(out, base.saturating_add(20), "0.0");
}

/// The start of an entity: its type, layer and colour.
fn entity(out: &mut String, kind: &str, layer: &str, color: u8) {
    pair(out, 0, kind);
    pair(out, 8, layer);
    pair(out, 62, color);
}

/// A text that R12 reads: printable ASCII as it is, every other character
/// as `\U+XXXX`, and control characters as spaces.
fn dxf_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if c.is_control() {
            out.push(' ');
        } else if c.is_ascii() {
            out.push(c);
        } else {
            let code = u32::from(c);
            if code <= 0xFFFF {
                let _ = write!(out, "\\U+{code:04X}");
            } else {
                out.push('?');
            }
        }
    }
    out
}

/// A polyline through `points`, closed when asked; `width` gives every
/// segment a width (a filled dot is a closed polyline of width).
fn polyline(
    out: &mut String,
    layer: &str,
    color: u8,
    points: &[(ScenePoint, i64)],
    closed: bool,
    width: Option<i64>,
) {
    entity(out, "POLYLINE", layer, color);
    pair(out, 66, 1);
    point(out, 10, ScenePoint::new(0, 0));
    pair(out, 70, u8::from(closed));
    if let Some(width) = width {
        pair(out, 40, mm(width));
        pair(out, 41, mm(width));
    }
    for (at, bulge_thousandths) in points {
        entity(out, "VERTEX", layer, color);
        point(out, 10, *at);
        if *bulge_thousandths != 0 {
            let whole = bulge_thousandths.checked_div(1000).unwrap_or(0);
            let part = bulge_thousandths
                .checked_rem(1000)
                .unwrap_or(0)
                .unsigned_abs();
            pair(out, 42, format!("{whole}.{part:03}"));
        }
    }
    pair(out, 0, "SEQEND");
    pair(out, 8, layer);
}

/// One entity for one shape.
fn shape(out: &mut String, item: &Shape, layer: &str, color: u8) {
    match item {
        Shape::Line { from, to } => {
            entity(out, "LINE", layer, color);
            point(out, 10, *from);
            point(out, 11, *to);
        }
        Shape::Polyline(points) => {
            let points: Vec<(ScenePoint, i64)> = points.iter().map(|p| (*p, 0)).collect();
            if points.len() >= 2 {
                polyline(out, layer, color, &points, false, None);
            }
        }
        Shape::Rect(rect) => {
            let corners = [
                ScenePoint::new(rect.min.x, rect.min.y),
                ScenePoint::new(rect.max.x, rect.min.y),
                ScenePoint::new(rect.max.x, rect.max.y),
                ScenePoint::new(rect.min.x, rect.max.y),
            ];
            let points: Vec<(ScenePoint, i64)> = corners.iter().map(|p| (*p, 0)).collect();
            polyline(out, layer, color, &points, true, None);
        }
        Shape::Circle { center, radius } => {
            entity(out, "CIRCLE", layer, color);
            point(out, 10, *center);
            pair(out, 40, mm(radius.saturating_abs()));
        }
        Shape::Dot { center, radius } => {
            // A donut: two half circles as wide as the radius.
            let r = radius.saturating_abs();
            let half = r.checked_div(2).unwrap_or(0);
            let points = [
                (
                    ScenePoint::new(center.x.saturating_sub(half), center.y),
                    1000,
                ),
                (
                    ScenePoint::new(center.x.saturating_add(half), center.y),
                    1000,
                ),
            ];
            polyline(out, layer, color, &points, true, Some(r));
        }
        Shape::Block(rect) => {
            // SOLID takes its corners in the order 1, 2, 4, 3.
            entity(out, "SOLID", layer, color);
            point(out, 10, ScenePoint::new(rect.min.x, rect.min.y));
            point(out, 11, ScenePoint::new(rect.max.x, rect.min.y));
            point(out, 12, ScenePoint::new(rect.min.x, rect.max.y));
            point(out, 13, ScenePoint::new(rect.max.x, rect.max.y));
        }
        Shape::Text { at, text, anchor } => {
            entity(out, "TEXT", layer, color);
            point(out, 10, *at);
            pair(out, 40, mm(FONT_SIZE));
            pair(out, 1, dxf_text(text));
            // Middle of the height; left or centre along it.
            pair(
                out,
                72,
                match anchor {
                    Anchor::Left => 0,
                    Anchor::Center => 1,
                },
            );
            point(out, 11, *at);
            pair(out, 73, 2);
        }
    }
}

/// Write a scene as a DXF drawing (AutoCAD R12, ASCII).
///
/// Every item becomes one entity, in scene order, on the layer of what it
/// is and in the colour of its look from `theme`; a sheet keeps its frame
/// and title block as lines and texts on the layer `FRAME`.
#[must_use]
pub fn dxf(scene: &Scene, theme: Theme) -> String {
    let mut out = String::new();
    pair(&mut out, 0, "SECTION");
    pair(&mut out, 2, "HEADER");
    pair(&mut out, 9, "$ACADVER");
    pair(&mut out, 1, "AC1009");
    pair(&mut out, 9, "$INSUNITS");
    pair(&mut out, 70, 4);
    if let Some(area) = scene.page.or(scene.bounds) {
        pair(&mut out, 9, "$EXTMIN");
        point(&mut out, 10, ScenePoint::new(area.min.x, area.max.y));
        pair(&mut out, 9, "$EXTMAX");
        point(&mut out, 10, ScenePoint::new(area.max.x, area.min.y));
    }
    pair(&mut out, 0, "ENDSEC");

    pair(&mut out, 0, "SECTION");
    pair(&mut out, 2, "TABLES");
    pair(&mut out, 0, "TABLE");
    pair(&mut out, 2, "LTYPE");
    pair(&mut out, 70, 1);
    pair(&mut out, 0, "LTYPE");
    pair(&mut out, 2, "CONTINUOUS");
    pair(&mut out, 70, 0);
    pair(&mut out, 3, "Solid line");
    pair(&mut out, 72, 65);
    pair(&mut out, 73, 0);
    pair(&mut out, 40, "0.0");
    pair(&mut out, 0, "ENDTAB");
    pair(&mut out, 0, "TABLE");
    pair(&mut out, 2, "LAYER");
    pair(&mut out, 70, LAYERS.len());
    for name in LAYERS {
        pair(&mut out, 0, "LAYER");
        pair(&mut out, 2, name);
        pair(&mut out, 70, 0);
        pair(&mut out, 62, 7);
        pair(&mut out, 6, "CONTINUOUS");
    }
    pair(&mut out, 0, "ENDTAB");
    pair(&mut out, 0, "ENDSEC");

    pair(&mut out, 0, "SECTION");
    pair(&mut out, 2, "ENTITIES");
    for item in &scene.items {
        let look = theme.look(item.style);
        shape(&mut out, &item.shape, layer(item.style), aci(look.color));
    }
    pair(&mut out, 0, "ENDSEC");
    pair(&mut out, 0, "EOF");
    out
}

#[cfg(test)]
mod tests {
    use circuit_renderer::{SceneRect, StyleKey};

    use super::*;

    fn scene(items: Vec<(Shape, StyleKey)>) -> Scene {
        let mut scene = Scene::default();
        for (item, style) in items {
            scene.items.push(circuit_renderer::SceneItem {
                shape: item,
                style,
                subject: None,
            });
        }
        scene
    }

    #[test]
    fn points_become_millimetres_with_y_upwards() {
        assert_eq!(mm(0), "0.0000");
        assert_eq!(mm(72), "25.4000");
        assert_eq!(mm(-72), "-25.4000");
        assert_eq!(mm(1), "0.3528");
        assert_eq!(mm(10), "3.5278");
    }

    fn every_kind() -> Scene {
        let p = ScenePoint::new;
        let none = circuit_model::ConductorSet::EMPTY;
        scene(vec![
            (
                Shape::Line {
                    from: p(0, 0),
                    to: p(72, 0),
                },
                StyleKey::Wire { conductors: none },
            ),
            (
                Shape::Polyline(vec![p(0, 0), p(10, 10), p(20, 0)]),
                StyleKey::Symbol,
            ),
            (
                Shape::Rect(SceneRect::from_corners(p(0, 0), p(10, 10))),
                StyleKey::Frame,
            ),
            (
                Shape::Circle {
                    center: p(5, 5),
                    radius: 3,
                },
                StyleKey::Symbol,
            ),
            (
                Shape::Dot {
                    center: p(0, 0),
                    radius: 2,
                },
                StyleKey::Junction { conductors: none },
            ),
            (
                Shape::Block(SceneRect::from_corners(p(0, 0), p(2, 2))),
                StyleKey::Symbol,
            ),
            (
                Shape::Text {
                    at: p(0, -72),
                    text: "K1 For\u{e7}a".to_owned(),
                    anchor: Anchor::Center,
                },
                StyleKey::Label,
            ),
        ])
    }

    #[test]
    fn every_shape_is_one_entity_on_its_layer() {
        let text = dxf(&every_kind(), Theme::Print);
        for kind in ["LINE", "POLYLINE", "CIRCLE", "SOLID", "TEXT", "SEQEND"] {
            assert!(text.contains(&format!("  0\n{kind}\n")), "{kind}");
        }
        assert!(text.starts_with("  0\nSECTION\n  2\nHEADER\n"));
        assert!(text.ends_with("  0\nEOF\n"));
        // y grows upwards: a text 72 points above the origin on the sheet.
        assert!(text.contains(" 20\n25.4000\n"));
        assert!(text.contains("K1 For\\U+00E7a"));
        assert!(text.contains("  8\nLABELS\n"));
        // The same scene gives the same bytes.
        assert_eq!(dxf(&every_kind(), Theme::Print), text);
    }

    /// Read the group codes back: every code a number on its own line,
    /// followed by its value; sections balanced; every POLYLINE closed by a
    /// SEQEND; no value with a line break.
    #[test]
    fn the_file_is_well_formed_group_codes() {
        let text = dxf(&every_kind(), Theme::Live);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len() % 2, 0);
        let mut depth = 0_i32;
        let mut open_polylines = 0_i32;
        for pair in lines.chunks(2) {
            let code: u16 = pair[0].trim().parse().expect("a group code");
            let value = pair[1];
            assert!(!value.contains('\r'));
            if code == 0 {
                match value {
                    "SECTION" => depth += 1,
                    "ENDSEC" => depth -= 1,
                    "POLYLINE" => open_polylines += 1,
                    "SEQEND" => open_polylines -= 1,
                    _ => {}
                }
                assert!((0..=1).contains(&depth));
            }
            if [10, 20, 40, 11, 21].contains(&code) {
                value.parse::<f64>().expect("a number");
            }
        }
        assert_eq!((depth, open_polylines), (0, 0));
    }

    #[test]
    fn colours_take_the_nearest_index() {
        assert_eq!(aci(Rgb(0, 0, 0)), 7);
        assert_eq!(aci(Rgb(128, 128, 128)), 8);
        assert_eq!(aci(Rgb(220, 30, 30)), 1);
        assert_eq!(aci(Rgb(30, 160, 70)), 3);
        assert_eq!(aci(Rgb(50, 100, 220)), 5);
        assert_eq!(aci(Rgb(0, 150, 210)), 4);
        assert_eq!(aci(Rgb(200, 0, 200)), 6);
        assert_eq!(aci(Rgb(225, 140, 0)), 30);
    }
}
