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
        let canvas_at_origin = Rect::from_min_size(Pos2::ZERO, canvas_rect.size());
        let canvas_to_viewport = RectTransform::from_to(canvas_at_origin, adjusted_viewport);
        canvas_to_viewport.transform_pos(screen_pos)
    }

    pub fn world_to_screen(&self, world_pos: Pos2, canvas_rect: &Rect) -> Pos2 {
        let adjusted_viewport = self.get_adjusted_viewport(canvas_rect);
        let canvas_at_origin = Rect::from_min_size(Pos2::ZERO, canvas_rect.size());
        let viewport_to_canvas = RectTransform::from_to(adjusted_viewport, canvas_at_origin);
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
        let world_pos = self.screen_to_world(screen_pos, canvas_rect);

        let new_viewport_size = self.viewport.size() / zoom_factor;

        let cursor_offset = world_pos - self.viewport.min;
        let new_cursor_offset = cursor_offset / zoom_factor;
        let new_viewport_min = world_pos - new_cursor_offset;

        self.viewport = Rect::from_min_size(new_viewport_min, new_viewport_size);

        self.zoom_level *= zoom_factor;
        self.zoom_level = self.zoom_level.clamp(MIN_ZOOM_LEVEL, MAX_ZOOM_LEVEL);
    }

    #[inline]
    pub fn reset(&mut self) {
        self.viewport = Rect::from_min_max(pos2(0.0, 0.0), pos2(1000.0, 1000.0));
        self.zoom_level = 1.0;
    }
}
