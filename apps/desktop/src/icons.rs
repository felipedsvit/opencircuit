//! Icons, drawn in code.
//!
//! Every icon is a handful of lines and circles on a 24 by 24 design grid,
//! scaled to the rectangle it is given: there is no image file, so nothing to
//! license, and the strokes follow the colours of the theme. The icons are
//! this project's own drawings. An icon is [`shapes`]; [`paint`] and
//! [`button`] put it on the screen.

use eframe::egui::{
    self, Color32, Pos2, Rect, Response, Role, Sense, Shape, Stroke, StrokeKind, Ui, WidgetInfo,
    vec2,
};

use crate::theme::Tokens;

/// What an icon shows. Some are used only by the palette and the inspector
/// that follow the window chrome.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[allow(dead_code, reason = "the palette and the inspector use the rest")]
pub enum Icon {
    New,
    Open,
    Save,
    SaveAs,
    Export,
    Library,
    Undo,
    Redo,
    Select,
    Wire,
    Rotate,
    Delete,
    Copy,
    Paste,
    Play,
    Pause,
    Step,
    Reset,
    Simulate,
    Edit,
    Fit,
    ZoomIn,
    ZoomOut,
    Pan,
    Grid,
    Snap,
    Ortho,
    Sheet,
    Warning,
    Info,
    Search,
    Plus,
    Close,
    ChevronDown,
    ChevronRight,
    Gear,
    CategorySupplies,
    CategoryProtection,
    CategoryControls,
    CategoryMotors,
    CategoryContacts,
    CategoryAutomation,
    CategoryActuators,
    CategoryLines,
}

impl Icon {
    /// Every icon, for tests and galleries.
    #[allow(dead_code, reason = "used by the tests of this module")]
    pub const ALL: [Self; 44] = [
        Self::New,
        Self::Open,
        Self::Save,
        Self::SaveAs,
        Self::Export,
        Self::Library,
        Self::Undo,
        Self::Redo,
        Self::Select,
        Self::Wire,
        Self::Rotate,
        Self::Delete,
        Self::Copy,
        Self::Paste,
        Self::Play,
        Self::Pause,
        Self::Step,
        Self::Reset,
        Self::Simulate,
        Self::Edit,
        Self::Fit,
        Self::ZoomIn,
        Self::ZoomOut,
        Self::Pan,
        Self::Grid,
        Self::Snap,
        Self::Ortho,
        Self::Sheet,
        Self::Warning,
        Self::Info,
        Self::Search,
        Self::Plus,
        Self::Close,
        Self::ChevronDown,
        Self::ChevronRight,
        Self::Gear,
        Self::CategorySupplies,
        Self::CategoryProtection,
        Self::CategoryControls,
        Self::CategoryMotors,
        Self::CategoryContacts,
        Self::CategoryAutomation,
        Self::CategoryActuators,
        Self::CategoryLines,
    ];
}

/// Draws on the 24 by 24 grid of an icon.
struct Pen {
    origin: Pos2,
    scale: f32,
    stroke: Stroke,
    accent: Stroke,
    fill: Color32,
    shapes: Vec<Shape>,
}

type P = (f32, f32);

impl Pen {
    fn new(rect: Rect, color: Color32, accent: Color32) -> Self {
        let scale = rect.width().min(rect.height()) / 24.0;
        let width = (scale * 1.6).clamp(1.0, 2.0);
        // Centre the square design grid in the rectangle.
        let origin = rect.center() - vec2(12.0, 12.0) * scale;
        Self {
            origin,
            scale,
            stroke: Stroke::new(width, color),
            accent: Stroke::new(width, accent),
            fill: accent,
            shapes: Vec::new(),
        }
    }

    fn at(&self, (x, y): P) -> Pos2 {
        self.origin + vec2(x, y) * self.scale
    }

    fn line(&mut self, a: P, b: P) {
        self.shapes
            .push(Shape::line_segment([self.at(a), self.at(b)], self.stroke));
    }

