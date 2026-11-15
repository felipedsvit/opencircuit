//! PDF written from the scene with `pdf-writer` (ADR-0006).

use circuit_renderer::{Anchor, Scene, ScenePoint, SceneRect, Shape};
use pdf_writer::types::{LineCapStyle, LineJoinStyle};
use pdf_writer::{Content, Finish, Name, Pdf, Rect, Ref, Str};

use crate::svg::MARGIN;
use crate::theme::{Look, Rgb, Theme};

/// Height of a text, in points (one point per scene subunit).
const FONT_SIZE: f32 = 8.0;

/// Width of every Courier glyph at [`FONT_SIZE`], in tenths of a point:
/// 0.6 em of 8 points.
const COURIER_ADVANCE_TENTHS: i64 = 48;

/// How far below its anchor the baseline of a text lies, in tenths of a
/// point (0.3 em of 8 points), so that it is centred vertically as in SVG.
const BASELINE_DROP_TENTHS: i64 = 24;

/// Distance of the control points of a quarter circle drawn as a cubic
/// Bézier curve, as a fraction of the radius.
const KAPPA: f32 = 0.552_284_8;

/// Write a scene as a one-page PDF document.
///
/// The page is the area of the scene plus the SVG margin, one point per
/// scene subunit, so the PDF and the SVG of a scene have the same size.
/// Texts use Courier, one of the standard fonts, so nothing is embedded;
/// characters other than printable ASCII are written as `?`. The document
/// has no date and no identifier, so the same scene gives the same bytes.
#[must_use]
pub fn pdf(scene: &Scene, theme: Theme) -> Vec<u8> {
    let area = scene.bounds.unwrap_or(SceneRect::from_corners(
        ScenePoint::new(0, 0),
        ScenePoint::new(0, 0),
    ));
    let page = Page {
        left: area.min.x.saturating_sub(MARGIN),
        top: area.min.y.saturating_sub(MARGIN),
        width: area
            .max
            .x
            .saturating_sub(area.min.x)
            .saturating_add(MARGIN.saturating_mul(2)),
        height: area
            .max
            .y
            .saturating_sub(area.min.y)
            .saturating_add(MARGIN.saturating_mul(2)),
    };

    let catalog = Ref::new(1);
    let tree = Ref::new(2);
    let page_ref = Ref::new(3);
    let font = Ref::new(4);
    let content_ref = Ref::new(5);

    let mut content = Content::new();
    content.set_line_cap(LineCapStyle::RoundCap);
    content.set_line_join(LineJoinStyle::RoundJoin);
    for item in &scene.items {
        draw(&mut content, &page, &item.shape, theme.look(item.style));
    }
    let content = content.finish();

    let mut pdf = Pdf::new();
    pdf.catalog(catalog).pages(tree);
    pdf.pages(tree).kids([page_ref]).count(1);
    let mut page_writer = pdf.page(page_ref);
    page_writer.media_box(Rect::new(0.0, 0.0, real(page.width), real(page.height)));
    page_writer.parent(tree);
    page_writer.contents(content_ref);
    page_writer.resources().fonts().pair(Name(b"F1"), font);
    page_writer.finish();
    pdf.type1_font(font)
        .base_font(Name(b"Courier"))
        .encoding_predefined(Name(b"WinAnsiEncoding"));
    pdf.stream(content_ref, &content);
    pdf.finish()
}

/// The page in scene coordinates.
struct Page {
    left: i64,
    top: i64,
    width: i64,
    height: i64,
}

impl Page {
    /// A scene point in page coordinates: `x` from the left edge, `y` from
    /// the bottom edge, upwards.
    fn point(&self, point: ScenePoint) -> (f32, f32) {
        let x = point.x.saturating_sub(self.left);
        let y = self.top.saturating_add(self.height).saturating_sub(point.y);
        (real(x), real(y))
    }
}

/// An integer scene length as a PDF number. Scene coordinates come from
/// grid points (`i32` times ten), so they are well inside the exact range
/// of the PDF reals viewers accept.
#[expect(
    clippy::cast_precision_loss,
    reason = "scene coordinates are far below 2^24 in practice; PDF numbers are reals"
)]
fn real(value: i64) -> f32 {
    value as f32
}

fn channel(value: u8) -> f32 {
    f32::from(value) / 255.0
}

fn stroke(content: &mut Content, look: Look) {
    let Rgb(red, green, blue) = look.color;
    content.set_stroke_rgb(channel(red), channel(green), channel(blue));
    content.set_line_width(real(i64::from(look.width_tenths)) / 10.0);
}

/// A circle as four quarter arcs, counter-clockwise from the right.
fn circle(content: &mut Content, (cx, cy): (f32, f32), radius: f32) {
    let k = radius * KAPPA;
    content.move_to(cx + radius, cy);
    content.cubic_to(cx + radius, cy + k, cx + k, cy + radius, cx, cy + radius);
    content.cubic_to(cx - k, cy + radius, cx - radius, cy + k, cx - radius, cy);
    content.cubic_to(cx - radius, cy - k, cx - k, cy - radius, cx, cy - radius);
    content.cubic_to(cx + k, cy - radius, cx + radius, cy - k, cx + radius, cy);
    content.close_path();
}

/// Printable ASCII as it is; everything else, which WinAnsi Courier may
/// not have, as `?`.
fn win_ansi(text: &str) -> Vec<u8> {
    text.chars()
        .map(|c| {
            if c.is_ascii_graphic() || c == ' ' {
                u8::try_from(c).unwrap_or(b'?')
            } else {
                b'?'
            }
        })
        .collect()
}

