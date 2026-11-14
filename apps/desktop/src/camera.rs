//! The view onto the sheet: scene subunits to screen points and back.
//!
//! The scene is in integer subunits; the screen is in `f32` points relative
//! to the top-left corner of the canvas. Floating point stays here, in the
//! application; the core never sees it.

use circuit_model::GridPoint;
use circuit_renderer::{SUBUNITS_PER_GRID, ScenePoint, SceneRect};

/// Smallest zoom, in screen points per subunit.
pub const MIN_ZOOM: f32 = 0.05;
/// Largest zoom, in screen points per subunit.
pub const MAX_ZOOM: f32 = 40.0;
/// Zoom of a new camera: one grid unit is 20 points.
const DEFAULT_ZOOM: f32 = 2.0;
/// Largest scene coordinate a screen point is converted back to. Beyond it a
/// conversion is refused instead of saturating.
const SCENE_LIMIT: f32 = 1.0e15;

/// Pan and zoom of the canvas.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    /// Screen position of the scene origin.
    pub offset: [f32; 2],
    /// Screen points per scene subunit.
    pub zoom: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            offset: [0.0, 0.0],
            zoom: DEFAULT_ZOOM,
        }
    }
}

/// A finite value, or `fallback`.
fn finite_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() { value } else { fallback }
}

impl Camera {
    /// The screen point of a scene point.
    #[must_use]
    pub fn to_screen(&self, point: ScenePoint) -> [f32; 2] {
        [
            (point.x as f32).mul_add(self.zoom, self.offset[0]),
            (point.y as f32).mul_add(self.zoom, self.offset[1]),
        ]
    }

    /// A length in subunits, on screen.
    #[must_use]
    pub fn to_screen_length(&self, length: i64) -> f32 {
        length as f32 * self.zoom
    }

    /// The scene point under a screen point, rounded to the nearest
    /// subunit; `None` when it is not a finite point of the scene.
    #[must_use]
    pub fn to_scene(&self, screen: [f32; 2]) -> Option<ScenePoint> {
        let convert = |value: f32, offset: f32| {
            let scene = ((value - offset) / self.zoom).round();
            // Cast only a finite value well inside the i64 range, where the
            // cast is exact; never NaN or an infinity.
            if scene.is_finite() && scene.abs() <= SCENE_LIMIT {
                Some(scene as i64)
            } else {
                None
            }
        };
        Some(ScenePoint::new(
            convert(screen[0], self.offset[0])?,
            convert(screen[1], self.offset[1])?,
        ))
    }

    /// The grid point nearest to a screen point; `None` when it is outside
    /// the document coordinate range.
    #[must_use]
    pub fn to_grid(&self, screen: [f32; 2]) -> Option<GridPoint> {
        let scene = self.to_scene(screen)?;
        let grid = |value: i64| {
            let rounded = value
                .checked_add(SUBUNITS_PER_GRID / 2)?
                .div_euclid(SUBUNITS_PER_GRID);
            i32::try_from(rounded).ok()
        };
        Some(GridPoint::new(grid(scene.x)?, grid(scene.y)?))
    }

    /// Move the view by a screen distance.
    pub fn pan(&mut self, delta: [f32; 2]) {
        self.offset = [
            finite_or(self.offset[0] + delta[0], self.offset[0]),
            finite_or(self.offset[1] + delta[1], self.offset[1]),
        ];
    }

