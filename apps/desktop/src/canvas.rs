//! The canvas: paints a scene through the camera and handles pan and zoom.

use circuit_renderer::{Anchor, SUBUNITS_PER_GRID, Scene, ScenePoint, SceneRect, Shape};
use eframe::egui::{self, Align2, FontId, Pos2, Rect, Sense, StrokeKind, Ui, Vec2};

use crate::camera::Camera;
use crate::theme::{self, Tokens};

/// Free space kept around the scene when fitting it.
pub const FIT_MARGIN: f32 = 24.0;
/// Grid dots are drawn only when grid units are at least this far apart on
/// screen, so a zoomed-out view is not a grey wash.
const MIN_GRID_POINTS: f32 = 8.0;
/// Radius of a grid dot on screen.
const GRID_DOT: f32 = 0.8;
/// Zoom change per scrolled point.
const SCROLL_ZOOM: f32 = 0.0015;
/// Texts are drawn at this height in subunits, then scaled with the zoom.
const TEXT_HEIGHT: f32 = 9.0;
/// Texts smaller than this on screen are not drawn.
const MIN_TEXT_POINTS: f32 = 5.0;
/// Largest text height on screen.
const MAX_TEXT_POINTS: f32 = 48.0;

/// What happened on the canvas this frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct CanvasEvents {
    /// Where a primary click landed, relative to the canvas.
    pub clicked: Option<[f32; 2]>,
    /// Where the primary button went down on the canvas this frame.
    pub pressed: Option<[f32; 2]>,
    /// Whether the primary button was released this frame, anywhere.
    pub released: bool,
    /// Where the pointer was when the primary button was released, if it
    /// was released this frame.
    pub released_at: Option<[f32; 2]>,
    /// Where the pointer hovers over the canvas.
    pub hover: Option<[f32; 2]>,
    /// Where a secondary click landed, relative to the canvas: a drag with
    /// that button pans and is not a click.
    pub context: Option<[f32; 2]>,
    /// The top-left corner of the canvas on screen.
    pub origin: [f32; 2],
    /// The size of the canvas.
    pub size: [f32; 2],
}