fn draw(content: &mut Content, page: &Page, shape: &Shape, look: Look) {
    match shape {
        Shape::Line { from, to } => {
            stroke(content, look);
            let (x1, y1) = page.point(*from);
            let (x2, y2) = page.point(*to);
            content.move_to(x1, y1);
            content.line_to(x2, y2);
            content.stroke();
        }
        Shape::Polyline(points) => {
            let mut points = points.iter().map(|point| page.point(*point));
            let Some((x, y)) = points.next() else {
                return;
            };
            stroke(content, look);
            content.move_to(x, y);
            for (x, y) in points {
                content.line_to(x, y);
            }
            content.stroke();
        }
        Shape::Rect(rect) => {
            stroke(content, look);
            let (left, bottom) = page.point(ScenePoint::new(rect.min.x, rect.max.y));
            content.rect(
                left,
                bottom,
                real(rect.max.x.saturating_sub(rect.min.x)),
                real(rect.max.y.saturating_sub(rect.min.y)),
            );
            content.stroke();
        }
        Shape::Circle { center, radius } => {
            stroke(content, look);
            circle(content, page.point(*center), real(radius.saturating_abs()));
            content.stroke();
        }
        Shape::Dot { center, radius } => {
            let Rgb(red, green, blue) = look.color;
            content.set_fill_rgb(channel(red), channel(green), channel(blue));
            circle(content, page.point(*center), real(radius.saturating_abs()));
            content.fill_nonzero();
        }
        Shape::Text { at, text, anchor } => {
            let encoded = win_ansi(text);
            // Positions in integer tenths of a point, so that they are
            // written as short exact decimals.
            let tenths = |value: i64| value.saturating_mul(10);
            let x = tenths(at.x.saturating_sub(page.left));
            let y = tenths(page.top.saturating_add(page.height).saturating_sub(at.y));
            let width = i64::try_from(encoded.len())
                .unwrap_or(i64::MAX)
                .saturating_mul(COURIER_ADVANCE_TENTHS);
            let start = match anchor {
                Anchor::Left => x,
                Anchor::Center => x.saturating_sub(width.checked_div(2).unwrap_or(0)),
            };
            let baseline = y.saturating_sub(BASELINE_DROP_TENTHS);
            let Rgb(red, green, blue) = look.color;
            content.set_fill_rgb(channel(red), channel(green), channel(blue));
            content.begin_text();
            content.set_font(Name(b"F1"), FONT_SIZE);
            content.next_line(real(start) / 10.0, real(baseline) / 10.0);
            content.show(Str(&encoded));
            content.end_text();
        }
    }
}

#[cfg(test)]
mod tests {
    use circuit_model::Subject;
    use circuit_renderer::{SceneItem, StyleKey};

    use super::*;

    fn text_scene(text: &str) -> Scene {
        let shape = Shape::Text {
            at: ScenePoint::new(0, 0),
            text: text.to_owned(),
            anchor: Anchor::Center,
        };
        Scene {
            bounds: shape.bounds(),
            items: vec![SceneItem {
                shape,
                style: StyleKey::Label,
                subject: Some(Subject::Document),
            }],
        }
    }

    fn as_text(bytes: &[u8]) -> String {
        String::from_utf8_lossy(bytes).into_owned()
    }

    #[test]
    fn empty_scene_is_a_one_page_document_of_the_margin() {
        let bytes = pdf(&Scene::default(), Theme::Print);
        let text = as_text(&bytes);

        assert!(text.starts_with("%PDF-"));
        assert!(text.trim_end().ends_with("%%EOF"));
        assert!(text.contains("/MediaBox [0 0 40 40]"), "{text}");
        assert!(text.contains("/Count 1"));
    }

    #[test]
    fn document_has_no_date_and_no_identifier() {
        let text = as_text(&pdf(&text_scene("K1"), Theme::Print));

        for key in ["/CreationDate", "/ModDate", "/ID"] {
            assert!(!text.contains(key), "{key}");
        }
        assert!(text.contains("/BaseFont /Courier"));
    }

    #[test]
    fn the_same_scene_gives_the_same_bytes() {
        assert_eq!(
            pdf(&text_scene("KM1"), Theme::Live),
            pdf(&text_scene("KM1"), Theme::Live)
        );
    }

    #[test]
    fn non_ascii_characters_are_written_as_question_marks() {
        assert_eq!(win_ansi("K\u{394}1 (x)"), b"K?1 (x)");
    }

    #[test]
    fn centred_text_starts_half_its_courier_width_left_of_the_anchor() {
        // "KM1": 3 glyphs of 0.6 em at 8 pt is 14.4 pt wide; the anchor is
        // at x = 20 on a page with a margin of 20.
        let text = as_text(&pdf(&text_scene("KM1"), Theme::Print));

        assert!(text.contains("12.8 17.6 Td"), "{text}");
    }

    #[test]
    fn y_grows_upwards_from_the_bottom_of_the_page() {
        let page = Page {
            left: -20,
            top: -20,
            width: 100,
            height: 60,
        };

        assert_eq!(page.point(ScenePoint::new(-20, -20)), (0.0, 60.0));
        assert_eq!(page.point(ScenePoint::new(80, 40)), (100.0, 0.0));
    }
}