    /// Multiply the zoom by `factor`, keeping the scene point under `anchor`
    /// where it is on screen. The zoom stays within [`MIN_ZOOM`] and
    /// [`MAX_ZOOM`]; a factor that is not a positive finite number is
    /// ignored.
    pub fn zoom_at(&mut self, anchor: [f32; 2], factor: f32) {
        if !(factor.is_finite() && factor > 0.0) {
            return;
        }
        let zoom = (self.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        let ratio = zoom / self.zoom;
        self.offset = [
            finite_or(
                (self.offset[0] - anchor[0]).mul_add(ratio, anchor[0]),
                self.offset[0],
            ),
            finite_or(
                (self.offset[1] - anchor[1]).mul_add(ratio, anchor[1]),
                self.offset[1],
            ),
        ];
        self.zoom = zoom;
    }

    /// Zoom and pan so that `bounds` fills a canvas of `size` points with a
    /// `margin` on every side, centred.
    pub fn fit(&mut self, bounds: SceneRect, size: [f32; 2], margin: f32) {
        let width = (bounds.max.x.saturating_sub(bounds.min.x)).max(1) as f32;
        let height = (bounds.max.y.saturating_sub(bounds.min.y)).max(1) as f32;
        let room = [
            (size[0] - 2.0 * margin).max(1.0),
            (size[1] - 2.0 * margin).max(1.0),
        ];
        let zoom = finite_or((room[0] / width).min(room[1] / height), DEFAULT_ZOOM)
            .clamp(MIN_ZOOM, MAX_ZOOM);
        let center = [
            (bounds.min.x as f32 + bounds.max.x as f32) / 2.0,
            (bounds.min.y as f32 + bounds.max.y as f32) / 2.0,
        ];
        self.zoom = zoom;
        self.offset = [
            finite_or(center[0].mul_add(-zoom, size[0] / 2.0), 0.0),
            finite_or(center[1].mul_add(-zoom, size[1] / 2.0), 0.0),
        ];
    }

    /// Pan so that the centre of `bounds` is in the middle of a canvas of
    /// `size` points, keeping the zoom.
    pub fn center_on(&mut self, bounds: SceneRect, size: [f32; 2]) {
        let center = [
            (bounds.min.x as f32 + bounds.max.x as f32) / 2.0,
            (bounds.min.y as f32 + bounds.max.y as f32) / 2.0,
        ];
        self.offset = [
            finite_or(center[0].mul_add(-self.zoom, size[0] / 2.0), self.offset[0]),
            finite_or(center[1].mul_add(-self.zoom, size[1] / 2.0), self.offset[1]),
        ];
    }

    /// The part of the scene visible in a canvas of `size` points.
    #[must_use]
    pub fn visible(&self, size: [f32; 2]) -> Option<SceneRect> {
        Some(SceneRect::from_corners(
            self.to_scene([0.0, 0.0])?,
            self.to_scene(size)?,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: [f32; 2], b: [f32; 2]) -> bool {
        (a[0] - b[0]).abs() < 1e-3 && (a[1] - b[1]).abs() < 1e-3
    }

    #[test]
    fn scene_point_survives_the_round_trip_through_the_screen() {
        for zoom in [MIN_ZOOM, 0.37, 1.0, 2.0, 13.0, MAX_ZOOM] {
            let camera = Camera {
                offset: [123.5, -40.25],
                zoom,
            };
            for point in [
                ScenePoint::new(0, 0),
                ScenePoint::new(250, -90),
                ScenePoint::new(-1_000, 7_000),
            ] {
                assert_eq!(
                    camera.to_scene(camera.to_screen(point)),
                    Some(point),
                    "zoom {zoom}"
                );
            }
        }
    }

    #[test]
    fn screen_point_maps_to_the_nearest_grid_point() {
        let camera = Camera {
            offset: [0.0, 0.0],
            zoom: 1.0,
        };

        assert_eq!(camera.to_grid([14.0, -16.0]), Some(GridPoint::new(1, -2)));
    }

    #[test]
    fn non_finite_screen_point_has_no_scene_point() {
        let camera = Camera::default();

        assert_eq!(camera.to_scene([f32::NAN, 0.0]), None);
        assert_eq!(camera.to_scene([0.0, f32::INFINITY]), None);
    }

    #[test]
    fn screen_point_beyond_the_document_range_has_no_grid_point() {
        let camera = Camera {
            offset: [0.0, 0.0],
            zoom: MIN_ZOOM,
        };

        assert_eq!(camera.to_grid([1.0e12, 0.0]), None);
    }

    #[test]
    fn zooming_keeps_the_point_under_the_cursor_in_place() {
        let mut camera = Camera {
            offset: [10.0, 20.0],
            zoom: 2.0,
        };
        let anchor = [300.0, 200.0];
        let before = camera.to_scene(anchor).unwrap();

        camera.zoom_at(anchor, 1.5);

        assert!(close(camera.to_screen(before), anchor));
    }

    #[test]
    fn zoom_stays_within_its_limits() {
        let mut camera = Camera::default();

        camera.zoom_at([0.0, 0.0], 1.0e9);

        assert!((camera.zoom - MAX_ZOOM).abs() < f32::EPSILON);
    }

    #[test]
    fn invalid_zoom_factor_is_ignored() {
        let mut camera = Camera::default();

        for factor in [0.0, -2.0, f32::NAN, f32::INFINITY] {
            camera.zoom_at([5.0, 5.0], factor);
        }

        assert_eq!(camera, Camera::default());
    }

    #[test]
    fn fitting_centres_the_bounds_on_the_canvas() {
        let mut camera = Camera::default();
        let bounds = SceneRect::from_corners(ScenePoint::new(-100, 0), ScenePoint::new(100, 400));

        camera.fit(bounds, [800.0, 600.0], 20.0);

        assert!(close(
            camera.to_screen(ScenePoint::new(0, 200)),
            [400.0, 300.0]
        ));
    }

    #[test]
    fn fitting_leaves_the_margin_free() {
        let mut camera = Camera::default();
        let bounds = SceneRect::from_corners(ScenePoint::new(0, 0), ScenePoint::new(100, 400));

        camera.fit(bounds, [800.0, 600.0], 20.0);

        assert!(close(
            camera.to_screen(ScenePoint::new(50, 0)),
            [400.0, 20.0]
        ));
    }

    #[test]
    fn centring_keeps_the_zoom() {
        let mut camera = Camera::default();
        let bounds = SceneRect::from_corners(ScenePoint::new(90, 90), ScenePoint::new(110, 110));

        camera.center_on(bounds, [400.0, 400.0]);

        assert!(close(
            camera.to_screen(ScenePoint::new(100, 100)),
            [200.0, 200.0]
        ));
        assert!((camera.zoom - 2.0).abs() < f32::EPSILON);
    }
}
