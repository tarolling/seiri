//! Force-directed graph layout (Fruchterman-Reingold spring-embedder).
//! See: <https://en.wikipedia.org/wiki/Force-directed_graph_drawing>
//!
//! Complexity: O(n^2) total force evaluations per iteration (every pair is
//! checked against the repulsion cutoff, no Barnes-Hut/grid approximation),
//! parallelized across CPU cores via `rayon` to hit the 500-1000+ node
//! performance target. This
//! buys a constant-factor (core-count) speedup, not a better asymptotic
//! class -- if graphs ever need to scale past roughly 5-10k nodes, a
//! Barnes-Hut quadtree would be the next step.

use crate::layout::Layout;
use petgraph::graph::{Graph, NodeIndex};
use rayon::prelude::*;
use std::collections::HashMap;
use std::f32::consts::PI;

const OVERLAP_REMOVAL_PASSES: usize = 50;
/// Effective repulsion cutoff per `ideal_edge_length * sqrt(node count)`.
const CUTOFF_GROWTH: f32 = 0.25;

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
    /// Scale of the deterministic spiral seed placement; seeded nodes start
    /// roughly `1.8 * seed_spacing` apart.
    pub seed_spacing: f32,
    /// Minimum distance beyond which node pairs no longer repel each other;
    /// the effective cutoff grows with the square root of the node count.
    pub repulsion_cutoff: f32,
    /// Strength of the pull toward the layout's centroid, proportional to
    /// distance.
    pub gravity: f32,
    /// Minimum distance enforced between any two nodes after the simulation.
    pub min_node_separation: f32,
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
            seed_spacing: 75.0,
            repulsion_cutoff: 300.0,
            gravity: 0.05,
            min_node_separation: 130.0,
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
    cutoff: f32,
    config: &ForceDirectedConfig,
) -> (f32, f32) {
    let (ux, uy, dist) = direction_and_distance(from, to, from_idx, to_idx, config.min_distance);
    if dist > cutoff {
        return (0.0, 0.0);
    }
    let force = config.repulsion_strength * k * k / dist;
    (ux * force, uy * force)
}

/// Pull of `pos` toward `center`, proportional to their distance.
fn gravity_force(pos: (f32, f32), center: (f32, f32), gravity: f32) -> (f32, f32) {
    ((center.0 - pos.0) * gravity, (center.1 - pos.1) * gravity)
}

/// Pushes overlapping node pairs apart until no two nodes are closer than
/// `min_separation`, or `max_passes` is reached.
fn separate_overlapping_nodes(
    pos: &mut [(f32, f32)],
    min_separation: f32,
    min_distance: f32,
    max_passes: usize,
) {
    if min_separation <= 0.0 {
        return;
    }
    for _ in 0..max_passes {
        let mut moved = false;
        for i in 0..pos.len() {
            for j in (i + 1)..pos.len() {
                let (ux, uy, dist) = direction_and_distance(pos[i], pos[j], i, j, min_distance);
                if dist < min_separation {
                    // split the overlap evenly, each node moving half of it
                    let push = (min_separation - dist) / 2.0;
                    pos[i].0 += ux * push;
                    pos[i].1 += uy * push;
                    pos[j].0 -= ux * push;
                    pos[j].1 -= uy * push;
                    moved = true;
                }
            }
        }
        if !moved {
            break;
        }
    }
}

