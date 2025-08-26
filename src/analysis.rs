use petgraph::{
    graph::{Graph, NodeIndex},
    visit::EdgeRef,
};
use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Debug)]
pub struct GraphAnalysis {
    /// Size of each strongly connected component
    pub scc_sizes: Vec<usize>,
    /// Mapping of nodes to their SCC index
    pub node_to_scc: HashMap<NodeIndex, usize>,
    /// Size of the largest SCC
    pub largest_scc_size: usize,
    /// Nodes in the largest SCC
    pub largest_scc_nodes: HashSet<NodeIndex>,
    /// All SCCs grouped by size (size -> set of SCCs)
    /// Each SCC is a set of node indices
    pub sccs_by_size: HashMap<usize, Vec<HashSet<NodeIndex>>>,
    /// Betweenness centrality scores for each node
    /// Higher values indicate nodes that appear on more shortest paths
    pub betweenness_centrality: HashMap<NodeIndex, f64>,
}

impl GraphAnalysis {
    /// Calculate betweenness centrality for a single source node
    fn calculate_betweenness_from_source(
        graph: &Graph<(), ()>,
        source: NodeIndex,
        centrality: &mut HashMap<NodeIndex, f64>,
    ) {
        let mut stack = Vec::new();
        let mut queue = VecDeque::new();
        let mut sigma = HashMap::new();
        let mut distance = HashMap::new();
        let mut pred: HashMap<NodeIndex, Vec<NodeIndex>> = HashMap::new();
        let mut delta = HashMap::new();

        // Initialize
        for node in graph.node_indices() {
            sigma.insert(node, 0.0);
            distance.insert(node, -1);
            pred.insert(node, Vec::new());
            delta.insert(node, 0.0);
        }

        sigma.insert(source, 1.0);
        distance.insert(source, 0);
        queue.push_back(source);

        // BFS phase - find shortest paths
        while let Some(v) = queue.pop_front() {
            stack.push(v);
            let v_dist = distance[&v];
            let v_sigma = sigma[&v];

            for neighbor in graph.neighbors(v) {
                // First time we found this node?
                if distance[&neighbor] < 0 {
                    queue.push_back(neighbor);
                    distance.insert(neighbor, v_dist + 1);
                }

                // Shortest path to neighbor via v?
                if distance[&neighbor] == v_dist + 1 {
                    *sigma.get_mut(&neighbor).unwrap() += v_sigma;
                    pred.get_mut(&neighbor).unwrap().push(v);
                }
            }
        }

        // Dependency accumulation phase - calculate contributions
        while let Some(w) = stack.pop() {
            for &v in &pred[&w] {
                let contribution = (sigma[&v] / sigma[&w]) * (1.0 + delta[&w]);
                *delta.get_mut(&v).unwrap() += contribution;
            }

            if w != source {
                *centrality.get_mut(&w).unwrap() += delta[&w];
            }
        }
    }

    /// Calculate betweenness centrality for all nodes
    fn calculate_betweenness_centrality(graph: &Graph<(), ()>) -> HashMap<NodeIndex, f64> {
        let mut centrality: HashMap<NodeIndex, f64> =
            graph.node_indices().map(|n| (n, 0.0)).collect();

        // Calculate betweenness from each source node
        for source in graph.node_indices() {
            Self::calculate_betweenness_from_source(graph, source, &mut centrality);
        }

        // Normalize for undirected graphs
        if graph.node_count() > 2 {
            let norm = 1.0 / ((graph.node_count() - 1) * (graph.node_count() - 2)) as f64;
            for score in centrality.values_mut() {
                *score *= norm;
            }
        }

        centrality
    }

