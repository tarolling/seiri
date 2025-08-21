use petgraph::{
    graph::{Graph, NodeIndex},
    visit::EdgeRef,
};
use std::collections::{HashMap, HashSet};

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
}

impl GraphAnalysis {
    /// Perform Kosaraju's algorithm to find strongly connected components
    pub fn analyze_graph(graph: &Graph<(), ()>) -> Self {
        let mut analysis = Self {
            scc_sizes: Vec::new(),
            node_to_scc: HashMap::new(),
            largest_scc_size: 0,
            largest_scc_nodes: HashSet::new(),
            sccs_by_size: HashMap::new(),
        };

        if graph.node_count() == 0 {
            return analysis;
        }

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
    pub fn get_scc_size(&self, node: NodeIndex) -> Option<usize> {
        self.node_to_scc.get(&node).map(|&idx| self.scc_sizes[idx])
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
}
