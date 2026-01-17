pub mod circular;
pub mod sugiyama;

use circular::{CircularConfig, CircularLayout};
use petgraph::graph::{Graph, NodeIndex};
use std::collections::HashMap;
use sugiyama::{SugiyamaConfig, SugiyamaLayout};

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum LayoutType {
    #[default]
    Circular,
    Sugiyama,
}

pub trait Layout {
    fn layout(&self, graph: &Graph<(), ()>) -> HashMap<NodeIndex, (f32, f32)>;
}

pub fn create_layout(layout_type: LayoutType) -> Box<dyn Layout> {
    match layout_type {
        LayoutType::Circular => Box::new(CircularLayout::new(CircularConfig::default())),
        LayoutType::Sugiyama => Box::new(SugiyamaLayout::new(SugiyamaConfig::default())),
    }
}

#[allow(dead_code)]
pub fn default_layout() -> Box<dyn Layout> {
    create_layout(LayoutType::default())
}