    /// Analyze the graph to find both SCCs and betweenness centrality
    pub fn analyze_graph(graph: &Graph<(), ()>) -> Self {
        let mut analysis = Self {
            scc_sizes: Vec::new(),
            node_to_scc: HashMap::new(),
            largest_scc_size: 0,
            largest_scc_nodes: HashSet::new(),
            sccs_by_size: HashMap::new(),
            betweenness_centrality: HashMap::new(),
        };

        if graph.node_count() == 0 {
            return analysis;
        }

        // Calculate betweenness centrality
        analysis.betweenness_centrality = Self::calculate_betweenness_centrality(graph);

        // Step 1: First DFS to get finishing times
        let mut visited = HashSet::new();
        let mut finish_order = Vec::new();

        for node in graph.node_indices() {
            if !visited.contains(&node) {
                Self::dfs_first_pass(graph, node, &mut visited, &mut finish_order);
            }
        }

        // Step 2: Create transposed graph
        let mut transposed = Graph::new();
        let mut node_map = HashMap::new();

        // Add all nodes
        for node in graph.node_indices() {
            node_map.insert(node, transposed.add_node(()));
        }

        // Add reversed edges
        for edge in graph.edge_references() {
            transposed.add_edge(node_map[&edge.target()], node_map[&edge.source()], ());
        }

        // Step 3: Second DFS to find SCCs
        visited.clear();
        let mut current_scc = HashSet::new();

        for &node in finish_order.iter().rev() {
            let transposed_node = node_map[&node];
            if !visited.contains(&transposed_node) {
                current_scc.clear();
                Self::dfs_second_pass(&transposed, transposed_node, &mut visited, &mut current_scc);

                // Map back to original nodes
                let original_scc: HashSet<_> = current_scc
                    .iter()
                    .map(|&n| {
                        node_map
                            .iter()
                            .find(|&(_, &v)| v == n)
                            .map(|(&k, _)| k)
                            .unwrap()
                    })
                    .collect();

                // Update largest SCC if this one is bigger
                if original_scc.len() > analysis.largest_scc_size {
                    analysis.largest_scc_size = original_scc.len();
                    analysis.largest_scc_nodes = original_scc.clone();
                }

                // Record SCC size and node mappings
                let scc_index = analysis.scc_sizes.len();
                let scc_size = original_scc.len();
                analysis.scc_sizes.push(scc_size);

                // Add to SCCs by size
                analysis
                    .sccs_by_size
                    .entry(scc_size)
                    .or_default()
                    .push(original_scc.clone());

                for node in original_scc {
                    analysis.node_to_scc.insert(node, scc_index);
                }
            }
        }

        analysis
    }

    fn dfs_first_pass(
        graph: &Graph<(), ()>,
        start: NodeIndex,
        visited: &mut HashSet<NodeIndex>,
        finish_order: &mut Vec<NodeIndex>,
    ) {
        visited.insert(start);

        for neighbor in graph.neighbors(start) {
            if !visited.contains(&neighbor) {
                Self::dfs_first_pass(graph, neighbor, visited, finish_order);
            }
        }

        finish_order.push(start);
    }

    fn dfs_second_pass(
        graph: &Graph<(), ()>,
        start: NodeIndex,
        visited: &mut HashSet<NodeIndex>,
        component: &mut HashSet<NodeIndex>,
    ) {
        visited.insert(start);
        component.insert(start);

        for neighbor in graph.neighbors(start) {
            if !visited.contains(&neighbor) {
                Self::dfs_second_pass(graph, neighbor, visited, component);
            }
        }
    }

    /// Returns whether a node is part of the largest SCC
    pub fn is_in_largest_scc(&self, node: NodeIndex) -> bool {
        self.largest_scc_nodes.contains(&node)
    }

    /// Returns the size of the SCC containing the given node
    #[allow(dead_code)]
    pub fn get_scc_size(&self, node: NodeIndex) -> Option<usize> {
        self.node_to_scc.get(&node).map(|&idx| self.scc_sizes[idx])
    }

