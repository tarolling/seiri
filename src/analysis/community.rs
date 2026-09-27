use petgraph::graph::Graph;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::hash::Hash;
use std::path::Path;

/// Minimum modularity gain (in edge-weight units) for Louvain to move a node.
const GAIN_EPSILON: f64 = 1e-12;

/// Undirected, weighted view of a dependency graph.
struct WeightedGraph {
    /// Neighbors of each node (excluding itself) with edge weights, sorted by neighbor index.
    adj: Vec<Vec<(usize, f64)>>,
    /// Self-loop weight of each node, counted once.
    loops: Vec<f64>,
    /// Weighted degree of each node; a self-loop counts twice.
    degree: Vec<f64>,
    /// Total edge weight `m`.
    total: f64,
}

impl WeightedGraph {
    /// Folds directed edges into undirected weights; self-loops are dropped.
    fn from_graph(graph: &Graph<(), ()>) -> Self {
        let mut edges = BTreeMap::new();
        for edge in graph.raw_edges() {
            let (a, b) = (edge.source().index(), edge.target().index());
            if a != b {
                *edges.entry((a.min(b), a.max(b))).or_insert(0.0) += 1.0;
            }
        }
        Self::from_edges(vec![0.0; graph.node_count()], &edges)
    }

    /// Builds the graph from per-node self-loop weights and `(low, high)` keyed edge weights.
    fn from_edges(loops: Vec<f64>, edges: &BTreeMap<(usize, usize), f64>) -> Self {
        let n = loops.len();
        let mut adj = vec![Vec::new(); n];
        let mut degree: Vec<f64> = loops.iter().map(|w| 2.0 * w).collect();
        let mut total: f64 = loops.iter().sum();
        // iterating keys in order leaves every adjacency list sorted by neighbor index
        for (&(a, b), &w) in edges {
            adj[a].push((b, w));
            adj[b].push((a, w));
            degree[a] += w;
            degree[b] += w;
            total += w;
        }
        Self {
            adj,
            loops,
            degree,
            total,
        }
    }

    fn node_count(&self) -> usize {
        self.adj.len()
    }

    fn modularity(&self, partition: &[usize]) -> f64 {
        if self.total == 0.0 {
            return 0.0;
        }

        // community -> (internal weight, total degree)
        let mut stats: BTreeMap<usize, (f64, f64)> = BTreeMap::new();
        for node in 0..self.node_count() {
            let community = partition[node];
            let entry = stats.entry(community).or_insert((0.0, 0.0));
            entry.0 += self.loops[node];
            entry.1 += self.degree[node];
            for &(neighbor, w) in &self.adj[node] {
                if neighbor > node && partition[neighbor] == community {
                    entry.0 += w;
                }
            }
        }

        let m = self.total;
        stats
            .values()
            .map(|&(internal, degree)| internal / m - (degree / (2.0 * m)).powi(2))
            .sum()
    }

    /// Greedily moves nodes between neighboring communities until no move improves modularity.
    /// Returns each node's community and whether any node moved.
    fn local_moving(&self) -> (Vec<usize>, bool) {
        let n = self.node_count();
        let two_m = 2.0 * self.total;
        let mut community: Vec<usize> = (0..n).collect();
        let mut community_degree = self.degree.clone();
        // scratch space: weight from the current node into each community it touches
        let mut weight_to = vec![0.0; n];
        let mut touched = Vec::new();
        let mut moved_any = false;

        loop {
            let mut moved = false;
            for node in 0..n {
                let current = community[node];
                let k = self.degree[node];

                for &(neighbor, w) in &self.adj[node] {
                    let c = community[neighbor];
                    if weight_to[c] == 0.0 {
                        touched.push(c);
                    }
                    weight_to[c] += w;
                }

                community_degree[current] -= k;
                let gain = |c: usize| weight_to[c] - community_degree[c] * k / two_m;

                // Staying put wins ties so the pass terminates.
                let mut best = current;
                let mut best_gain = gain(current);
                for &c in &touched {
                    let g = gain(c);
                    if g > best_gain + GAIN_EPSILON {
                        best = c;
                        best_gain = g;
                    }
                }

                community_degree[best] += k;
                if best != current {
                    community[node] = best;
                    moved = true;
                }

                for &c in &touched {
                    weight_to[c] = 0.0;
                }
                touched.clear();
            }

            if !moved {
                break;
            }
            moved_any = true;
        }

        (community, moved_any)
    }