/// Paint `scene` in the remaining space of `ui` and apply pan and zoom.
/// With `fit` set, the camera first frames the whole scene; with `grid` set,
/// the grid points are drawn as dots under the scene.
pub fn show(
    ui: &mut Ui,
    scene: &Scene,
    camera: &mut Camera,
    fit: bool,
    grid: bool,
) -> CanvasEvents {
    let size = ui.available_size();
    let (response, painter) = ui.allocate_painter(size, Sense::click_and_drag());
    let origin = response.rect.min;
    let size = [response.rect.width(), response.rect.height()];

    if fit {
        match scene.bounds {
            Some(bounds) => camera.fit(bounds, size, FIT_MARGIN),
            // An empty sheet: put the grid origin in the middle, so that
            // there is room to draw in every direction.
            None => {
                let origin = ScenePoint::new(0, 0);
                camera.center_on(SceneRect::from_corners(origin, origin), size);
            }
        }
    }
    if response.double_clicked()
        && let Some(bounds) = scene.bounds
    {
        camera.fit(bounds, size, FIT_MARGIN);
    }
    if response.dragged_by(egui::PointerButton::Secondary)
        || response.dragged_by(egui::PointerButton::Middle)
    {
        let delta = response.drag_delta();
        camera.pan([delta.x, delta.y]);
    }
    if let Some(hover) = response.hover_pos() {
        let scroll = ui.input(|input| input.smooth_scroll_delta.y);
        let pinch = ui.input(egui::InputState::zoom_delta);
        let factor = (scroll * SCROLL_ZOOM).exp() * pinch;
        if (factor - 1.0).abs() > f32::EPSILON {
            let local = hover - origin;
            camera.zoom_at([local.x, local.y], factor);
        }
    }

    let to_screen = |point| {
        let [x, y] = camera.to_screen(point);
        origin + Vec2::new(x, y)
    };
    let visible = camera.visible(size);
    let dark = ui.visuals().dark_mode;
    if grid {
        paint_grid(&painter, camera, visible, to_screen, Tokens::of(ui).grid);
    }
    let text_points = (TEXT_HEIGHT * camera.zoom).min(MAX_TEXT_POINTS);

    for item in &scene.items {
        if let (Some(visible), Some(bounds)) = (visible, item.shape.bounds())
            && !visible.intersects(&bounds)
            && !matches!(item.shape, Shape::Text { .. })
        {
            continue;
        }
        let look = theme::look(item.style, dark);
        let stroke = look.stroke();
        match &item.shape {
            Shape::Line { from, to } => {
                painter.line_segment([to_screen(*from), to_screen(*to)], stroke);
            }
            Shape::Polyline(points) => {
                painter.line(points.iter().map(|p| to_screen(*p)).collect(), stroke);
            }
            Shape::Rect(rect) => {
                painter.rect_stroke(
                    Rect::from_two_pos(to_screen(rect.min), to_screen(rect.max)),
                    0.0,
                    stroke,
                    StrokeKind::Middle,
                );
            }
            Shape::Circle { center, radius } => {
                painter.circle_stroke(to_screen(*center), camera.to_screen_length(*radius), stroke);
            }
            Shape::Dot { center, radius } => {
                painter.circle_filled(
                    to_screen(*center),
                    camera.to_screen_length(*radius).max(1.5),
                    look.color,
                );
            }
            Shape::Block(rect) => {
                painter.rect_filled(
                    Rect::from_two_pos(to_screen(rect.min), to_screen(rect.max)),
                    0.0,
                    look.color,
                );
            }
            Shape::Text { at, text, anchor } => {
                if text_points >= MIN_TEXT_POINTS {
                    let align = match anchor {
                        Anchor::Left => Align2::LEFT_CENTER,
                        Anchor::Center => Align2::CENTER_CENTER,
                    };
                    painter.text(
                        to_screen(*at),
                        align,
                        text,
                        FontId::proportional(text_points),
                        look.color,
                    );
                }
            }
        }
    }

    let local = |pos: Pos2| {
        let local = pos - origin;
        [local.x, local.y]
    };
    CanvasEvents {
        clicked: response
            .clicked()
            .then(|| response.interact_pointer_pos().map(local))
            .flatten(),
        pressed: (response.hovered() && ui.input(|input| input.pointer.primary_pressed()))
            .then(|| ui.input(|input| input.pointer.press_origin()).map(local))
            .flatten(),
        released: ui.input(|input| input.pointer.primary_released()),
        released_at: ui
            .input(|input| {
                input
                    .pointer
                    .primary_released()
                    .then(|| input.pointer.latest_pos())
                    .flatten()
            })
            .map(local),
        hover: response.hover_pos().map(local),
        context: response
            .secondary_clicked()
            .then(|| response.interact_pointer_pos().map(local))
            .flatten(),
        origin: [origin.x, origin.y],
        size,
    }
}

/// Draw a dot on every grid point of the visible area, when grid units are
/// far enough apart on screen.
fn paint_grid(
    painter: &egui::Painter,
    camera: &Camera,
    visible: Option<SceneRect>,
    to_screen: impl Fn(ScenePoint) -> Pos2,
    color: egui::Color32,
) {
    let step = SUBUNITS_PER_GRID;
    let (Some(visible), true) = (visible, camera.to_screen_length(step) >= MIN_GRID_POINTS) else {
        return;
    };
    let first = |value: i64| value.div_euclid(step).saturating_mul(step);
    let mut y = first(visible.min.y);
    while y <= visible.max.y {
        let mut x = first(visible.min.x);
        while x <= visible.max.x {
            painter.circle_filled(to_screen(ScenePoint::new(x, y)), GRID_DOT, color);
            x = x.saturating_add(step);
        }
        y = y.saturating_add(step);
    }
}
