//! PDF written from the scene with `pdf-writer` (ADR-0006).

use std::collections::BTreeSet;

use circuit_renderer::{Anchor, Scene, ScenePoint, Shape};
use pdf_writer::types::{
    CidFontType, FontFlags, LineCapStyle, LineJoinStyle, SystemInfo, UnicodeCmap,
};
use pdf_writer::{Content, Finish, Name, Pdf, Rect, Ref, Str};

use crate::font::Subset;
use crate::svg::page_of;
use crate::theme::{Look, Rgb, Theme};

/// Height of a text, in points (one point per scene subunit).
const FONT_SIZE: f32 = 8.0;

/// Width of every glyph of the monospaced font at [`FONT_SIZE`], in tenths
/// of a point: 0.6 em of 8 points.
const ADVANCE_TENTHS: i64 = 48;

/// How far below its anchor the baseline of a text lies, in tenths of a
/// point (0.3 em of 8 points), so that it is centred vertically as in SVG.
const BASELINE_DROP_TENTHS: i64 = 24;

/// Distance of the control points of a quarter circle drawn as a cubic
/// Bézier curve, as a fraction of the radius.
const KAPPA: f32 = 0.552_284_8;

/// Write a scene as a one-page PDF document.
///
/// The page is the area of the scene plus the SVG margin, one point per
/// scene subunit, so the PDF and the SVG of a scene have the same size; a
/// scene that is one sheet has exactly the paper of the sheet. See
/// [`pdf_pages`] for the text and the determinism.
#[must_use]
pub fn pdf(scene: &Scene, theme: Theme) -> Vec<u8> {
    pdf_pages(std::slice::from_ref(scene), theme)
}

/// Write scenes as a PDF document with one page each, in order.
///
/// Texts use Liberation Mono, embedded as one subset with only the glyphs of
/// all the pages, so any character the font has is drawn as itself and can
/// be searched and copied; one it lacks is drawn as the font's empty box.
/// The document has no date and no identifier, so the same scenes give the
/// same bytes. No scene gives a document with one empty page: a PDF needs
/// at least one.
#[must_use]
pub fn pdf_pages(scenes: &[Scene], theme: Theme) -> Vec<u8> {
    let empty = Scene::default();
    let scenes = if scenes.is_empty() {
        std::slice::from_ref(&empty)
    } else {
        scenes
    };

    let chars: BTreeSet<char> = scenes
        .iter()
        .flat_map(|scene| &scene.items)
        .filter_map(|item| match &item.shape {
            Shape::Text { text, .. } => Some(text.chars()),
            _ => None,
        })
        .flatten()
        .collect();
    let subset = if chars.is_empty() {
        None
    } else {
        Subset::new(&chars)
    };

    // Object numbers: the catalog, the page tree, then a page and its
    // content stream for each scene, then the font.
    let catalog = Ref::new(1);
    let tree = Ref::new(2);
    let mut next = 3_i32;
    let mut take = || {
        let id = Ref::new(next);
        next = next.saturating_add(1);
        id
    };
    let numbered: Vec<(Ref, Ref)> = scenes.iter().map(|_| (take(), take())).collect();
    let fonts = FontRefs {
        type0: take(),
        cid: take(),
        descriptor: take(),
        file: take(),
        unicode: take(),
    };

    let mut pdf = Pdf::new();
    pdf.catalog(catalog).pages(tree);
    pdf.pages(tree)
        .kids(numbered.iter().map(|(page, _)| *page))
        .count(i32::try_from(numbered.len()).unwrap_or(i32::MAX));
    for (scene, (page_ref, content_ref)) in scenes.iter().zip(&numbered) {
        let (left, top, width, height) = page_of(scene);
        let page = Page {
            left,
            top,
            width,
            height,
        };
        let mut content = Content::new();
        content.set_line_cap(LineCapStyle::RoundCap);
        content.set_line_join(LineJoinStyle::RoundJoin);
        for item in &scene.items {
            draw(
                &mut content,
                &page,
                subset.as_ref(),
                &item.shape,
                theme.look(item.style),
            );
        }
        let content = content.finish();

        let mut page_writer = pdf.page(*page_ref);
        page_writer.media_box(Rect::new(0.0, 0.0, real(page.width), real(page.height)));
        page_writer.parent(tree);
        page_writer.contents(*content_ref);
        let mut resources = page_writer.resources();
        if subset.is_some() {
            resources.fonts().pair(Name(b"F1"), fonts.type0);
        }
        resources.finish();
        page_writer.finish();
        pdf.stream(*content_ref, &content);
    }
    if let Some(subset) = &subset {
        write_font(&mut pdf, &fonts, subset);
    }
    pdf.finish()
}