    fn accent_line(&mut self, a: P, b: P) {
        self.shapes
            .push(Shape::line_segment([self.at(a), self.at(b)], self.accent));
    }

    fn poly(&mut self, points: &[P], closed: bool) {
        let points: Vec<Pos2> = points.iter().map(|&p| self.at(p)).collect();
        self.shapes.push(if closed {
            Shape::closed_line(points, self.stroke)
        } else {
            Shape::line(points, self.stroke)
        });
    }

    fn accent_poly(&mut self, points: &[P], closed: bool) {
        let points: Vec<Pos2> = points.iter().map(|&p| self.at(p)).collect();
        self.shapes.push(if closed {
            Shape::closed_line(points, self.accent)
        } else {
            Shape::line(points, self.accent)
        });
    }

    fn rect(&mut self, a: P, b: P) {
        let rect = Rect::from_two_pos(self.at(a), self.at(b));
        self.shapes.push(Shape::rect_stroke(
            rect,
            1.0,
            self.stroke,
            StrokeKind::Middle,
        ));
    }

    fn circle(&mut self, centre: P, radius: f32) {
        self.shapes.push(Shape::circle_stroke(
            self.at(centre),
            radius * self.scale,
            self.stroke,
        ));
    }

    fn dot(&mut self, centre: P, radius: f32) {
        self.shapes.push(Shape::circle_filled(
            self.at(centre),
            radius * self.scale,
            self.fill,
        ));
    }
}

/// The shapes of `icon` in `rect`: strokes in `color`, with the part that
/// stands for what is created or selected in `accent`.
#[must_use]
pub fn shapes(icon: Icon, rect: Rect, color: Color32, accent: Color32) -> Vec<Shape> {
    let mut pen = Pen::new(rect, color, accent);
    draw(&mut pen, icon);
    pen.shapes
}

