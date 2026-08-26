//! Sugiyama graph layout. See: https://en.wikipedia.org/wiki/Layered_graph_drawing

use crate::layout::Layout;
use petgraph::Direction;
use petgraph::graph::{Graph, NodeIndex};
use std::collections::{HashMap, HashSet};

/// Configuration options for Sugiyama layout
#[derive(Debug, Clone)]
pub struct SugiyamaConfig {
    /// Maximum number of iterations for crossing minimization.
    pub max_iterations: usize,
    /// Minimum horizontal distance between nodes in the same layer.
    pub node_spacing: f32,
    /// Vertical distance between layers.
    pub layer_spacing: f32,
    /// Scaling factor for the entire layout.
    pub scale_factor: f32,
}

impl Default for SugiyamaConfig {
    fn default() -> Self {
        Self {
            max_iterations: 50,
            node_spacing: 120.0,
            layer_spacing: 150.0,
            scale_factor: 1.5,
        }
    }
}

/// A node in the layered graph with positioning information
#[derive(Debug, Clone, Default)]
struct LayeredNode {
    id: NodeIndex,
    layer: usize,
    #[allow(dead_code)]
    position: f32,
    is_dummy: bool,
}

impl LayeredNode {
    fn new(id: NodeIndex, layer: usize, is_dummy: bool) -> Self {
        Self {
            id,
            layer,
            position: 0.0,
            is_dummy,
        }
    }
}

/// Sugiyama (hierarchical) layout implementation.
pub struct SugiyamaLayout {
    config: SugiyamaConfig,
}

impl SugiyamaLayout {
    pub fn new(config: SugiyamaConfig) -> Self {
        Self { config }
    }

    /// Create a directed acyclic graph by removing a minimal set of edges.
    ///
    /// Uses an iterative DFS (explicit stack) rather than recursion so this
    /// can't stack-overflow on large/deep graphs, and shares a single
    /// `visited` set across all start nodes so each node is only traversed
    /// from once instead of restarting a fresh traversal per start node.
    fn make_dag(&self, graph: &Graph<(), ()>) -> Graph<(), ()> {
        let mut dag = graph.clone();
        let mut visited = HashSet::new();

        for start_node in graph.node_indices() {
            if visited.contains(&start_node) {
                continue;
            }

            let mut on_stack = HashSet::new();
            let mut neighbor_cache: HashMap<NodeIndex, Vec<NodeIndex>> = HashMap::new();
            // explicit DFS stack: (node, index of the next neighbor to visit)
            let mut stack: Vec<(NodeIndex, usize)> = Vec::new();

            visited.insert(start_node);
            on_stack.insert(start_node);
            stack.push((start_node, 0));

            while let Some(&(current, idx)) = stack.last() {
                let neighbors = neighbor_cache.entry(current).or_insert_with(|| {
                    dag.neighbors_directed(current, Direction::Outgoing)
                        .collect()
                });

                if idx < neighbors.len() {
                    let neighbor = neighbors[idx];
                    stack.last_mut().unwrap().1 += 1;

                    if !visited.contains(&neighbor) {
                        visited.insert(neighbor);
                        on_stack.insert(neighbor);
                        stack.push((neighbor, 0));
                    } else if on_stack.contains(&neighbor) {
                        // found a cycle, remove the edge that closes it
                        if let Some(edge) = dag.find_edge(current, neighbor) {
                            dag.remove_edge(edge);
                        }
                    }
                } else {
                    on_stack.remove(&current);
                    stack.pop();
                }
            }
        }

        dag
    }

    /// Assign vertices to layers using the longest-path algorithm: each
    /// node's layer is the maximum of `predecessor_layer + 1` over all of
    /// its incoming edges, computed via a single pass over a topological
    /// order of the (already acyclic) graph.
    fn assign_layers(&self, dag: &Graph<(), ()>) -> Vec<Vec<LayeredNode>> {
        let mut layers = Vec::new();
        let mut node_layers = HashMap::new();

        let topo_order = petgraph::algo::toposort(dag, None).unwrap_or_else(|_| {
            // Should not happen since `dag` has already had its cycles broken,
            // but fall back to insertion order rather than panicking
            dag.node_indices().collect()
        });

        for node in topo_order {
            let layer = dag
                .neighbors_directed(node, Direction::Incoming)
                .filter_map(|pred| node_layers.get(&pred))
                .max()
                .map_or(0, |&max_pred_layer| max_pred_layer + 1);
            node_layers.insert(node, layer);
        }

        // Handle any remaining unassigned nodes (in case of disconnected components)
        for node in dag.node_indices() {
            node_layers.entry(node).or_insert(0);
        }

        // group nodes by layer. iterate `dag.node_indices()` rather than `node_layers` directly, so
        // that nodes are pushed into each layer in a consistent order
        let max_layer = *node_layers.values().max().unwrap_or(&0) + 1;
        layers.resize(max_layer, Vec::new());

        for node in dag.node_indices() {
            let layer = node_layers[&node];
            layers[layer].push(LayeredNode::new(node, layer, false));
        }

        // Sort nodes within each layer by their number of connections
        for layer in &mut layers {
            layer.sort_by_key(|node| {
                -(dag.neighbors_directed(node.id, Direction::Outgoing).count() as i32
                    + dag.neighbors_directed(node.id, Direction::Incoming).count() as i32)
            });
        }

        layers
    }