    /// Collapses each community (ids must be contiguous) into a single node.
    fn aggregate(&self, communities: &[usize]) -> Self {
        let count = communities.iter().max().map_or(0, |&max| max + 1);
        let mut loops = vec![0.0; count];
        let mut edges = BTreeMap::new();

        for node in 0..self.node_count() {
            let c = communities[node];
            loops[c] += self.loops[node];
            for &(neighbor, w) in &self.adj[node] {
                if neighbor < node {
                    continue;
                }
                let d = communities[neighbor];
                if c == d {
                    loops[c] += w;
                } else {
                    *edges.entry((c.min(d), c.max(d))).or_insert(0.0) += w;
                }
            }
        }

        Self::from_edges(loops, &edges)
    }
}

/// Relabels ids to `0..k` in order of first appearance.
fn renumber<T: Copy + Eq + Hash>(ids: impl IntoIterator<Item = T>) -> Vec<usize> {
    let mut seen = HashMap::new();
    ids.into_iter()
        .map(|id| {
            let next = seen.len();
            *seen.entry(id).or_insert(next)
        })
        .collect()
}

/// Newman–Girvan modularity of `partition` over the undirected view of `graph`.
/// `partition[i]` is the community of the node with index `i`. See:
/// https://en.wikipedia.org/wiki/Girvan%E2%80%93Newman_algorithm.
pub fn modularity(graph: &Graph<(), ()>, partition: &[usize]) -> f64 {
    debug_assert_eq!(partition.len(), graph.node_count());
    WeightedGraph::from_graph(graph).modularity(partition)
}

/// Detects communities with the Louvain method, returning a community id per node index.
pub fn louvain(graph: &Graph<(), ()>) -> Vec<usize> {
    let mut level = WeightedGraph::from_graph(graph);
    // Maps each original node to its node in the current aggregated level.
    let mut membership: Vec<usize> = (0..graph.node_count()).collect();

    if level.total == 0.0 {
        return membership;
    }

    loop {
        let (communities, moved) = level.local_moving();
        if !moved {
            break;
        }
        let communities = renumber(communities);
        for member in &mut membership {
            *member = communities[*member];
        }
        level = level.aggregate(&communities);
    }

    renumber(membership)
}

