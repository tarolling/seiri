use egui::Pos2;

/// Returns the index of the topmost node whose circle (center, radius) contains `cursor`.
/// Nodes are given in draw order, so later nodes are on top.
pub fn topmost_node_at(
    cursor: Pos2,
    nodes: impl DoubleEndedIterator<Item = (Pos2, f32)> + ExactSizeIterator,
) -> Option<usize> {
    let count = nodes.len();
    nodes
        .rev()
        .position(|(center, radius)| (center - cursor).length_sq() <= radius * radius)
        .map(|rev_index| count - 1 - rev_index)
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::pos2;

    fn single(center: Pos2, radius: f32) -> Vec<(Pos2, f32)> {
        vec![(center, radius)]
    }

    #[test]
    fn cursor_at_center_hits() {
        let nodes = single(pos2(100.0, 100.0), 20.0);
        assert_eq!(
            topmost_node_at(pos2(100.0, 100.0), nodes.into_iter()),
            Some(0)
        );
    }

    #[test]
    fn cursor_just_inside_rim_hits() {
        let nodes = single(pos2(100.0, 100.0), 20.0);
        assert_eq!(
            topmost_node_at(pos2(119.8, 100.0), nodes.into_iter()),
            Some(0)
        );
    }

    #[test]
    fn cursor_on_boundary_hits() {
        let nodes = single(pos2(0.0, 0.0), 20.0);
        assert_eq!(topmost_node_at(pos2(0.0, 20.0), nodes.into_iter()), Some(0));
    }

    #[test]
    fn cursor_just_outside_misses() {
        let nodes = single(pos2(0.0, 0.0), 20.0);
        assert_eq!(topmost_node_at(pos2(20.1, 0.0), nodes.into_iter()), None);
    }

    #[test]
    fn overlapping_nodes_pick_the_one_drawn_last() {
        let nodes = vec![(pos2(0.0, 0.0), 20.0), (pos2(10.0, 0.0), 20.0)];
        assert_eq!(topmost_node_at(pos2(5.0, 0.0), nodes.into_iter()), Some(1));
    }

    #[test]
    fn earlier_node_hit_when_later_node_does_not_cover_cursor() {
        let nodes = vec![(pos2(0.0, 0.0), 20.0), (pos2(100.0, 0.0), 20.0)];
        assert_eq!(topmost_node_at(pos2(5.0, 0.0), nodes.into_iter()), Some(0));
    }

    #[test]
    fn no_nodes_returns_none() {
        assert_eq!(topmost_node_at(pos2(0.0, 0.0), std::iter::empty()), None);
    }
}
