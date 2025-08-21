use crate::layout::Layout;
use petgraph::Direction;
use petgraph::graph::{Graph, NodeIndex};
use std::collections::{HashMap, HashSet, VecDeque};

/// Configuration options for Sugiyama layout
#[derive(Debug, Clone)]
pub struct SugiyamaConfig {
    /// Maximum number of iterations for crossing minimization
    pub max_iterations: usize,
    /// Minimum horizontal distance between nodes in the same layer
    pub node_spacing: f32,
    /// Vertical distance between layers
    pub layer_spacing: f32,
}

impl Default for SugiyamaConfig {
    fn default() -> Self {
        Self {
            max_iterations: 50,
            node_spacing: 50.0,
            layer_spacing: 70.0,
        }
    }
}

/// A node in the layered graph with positioning information
#[derive(Debug, Clone, Default)]
struct LayeredNode {
    id: NodeIndex,
    layer: usize,
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

/// Sugiyama (hierarchical) layout implementation
pub struct SugiyamaLayout {
    config: SugiyamaConfig,
}

impl SugiyamaLayout {
    pub fn new(config: SugiyamaConfig) -> Self {
        Self { config }
    }

    /// Create a directed acyclic graph by removing a minimal set of edges
    fn make_dag(&self, graph: &Graph<(), ()>) -> Graph<(), ()> {
        let mut dag = graph.clone();

        // Find all cycles and break them
        for start_node in graph.node_indices() {
            let mut visited = HashSet::new();
            let mut path = Vec::new();
            let mut on_stack = HashSet::new();

            fn dfs(
                current: NodeIndex,
                graph: &mut Graph<(), ()>,
                visited: &mut HashSet<NodeIndex>,
                path: &mut Vec<NodeIndex>,
                on_stack: &mut HashSet<NodeIndex>,
            ) {
                visited.insert(current);
                on_stack.insert(current);
                path.push(current);

                for neighbor in graph
                    .neighbors_directed(current, Direction::Outgoing)
                    .collect::<Vec<_>>()
                {
                    if !visited.contains(&neighbor) {
                        dfs(neighbor, graph, visited, path, on_stack);
                    } else if on_stack.contains(&neighbor) {
                        // Found a cycle, remove the last edge
                        if let Some(&last) = path.last() {
                            if let Some(edge) = graph.find_edge(last, neighbor) {
                                graph.remove_edge(edge);
                            }
                        }
                    }
                }

                path.pop();
                on_stack.remove(&current);
            }

            if !visited.contains(&start_node) {
                dfs(start_node, &mut dag, &mut visited, &mut path, &mut on_stack);
            }
        }

        dag
    }

    /// Assign vertices to layers using longest path algorithm
    fn assign_layers(&self, dag: &Graph<(), ()>) -> Vec<Vec<LayeredNode>> {
        let mut layers = Vec::new();
        let mut node_layers = HashMap::new();

        // First pass: assign minimum layers based on longest path from a root
        let mut queue = VecDeque::new();
        let mut roots: Vec<_> = dag
            .node_indices()
            .filter(|&n| dag.neighbors_directed(n, Direction::Incoming).count() == 0)
            .collect();

        // If no roots found, use nodes with minimal incoming edges
        if roots.is_empty() {
            let min_in_degree = dag
                .node_indices()
                .map(|n| dag.neighbors_directed(n, Direction::Incoming).count())
                .min()
                .unwrap_or(0);
            roots.extend(dag.node_indices().filter(|&n| {
                dag.neighbors_directed(n, Direction::Incoming).count() == min_in_degree
            }));
        }

        // Initialize roots to layer 0
        for &root in &roots {
            queue.push_back(root);
            node_layers.insert(root, 0);
        }

        // BFS to assign layers
        while let Some(node) = queue.pop_front() {
            let current_layer = *node_layers.get(&node).unwrap();

            for neighbor in dag.neighbors_directed(node, Direction::Outgoing) {
                let next_layer = current_layer + 1;
                match node_layers.get(&neighbor) {
                    Some(&existing_layer) if existing_layer <= next_layer => continue,
                    _ => {
                        node_layers.insert(neighbor, next_layer);
                        queue.push_back(neighbor);
                    }
                }
            }
        }

        // Handle any remaining unassigned nodes (in case of disconnected components)
        for node in dag.node_indices() {
            if !node_layers.contains_key(&node) {
                node_layers.insert(node, 0);
            }
        }

        // Group nodes by layer
        let max_layer = *node_layers.values().max().unwrap_or(&0) + 1;
        layers.resize(max_layer, Vec::new());

        for (node, &layer) in &node_layers {
            layers[layer].push(LayeredNode::new(*node, layer, false));
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

    /// Add dummy nodes for edges that span multiple layers
    fn expand_long_edges(&self, dag: &mut Graph<(), ()>, layers: &mut Vec<Vec<LayeredNode>>) {
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

    /// Count crossings between two adjacent layers
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

                        if let (Some(p1), Some(p2)) = (pos1, pos2) {
                            if (i1 < i2) != (p1 < p2) {
                                crossings += 1;
                            }
                        }
                    }
                }
            }
        }

        crossings
    }

    /// Reduce edge crossings between layers
    fn reduce_crossings(&self, layers: &mut Vec<Vec<LayeredNode>>, dag: &Graph<(), ()>) {
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
        layers: &Vec<Vec<LayeredNode>>,
    ) -> HashMap<NodeIndex, (f32, f32)> {
        let mut coordinates = HashMap::new();
        let max_width = layers.iter().map(|layer| layer.len()).max().unwrap_or(1);

        for (layer_idx, layer) in layers.iter().enumerate() {
            let y = layer_idx as f32 * self.config.layer_spacing;
            let layer_width = (layer.len() - 1) as f32 * self.config.node_spacing;
            let offset = ((max_width - 1) as f32 * self.config.node_spacing - layer_width) / 2.0;

            for (node_idx, node) in layer.iter().enumerate() {
                let x = offset + node_idx as f32 * self.config.node_spacing;
                coordinates.insert(node.id, (x, y));
            }
        }

        // Fine-tune positions by averaging connected nodes' x coordinates
        let mut adjusted_coords = coordinates.clone();
        for _ in 0..2 {
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

        coordinates
    }
}

impl Layout for SugiyamaLayout {
    fn layout(&self, graph: &Graph<(), ()>) -> HashMap<NodeIndex, (f32, f32)> {
        if graph.node_count() == 0 {
            return HashMap::new();
        }

        // Step 1: Make the graph acyclic
        let mut dag = self.make_dag(graph);

        // Step 2: Assign vertices to layers
        let mut layers = self.assign_layers(&dag);

        // Step 3: Add dummy nodes for long edges
        self.expand_long_edges(&mut dag, &mut layers);

        // Step 4: Reduce edge crossings
        self.reduce_crossings(&mut layers, &dag);

        // Step 5: Assign coordinates
        self.assign_coordinates(&dag, &layers)
    }
}