/// The objects of the embedded font.
struct FontRefs {
    type0: Ref,
    cid: Ref,
    descriptor: Ref,
    file: Ref,
    unicode: Ref,
}

/// The subset as a composite font whose codes are its glyph IDs
/// (`Identity-H`), with a `ToUnicode` map for searching and copying. Every
/// glyph has the width of the font, so one default width serves them all.
fn write_font(pdf: &mut Pdf, refs: &FontRefs, subset: &Subset) {
    let name_bytes: Vec<u8> = subset
        .tag
        .iter()
        .copied()
        .chain(b"+LiberationMono".iter().copied())
        .collect();
    let base = Name(&name_bytes);
    let metrics = subset.metrics;
    let to_real = |value: i32| f32::from(i16::try_from(value).unwrap_or(0));

    pdf.type0_font(refs.type0)
        .base_font(base)
        .encoding_predefined(Name(b"Identity-H"))
        .descendant_font(refs.cid)
        .to_unicode(refs.unicode);
    pdf.cid_font(refs.cid)
        .subtype(CidFontType::Type2)
        .base_font(base)
        .system_info(SystemInfo {
            registry: Str(b"Adobe"),
            ordering: Str(b"Identity"),
            supplement: 0,
        })
        .font_descriptor(refs.descriptor)
        .default_width(to_real(metrics.advance))
        .cid_to_gid_map_predefined(Name(b"Identity"));
    pdf.font_descriptor(refs.descriptor)
        .name(base)
        .flags(FontFlags::FIXED_PITCH | FontFlags::NON_SYMBOLIC)
        .bbox(Rect::new(
            to_real(metrics.bbox[0]),
            to_real(metrics.bbox[1]),
            to_real(metrics.bbox[2]),
            to_real(metrics.bbox[3]),
        ))
        .italic_angle(0.0)
        .ascent(to_real(metrics.ascent))
        .descent(to_real(metrics.descent))
        .cap_height(to_real(metrics.cap_height))
        .stem_v(80.0)
        .font_file2(refs.file);
    let length = i32::try_from(subset.data.len()).unwrap_or(i32::MAX);
    pdf.stream(refs.file, &subset.data)
        .pair(Name(b"Length1"), length);

    let mut map = UnicodeCmap::new(
        Name(b"Adobe-Identity-UCS"),
        SystemInfo {
            registry: Str(b"Adobe"),
            ordering: Str(b"UCS"),
            supplement: 0,
        },
    );
    for (glyph, c) in subset.unicode() {
        map.pair(glyph, c);
    }
    pdf.cmap(refs.unicode, map.finish().as_slice());
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

fn draw(content: &mut Content, page: &Page, subset: Option<&Subset>, shape: &Shape, look: Look) {
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
            let Some(subset) = subset else {
                return;
            };
            let encoded = subset.encode(text);
            // Positions in integer tenths of a point, so that they are
            // written as short exact decimals.
            let tenths = |value: i64| value.saturating_mul(10);
            let x = tenths(at.x.saturating_sub(page.left));
            let y = tenths(page.top.saturating_add(page.height).saturating_sub(at.y));
            let width = i64::try_from(text.chars().count())
                .unwrap_or(i64::MAX)
                .saturating_mul(ADVANCE_TENTHS);
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
    use circuit_renderer::{SceneItem, SceneRect, StyleKey};

    use super::*;

    fn text_scene(text: &str) -> Scene {
        let shape = Shape::Text {
            at: ScenePoint::new(0, 0),
            text: text.to_owned(),
            anchor: Anchor::Center,
        };
        Scene {
            page: None,
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

    fn paper(x: i64, y: i64, text: &str) -> Scene {
        let mut scene = text_scene(text);
        scene.page = Some(SceneRect::from_corners(
            ScenePoint::new(x, y),
            ScenePoint::new(x + 840, y + 590),
        ));
        scene
    }

    #[test]
    fn one_page_per_scene_each_of_the_paper_size_and_one_font_for_all() {
        let pages = [paper(0, 0, "Sheet 1/2"), paper(1000, -50, "Fo\u{e7}a")];

        let text = as_text(&pdf_pages(&pages, Theme::Print));

        assert!(text.contains("/Count 2"), "{text}");
        assert_eq!(text.matches("/MediaBox [0 0 840 590]").count(), 2);
        assert_eq!(text.matches("/FontFile2").count(), 1);
        assert_eq!(text.matches("/Type0").count(), 1);
        // The ToUnicode map has the characters of both pages.
        assert!(text.contains("<00E7>"));
    }

    #[test]
    fn no_scenes_still_make_a_document_with_one_empty_page() {
        let text = as_text(&pdf_pages(&[], Theme::Print));

        assert!(text.contains("/Count 1") && text.contains("/MediaBox [0 0 40 40]"));
        assert_eq!(
            pdf_pages(&[], Theme::Print),
            pdf(&Scene::default(), Theme::Print)
        );
    }

    #[test]
    fn a_single_scene_is_the_same_document_through_either_entry_point() {
        let scene = paper(5, 5, "K1");

        assert_eq!(pdf(&scene, Theme::Live), pdf_pages(&[scene], Theme::Live));
    }

    #[test]
    fn pages_are_deterministic_and_depend_on_their_order() {
        let a = paper(0, 0, "A");
        let b = paper(0, 0, "B");

        assert_eq!(
            pdf_pages(&[a.clone(), b.clone()], Theme::Print),
            pdf_pages(&[a.clone(), b.clone()], Theme::Print)
        );
        assert_ne!(
            pdf_pages(&[a.clone(), b.clone()], Theme::Print),
            pdf_pages(&[b, a], Theme::Print)
        );
    }

    #[test]
    fn document_has_no_date_and_no_identifier() {
        let text = as_text(&pdf(&text_scene("K1"), Theme::Print));

        for key in ["/CreationDate", "/ModDate", "/ID"] {
            assert!(!text.contains(key), "{key}");
        }
    }

    #[test]
    fn text_is_drawn_with_an_embedded_subset_of_liberation_mono() {
        let text = as_text(&pdf(&text_scene("K1"), Theme::Print));

        assert!(!text.contains("/BaseFont /Courier"));
        for part in [
            "/Subtype /Type0",
            "/Encoding /Identity-H",
            "/Subtype /CIDFontType2",
            "/CIDToGIDMap /Identity",
            "+LiberationMono",
            "/FontFile2",
            "/ToUnicode",
            "/Length1",
        ] {
            assert!(text.contains(part), "{part}");
        }
    }

    #[test]
    fn a_scene_without_text_embeds_no_font() {
        let text = as_text(&pdf(&Scene::default(), Theme::Print));

        assert!(!text.contains("/FontFile2") && !text.contains("/Type0"));
    }

    #[test]
    fn characters_beyond_ascii_are_kept_and_searchable() {
        let text = as_text(&pdf(&text_scene("Mot\u{e9}r \u{394} 3~"), Theme::Print));

        // The ToUnicode map names the code points of the scene.
        for code in ["<00E9>", "<0394>"] {
            assert!(text.contains(code), "{code}");
        }
    }

    #[test]
    fn a_character_the_font_lacks_is_drawn_as_the_empty_box_not_as_a_question_mark() {
        let with = pdf(&text_scene("K\u{4e2d}1"), Theme::Print);
        let without = pdf(&text_scene("K1"), Theme::Print);

        // Three glyph codes against two, and no question mark in the codes.
        assert_ne!(with, without);
        let text = as_text(&with);
        assert!(text.contains("/ToUnicode"));
        assert!(!text.contains("<003F>"));
    }

    #[test]
    fn the_same_scene_gives_the_same_bytes() {
        assert_eq!(
            pdf(&text_scene("KM1"), Theme::Live),
            pdf(&text_scene("KM1"), Theme::Live)
        );
    }

    #[test]
    fn centred_text_starts_half_its_glyph_width_left_of_the_anchor() {
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
