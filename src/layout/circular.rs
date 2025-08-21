use crate::layout::Layout;
use petgraph::graph::{Graph, NodeIndex};
use std::collections::HashMap;
use std::f32::consts::PI;

/// Configuration options for Circular layout
#[derive(Debug, Clone)]
pub struct CircularConfig {
    /// Radius of the circle
    pub radius: f32,
    /// Starting angle in radians
    pub start_angle: f32,
}

impl Default for CircularConfig {
    fn default() -> Self {
        Self {
            radius: 300.0,
            start_angle: 0.0,
        }
    }
}

/// Circular layout implementation that places nodes in a circle
pub struct CircularLayout {
    config: CircularConfig,
}

impl CircularLayout {
    pub fn new(config: CircularConfig) -> Self {
        Self { config }
    }
}

impl Layout for CircularLayout {
    fn layout(&self, graph: &Graph<(), ()>) -> HashMap<NodeIndex, (f32, f32)> {
        let node_count = graph.node_count();
        if node_count == 0 {
            return HashMap::new();
        }

        let mut positions = HashMap::new();
        let angle_step = 2.0 * PI / node_count as f32;

        for (i, node) in graph.node_indices().enumerate() {
            let angle = self.config.start_angle + i as f32 * angle_step;
            let x = self.config.radius * angle.cos();
            let y = self.config.radius * angle.sin();
            positions.insert(node, (x, y));
        }

        positions
    }
}