    /// Get the betweenness centrality score for a node
    #[allow(dead_code)]
    pub fn get_betweenness_centrality(&self, node: NodeIndex) -> Option<f64> {
        self.betweenness_centrality.get(&node).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use petgraph::Graph;

    fn create_test_graph(edges: &[(usize, usize)]) -> Graph<(), ()> {
        let mut graph = Graph::new();
        // Add enough nodes for all edges
        let max_node = edges.iter().flat_map(|&(a, b)| [a, b]).max().unwrap_or(0);
        for _ in 0..=max_node {
            graph.add_node(());
        }
        // Add edges
        for &(from, to) in edges {
            graph.add_edge(NodeIndex::new(from), NodeIndex::new(to), ());
        }
        graph
    }

    #[test]
    fn test_no_cycles() {
        // Create a simple DAG: 0 -> 1 -> 2
        let graph = create_test_graph(&[(0, 1), (1, 2)]);
        let analysis = GraphAnalysis::analyze_graph(&graph);

        assert_eq!(analysis.largest_scc_size, 1);
        assert_eq!(analysis.scc_sizes.len(), 3);
        assert!(analysis.sccs_by_size.get(&1).unwrap().len() == 3); // Three single-node SCCs
    }

    #[test]
    fn test_simple_cycle() {
        // Create a cycle: 0 -> 1 -> 2 -> 0
        let graph = create_test_graph(&[(0, 1), (1, 2), (2, 0)]);
        let analysis = GraphAnalysis::analyze_graph(&graph);

        assert_eq!(analysis.largest_scc_size, 3);
        assert_eq!(analysis.scc_sizes.len(), 1);

        // Check if all nodes are in the same SCC
        let largest_scc = &analysis.sccs_by_size.get(&3).unwrap()[0];
        assert!(largest_scc.contains(&NodeIndex::new(0)));
        assert!(largest_scc.contains(&NodeIndex::new(1)));
        assert!(largest_scc.contains(&NodeIndex::new(2)));
    }

    #[test]
    fn test_multiple_cycles() {
        // Create two cycles: (0 -> 1 -> 0) and (2 -> 3 -> 4 -> 2)
        let graph = create_test_graph(&[
            (0, 1),
            (1, 0), // First cycle
            (2, 3),
            (3, 4),
            (4, 2), // Second cycle
        ]);
        let analysis = GraphAnalysis::analyze_graph(&graph);

        assert_eq!(analysis.largest_scc_size, 3);
        assert_eq!(analysis.scc_sizes.len(), 2);

        // Check SCCs by size
        assert!(analysis.sccs_by_size.get(&2).unwrap().len() == 1); // One 2-node SCC
        assert!(analysis.sccs_by_size.get(&3).unwrap().len() == 1); // One 3-node SCC
    }

    #[test]
    fn test_nested_cycles() {
        // Create nested cycles:
        // Inner cycle: 1 -> 2 -> 1
        // Outer cycle: 0 -> 1 -> 2 -> 3 -> 0
        let graph = create_test_graph(&[
            (1, 2),
            (2, 1), // Inner cycle
            (0, 1),
            (3, 0), // Complete outer cycle
            (2, 3),
        ]);
        let analysis = GraphAnalysis::analyze_graph(&graph);

        assert_eq!(analysis.largest_scc_size, 4);
        assert_eq!(analysis.scc_sizes.len(), 1);

        let largest_scc = &analysis.sccs_by_size.get(&4).unwrap()[0];
        for i in 0..4 {
            assert!(largest_scc.contains(&NodeIndex::new(i)));
        }
    }

    #[test]
    fn test_disconnected_components() {
        // Create two disconnected subgraphs:
        // Component 1: 0 -> 1 -> 0
        // Component 2: 2 -> 3
        let graph = create_test_graph(&[
            (0, 1),
            (1, 0), // First component (cycle)
            (2, 3), // Second component (linear)
        ]);
        let analysis = GraphAnalysis::analyze_graph(&graph);

        assert_eq!(analysis.largest_scc_size, 2);
        assert_eq!(analysis.scc_sizes.len(), 3); // One 2-node SCC and two 1-node SCCs

        // Check SCCs by size
        assert!(analysis.sccs_by_size.get(&2).unwrap().len() == 1); // One 2-node SCC
        assert!(analysis.sccs_by_size.get(&1).unwrap().len() == 2); // Two 1-node SCCs
    }

    #[test]
    fn test_empty_graph() {
        let graph = Graph::<(), ()>::new();
        let analysis = GraphAnalysis::analyze_graph(&graph);

        assert_eq!(analysis.largest_scc_size, 0);
        assert_eq!(analysis.scc_sizes.len(), 0);
        assert!(analysis.sccs_by_size.is_empty());
    }

    #[test]
    fn test_single_node() {
        let mut graph = Graph::new();
        graph.add_node(());
        let analysis = GraphAnalysis::analyze_graph(&graph);

        assert_eq!(analysis.largest_scc_size, 1);
        assert_eq!(analysis.scc_sizes.len(), 1);
        assert!(analysis.sccs_by_size.get(&1).unwrap().len() == 1);
    }

    #[test]
    fn test_self_cycle() {
        let graph = create_test_graph(&[(0, 0)]);
        let analysis = GraphAnalysis::analyze_graph(&graph);

        assert_eq!(analysis.largest_scc_size, 1);
        let scc = &analysis.sccs_by_size.get(&1).unwrap()[0];
        assert!(scc.contains(&NodeIndex::new(0)));
    }

    #[test]
    fn test_complex_graph() {
        // Create a more complex graph with multiple SCCs of different sizes
        let graph = create_test_graph(&[
            (0, 1),
            (1, 2),
            (2, 0), // 3-node cycle
            (3, 4),
            (4, 3), // 2-node cycle
            (5, 6), // Linear component
            (7, 8),
            (8, 9),
            (9, 7), // Another 3-node cycle
            (2, 3), // Connection between components
            (6, 7), // Another connection
        ]);
        let analysis = GraphAnalysis::analyze_graph(&graph);

        // Verify SCC sizes
        assert_eq!(analysis.largest_scc_size, 3);
        assert!(analysis.sccs_by_size.get(&3).unwrap().len() == 2); // Two 3-node SCCs
        assert!(analysis.sccs_by_size.get(&2).unwrap().len() == 1); // One 2-node SCC
        assert!(analysis.sccs_by_size.get(&1).unwrap().len() == 2); // Two 1-node SCCs
    }

    #[test]
    fn test_empty_graph_betweenness() {
        let graph = Graph::<(), ()>::new();
        let analysis = GraphAnalysis::analyze_graph(&graph);
        assert!(analysis.betweenness_centrality.is_empty());
    }

    #[test]
    fn test_single_node_betweenness() {
        let mut graph = Graph::new();
        let n0 = graph.add_node(());
        let analysis = GraphAnalysis::analyze_graph(&graph);
        assert_eq!(analysis.get_betweenness_centrality(n0), Some(0.0));
    }

    #[test]
    fn test_path_graph_betweenness() {
        // Create a path: 0 -> 1 -> 2
        let graph = create_test_graph(&[(0, 1), (1, 2)]);
        let analysis = GraphAnalysis::analyze_graph(&graph);

        // Middle node should have highest betweenness
        assert!(
            analysis
                .get_betweenness_centrality(NodeIndex::new(1))
                .unwrap()
                > analysis
                    .get_betweenness_centrality(NodeIndex::new(0))
                    .unwrap()
        );
        assert!(
            analysis
                .get_betweenness_centrality(NodeIndex::new(1))
                .unwrap()
                > analysis
                    .get_betweenness_centrality(NodeIndex::new(2))
                    .unwrap()
        );
    }

    #[test]
    fn test_star_graph_betweenness() {
        // Create a star: center (0) connected to three leaves (1,2,3)
        let graph = create_test_graph(&[
            (0, 1),
            (1, 0), // Bidirectional edge
            (0, 2),
            (2, 0), // Bidirectional edge
            (0, 3),
            (3, 0), // Bidirectional edge
        ]);
        let analysis = GraphAnalysis::analyze_graph(&graph);

        // Center should have highest betweenness
        let center_score = analysis
            .get_betweenness_centrality(NodeIndex::new(0))
            .unwrap();
        println!("Betweenness centrality of center node: {}", center_score);
        for i in 1..4 {
            println!(
                "Betweenness centrality of node {}: {}",
                i,
                analysis
                    .get_betweenness_centrality(NodeIndex::new(i))
                    .unwrap()
            );
            assert!(
                center_score
                    > analysis
                        .get_betweenness_centrality(NodeIndex::new(i))
                        .unwrap(),
            );
        }
    }

    #[test]
    fn test_cycle_betweenness() {
        // Create a cycle: 0 -> 1 -> 2 -> 0
        let graph = create_test_graph(&[(0, 1), (1, 2), (2, 0)]);
        let analysis = GraphAnalysis::analyze_graph(&graph);

        // In a cycle, all nodes should have equal betweenness
        let score = analysis
            .get_betweenness_centrality(NodeIndex::new(0))
            .unwrap();
        assert!(
            (analysis
                .get_betweenness_centrality(NodeIndex::new(1))
                .unwrap()
                - score)
                .abs()
                < 1e-10
        );
        assert!(
            (analysis
                .get_betweenness_centrality(NodeIndex::new(2))
                .unwrap()
                - score)
                .abs()
                < 1e-10
        );
    }

    #[test]
    fn test_bridge_node_betweenness() {
        // Create two triangles connected by a bridge node:
        // 0 -> 1 -> 2 -> 0 (first triangle)
        // 3 -> 4 -> 5 -> 3 (second triangle)
        // 2 -> 3 (bridge)
        let graph = create_test_graph(&[
            (0, 1),
            (1, 2),
            (2, 0), // First triangle
            (3, 4),
            (4, 5),
            (5, 3), // Second triangle
            (2, 3), // Bridge
        ]);
        let analysis = GraphAnalysis::analyze_graph(&graph);

        // Nodes 2 and 3 (the bridge nodes) should have higher betweenness
        let bridge_score1 = analysis
            .get_betweenness_centrality(NodeIndex::new(2))
            .unwrap();
        let bridge_score2 = analysis
            .get_betweenness_centrality(NodeIndex::new(3))
            .unwrap();
        for i in [0, 1, 4, 5] {
            assert!(
                bridge_score1
                    > analysis
                        .get_betweenness_centrality(NodeIndex::new(i))
                        .unwrap()
            );
            assert!(
                bridge_score2
                    > analysis
                        .get_betweenness_centrality(NodeIndex::new(i))
                        .unwrap()
            );
        }
    }
}