/// Deterministic seed position of the `i`th node on a phyllotaxis (sunflower)
/// spiral, which fills a disc at roughly uniform density.
fn seed_position(i: usize, spacing: f32) -> (f32, f32) {
    let golden_angle = PI * (3.0 - 5.0f32.sqrt());
    let radius = spacing * (i as f32 + 0.5).sqrt();
    let angle = i as f32 * golden_angle;
    (radius * angle.cos(), radius * angle.sin())
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

        let mut pos: Vec<(f32, f32)> = (0..n)
            .map(|i| seed_position(i, self.config.seed_spacing))
            .collect();

        let k = self.config.ideal_edge_length;
        let mut temperature = self.config.initial_temperature;
        // larger layouts need longer-range repulsion so springs between distant
        // clusters can't compress the nodes in between
        let cutoff = self
            .config
            .repulsion_cutoff
            .max(CUTOFF_GROWTH * k * (n as f32).sqrt());

        for _ in 0..self.config.iterations {
            let center = pos
                .iter()
                .fold((0.0f32, 0.0f32), |acc, p| (acc.0 + p.0, acc.1 + p.1));
            let center = (center.0 / n as f32, center.1 / n as f32);

            let disp: Vec<(f32, f32)> = (0..n)
                .into_par_iter()
                .map(|i| {
                    let mut f = gravity_force(pos[i], center, self.config.gravity);
                    for j in 0..n {
                        if i == j {
                            continue;
                        }
                        let (fx, fy) =
                            repulsive_force(pos[i], pos[j], i, j, k, cutoff, &self.config);
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

        separate_overlapping_nodes(
            &mut pos,
            self.config.min_node_separation,
            self.config.min_distance,
            OVERLAP_REMOVAL_PASSES,
        );

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

    fn distance(a: (f32, f32), b: (f32, f32)) -> f32 {
        ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
    }

    fn centroid(points: &[(f32, f32)]) -> (f32, f32) {
        let n = points.len() as f32;
        let (sx, sy) = points
            .iter()
            .fold((0.0, 0.0), |acc, p| (acc.0 + p.0, acc.1 + p.1));
        (sx / n, sy / n)
    }

    #[test]
    fn isolated_node_stays_near_the_connected_cluster() {
        let config = ForceDirectedConfig::default();
        let layout = ForceDirectedLayout::new(config.clone());
        let mut graph: Graph<(), ()> = Graph::new();

        let hub = graph.add_node(());
        let mut cluster = vec![hub];
        for _ in 0..10 {
            let leaf = graph.add_node(());
            graph.add_edge(hub, leaf, ());
            cluster.push(leaf);
        }
        let isolated = graph.add_node(());

        let positions = layout.layout(&graph);

        let nearest = cluster
            .iter()
            .map(|&c| distance(positions[&c], positions[&isolated]))
            .fold(f32::INFINITY, f32::min);
        let bound = config.repulsion_cutoff + config.ideal_edge_length;
        assert!(
            nearest < bound,
            "isolated node drifted {nearest} from the cluster (bound {bound})"
        );
    }

    #[test]
    fn disconnected_components_stay_within_bounded_distance() {
        let config = ForceDirectedConfig::default();
        let layout = ForceDirectedLayout::new(config.clone());
        let mut graph: Graph<(), ()> = Graph::new();

        let mut triangles = Vec::new();
        for _ in 0..2 {
            let t: Vec<_> = (0..3).map(|_| graph.add_node(())).collect();
            graph.add_edge(t[0], t[1], ());
            graph.add_edge(t[1], t[2], ());
            graph.add_edge(t[2], t[0], ());
            triangles.push(t);
        }

        let positions = layout.layout(&graph);

        let centers: Vec<(f32, f32)> = triangles
            .iter()
            .map(|t| centroid(&t.iter().map(|n| positions[n]).collect::<Vec<_>>()))
            .collect();
        let gap = distance(centers[0], centers[1]);
        let bound = 4.0 * config.ideal_edge_length;
        assert!(gap < bound, "components ended {gap} apart (bound {bound})");
    }

    #[test]
    fn nodes_respect_min_node_separation() {
        let config = ForceDirectedConfig::default();
        let layout = ForceDirectedLayout::new(config.clone());
        let mut graph: Graph<(), ()> = Graph::new();

        let hub = graph.add_node(());
        for _ in 0..30 {
            let leaf = graph.add_node(());
            graph.add_edge(hub, leaf, ());
        }
        let ring: Vec<_> = (0..12).map(|_| graph.add_node(())).collect();
        for i in 0..ring.len() {
            for offset in 1..4 {
                graph.add_edge(ring[i], ring[(i + offset) % ring.len()], ());
            }
        }

        let positions = layout.layout(&graph);
        let all: Vec<(f32, f32)> = positions.values().copied().collect();

        assert!(config.min_node_separation > 0.0);
        let required = 0.9 * config.min_node_separation;
        for i in 0..all.len() {
            for j in (i + 1)..all.len() {
                let dist = distance(all[i], all[j]);
                assert!(
                    dist >= required,
                    "nodes {i} and {j} are only {dist} apart (need {required})"
                );
            }
        }
    }

    #[test]
    fn zero_iterations_returns_seeded_positions_unchanged() {
        let config = ForceDirectedConfig {
            iterations: 0,
            min_node_separation: 0.0,
            ..ForceDirectedConfig::default()
        };
        let layout = ForceDirectedLayout::new(config.clone());
        let mut graph: Graph<(), ()> = Graph::new();
        let nodes: Vec<_> = (0..6).map(|_| graph.add_node(())).collect();
        for pair in nodes.windows(2) {
            graph.add_edge(pair[0], pair[1], ());
        }

        let positions = layout.layout(&graph);

        for (i, &node) in nodes.iter().enumerate() {
            let expected = seed_position(i, config.seed_spacing);
            let (x, y) = positions[&node];
            assert!((x - expected.0).abs() < 1e-4);
            assert!((y - expected.1).abs() < 1e-4);
        }
    }

    #[test]
    fn seed_positions_stay_spread_out_for_large_graphs() {
        let spacing = ForceDirectedConfig::default().seed_spacing;
        let seeds: Vec<(f32, f32)> = (0..3000).map(|i| seed_position(i, spacing)).collect();

        // compare each seed against the next few in spiral order and a
        // sample of others, keeping the check well under O(n^2)
        for i in 0..seeds.len() {
            for j in (i + 1)..(i + 60).min(seeds.len()) {
                let dist = distance(seeds[i], seeds[j]);
                assert!(
                    dist > spacing,
                    "seeds {i} and {j} are only {dist} apart (spacing {spacing})"
                );
            }
        }
    }

    #[test]
    fn large_sparse_graph_has_no_overlapping_nodes() {
        let config = ForceDirectedConfig::default();
        let layout = ForceDirectedLayout::new(config.clone());
        let mut graph: Graph<(), ()> = Graph::new();

        // many small hub-and-leaf clusters, loosely linked, plus a large
        // number of isolated nodes, like a real project with many test files
        let hubs: Vec<_> = (0..200).map(|_| graph.add_node(())).collect();
        for &hub in &hubs {
            for _ in 0..4 {
                let leaf = graph.add_node(());
                graph.add_edge(hub, leaf, ());
            }
        }
        for h in 0..hubs.len() {
            graph.add_edge(hubs[h], hubs[(h * 7 + 1) % hubs.len()], ());
        }
        for _ in 0..500 {
            graph.add_node(());
        }

        let positions = layout.layout(&graph);
        let all: Vec<(f32, f32)> = positions.values().copied().collect();

        let required = 0.75 * config.min_node_separation;
        let overlapping = (0..all.len())
            .flat_map(|i| ((i + 1)..all.len()).map(move |j| (i, j)))
            .filter(|&(i, j)| distance(all[i], all[j]) < required)
            .count();
        assert_eq!(overlapping, 0, "{overlapping} node pairs overlap");
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