fn draw(pen: &mut Pen, icon: Icon) {
    match icon {
        Icon::New => {
            pen.poly(
                &[
                    (6.0, 3.0),
                    (14.0, 3.0),
                    (18.0, 7.0),
                    (18.0, 21.0),
                    (6.0, 21.0),
                ],
                true,
            );
            pen.line((14.0, 3.0), (14.0, 7.0));
            pen.line((14.0, 7.0), (18.0, 7.0));
            pen.accent_line((12.0, 12.0), (12.0, 18.0));
            pen.accent_line((9.0, 15.0), (15.0, 15.0));
        }
        Icon::Open => {
            pen.poly(
                &[
                    (3.0, 7.0),
                    (9.0, 7.0),
                    (11.0, 9.0),
                    (20.0, 9.0),
                    (20.0, 19.0),
                    (3.0, 19.0),
                ],
                true,
            );
            pen.accent_line((3.0, 19.0), (6.0, 12.0));
            pen.accent_line((6.0, 12.0), (23.0, 12.0));
        }
        Icon::Save | Icon::SaveAs => {
            pen.poly(
                &[
                    (4.0, 4.0),
                    (17.0, 4.0),
                    (20.0, 7.0),
                    (20.0, 20.0),
                    (4.0, 20.0),
                ],
                true,
            );
            pen.rect((8.0, 4.0), (15.0, 9.0));
            pen.rect((8.0, 14.0), (16.0, 20.0));
            if icon == Icon::SaveAs {
                pen.accent_line((19.0, 15.0), (19.0, 23.0));
                pen.accent_line((15.0, 19.0), (23.0, 19.0));
            }
        }
        Icon::Export => {
            pen.poly(
                &[(5.0, 12.0), (5.0, 20.0), (19.0, 20.0), (19.0, 12.0)],
                false,
            );
            pen.accent_line((12.0, 15.0), (12.0, 3.0));
            pen.accent_poly(&[(8.0, 7.0), (12.0, 3.0), (16.0, 7.0)], false);
        }
        Icon::Library => {
            pen.rect((4.0, 4.0), (8.0, 20.0));
            pen.rect((10.0, 4.0), (14.0, 20.0));
            pen.poly(
                &[(16.0, 6.0), (20.0, 5.0), (22.0, 19.0), (18.0, 20.0)],
                true,
            );
            pen.accent_line((5.0, 8.0), (7.0, 8.0));
        }
        Icon::Undo | Icon::Redo => {
            let flip = if icon == Icon::Redo { -1.0 } else { 1.0 };
            let x = |v: f32| 12.0 + (v - 12.0) * flip;
            pen.poly(
                &[
                    (x(18.0), 18.0),
                    (x(18.0), 12.0),
                    (x(13.0), 8.0),
                    (x(8.0), 8.0),
                ],
                false,
            );
            pen.accent_poly(&[(x(11.0), 4.0), (x(7.0), 8.0), (x(11.0), 12.0)], false);
        }
        Icon::Select => {
            pen.poly(
                &[
                    (6.0, 3.0),
                    (6.0, 19.0),
                    (10.0, 15.0),
                    (13.0, 21.0),
                    (16.0, 19.5),
                    (13.0, 14.0),
                    (18.0, 14.0),
                ],
                true,
            );
        }
        Icon::Wire => {
            pen.accent_poly(
                &[
                    (4.0, 18.0),
                    (4.0, 10.0),
                    (14.0, 10.0),
                    (14.0, 5.0),
                    (20.0, 5.0),
                ],
                false,
            );
            pen.dot((4.0, 18.0), 1.8);
            pen.dot((20.0, 5.0), 1.8);
        }
        Icon::Rotate => {
            pen.poly(
                &[
                    (19.0, 12.0),
                    (18.0, 16.0),
                    (15.0, 19.0),
                    (11.0, 20.0),
                    (7.0, 19.0),
                    (4.5, 16.0),
                ],
                false,
            );
            pen.poly(
                &[
                    (4.5, 12.0),
                    (5.5, 8.0),
                    (8.5, 5.0),
                    (12.5, 4.0),
                    (16.5, 5.0),
                    (19.0, 8.0),
                ],
                false,
            );
            pen.accent_poly(&[(15.0, 8.0), (19.0, 8.0), (19.0, 4.0)], false);
        }
        Icon::Delete => {
            pen.poly(&[(6.0, 7.0), (7.0, 21.0), (17.0, 21.0), (18.0, 7.0)], false);
            pen.line((4.0, 7.0), (20.0, 7.0));
            pen.poly(&[(9.0, 7.0), (9.0, 4.0), (15.0, 4.0), (15.0, 7.0)], false);
            pen.accent_line((10.0, 11.0), (10.0, 17.0));
            pen.accent_line((14.0, 11.0), (14.0, 17.0));
        }
        Icon::Copy => {
            pen.rect((4.0, 4.0), (15.0, 17.0));
            pen.accent_poly(&[(9.0, 20.0), (20.0, 20.0), (20.0, 9.0)], false);
        }
        Icon::Paste => {
            pen.rect((5.0, 5.0), (19.0, 21.0));
            pen.rect((9.0, 3.0), (15.0, 7.0));
            pen.accent_line((8.0, 13.0), (16.0, 13.0));
            pen.accent_line((8.0, 17.0), (14.0, 17.0));
        }
        Icon::Play => {
            pen.accent_poly(&[(7.0, 4.0), (19.0, 12.0), (7.0, 20.0)], true);
        }
        Icon::Pause => {
            pen.accent_line((8.0, 5.0), (8.0, 19.0));
            pen.accent_line((16.0, 5.0), (16.0, 19.0));
        }
        Icon::Step => {
            pen.accent_poly(&[(5.0, 5.0), (15.0, 12.0), (5.0, 19.0)], true);
            pen.line((19.0, 5.0), (19.0, 19.0));
        }
        Icon::Reset => {
            pen.poly(
                &[
                    (19.0, 12.0),
                    (18.0, 16.0),
                    (15.0, 19.0),
                    (11.0, 20.0),
                    (7.0, 19.0),
                    (4.5, 16.0),
                    (4.0, 12.0),
                    (5.0, 8.0),
                    (8.0, 5.0),
                    (12.0, 4.0),
                ],
                false,
            );
            pen.accent_poly(&[(9.0, 1.5), (12.5, 4.0), (9.5, 7.0)], false);
        }
        Icon::Simulate => {
            pen.accent_poly(
                &[
                    (13.0, 2.0),
                    (5.0, 13.0),
                    (11.0, 13.0),
                    (9.0, 22.0),
                    (19.0, 10.0),
                    (13.0, 10.0),
                ],
                true,
            );
        }
        Icon::Edit => {
            pen.poly(
                &[
                    (4.0, 20.0),
                    (5.0, 15.0),
                    (16.0, 4.0),
                    (20.0, 8.0),
                    (9.0, 19.0),
                ],
                true,
            );
            pen.accent_line((14.0, 6.0), (18.0, 10.0));
        }
        Icon::Fit => {
            for (corner, dx, dy) in [
                ((4.0, 4.0), 5.0, 5.0),
                ((20.0, 4.0), -5.0, 5.0),
                ((4.0, 20.0), 5.0, -5.0),
                ((20.0, 20.0), -5.0, -5.0),
            ] {
                pen.line(corner, (corner.0 + dx, corner.1));
                pen.line(corner, (corner.0, corner.1 + dy));
            }
            pen.accent_poly(&[(9.0, 9.0), (15.0, 9.0), (15.0, 15.0), (9.0, 15.0)], true);
        }
        Icon::ZoomIn | Icon::ZoomOut | Icon::Search => {
            pen.circle((10.0, 10.0), 6.0);
            pen.line((14.5, 14.5), (21.0, 21.0));
            if icon != Icon::Search {
                pen.accent_line((7.0, 10.0), (13.0, 10.0));
            }
            if icon == Icon::ZoomIn {
                pen.accent_line((10.0, 7.0), (10.0, 13.0));
            }
        }
        Icon::Pan => {
            pen.line((12.0, 3.0), (12.0, 21.0));
            pen.line((3.0, 12.0), (21.0, 12.0));
            pen.accent_poly(&[(9.0, 6.0), (12.0, 3.0), (15.0, 6.0)], false);
            pen.accent_poly(&[(9.0, 18.0), (12.0, 21.0), (15.0, 18.0)], false);
            pen.accent_poly(&[(6.0, 9.0), (3.0, 12.0), (6.0, 15.0)], false);
            pen.accent_poly(&[(18.0, 9.0), (21.0, 12.0), (18.0, 15.0)], false);
        }
        Icon::Grid => {
            for x in [6.0, 12.0, 18.0] {
                for y in [6.0, 12.0, 18.0] {
                    pen.dot((x, y), 1.3);
                }
            }
        }
        Icon::Snap => {
            pen.rect((5.0, 5.0), (19.0, 19.0));
            pen.accent_line((12.0, 2.0), (12.0, 8.0));
            pen.accent_line((2.0, 12.0), (8.0, 12.0));
            pen.dot((12.0, 12.0), 2.0);
        }
        Icon::Ortho => {
            pen.poly(&[(5.0, 4.0), (5.0, 19.0), (20.0, 19.0)], false);
            pen.accent_poly(&[(5.0, 15.0), (9.0, 15.0), (9.0, 19.0)], false);
        }
        Icon::Sheet => {
            pen.rect((3.0, 4.0), (21.0, 20.0));
            pen.rect((6.0, 7.0), (18.0, 17.0));
            pen.accent_line((14.0, 20.0), (14.0, 17.0));
        }
        Icon::Warning => {
            pen.accent_poly(&[(12.0, 3.0), (22.0, 20.0), (2.0, 20.0)], true);
            pen.line((12.0, 9.0), (12.0, 14.0));
            pen.dot((12.0, 17.0), 1.2);
        }
        Icon::Info => {
            pen.circle((12.0, 12.0), 9.0);
            pen.line((12.0, 11.0), (12.0, 17.0));
            pen.dot((12.0, 7.5), 1.2);
        }
        Icon::Plus => {
            pen.accent_line((12.0, 5.0), (12.0, 19.0));
            pen.accent_line((5.0, 12.0), (19.0, 12.0));
        }
        Icon::Close => {
            pen.line((6.0, 6.0), (18.0, 18.0));
            pen.line((18.0, 6.0), (6.0, 18.0));
        }
        Icon::ChevronDown => pen.poly(&[(6.0, 9.0), (12.0, 15.0), (18.0, 9.0)], false),
        Icon::ChevronRight => pen.poly(&[(9.0, 6.0), (15.0, 12.0), (9.0, 18.0)], false),
        Icon::Gear => {
            pen.circle((12.0, 12.0), 3.5);
            pen.circle((12.0, 12.0), 8.0);
            for (a, b) in [
                ((12.0, 2.0), (12.0, 4.0)),
                ((12.0, 20.0), (12.0, 22.0)),
                ((2.0, 12.0), (4.0, 12.0)),
                ((20.0, 12.0), (22.0, 12.0)),
            ] {
                pen.line(a, b);
            }
        }
        Icon::CategorySupplies => {
            pen.circle((12.0, 12.0), 9.0);
            pen.accent_line((8.0, 9.0), (12.0, 9.0));
            pen.accent_line((10.0, 7.0), (10.0, 11.0));
            pen.line((12.0, 15.5), (16.0, 15.5));
        }
        Icon::CategoryProtection => {
            pen.poly(
                &[
                    (12.0, 3.0),
                    (20.0, 6.0),
                    (20.0, 13.0),
                    (12.0, 21.0),
                    (4.0, 13.0),
                    (4.0, 6.0),
                ],
                true,
            );
            pen.accent_poly(&[(8.0, 12.0), (11.0, 15.0), (16.0, 9.0)], false);
        }
        Icon::CategoryControls => {
            pen.circle((12.0, 10.0), 6.0);
            pen.accent_line((12.0, 4.0), (12.0, 10.0));
            pen.line((6.0, 18.0), (18.0, 18.0));
            pen.line((8.0, 21.0), (16.0, 21.0));
        }
        Icon::CategoryMotors => {
            pen.circle((12.0, 12.0), 9.0);
            pen.accent_poly(
                &[
                    (7.0, 16.0),
                    (7.0, 8.0),
                    (12.0, 13.0),
                    (17.0, 8.0),
                    (17.0, 16.0),
                ],
                false,
            );
        }
        Icon::CategoryContacts => {
            pen.circle((5.0, 12.0), 2.0);
            pen.circle((19.0, 12.0), 2.0);
            pen.line((7.0, 12.0), (10.0, 12.0));
            pen.accent_line((10.0, 12.0), (16.0, 6.0));
        }
        Icon::CategoryAutomation => {
            pen.rect((7.0, 7.0), (17.0, 17.0));
            for v in [10.0, 14.0] {
                pen.line((v, 3.0), (v, 7.0));
                pen.line((v, 17.0), (v, 21.0));
                pen.line((3.0, v), (7.0, v));
                pen.line((17.0, v), (21.0, v));
            }
            pen.dot((12.0, 12.0), 1.6);
        }
        Icon::CategoryActuators => {
            pen.rect((3.0, 8.0), (14.0, 16.0));
            pen.line((14.0, 12.0), (21.0, 12.0));
            pen.accent_line((9.0, 8.0), (9.0, 16.0));
        }
        Icon::CategoryLines => {
            pen.line((3.0, 8.0), (21.0, 8.0));
            pen.line((3.0, 16.0), (21.0, 16.0));
            pen.accent_line((8.0, 3.0), (16.0, 21.0));
            pen.dot((12.0, 12.0), 1.6);
        }
    }
}

