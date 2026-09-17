//! Force-directed graph layout (Fruchterman-Reingold spring-embedder).
//! See: <https://en.wikipedia.org/wiki/Force-directed_graph_drawing>
//!
//! Complexity: O(n^2) total force evaluations per iteration (all-pairs
//! repulsion, no Barnes-Hut/grid approximation), parallelized across CPU
//! cores via `rayon` to hit the 500-1000+ node performance target. This
//! buys a constant-factor (core-count) speedup, not a better asymptotic
//! class -- if graphs ever need to scale past roughly 5-10k nodes, a
//! Barnes-Hut quadtree would be the next step.

use crate::layout::Layout;
use petgraph::graph::{Graph, NodeIndex};
use rayon::prelude::*;
use std::collections::HashMap;
use std::f32::consts::PI;

/// Configuration options for force-directed layout.
#[derive(Debug, Clone)]
pub struct ForceDirectedConfig {
    /// Number of simulation iterations to run.
    pub iterations: usize,
    /// Ideal spring length (k): the edge length at which attractive and
    /// repulsive forces balance.
    pub ideal_edge_length: f32,
    /// Multiplier on the repulsive force between all node pairs.
    pub repulsion_strength: f32,
    /// Starting value of the cooling "temperature" that caps per-iteration
    /// displacement.
    pub initial_temperature: f32,
    /// Multiplicative decay applied to temperature after each iteration.
    pub cooling_factor: f32,
    /// Minimum distance used in force calculations to avoid division by zero.
    pub min_distance: f32,
    /// Radius of the deterministic circular seed placement.
    pub initial_radius: f32,
}

impl Default for ForceDirectedConfig {
    fn default() -> Self {
        Self {
            iterations: 300,
            ideal_edge_length: 150.0,
            repulsion_strength: 1.0,
            initial_temperature: 150.0,
            cooling_factor: 0.97,
            min_distance: 0.01,
            initial_radius: 100.0,
        }
    }
}

/// Force-directed (spring-embedder) layout implementation.
pub struct ForceDirectedLayout {
    config: ForceDirectedConfig,
}

impl ForceDirectedLayout {
    pub fn new(config: ForceDirectedConfig) -> Self {
        Self { config }
    }
}

/// Direction and (clamped) distance between two points, with a deterministic
/// fallback direction when the points coincide (or are closer than
/// `min_distance`) so forces never divide by zero or produce NaN.
fn direction_and_distance(
    from: (f32, f32),
    to: (f32, f32),
    from_idx: usize,
    to_idx: usize,
    min_distance: f32,
) -> (f32, f32, f32) {
    let dx = from.0 - to.0;
    let dy = from.1 - to.1;
    let dist = (dx * dx + dy * dy).sqrt();
    if dist < min_distance {
        let sign = if from_idx < to_idx { 1.0 } else { -1.0 };
        (sign, 0.0, min_distance)
    } else {
        (dx / dist, dy / dist, dist)
    }
}

/// Repulsive force pushing `from` away from `to` (`k^2 / dist`, scaled by
/// `repulsion_strength`).
fn repulsive_force(
    from: (f32, f32),
    to: (f32, f32),
    from_idx: usize,
    to_idx: usize,
    k: f32,
    config: &ForceDirectedConfig,
) -> (f32, f32) {
    let (ux, uy, dist) = direction_and_distance(from, to, from_idx, to_idx, config.min_distance);
    let force = config.repulsion_strength * k * k / dist;
    (ux * force, uy * force)
}

/// Attractive spring force pulling `from` toward `to` (`dist^2 / k`).
fn attractive_force(
    from: (f32, f32),
    to: (f32, f32),
    from_idx: usize,
    to_idx: usize,
    k: f32,
    config: &ForceDirectedConfig,
) -> (f32, f32) {
    // Direction points from `from` toward `to`, i.e. the reverse of
    // `direction_and_distance`'s from-minus-to convention, since attraction
    // pulls together rather than pushing apart.
    let (ux, uy, dist) = direction_and_distance(to, from, to_idx, from_idx, config.min_distance);
    let force = dist * dist / k;
    (ux * force, uy * force)
}

