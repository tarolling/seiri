use egui::{Pos2, Rect};

/// Returns true if any part of the segment `a`-`b` lies within `rect`, using Liang-Barsky
/// clipping. Used for edge frustum culling: an edge can be entirely outside the canvas at
/// both endpoints while still passing through the visible viewport (e.g. a long edge between
/// two off-screen nodes), so checking endpoint containment alone misses it.
pub fn segment_intersects_rect(a: Pos2, b: Pos2, rect: Rect) -> bool {
    let dx = b.x - a.x;
    let dy = b.y - a.y;

    let p = [-dx, dx, -dy, dy];
    let q = [
        a.x - rect.min.x,
        rect.max.x - a.x,
        a.y - rect.min.y,
        rect.max.y - a.y,
    ];

    let mut t0 = 0.0f32;
    let mut t1 = 1.0f32;

    for i in 0..4 {
        if p[i] == 0.0 {
            if q[i] < 0.0 {
                return false;
            }
        } else {
            let r = q[i] / p[i];
            if p[i] < 0.0 {
                if r > t1 {
                    return false;
                }
                if r > t0 {
                    t0 = r;
                }
            } else {
                if r < t0 {
                    return false;
                }
                if r < t1 {
                    t1 = r;
                }
            }
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::pos2;

    fn rect(min_x: f32, min_y: f32, max_x: f32, max_y: f32) -> Rect {
        Rect::from_min_max(pos2(min_x, min_y), pos2(max_x, max_y))
    }

    #[test]
    fn segment_visible_when_both_endpoints_inside_rect() {
        let r = rect(0.0, 0.0, 100.0, 100.0);
        assert!(segment_intersects_rect(
            pos2(10.0, 10.0),
            pos2(90.0, 90.0),
            r
        ));
    }

    #[test]
    fn segment_visible_when_one_endpoint_inside_rect() {
        let r = rect(0.0, 0.0, 100.0, 100.0);
        assert!(segment_intersects_rect(
            pos2(50.0, 50.0),
            pos2(500.0, 500.0),
            r
        ));
    }

    #[test]
    fn segment_visible_when_it_crosses_rect_with_both_endpoints_outside() {
        // Both endpoints are off-screen on opposite sides, but the segment passes straight
        // through the visible canvas in between - this is the case the naive
        // endpoint-containment check used to miss entirely.
        let r = rect(0.0, 0.0, 100.0, 100.0);
        assert!(segment_intersects_rect(
            pos2(-500.0, 50.0),
            pos2(500.0, 50.0),
            r
        ));
    }

    #[test]
    fn segment_not_visible_when_entirely_outside_rect() {
        let r = rect(0.0, 0.0, 100.0, 100.0);
        assert!(!segment_intersects_rect(
            pos2(200.0, 200.0),
            pos2(300.0, 300.0),
            r
        ));
    }

    #[test]
    fn segment_not_visible_when_parallel_and_outside_rect() {
        let r = rect(0.0, 0.0, 100.0, 100.0);
        assert!(!segment_intersects_rect(
            pos2(-50.0, 500.0),
            pos2(500.0, 500.0),
            r
        ));
    }

    #[test]
    fn segment_visible_when_touching_rect_edge() {
        let r = rect(0.0, 0.0, 100.0, 100.0);
        assert!(segment_intersects_rect(
            pos2(-50.0, 50.0),
            pos2(0.0, 50.0),
            r
        ));
    }
}
