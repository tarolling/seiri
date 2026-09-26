pub mod circular;
pub mod force_directed;
pub mod sugiyama;

use circular::{CircularConfig, CircularLayout};
use force_directed::{ForceDirectedConfig, ForceDirectedLayout};
use petgraph::graph::{Graph, NodeIndex};
use std::collections::HashMap;
use sugiyama::{SugiyamaConfig, SugiyamaLayout};

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum LayoutType {
    #[default]
    ForceDirected,
    Circular,
    Sugiyama,
}

pub trait Layout {
    fn layout(&self, graph: &Graph<(), ()>) -> HashMap<NodeIndex, (f32, f32)>;
}

pub fn create_layout(layout_type: LayoutType) -> Box<dyn Layout> {
    match layout_type {
        LayoutType::ForceDirected => {
            Box::new(ForceDirectedLayout::new(ForceDirectedConfig::default()))
        }
        LayoutType::Circular => Box::new(CircularLayout::new(CircularConfig::default())),
        LayoutType::Sugiyama => Box::new(SugiyamaLayout::new(SugiyamaConfig::default())),
    }
}