    /// Add dummy nodes for edges that span multiple layers.
    fn expand_long_edges(&self, dag: &mut Graph<(), ()>, layers: &mut [Vec<LayeredNode>]) {
        let mut new_edges = Vec::new();
        let mut dummy_nodes = Vec::new();

        for layer_idx in 0..layers.len() - 1 {
            for node in &layers[layer_idx] {
                let neighbors: Vec<_> = dag
                    .neighbors_directed(node.id, Direction::Outgoing)
                    .collect();

                for &neighbor in &neighbors {
                    let target_layer = layers
                        .iter()
                        .position(|l| l.iter().any(|n| n.id == neighbor))
                        .unwrap();

                    if target_layer > layer_idx + 1 {
                        // Create dummy nodes for long edges
                        let mut prev = node.id;
                        for l in (layer_idx + 1)..target_layer {
                            let dummy = dag.add_node(());
                            dummy_nodes.push(LayeredNode::new(dummy, l, true));
                            new_edges.push((prev, dummy));
                            prev = dummy;
                        }
                        new_edges.push((prev, neighbor));
                        dag.remove_edge(dag.find_edge(node.id, neighbor).unwrap());
                    }
                }
            }
        }

        // Add dummy nodes to layers
        for dummy in dummy_nodes {
            layers[dummy.layer].push(dummy);
        }

        // Add new edges
        for (src, dst) in new_edges {
            dag.add_edge(src, dst, ());
        }
    }

    /// Count crossings between two adjacent layers.
    fn count_crossings(
        &self,
        layer1: &[LayeredNode],
        layer2: &[LayeredNode],
        dag: &Graph<(), ()>,
    ) -> usize {
        let mut crossings = 0;

        for (i1, n1) in layer1.iter().enumerate() {
            for (i2, n2) in layer1.iter().enumerate().skip(i1 + 1) {
                for n1_neighbor in dag.neighbors_directed(n1.id, Direction::Outgoing) {
                    for n2_neighbor in dag.neighbors_directed(n2.id, Direction::Outgoing) {
                        let pos1 = layer2.iter().position(|n| n.id == n1_neighbor);
                        let pos2 = layer2.iter().position(|n| n.id == n2_neighbor);

                        if let (Some(p1), Some(p2)) = (pos1, pos2)
                            && (i1 < i2) != (p1 < p2)
                        {
                            crossings += 1;
                        }
                    }
                }
            }
        }

        crossings
    }

    /// Reduce edge crossings between layers.
    fn reduce_crossings(&self, layers: &mut [Vec<LayeredNode>], dag: &Graph<(), ()>) {
        for _ in 0..self.config.max_iterations {
            let mut improved = false;

            // Forward pass
            for i in 0..layers.len() - 1 {
                let crossings = self.count_crossings(&layers[i], &layers[i + 1], dag);
                let mut best_crossings = crossings;
                let mut best_order = layers[i].clone();

                // Try swapping adjacent nodes
                for j in 0..layers[i].len() - 1 {
                    layers[i].swap(j, j + 1);
                    let new_crossings = self.count_crossings(&layers[i], &layers[i + 1], dag);

                    if new_crossings < best_crossings {
                        best_crossings = new_crossings;
                        best_order = layers[i].clone();
                        improved = true;
                    }

                    layers[i].swap(j, j + 1);
                }

                layers[i] = best_order;
            }

            if !improved {
                break;
            }
        }
    }

