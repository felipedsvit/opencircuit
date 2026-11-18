//! SVG written directly from the scene.

use circuit_renderer::{Anchor, Scene, ScenePoint, SceneRect, Shape};

use crate::theme::{Look, Rgb, Theme};

/// Empty space around the drawing, in scene subunits (two grid units).
pub const MARGIN: i64 = 20;

/// Height of a text, in scene subunits.
const FONT_SIZE: i64 = 8;

/// The page of a scene: its left and top edges and its width and height, in
/// subunits. The area of the scene plus [`MARGIN`] on every side, or exactly
/// the paper of a sheet when the scene is one.
pub(crate) fn page_of(scene: &Scene) -> (i64, i64, i64, i64) {
    if let Some(page) = scene.page {
        return (
            page.min.x,
            page.min.y,
            page.max.x.saturating_sub(page.min.x),
            page.max.y.saturating_sub(page.min.y),
        );
    }
    let area = scene.bounds.unwrap_or(SceneRect::from_corners(
        ScenePoint::new(0, 0),
        ScenePoint::new(0, 0),
    ));
    (
        area.min.x.saturating_sub(MARGIN),
        area.min.y.saturating_sub(MARGIN),
        area.max
            .x
            .saturating_sub(area.min.x)
            .saturating_add(MARGIN.saturating_mul(2)),
        area.max
            .y
            .saturating_sub(area.min.y)
            .saturating_add(MARGIN.saturating_mul(2)),
    )
}

/// Write a scene as an SVG document.
///
/// The view box is the area of the scene plus [`MARGIN`] on every side, or
/// the paper of the sheet when the scene is one; one user unit is one scene
/// subunit. Every item becomes one element, in scene order, with its look
/// from `theme`. Texts are escaped for XML and lose any character XML 1.0
/// does not allow, so a designator from a hostile file cannot break the
/// document.
#[must_use]
pub fn svg(scene: &Scene, theme: Theme) -> String {
    let (x, y, width, height) = page_of(scene);

    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" version=\"1.1\" viewBox=\"{x} {y} {width} {height}\" width=\"{width}\" height=\"{height}\" stroke-linecap=\"round\" stroke-linejoin=\"round\">\n"
    ));
    out.push_str(&format!(
        "<rect x=\"{x}\" y=\"{y}\" width=\"{width}\" height=\"{height}\" fill=\"#ffffff\"/>\n"
    ));
    for item in &scene.items {
        element(&mut out, &item.shape, theme.look(item.style));
    }
    out.push_str("</svg>\n");
    out
}

/// One element for one shape.
fn element(out: &mut String, shape: &Shape, look: Look) {
    let stroke = format!(
        "fill=\"none\" stroke=\"{}\" stroke-width=\"{}\"",
        color(look.color),
        width(look.width_tenths)
    );
    let text = match shape {
        Shape::Line { from, to } => format!(
            "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" {stroke}/>",
            from.x, from.y, to.x, to.y
        ),
        Shape::Polyline(points) => {
            let points: Vec<String> = points.iter().map(|p| format!("{},{}", p.x, p.y)).collect();
            format!("<polyline points=\"{}\" {stroke}/>", points.join(" "))
        }
        Shape::Rect(rect) => format!(
            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" {stroke}/>",
            rect.min.x,
            rect.min.y,
            rect.max.x.saturating_sub(rect.min.x),
            rect.max.y.saturating_sub(rect.min.y)
        ),
        Shape::Circle { center, radius } => format!(
            "<circle cx=\"{}\" cy=\"{}\" r=\"{}\" {stroke}/>",
            center.x,
            center.y,
            radius.saturating_abs()
        ),
        Shape::Block(rect) => format!(
            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{}\"/>",
            rect.min.x,
            rect.min.y,
            rect.max.x.saturating_sub(rect.min.x),
            rect.max.y.saturating_sub(rect.min.y),
            color(look.color)
        ),
        Shape::Dot { center, radius } => format!(
            "<circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"{}\"/>",
            center.x,
            center.y,
            radius.saturating_abs(),
            color(look.color)
        ),
        Shape::Text { at, text, anchor } => format!(
            "<text x=\"{}\" y=\"{}\" text-anchor=\"{}\" dominant-baseline=\"central\" font-family=\"sans-serif\" font-size=\"{FONT_SIZE}\" fill=\"{}\">{}</text>",
            at.x,
            at.y,
            match anchor {
                Anchor::Left => "start",
                Anchor::Center => "middle",
            },
            color(look.color),
            escape(text)
        ),
    };
    out.push_str(&text);
    out.push('\n');
}

