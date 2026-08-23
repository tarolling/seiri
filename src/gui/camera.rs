use egui::{Pos2, Rect, Vec2, emath::RectTransform, pos2, vec2};

const MIN_ZOOM_LEVEL: f32 = 0.1;
const MAX_ZOOM_LEVEL: f32 = 1000.0;

pub struct Camera {
    viewport: Rect,
    // current zoom level, used for display purposes
    zoom_level: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            viewport: Rect::from_min_max(pos2(0.0, 0.0), pos2(1000.0, 1000.0)),
            zoom_level: 1.0,
        }
    }
}

impl Camera {
    #[inline(always)]
    pub fn zoom_level(&self) -> f32 {
        self.zoom_level
    }

    pub fn screen_to_world(&self, screen_pos: Pos2, canvas_rect: &Rect) -> Pos2 {
        let adjusted_viewport = self.get_adjusted_viewport(canvas_rect);
        let canvas_to_viewport = RectTransform::from_to(*canvas_rect, adjusted_viewport);
        canvas_to_viewport.transform_pos(screen_pos)
    }

    pub fn world_to_screen(&self, world_pos: Pos2, canvas_rect: &Rect) -> Pos2 {
        let adjusted_viewport = self.get_adjusted_viewport(canvas_rect);
        let viewport_to_canvas = RectTransform::from_to(adjusted_viewport, *canvas_rect);
        viewport_to_canvas.transform_pos(world_pos)
    }

    /// adjust viewport to match canvas aspect ratio
    pub fn get_adjusted_viewport(&self, canvas_rect: &Rect) -> Rect {
        let canvas_aspect = canvas_rect.width() / canvas_rect.height();
        let viewport_aspect = self.viewport.width() / self.viewport.height();

        if canvas_aspect > viewport_aspect {
            // canvas is wider - expand viewport width to match
            let new_width = self.viewport.height() * canvas_aspect;
            Rect::from_center_size(
                self.viewport.center(),
                vec2(new_width, self.viewport.height()),
            )
        } else {
            // canvas is taller - expand viewport height to match
            let new_height = self.viewport.width() / canvas_aspect;
            Rect::from_center_size(
                self.viewport.center(),
                vec2(self.viewport.width(), new_height),
            )
        }
    }

    #[inline]
    pub fn pan(&mut self, screen_delta: Vec2, canvas_rect: &Rect) {
        let scale = self.viewport.width() / canvas_rect.width();

        self.viewport = self
            .viewport
            .translate(vec2(-screen_delta.x * scale, -screen_delta.y * scale));
    }

    pub fn zoom_at(&mut self, zoom_factor: f32, screen_pos: Pos2, canvas_rect: &Rect) {
        if !zoom_factor.is_finite() || zoom_factor <= 0.0 {
            return;
        }

        // clamp the *target* zoom level first, then derive the effective
        // per-frame zoom factor from it, so the reported zoom level and the
        // actual viewport scale never diverge
        let target_zoom_level =
            (self.zoom_level * zoom_factor).clamp(MIN_ZOOM_LEVEL, MAX_ZOOM_LEVEL);
        let effective_zoom_factor = target_zoom_level / self.zoom_level;

        if !effective_zoom_factor.is_finite() || effective_zoom_factor <= 0.0 {
            return;
        }

        let world_pos = self.screen_to_world(screen_pos, canvas_rect);

        let new_viewport_size = self.viewport.size() / effective_zoom_factor;

        let cursor_offset = world_pos - self.viewport.min;
        let new_cursor_offset = cursor_offset / effective_zoom_factor;
        let new_viewport_min = world_pos - new_cursor_offset;

        self.viewport = Rect::from_min_size(new_viewport_min, new_viewport_size);
        self.zoom_level = target_zoom_level;
    }

