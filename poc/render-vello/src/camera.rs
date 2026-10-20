//! View transform for zoom and pan. Screen coordinates are physical pixels.

use vello::kurbo::{Affine, Point, Rect, Vec2};

pub const MIN_ZOOM: f64 = 0.02;
pub const MAX_ZOOM: f64 = 64.0;

/// `screen = world * zoom + pan`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    pub zoom: f64,
    pub pan: Vec2,
}

impl Camera {
    pub fn transform(&self) -> Affine {
        Affine::translate(self.pan) * Affine::scale(self.zoom)
    }

    pub fn to_world(&self, screen: Point) -> Point {
        ((screen.to_vec2() - self.pan) / self.zoom).to_point()
    }

    /// Zooms by `factor` keeping the world point under `screen` in place.
    pub fn zoom_at(&mut self, screen: Point, factor: f64) {
        if !factor.is_finite() || factor <= 0.0 {
            return;
        }
        let world = self.to_world(screen);
        self.zoom = (self.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        self.pan = screen.to_vec2() - world.to_vec2() * self.zoom;
    }

    pub fn pan_by(&mut self, delta: Vec2) {
        self.pan += delta;
    }

    /// Shows the whole of `world` centered in a viewport of `width` × `height`.
    pub fn fit(world: Rect, width: f64, height: f64) -> Self {
        let zoom = (width / world.width())
            .min(height / world.height())
            .clamp(MIN_ZOOM, MAX_ZOOM);
        let pan = Vec2::new(width, height) / 2.0 - world.center().to_vec2() * zoom;
        Self { zoom, pan }
    }
}

/// Duration of the automatic zoom and pan tour, in seconds.
pub const TOUR_SECONDS: f64 = 12.0;
/// Deepest zoom of the tour, relative to showing the whole scene.
const TOUR_MAX_ZOOM_IN: f64 = 40.0;

/// Camera at `seconds` into the smoothness tour: it circles around the scene
/// while zooming in and out three times, from the whole scene to 40×.
pub fn tour(seconds: f64, world: Rect, width: f64, height: f64) -> Camera {
    use std::f64::consts::{PI, TAU};
    let u = (seconds / TOUR_SECONDS).clamp(0.0, 1.0);
    let fit = Camera::fit(world, width, height);
    let zoom =
        (fit.zoom * TOUR_MAX_ZOOM_IN.powf((3.0 * PI * u).sin().abs())).clamp(MIN_ZOOM, MAX_ZOOM);
    let radius = 0.3 * world.width().min(world.height());
    let center = world.center() + Vec2::new((TAU * u).cos(), (TAU * u).sin()) * radius;
    Camera {
        zoom,
        pan: Vec2::new(width, height) / 2.0 - center.to_vec2() * zoom,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tour_starts_and_ends_showing_everything_zoomed_out() {
        let world = Rect::new(0.0, 0.0, 4800.0, 4800.0);
        let fit = Camera::fit(world, 1000.0, 700.0);
        assert!((tour(0.0, world, 1000.0, 700.0).zoom - fit.zoom).abs() < 1e-9);
        assert!((tour(TOUR_SECONDS, world, 1000.0, 700.0).zoom - fit.zoom).abs() < 1e-6);
    }

    #[test]
    fn tour_zooms_deep_and_stays_in_limits() {
        let world = Rect::new(0.0, 0.0, 4800.0, 4800.0);
        let fit = Camera::fit(world, 1000.0, 700.0);
        let mut deepest: f64 = 0.0;
        for i in 0..=1200 {
            let camera = tour(f64::from(i) / 100.0, world, 1000.0, 700.0);
            assert!((MIN_ZOOM..=MAX_ZOOM).contains(&camera.zoom));
            deepest = deepest.max(camera.zoom / fit.zoom);
        }
        assert!(deepest > 39.0);
    }

    #[test]
    fn tour_center_stays_inside_the_world() {
        let world = Rect::new(0.0, -760.0, 4800.0, 4800.0);
        for i in 0..=120 {
            let camera = tour(f64::from(i) / 10.0, world, 1000.0, 700.0);
            assert!(world.contains(camera.to_world(Point::new(500.0, 350.0))));
        }
    }

    fn close(a: Point, b: Point) -> bool {
        (a - b).hypot() < 1e-9
    }

    #[test]
    fn point_under_cursor_stays_put_while_zooming() {
        let mut camera = Camera {
            zoom: 1.5,
            pan: Vec2::new(-30.0, 12.0),
        };
        let cursor = Point::new(400.0, 250.0);
        let before = camera.to_world(cursor);
        for factor in [1.1, 0.5, 3.0, 0.9] {
            camera.zoom_at(cursor, factor);
            assert!(close(camera.to_world(cursor), before));
        }
    }

    #[test]
    fn zoom_is_clamped() {
        let mut camera = Camera::fit(Rect::new(0.0, 0.0, 100.0, 100.0), 100.0, 100.0);
        for _ in 0..200 {
            camera.zoom_at(Point::ZERO, 2.0);
        }
        assert_eq!(camera.zoom, MAX_ZOOM);
        for _ in 0..200 {
            camera.zoom_at(Point::ZERO, 0.5);
        }
        assert_eq!(camera.zoom, MIN_ZOOM);
    }

    #[test]
    fn invalid_zoom_factor_is_ignored() {
        let mut camera = Camera::fit(Rect::new(0.0, 0.0, 10.0, 10.0), 100.0, 100.0);
        let before = camera;
        for factor in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            camera.zoom_at(Point::new(5.0, 5.0), factor);
        }
        assert_eq!(camera, before);
    }

    #[test]
    fn transform_and_to_world_are_inverse() {
        let camera = Camera {
            zoom: 2.5,
            pan: Vec2::new(17.0, -4.0),
        };
        let world = Point::new(123.0, -45.0);
        assert!(close(camera.to_world(camera.transform() * world), world));
    }

    #[test]
    fn pan_moves_the_view_by_the_same_amount() {
        let mut camera = Camera::fit(Rect::new(0.0, 0.0, 10.0, 10.0), 100.0, 100.0);
        let world = Point::new(3.0, 4.0);
        let before = camera.transform() * world;
        camera.pan_by(Vec2::new(10.0, -20.0));
        assert!(close(
            camera.transform() * world,
            before + Vec2::new(10.0, -20.0)
        ));
    }

    #[test]
    fn fit_shows_the_whole_world_centered() {
        let world = Rect::new(-50.0, 20.0, 950.0, 520.0);
        let camera = Camera::fit(world, 800.0, 600.0);
        let screen = camera.transform().transform_rect_bbox(world);
        assert!(
            Rect::new(0.0, 0.0, 800.0, 600.0)
                .inflate(1e-9, 1e-9)
                .contains_rect(screen)
        );
        assert!(close(screen.center(), Point::new(400.0, 300.0)));
    }
}