    /// Assign x, y coordinates to all nodes
    fn assign_coordinates(
        &self,
        dag: &Graph<(), ()>,
        layers: &[Vec<LayeredNode>],
    ) -> HashMap<NodeIndex, (f32, f32)> {
        let mut coordinates = HashMap::new();

        // Calculate the maximum width needed for proper centering
        let total_height = (layers.len().saturating_sub(1)) as f32 * self.config.layer_spacing;

        for (layer_idx, layer) in layers.iter().enumerate() {
            let y = (layer_idx as f32 * self.config.layer_spacing - total_height / 2.0)
                * self.config.scale_factor;

            // Center this layer horizontally
            let layer_width = (layer.len().saturating_sub(1)) as f32 * self.config.node_spacing;
            let start_x = -layer_width / 2.0;

            for (node_idx, node) in layer.iter().enumerate() {
                let x = (start_x + node_idx as f32 * self.config.node_spacing)
                    * self.config.scale_factor;
                coordinates.insert(node.id, (x, y));
            }
        }

        // Fine-tune positions by averaging connected nodes' x coordinates
        let mut adjusted_coords = coordinates.clone();
        for _ in 0..3 {
            // Do a few iterations of position refinement
            for layer in layers.iter() {
                for node in layer {
                    if !node.is_dummy {
                        // Only adjust real nodes
                        let mut sum_x = 0.0;
                        let mut count = 0;

                        // Consider incoming edges
                        for neighbor in dag.neighbors_directed(node.id, Direction::Incoming) {
                            if let Some(&(x, _)) = coordinates.get(&neighbor) {
                                sum_x += x;
                                count += 1;
                            }
                        }

                        // Consider outgoing edges
                        for neighbor in dag.neighbors_directed(node.id, Direction::Outgoing) {
                            if let Some(&(x, _)) = coordinates.get(&neighbor) {
                                sum_x += x;
                                count += 1;
                            }
                        }

                        if count > 0 {
                            let (current_x, y) = coordinates[&node.id];
                            let target_x = sum_x / count as f32;
                            // Move partially toward target
                            let new_x = current_x * 0.5 + target_x * 0.5;
                            adjusted_coords.insert(node.id, (new_x, y));
                        }
                    }
                }
            }
            coordinates = adjusted_coords.clone();
        }

        // refinement above pulls nodes toward their neighbors' x
        // coordinates with no lower bound, which can collapse siblings in
        // the same layer on top of each other; restore `node_spacing` as a
        // hard minimum by sweeping each layer left-to-right in x order and
        // pushing any node that ended up too close to its left neighbor
        let min_gap = self.config.node_spacing * self.config.scale_factor;
        for layer in layers.iter() {
            let mut ids: Vec<NodeIndex> = layer.iter().map(|node| node.id).collect();
            ids.sort_by(|&a, &b| coordinates[&a].0.partial_cmp(&coordinates[&b].0).unwrap());

            for pair in ids.windows(2) {
                let (left, right) = (pair[0], pair[1]);
                let min_x = coordinates[&left].0 + min_gap;
                if coordinates[&right].0 < min_x {
                    let y = coordinates[&right].1;
                    coordinates.insert(right, (min_x, y));
                }
            }
        }

        coordinates
    }
}