/// Assigns one community per distinct parent directory, in first-seen order.
/// A module file with a sibling directory of the same name (`foo.rs` and `foo/`) joins that directory.
pub fn partition_by_parent_dir<'a>(paths: impl IntoIterator<Item = &'a Path>) -> Vec<usize> {
    let paths: Vec<&Path> = paths.into_iter().collect();
    let directories: HashSet<&Path> = paths.iter().filter_map(|path| path.parent()).collect();

    renumber(paths.iter().map(|path| {
        // A module file like `src/foo.rs` belongs with its children in `src/foo/`.
        let module_dir = path.with_extension("");
        match directories.get(module_dir.as_path()) {
            Some(&dir) => dir,
            None => path.parent().unwrap_or(Path::new("")),
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use petgraph::graph::NodeIndex;
    use std::path::PathBuf;

    const EPS: f64 = 1e-10;

    fn create_test_graph(node_count: usize, edges: &[(usize, usize)]) -> Graph<(), ()> {
        let mut graph = Graph::new();
        for _ in 0..node_count {
            graph.add_node(());
        }
        for &(from, to) in edges {
            graph.add_edge(NodeIndex::new(from), NodeIndex::new(to), ());
        }
        graph
    }

    /// Two triangles {0,1,2} and {3,4,5} joined by the edge 2 -> 3.
    fn two_triangles() -> Graph<(), ()> {
        create_test_graph(6, &[(0, 1), (1, 2), (2, 0), (3, 4), (4, 5), (5, 3), (2, 3)])
    }

    fn clique_edges(nodes: &[usize]) -> Vec<(usize, usize)> {
        let mut edges = Vec::new();
        for (i, &a) in nodes.iter().enumerate() {
            for &b in &nodes[i + 1..] {
                edges.push((a, b));
            }
        }
        edges
    }

    fn same_community(partition: &[usize], nodes: &[usize]) -> bool {
        nodes.iter().all(|&n| partition[n] == partition[nodes[0]])
    }

    fn assert_contiguous(partition: &[usize]) {
        let distinct: HashSet<_> = partition.iter().copied().collect();
        for id in 0..distinct.len() {
            assert!(distinct.contains(&id), "missing community id {id}");
        }
    }

    // --- modularity ---

    #[test]
    fn modularity_of_two_triangles_split_by_triangle_is_five_fourteenths() {
        let q = modularity(&two_triangles(), &[0, 0, 0, 1, 1, 1]);
        assert!((q - 5.0 / 14.0).abs() < EPS, "got {q}");
    }

    #[test]
    fn modularity_of_single_community_is_zero() {
        let q = modularity(&two_triangles(), &[0; 6]);
        assert!(q.abs() < EPS, "got {q}");
    }

    #[test]
    fn modularity_of_singletons_is_negative_sum_of_squared_degree_shares() {
        // Degrees: 2,2,3,3,2,2; 2m = 14.
        let expected = -[2.0, 2.0, 3.0, 3.0, 2.0, 2.0]
            .iter()
            .map(|k: &f64| (k / 14.0).powi(2))
            .sum::<f64>();
        let q = modularity(&two_triangles(), &[0, 1, 2, 3, 4, 5]);
        assert!((q - expected).abs() < EPS, "got {q}, expected {expected}");
    }

    #[test]
    fn modularity_without_edges_is_zero() {
        let graph = create_test_graph(3, &[]);
        assert_eq!(modularity(&graph, &[0, 1, 2]), 0.0);
        assert_eq!(modularity(&Graph::new(), &[]), 0.0);
    }

    #[test]
    fn modularity_treats_reciprocal_edges_like_parallel_edges() {
        let reciprocal = create_test_graph(3, &[(0, 1), (1, 0), (1, 2)]);
        let parallel = create_test_graph(3, &[(0, 1), (0, 1), (1, 2)]);
        let partition = [0, 0, 1];
        assert!(
            (modularity(&reciprocal, &partition) - modularity(&parallel, &partition)).abs() < EPS
        );
    }

    #[test]
    fn modularity_ignores_self_loops() {
        let with_loop = create_test_graph(
            6,
            &[
                (0, 1),
                (1, 2),
                (2, 0),
                (3, 4),
                (4, 5),
                (5, 3),
                (2, 3),
                (0, 0),
            ],
        );
        let partition = [0, 0, 0, 1, 1, 1];
        assert!(
            (modularity(&with_loop, &partition) - modularity(&two_triangles(), &partition)).abs()
                < EPS
        );
    }

    // --- louvain ---

    #[test]
    fn louvain_splits_two_triangles_joined_by_bridge() {
        let partition = louvain(&two_triangles());
        assert!(same_community(&partition, &[0, 1, 2]));
        assert!(same_community(&partition, &[3, 4, 5]));
        assert_ne!(partition[0], partition[3]);
    }

    #[test]
    fn louvain_finds_each_clique_in_ring_of_cliques() {
        let cliques = 6;
        let size = 4;
        let mut edges = Vec::new();
        for c in 0..cliques {
            let nodes: Vec<usize> = (c * size..(c + 1) * size).collect();
            edges.extend(clique_edges(&nodes));
            // Link the last node of this clique to the first node of the next.
            edges.push(((c + 1) * size - 1, ((c + 1) % cliques) * size));
        }
        let graph = create_test_graph(cliques * size, &edges);

        let partition = louvain(&graph);

        let distinct: HashSet<_> = partition.iter().collect();
        assert_eq!(distinct.len(), cliques);
        for c in 0..cliques {
            let nodes: Vec<usize> = (c * size..(c + 1) * size).collect();
            assert!(same_community(&partition, &nodes), "clique {c} was split");
        }
    }

    #[test]
    fn louvain_separates_disconnected_cliques() {
        let mut edges = clique_edges(&[0, 1, 2, 3]);
        edges.extend(clique_edges(&[4, 5, 6, 7]));
        let partition = louvain(&create_test_graph(8, &edges));

        assert!(same_community(&partition, &[0, 1, 2, 3]));
        assert!(same_community(&partition, &[4, 5, 6, 7]));
        assert_ne!(partition[0], partition[4]);
    }

    #[test]
    fn louvain_keeps_isolated_nodes_apart() {
        let partition = louvain(&create_test_graph(3, &[]));
        assert_eq!(partition, vec![0, 1, 2]);
    }

    #[test]
    fn louvain_of_empty_graph_is_empty() {
        assert!(louvain(&Graph::new()).is_empty());
    }

    #[test]
    fn louvain_is_deterministic_with_contiguous_ids() {
        let graph = two_triangles();
        let first = louvain(&graph);
        assert_eq!(first, louvain(&graph));
        assert_contiguous(&first);
        assert_eq!(first[0], 0, "ids follow first appearance by node index");
    }

    #[test]
    fn louvain_improves_on_singleton_partition() {
        let graph = create_test_graph(
            7,
            &[
                (0, 1),
                (1, 2),
                (2, 3),
                (3, 0),
                (3, 4),
                (4, 5),
                (5, 6),
                (6, 4),
                (1, 5),
            ],
        );
        let singletons: Vec<usize> = (0..7).collect();
        assert!(modularity(&graph, &louvain(&graph)) >= modularity(&graph, &singletons));
    }

    #[test]
    fn aggregation_preserves_modularity_of_collapsed_partition() {
        let graph = create_test_graph(
            7,
            &[
                (0, 1),
                (1, 2),
                (2, 3),
                (3, 0),
                (3, 4),
                (4, 5),
                (5, 6),
                (6, 4),
                (1, 5),
                (1, 0),
            ],
        );
        let weighted = WeightedGraph::from_graph(&graph);
        let partition = [0, 0, 1, 1, 2, 2, 2];

        let aggregated = weighted.aggregate(&partition);

        assert_eq!(aggregated.node_count(), 3);
        assert!((aggregated.total - weighted.total).abs() < EPS);
        let collapsed_q = aggregated.modularity(&[0, 1, 2]);
        assert!((collapsed_q - weighted.modularity(&partition)).abs() < EPS);
    }

    // --- partition_by_parent_dir ---

    #[test]
    fn partition_by_parent_dir_groups_files_sharing_a_directory() {
        let paths: Vec<PathBuf> = [
            "src/a.rs",
            "src/core/b.rs",
            "src/c.rs",
            "main.rs",
            "src/core/d.rs",
        ]
        .iter()
        .map(PathBuf::from)
        .collect();

        let partition = partition_by_parent_dir(paths.iter().map(PathBuf::as_path));

        assert_eq!(partition, vec![0, 1, 0, 2, 1]);
    }

    #[test]
    fn partition_by_parent_dir_groups_module_file_with_its_directory() {
        let paths: Vec<PathBuf> = [
            "src/parsers.rs",
            "src/main.rs",
            "src/parsers/rust.rs",
            "src/core/resolvers.rs",
            "src/core/resolvers/cpp.rs",
            "src/core/defs.rs",
            "src/layout.rs",
        ]
        .iter()
        .map(PathBuf::from)
        .collect();

        let partition = partition_by_parent_dir(paths.iter().map(PathBuf::as_path));

        // parsers.rs joins src/parsers/, resolvers.rs joins src/core/resolvers/;
        // layout.rs has no sibling directory among the paths, so it stays in src/.
        assert_eq!(partition, vec![0, 1, 0, 2, 2, 3, 1]);
    }

    #[test]
    fn partition_by_parent_dir_of_no_paths_is_empty() {
        assert!(partition_by_parent_dir(std::iter::empty()).is_empty());
    }
}