/// Paint `icon` in `rect`, in the icon colour of the theme.
#[allow(dead_code, reason = "the palette paints category icons with it")]
pub fn paint(ui: &Ui, rect: Rect, icon: Icon) {
    let tokens = Tokens::of(ui);
    ui.painter()
        .extend(shapes(icon, rect, tokens.icon, tokens.accent));
}

/// A square button with an icon. `label` is its tooltip and its accessible
/// name; `selected` draws it pressed in.
pub fn button(ui: &mut Ui, icon: Icon, label: &str, selected: bool) -> Response {
    button_sized(ui, icon, label, label, selected, 26.0)
}

/// [`button`] whose tooltip says more than its name.
pub fn button_hint(ui: &mut Ui, icon: Icon, label: &str, hint: &str, selected: bool) -> Response {
    button_sized(ui, icon, label, hint, selected, 26.0)
}

/// [`button`] with a side of `size` points and its own tooltip.
pub fn button_sized(
    ui: &mut Ui,
    icon: Icon,
    label: &str,
    hint: &str,
    selected: bool,
    size: f32,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(egui::Vec2::splat(size), Sense::click());
    let enabled = ui.is_enabled();
    response.widget_info(|| WidgetInfo::selected(Role::Button, enabled, selected, label));
    if ui.is_rect_visible(rect) {
        let tokens = Tokens::of(ui);
        let fill = if selected {
            tokens.tab_active
        } else if response.hovered() && enabled {
            tokens.control_hover
        } else {
            Color32::TRANSPARENT
        };
        ui.painter().rect_filled(rect, 4.0, fill);
        if selected {
            ui.painter().rect_stroke(
                rect,
                4.0,
                Stroke::new(1.0, tokens.accent),
                StrokeKind::Inside,
            );
        }
        let inner = rect.shrink(size * 0.17);
        let (color, accent) = if enabled {
            (tokens.icon, tokens.accent)
        } else {
            (tokens.text_faint, tokens.text_faint)
        };
        ui.painter().extend(shapes(icon, inner, color, accent));
    }
    response.on_hover_text(hint)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    fn square() -> Rect {
        Rect::from_min_size(Pos2::new(100.0, 100.0), egui::Vec2::splat(24.0))
    }

    #[test]
    fn the_list_of_icons_is_complete_and_has_no_repeat() {
        let unique: BTreeSet<String> = Icon::ALL.iter().map(|icon| format!("{icon:?}")).collect();

        assert_eq!(unique.len(), Icon::ALL.len());
    }

    #[test]
    fn every_icon_draws_something_inside_its_rectangle() {
        let allowed = square().expand(1.5);
        for icon in Icon::ALL {
            let shapes = shapes(icon, square(), Color32::WHITE, Color32::BLUE);

            assert!(!shapes.is_empty(), "{icon:?}");
            for shape in &shapes {
                let bounds = shape.visual_bounding_rect();
                assert!(
                    bounds.is_finite() && allowed.contains_rect(bounds),
                    "{icon:?} draws outside its rectangle: {bounds:?}"
                );
            }
        }
    }

    #[test]
    fn an_icon_scales_with_its_rectangle() {
        let small = Rect::from_min_size(Pos2::ZERO, egui::Vec2::splat(12.0));
        let large = Rect::from_min_size(Pos2::ZERO, egui::Vec2::splat(48.0));
        let extent = |rect| {
            shapes(Icon::Fit, rect, Color32::WHITE, Color32::BLUE)
                .iter()
                .fold(Rect::NOTHING, |all, shape| {
                    all.union(shape.visual_bounding_rect())
                })
                .width()
        };

        assert!(extent(large) > 3.0 * extent(small));
    }

    #[test]
    fn a_degenerate_rectangle_does_not_panic() {
        for rect in [Rect::NOTHING, Rect::ZERO, Rect::EVERYTHING] {
            for icon in Icon::ALL {
                let _ = shapes(icon, rect, Color32::WHITE, Color32::BLUE);
            }
        }
    }
}