impl Layout for SugiyamaLayout {
    fn layout(&self, graph: &Graph<(), ()>) -> HashMap<NodeIndex, (f32, f32)> {
        if graph.node_count() == 0 {
            return HashMap::new();
        }

        // step 1: make the graph acyclic
        let mut dag = self.make_dag(graph);

        // step 2: assign vertices to layers
        let mut layers = self.assign_layers(&dag);

        // step 3: add dummy nodes for long edges
        self.expand_long_edges(&mut dag, &mut layers);

        // step 4: reduce edge crossings
        self.reduce_crossings(&mut layers, &dag);

        // step 5: assign coordinates
        self.assign_coordinates(&dag, &layers)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Diamond-shaped dependency: A->B, A->C->B. B must land in the layer
    /// one past its deepest predecessor (C), not the layer from the first
    /// edge visited (the direct A->B edge).
    #[test]
    fn assign_layers_uses_longest_path_for_diamond() {
        let layout = SugiyamaLayout::new(SugiyamaConfig::default());
        let mut graph: Graph<(), ()> = Graph::new();
        let a = graph.add_node(());
        let b = graph.add_node(());
        let c = graph.add_node(());
        graph.add_edge(a, b, ());
        graph.add_edge(a, c, ());
        graph.add_edge(c, b, ());

        let layers = layout.assign_layers(&graph);

        let layer_of = |id: NodeIndex| -> usize {
            layers
                .iter()
                .position(|layer| layer.iter().any(|n| n.id == id))
                .expect("node should be assigned a layer")
        };

        assert_eq!(layer_of(a), 0);
        assert_eq!(layer_of(c), 1);
        assert_eq!(
            layer_of(b),
            2,
            "B must be promoted to layer 2 via the longer A->C->B path"
        );
    }

    /// Regression test for issue #154: `assign_layers` used to push nodes
    /// into each layer by iterating a `HashMap<NodeIndex, usize>` directly,
    /// so same-degree nodes (which the subsequent stable sort leaves in
    /// whatever order they were pushed) ended up in HashMap iteration order
    /// -- different on every call, since each `HashMap::new()` gets a fresh
    /// random hasher seed. Same-degree siblings must now come out in a
    /// consistent order (ascending `NodeIndex`, i.e. creation order) on every
    /// call.
    #[test]
    fn assign_layers_orders_same_degree_nodes_deterministically() {
        let layout = SugiyamaLayout::new(SugiyamaConfig::default());
        let mut graph: Graph<(), ()> = Graph::new();
        let hub = graph.add_node(());
        let children: Vec<_> = (0..8).map(|_| graph.add_node(())).collect();
        for &c in &children {
            graph.add_edge(hub, c, ());
        }

        for _ in 0..20 {
            let layers = layout.assign_layers(&graph);
            let child_order: Vec<_> = layers[1].iter().map(|n| n.id).collect();
            assert_eq!(
                child_order, children,
                "same-degree nodes must be ordered deterministically (by creation order) across calls"
            );
        }
    }

    /// `node_spacing` is documented as "Minimum horizontal distance between
    /// nodes in the same layer". A hub node with several children pulls all
    /// of them toward the same x during the connectivity-based refinement
    /// pass; that pass must not be allowed to collapse siblings closer than
    /// the configured minimum.
    #[test]
    fn assign_coordinates_respects_minimum_node_spacing_after_refinement() {
        let config = SugiyamaConfig::default();
        let layout = SugiyamaLayout::new(config.clone());
        let mut graph: Graph<(), ()> = Graph::new();
        let hub = graph.add_node(());
        let children: Vec<_> = (0..5).map(|_| graph.add_node(())).collect();
        for &c in &children {
            graph.add_edge(hub, c, ());
        }

        let positions = layout.layout(&graph);

        let min_gap = config.node_spacing * config.scale_factor;
        let mut child_xs: Vec<f32> = children.iter().map(|c| positions[c].0).collect();
        child_xs.sort_by(|a, b| a.partial_cmp(b).unwrap());

        for pair in child_xs.windows(2) {
            let gap = pair[1] - pair[0];
            assert!(
                gap >= min_gap - 1e-3,
                "siblings collapsed too close together: gap {gap} < configured minimum {min_gap}"
            );
        }
    }

    /// `make_dag`'s cycle-breaking DFS must be iterative: a long chain
    /// closed into one big cycle would blow the stack with a naive
    /// recursive DFS, and restarting a fresh traversal per start node would
    /// make this quadratic. Neither should happen here.
    #[test]
    fn make_dag_breaks_large_cycle_without_stack_overflow() {
        let layout = SugiyamaLayout::new(SugiyamaConfig::default());
        let mut graph: Graph<(), ()> = Graph::new();
        let n = 100_000;
        let nodes: Vec<_> = (0..n).map(|_| graph.add_node(())).collect();
        for pair in nodes.windows(2) {
            graph.add_edge(pair[0], pair[1], ());
        }
        // Close the chain into one long cycle.
        graph.add_edge(nodes[n - 1], nodes[0], ());

        let dag = layout.make_dag(&graph);

        assert!(
            petgraph::algo::toposort(&dag, None).is_ok(),
            "make_dag must remove enough edges to leave an acyclic graph"
        );
        assert_eq!(dag.edge_count(), graph.edge_count() - 1);
    }

    /// Several disjoint cycles must each be broken independently, even
    /// though `make_dag` now shares one `visited` set across start nodes
    /// instead of resetting it per component.
    #[test]
    fn make_dag_breaks_multiple_disjoint_cycles() {
        let layout = SugiyamaLayout::new(SugiyamaConfig::default());
        let mut graph: Graph<(), ()> = Graph::new();

        for _ in 0..5 {
            let a = graph.add_node(());
            let b = graph.add_node(());
            let c = graph.add_node(());
            graph.add_edge(a, b, ());
            graph.add_edge(b, c, ());
            graph.add_edge(c, a, ());
        }

        let dag = layout.make_dag(&graph);

        assert!(petgraph::algo::toposort(&dag, None).is_ok());
        // One edge removed per 3-node cycle.
        assert_eq!(dag.edge_count(), graph.edge_count() - 5);
    }
}