/// `#rrggbb`.
fn color(Rgb(red, green, blue): Rgb) -> String {
    format!("#{red:02x}{green:02x}{blue:02x}")
}

/// A width in tenths written as a decimal: 12 becomes `1.2`, 20 becomes `2`.
fn width(tenths: u32) -> String {
    let whole = tenths.checked_div(10).unwrap_or(0);
    let fraction = tenths.checked_rem(10).unwrap_or(0);
    if fraction == 0 {
        whole.to_string()
    } else {
        format!("{whole}.{fraction}")
    }
}

/// Text content escaped for XML; characters that XML 1.0 forbids (control
/// characters other than tab, line feed and carriage return, and the
/// non-characters U+FFFE and U+FFFF) are dropped.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            '\t' | '\n' | '\r' => out.push(c),
            c if c < ' ' || c == '\u{FFFE}' || c == '\u{FFFF}' => {}
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use circuit_model::{ConductorSet, Subject};
    use circuit_renderer::{SceneItem, StyleKey};
    use proptest::prelude::*;

    use super::*;

    fn scene(items: Vec<(Shape, StyleKey)>) -> Scene {
        let mut scene = Scene::default();
        for (shape, style) in items {
            if let Some(area) = shape.bounds() {
                scene.bounds = Some(match scene.bounds {
                    Some(bounds) => bounds.union(&area),
                    None => area,
                });
            }
            scene.items.push(SceneItem {
                shape,
                style,
                subject: Some(Subject::Document),
            });
        }
        scene
    }

    #[test]
    fn a_scene_that_is_one_sheet_has_exactly_the_paper_as_its_view_box() {
        let mut sheet = scene(vec![(
            Shape::Line {
                from: ScenePoint::new(10, 10),
                to: ScenePoint::new(20, 20),
            },
            StyleKey::Symbol,
        )]);
        let plain = svg(&sheet, Theme::Print);
        sheet.page = Some(SceneRect::from_corners(
            ScenePoint::new(-100, 50),
            ScenePoint::new(740, 640),
        ));

        let page = svg(&sheet, Theme::Print);

        assert!(plain.contains("viewBox=\"-10 -10 50 50\""), "{plain}");
        assert!(
            page.contains("viewBox=\"-100 50 840 590\" width=\"840\" height=\"590\""),
            "{page}"
        );
    }

    #[test]
    fn empty_scene_is_a_valid_document_of_the_margin_only() {
        assert_eq!(
            svg(&Scene::default(), Theme::Print),
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
             <svg xmlns=\"http://www.w3.org/2000/svg\" version=\"1.1\" viewBox=\"-20 -20 40 40\" width=\"40\" height=\"40\" stroke-linecap=\"round\" stroke-linejoin=\"round\">\n\
             <rect x=\"-20\" y=\"-20\" width=\"40\" height=\"40\" fill=\"#ffffff\"/>\n\
             </svg>\n"
        );
    }

    #[test]
    fn every_shape_becomes_one_element_with_integer_coordinates() {
        let p = ScenePoint::new;
        let drawn = svg(
            &scene(vec![
                (
                    Shape::Line {
                        from: p(0, 0),
                        to: p(10, 0),
                    },
                    StyleKey::Wire {
                        conductors: ConductorSet::EMPTY,
                    },
                ),
                (
                    Shape::Polyline(vec![p(0, 0), p(0, 10), p(10, 10)]),
                    StyleKey::Symbol,
                ),
                (
                    Shape::Rect(SceneRect::from_corners(p(10, 10), p(-10, -10))),
                    StyleKey::Symbol,
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
                    StyleKey::Junction {
                        conductors: ConductorSet::EMPTY,
                    },
                ),
                (
                    Shape::Text {
                        at: p(0, -5),
                        text: "K1".to_owned(),
                        anchor: Anchor::Left,
                    },
                    StyleKey::Label,
                ),
            ]),
            Theme::Print,
        );
        let body: Vec<&str> = drawn.lines().skip(3).collect();

        assert_eq!(
            body,
            [
                "<line x1=\"0\" y1=\"0\" x2=\"10\" y2=\"0\" fill=\"none\" stroke=\"#000000\" stroke-width=\"1\"/>",
                "<polyline points=\"0,0 0,10 10,10\" fill=\"none\" stroke=\"#000000\" stroke-width=\"1.2\"/>",
                "<rect x=\"-10\" y=\"-10\" width=\"20\" height=\"20\" fill=\"none\" stroke=\"#000000\" stroke-width=\"1.2\"/>",
                "<circle cx=\"5\" cy=\"5\" r=\"3\" fill=\"none\" stroke=\"#000000\" stroke-width=\"1.2\"/>",
                "<circle cx=\"0\" cy=\"0\" r=\"2\" fill=\"#000000\"/>",
                "<text x=\"0\" y=\"-5\" text-anchor=\"start\" dominant-baseline=\"central\" font-family=\"sans-serif\" font-size=\"8\" fill=\"#000000\">K1</text>",
                "</svg>",
            ]
        );
        assert!(drawn.contains("viewBox=\"-30 -30 60 60\""));
    }

    #[test]
    fn text_is_escaped_and_forbidden_characters_are_dropped() {
        assert_eq!(
            escape("<K1 & \"S2\" 'x'>\u{0}\u{7}\u{FFFF}ok\u{394}"),
            "&lt;K1 &amp; &quot;S2&quot; &apos;x&apos;&gt;ok\u{394}"
        );
    }

    #[test]
    fn widths_are_written_as_fixed_decimals() {
        assert_eq!(
            [width(10), width(12), width(25), width(0)],
            ["1", "1.2", "2.5", "0"]
        );
    }

    proptest! {
        /// Whatever the text, the content of a text element can only close
        /// at its own end tag and holds no character XML 1.0 forbids.
        #[test]
        fn escaped_text_cannot_break_the_document(text in any::<String>()) {
            let escaped = escape(&text);

            prop_assert!(!escaped.contains('<') && !escaped.contains('>'));
            prop_assert!(escaped
                .split('&')
                .skip(1)
                .all(|rest| ["amp;", "lt;", "gt;", "quot;", "apos;"]
                    .iter()
                    .any(|entity| rest.starts_with(entity))));
            let allowed = escaped.chars().all(|c| {
                matches!(c, '\t' | '\n' | '\r') || (c >= ' ' && c != '\u{FFFE}' && c != '\u{FFFF}')
            });
            prop_assert!(allowed);
        }

        /// The same scene always gives the same bytes.
        #[test]
        fn output_is_deterministic(
            x in -1_000_i64..1_000,
            y in -1_000_i64..1_000,
            text in "[A-Z][A-Z0-9]{0,6}",
        ) {
            let built = || scene(vec![(
                Shape::Text { at: ScenePoint::new(x, y), text: text.clone(), anchor: Anchor::Center },
                StyleKey::Label,
            )]);

            prop_assert_eq!(svg(&built(), Theme::Live), svg(&built(), Theme::Live));
        }
    }
}