impl Layout for ForceDirectedLayout {
    fn layout(&self, graph: &Graph<(), ()>) -> HashMap<NodeIndex, (f32, f32)> {
        let n = graph.node_count();
        if n == 0 {
            return HashMap::new();
        }

        let nodes: Vec<NodeIndex> = graph.node_indices().collect();
        if n == 1 {
            return HashMap::from([(nodes[0], (0.0, 0.0))]);
        }

        let index_of: HashMap<NodeIndex, usize> =
            nodes.iter().enumerate().map(|(i, &nx)| (nx, i)).collect();

        let mut neighbors: Vec<Vec<usize>> = vec![Vec::new(); n];
        for edge in graph.edge_indices() {
            if let Some((a, b)) = graph.edge_endpoints(edge) {
                let (a, b) = (index_of[&a], index_of[&b]);
                if a != b {
                    neighbors[a].push(b);
                    neighbors[b].push(a);
                }
            }
        }

        let angle_step = 2.0 * PI / n as f32;
        let mut pos: Vec<(f32, f32)> = (0..n)
            .map(|i| {
                let angle = i as f32 * angle_step;
                (
                    self.config.initial_radius * angle.cos(),
                    self.config.initial_radius * angle.sin(),
                )
            })
            .collect();

        let k = self.config.ideal_edge_length;
        let mut temperature = self.config.initial_temperature;

        for _ in 0..self.config.iterations {
            let disp: Vec<(f32, f32)> = (0..n)
                .into_par_iter()
                .map(|i| {
                    let mut f = (0.0f32, 0.0f32);
                    for j in 0..n {
                        if i == j {
                            continue;
                        }
                        let (fx, fy) = repulsive_force(pos[i], pos[j], i, j, k, &self.config);
                        f.0 += fx;
                        f.1 += fy;
                    }
                    for &j in &neighbors[i] {
                        let (fx, fy) = attractive_force(pos[i], pos[j], i, j, k, &self.config);
                        f.0 += fx;
                        f.1 += fy;
                    }
                    f
                })
                .collect();

            for i in 0..n {
                let (dx, dy) = disp[i];
                let len = (dx * dx + dy * dy).sqrt();
                if len > self.config.min_distance {
                    let capped = len.min(temperature);
                    pos[i].0 += dx / len * capped;
                    pos[i].1 += dy / len * capped;
                }
            }

            temperature *= self.config.cooling_factor;
        }

        nodes.into_iter().zip(pos).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_graph_returns_empty_positions() {
        let layout = ForceDirectedLayout::new(ForceDirectedConfig::default());
        let graph: Graph<(), ()> = Graph::new();

        let positions = layout.layout(&graph);

        assert!(positions.is_empty());
    }

    #[test]
    fn single_node_returns_finite_origin_position() {
        let layout = ForceDirectedLayout::new(ForceDirectedConfig::default());
        let mut graph: Graph<(), ()> = Graph::new();
        let a = graph.add_node(());

        let positions = layout.layout(&graph);

        assert_eq!(positions.len(), 1);
        let (x, y) = positions[&a];
        assert!(x.is_finite() && y.is_finite());
    }

    /// The per-node force computation is parallelized across `rayon` tasks,
    /// but each node's own force is accumulated sequentially within one
    /// task (no shared mutable state between tasks), so results should be
    /// bit-for-bit reproducible across repeated calls despite not being a
    /// hard design requirement (determinism was explicitly relaxed for this
    /// layout since it's GUI-only). If a future change to the accumulation
    /// strategy (e.g. real cross-thread reduction) makes this flake, delete
    /// it rather than fight it.
    #[test]
    fn layout_is_deterministic_across_repeated_calls() {
        let layout = ForceDirectedLayout::new(ForceDirectedConfig::default());
        let mut graph: Graph<(), ()> = Graph::new();
        let hub = graph.add_node(());
        let children: Vec<_> = (0..8).map(|_| graph.add_node(())).collect();
        for &c in &children {
            graph.add_edge(hub, c, ());
        }

        let first = layout.layout(&graph);
        let second = layout.layout(&graph);

        assert_eq!(first, second);
    }

    #[test]
    fn layout_never_produces_nan_or_infinite_positions() {
        let layout = ForceDirectedLayout::new(ForceDirectedConfig::default());
        let mut graph: Graph<(), ()> = Graph::new();

        // A hub-and-spoke cluster plus a disjoint cycle plus isolated nodes.
        let hub = graph.add_node(());
        let children: Vec<_> = (0..5).map(|_| graph.add_node(())).collect();
        for &c in &children {
            graph.add_edge(hub, c, ());
        }
        let cycle: Vec<_> = (0..4).map(|_| graph.add_node(())).collect();
        for pair in cycle.windows(2) {
            graph.add_edge(pair[0], pair[1], ());
        }
        graph.add_edge(cycle[3], cycle[0], ());
        for _ in 0..3 {
            graph.add_node(());
        }

        let positions = layout.layout(&graph);

        for (x, y) in positions.values() {
            assert!(
                x.is_finite() && y.is_finite(),
                "position should be finite: ({x}, {y})"
            );
        }
    }

    /// Two fully disjoint components (no edges between them) must not
    /// collapse onto the same point -- the min-distance clamp exists so
    /// coincident nodes still repel deterministically instead of dividing
    /// by zero, but the simulation as a whole must actually spread them out.
    #[test]
    fn disconnected_components_do_not_collapse_to_the_same_point() {
        let layout = ForceDirectedLayout::new(ForceDirectedConfig::default());
        let mut graph: Graph<(), ()> = Graph::new();

        let mut triangle_a = Vec::new();
        for _ in 0..3 {
            triangle_a.push(graph.add_node(()));
        }
        graph.add_edge(triangle_a[0], triangle_a[1], ());
        graph.add_edge(triangle_a[1], triangle_a[2], ());
        graph.add_edge(triangle_a[2], triangle_a[0], ());

        let mut triangle_b = Vec::new();
        for _ in 0..3 {
            triangle_b.push(graph.add_node(()));
        }
        graph.add_edge(triangle_b[0], triangle_b[1], ());
        graph.add_edge(triangle_b[1], triangle_b[2], ());
        graph.add_edge(triangle_b[2], triangle_b[0], ());

        let positions = layout.layout(&graph);
        let all: Vec<(f32, f32)> = positions.values().copied().collect();

        for i in 0..all.len() {
            for j in (i + 1)..all.len() {
                let dx = all[i].0 - all[j].0;
                let dy = all[i].1 - all[j].1;
                let dist = (dx * dx + dy * dy).sqrt();
                assert!(
                    dist > 1.0,
                    "nodes {i} and {j} ended up nearly coincident: dist {dist}"
                );
            }
        }
    }

    /// Sanity check that attraction/repulsion are actually balanced: leaves
    /// connected to a hub should end up closer to it than a fully
    /// unconnected outlier node.
    #[test]
    fn connected_nodes_end_up_closer_than_unrelated_nodes() {
        let layout = ForceDirectedLayout::new(ForceDirectedConfig::default());
        let mut graph: Graph<(), ()> = Graph::new();

        let hub = graph.add_node(());
        let leaves: Vec<_> = (0..4).map(|_| graph.add_node(())).collect();
        for &leaf in &leaves {
            graph.add_edge(hub, leaf, ());
        }
        let outlier = graph.add_node(());

        let positions = layout.layout(&graph);

        let dist = |a: NodeIndex, b: NodeIndex| -> f32 {
            let (ax, ay) = positions[&a];
            let (bx, by) = positions[&b];
            ((ax - bx).powi(2) + (ay - by).powi(2)).sqrt()
        };

        let avg_leaf_dist: f32 =
            leaves.iter().map(|&l| dist(hub, l)).sum::<f32>() / leaves.len() as f32;
        let outlier_dist = dist(hub, outlier);

        assert!(
            avg_leaf_dist < outlier_dist,
            "connected leaves (avg dist {avg_leaf_dist}) should end up closer to the hub \
             than the unconnected outlier (dist {outlier_dist})"
        );
    }

    #[test]
    fn zero_iterations_returns_seeded_positions_unchanged() {
        let config = ForceDirectedConfig {
            iterations: 0,
            ..ForceDirectedConfig::default()
        };
        let layout = ForceDirectedLayout::new(config.clone());
        let mut graph: Graph<(), ()> = Graph::new();
        let nodes: Vec<_> = (0..6).map(|_| graph.add_node(())).collect();
        for pair in nodes.windows(2) {
            graph.add_edge(pair[0], pair[1], ());
        }

        let positions = layout.layout(&graph);

        let angle_step = 2.0 * PI / nodes.len() as f32;
        for (i, &node) in nodes.iter().enumerate() {
            let angle = i as f32 * angle_step;
            let expected = (
                config.initial_radius * angle.cos(),
                config.initial_radius * angle.sin(),
            );
            let (x, y) = positions[&node];
            assert!((x - expected.0).abs() < 1e-4);
            assert!((y - expected.1).abs() < 1e-4);
        }
    }

    /// Coarse performance regression guard: a ~1000 node graph with a
    /// realistic amount of edges must complete comfortably within a
    /// generous time budget. This is deliberately loose (not a precise
    /// benchmark) to avoid flaking on slow/shared CI runners -- it exists
    /// to catch a regression that loses parallelization entirely or
    /// introduces a worse complexity class, per the 500-1000+ file
    /// performance requirement for this layout.
    #[test]
    fn layout_of_1000_nodes_completes_within_time_budget() {
        let layout = ForceDirectedLayout::new(ForceDirectedConfig::default());
        let mut graph: Graph<(), ()> = Graph::new();
        let n = 1000;
        let nodes: Vec<_> = (0..n).map(|_| graph.add_node(())).collect();
        for i in 0..n {
            // Each node links to a handful of others, deterministically,
            // without pulling in a `rand` dependency.
            for offset in [1usize, 7, 13] {
                let j = (i + offset) % n;
                if i != j {
                    graph.add_edge(nodes[i], nodes[j], ());
                }
            }
        }

        let start = std::time::Instant::now();
        let positions = layout.layout(&graph);
        let elapsed = start.elapsed();

        assert_eq!(positions.len(), n);
        for (x, y) in positions.values() {
            assert!(x.is_finite() && y.is_finite());
        }
        assert!(
            elapsed.as_secs() < 10,
            "1000-node layout took too long: {elapsed:?} (expected well under 10s)"
        );
    }
}