    #[inline]
    pub fn reset(&mut self) {
        self.viewport = Rect::from_min_max(pos2(0.0, 0.0), pos2(1000.0, 1000.0));
        self.zoom_level = 1.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A canvas rect offset from the window origin, as it would be when the
    /// canvas panel sits beside a toolbar/side panel rather than at (0, 0).
    fn offset_canvas_rect() -> Rect {
        Rect::from_min_size(pos2(200.0, 100.0), vec2(1000.0, 1000.0))
    }

    #[test]
    fn world_to_screen_offsets_by_canvas_rect_min() {
        let camera = Camera::default();
        let canvas_rect = offset_canvas_rect();

        // the center of the default viewport should map to the center of the
        // (offset) canvas rect, not the center of a canvas assumed to start
        // at the window origin
        let world_center = camera.get_adjusted_viewport(&canvas_rect).center();
        let screen_pos = camera.world_to_screen(world_center, &canvas_rect);

        assert!((screen_pos.x - canvas_rect.center().x).abs() < 0.001);
        assert!((screen_pos.y - canvas_rect.center().y).abs() < 0.001);
    }

    #[test]
    fn screen_to_world_offsets_by_canvas_rect_min() {
        let camera = Camera::default();
        let canvas_rect = offset_canvas_rect();

        // the center of the (offset) canvas rect in screen space should map
        // back to the center of the world viewport
        let world_pos = camera.screen_to_world(canvas_rect.center(), &canvas_rect);
        let expected = camera.get_adjusted_viewport(&canvas_rect).center();

        assert!((world_pos.x - expected.x).abs() < 0.001);
        assert!((world_pos.y - expected.y).abs() < 0.001);
    }

    #[test]
    fn screen_world_round_trip_with_offset_canvas() {
        let camera = Camera::default();
        let canvas_rect = offset_canvas_rect();

        let original = pos2(canvas_rect.min.x + 37.0, canvas_rect.min.y + 42.0);
        let world = camera.screen_to_world(original, &canvas_rect);
        let back = camera.world_to_screen(world, &canvas_rect);

        assert!((back.x - original.x).abs() < 0.001);
        assert!((back.y - original.y).abs() < 0.001);
    }

    #[test]
    fn zoom_level_and_viewport_scale_stay_consistent_when_clamped() {
        let mut camera = Camera::default();
        let canvas_rect = Rect::from_min_size(Pos2::ZERO, vec2(1000.0, 1000.0));
        let initial_viewport_width = camera.viewport.width();

        // zoom out far enough to hit MIN_ZOOM_LEVEL
        camera.zoom_at(0.0001, canvas_rect.center(), &canvas_rect);

        assert_eq!(camera.zoom_level(), MIN_ZOOM_LEVEL);

        // the viewport must have scaled by exactly the same factor as the
        // reported zoom level, not by the raw (unclamped) zoom_factor
        let expected_width = initial_viewport_width / MIN_ZOOM_LEVEL;
        assert!((camera.viewport.width() - expected_width).abs() < 0.01);
    }

    #[test]
    fn zoom_level_and_viewport_scale_stay_consistent_at_max_clamp() {
        let mut camera = Camera::default();
        let canvas_rect = Rect::from_min_size(Pos2::ZERO, vec2(1000.0, 1000.0));
        let initial_viewport_width = camera.viewport.width();

        // zoom in far enough to hit MAX_ZOOM_LEVEL
        camera.zoom_at(100000.0, canvas_rect.center(), &canvas_rect);

        assert_eq!(camera.zoom_level(), MAX_ZOOM_LEVEL);

        let expected_width = initial_viewport_width / MAX_ZOOM_LEVEL;
        assert!((camera.viewport.width() - expected_width).abs() < 0.01);
    }

    #[test]
    fn zoom_at_rejects_non_positive_zoom_factor() {
        let mut camera = Camera::default();
        let canvas_rect = Rect::from_min_size(Pos2::ZERO, vec2(1000.0, 1000.0));
        let viewport_before = camera.viewport;
        let zoom_level_before = camera.zoom_level();

        camera.zoom_at(0.0, canvas_rect.center(), &canvas_rect);
        camera.zoom_at(-1.0, canvas_rect.center(), &canvas_rect);

        assert_eq!(camera.viewport, viewport_before);
        assert_eq!(camera.zoom_level(), zoom_level_before);
    }

    #[test]
    fn repeated_zoom_out_never_produces_non_positive_viewport() {
        let mut camera = Camera::default();
        let canvas_rect = Rect::from_min_size(Pos2::ZERO, vec2(1000.0, 1000.0));

        for _ in 0..100 {
            camera.zoom_at(0.5, canvas_rect.center(), &canvas_rect);
        }

        assert!(camera.viewport.width() > 0.0);
        assert!(camera.viewport.height() > 0.0);
        assert!(camera.zoom_level() >= MIN_ZOOM_LEVEL);
    }
}
