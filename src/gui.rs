use crate::analysis::GraphAnalysis;
use crate::core::defs::GraphNode;
use crate::gui::camera::Camera;
use crate::layout::{self, LayoutType};
use eframe::egui;
use egui::{Rect, Response, Sense, Ui, Vec2, pos2, vec2};
use petgraph::{Graph, graph::NodeIndex};
use std::collections::HashMap;

mod camera;

pub struct SeiriGraph {
    pub graph_nodes: Vec<GraphNode>,

    // View state
    camera_pos: Vec2,
    camera: Camera,

    // Node layout
    node_positions: Vec<Vec2>,
    layout_type: LayoutType,

    // Interaction state
    selected_node: Option<usize>,
    hovered_node: Option<usize>,

    // Visual settings
    min_node_radius: f32,
    max_node_radius: f32,
    show_labels: bool,
    show_dependencies: bool,

    // Node size calculation
    min_loc: u32,
    max_loc: u32,

    // Graph analysis
    graph_analysis: Option<GraphAnalysis>,
}

impl SeiriGraph {
    pub fn new(graph_nodes: Vec<GraphNode>) -> Self {
        let n = graph_nodes.len();

        // Calculate min/max LOC
        let min_loc = graph_nodes
            .iter()
            .map(|n| n.data().loc())
            .min()
            .unwrap_or(0);
        let max_loc = graph_nodes
            .iter()
            .map(|n| n.data().loc())
            .max()
            .unwrap_or(0);

        let mut app = Self {
            graph_nodes,
            camera_pos: Vec2::ZERO,
            camera: Camera::default(),
            node_positions: vec![Vec2::ZERO; n],
            layout_type: LayoutType::default(),
            selected_node: None,
            hovered_node: None,
            min_node_radius: 20.0,
            max_node_radius: 40.0,
            show_labels: true,
            show_dependencies: true,
            min_loc,
            max_loc,
            graph_analysis: None,
        };
        app.initialize_positions();
        app
    }

    fn initialize_positions(&mut self) {
        let n = self.graph_nodes.len();
        if n == 0 {
            return;
        }

        // Create a graph for layout
        let mut graph = Graph::new();
        let mut node_indices = Vec::with_capacity(n);

        // Add nodes
        for _ in 0..n {
            node_indices.push(graph.add_node(()));
        }

        // Add edges based on dependencies
        for (from_idx, node) in self.graph_nodes.iter().enumerate() {
            for (dep_idx, _edge) in node.edges().iter().enumerate() {
                graph.add_edge(node_indices[from_idx], node_indices[dep_idx], ());
            }
        }

        // Get layout positions
        let layout = layout::create_layout(self.layout_type);
        let raw_positions = layout.layout(&graph);

        // Find the bounds of the layout
        let mut min_x = f32::INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut max_y = f32::NEG_INFINITY;

        for &(x, y) in raw_positions.values() {
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }

        // Calculate center and scale
        let width = max_x - min_x;
        let height = max_y - min_y;
        let target_size = 800.0; // Target layout size
        let scale = if width > height {
            target_size / width
        } else {
            target_size / height
        };

        // Center of the layout
        let center_x = (min_x + max_x) / 2.0;
        let center_y = (min_y + max_y) / 2.0;

        // initialize positions
        for (i, node_idx) in node_indices.iter().enumerate() {
            if let Some(&(x, y)) = raw_positions.get(node_idx) {
                // scale and center the coordinates in world space
                let world_x = (x - center_x) * scale + 500.0; // center at world position 500, matches default of 1000
                let world_y = (y - center_y) * scale + 500.0;
                self.node_positions[i] = vec2(world_x, world_y);
            }
        }

        // Analyze graph structure
        self.graph_analysis = Some(GraphAnalysis::analyze_graph(&graph));

        // Reset camera and zoom to frame the layout
        self.camera_pos = egui::Vec2::ZERO;
        self.camera.reset();
    }

    fn get_node_color(&self, index: usize) -> egui::Color32 {
        let node = &self.graph_nodes[index];
        let is_external = !node.data().file().exists();
        let in_largest_scc = self
            .graph_analysis
            .as_ref()
            .map(|analysis| analysis.is_in_largest_scc(NodeIndex::new(index)))
            .unwrap_or(false);

        // change base color based on node type
        let base_color = if in_largest_scc {
            egui::Color32::from_rgb(255, 100, 100) // Red for SCC nodes
        } else if is_external {
            egui::Color32::from_hex(node.data().language().color()).unwrap_or(egui::Color32::GRAY)
        } else {
            egui::Color32::from_hex(node.data().language().color())
                .map(|c| c.gamma_multiply(0.5))
                .unwrap_or(egui::Color32::GRAY)
        };

        if Some(index) == self.selected_node {
            egui::Color32::ORANGE
        } else if Some(index) == self.hovered_node {
            egui::Color32::LIGHT_BLUE
        } else {
            base_color
        }
    }

    fn draw_graph(&mut self, ui: &mut Ui, canvas_rect: &Rect) {
        let painter = ui.painter_at(*canvas_rect);

        // Draw edges first (behind nodes)
        if self.show_dependencies {
            for (i, node) in self.graph_nodes.iter().enumerate() {
                let from_pos = self
                    .camera
                    .world_to_screen(self.node_positions[i].to_pos2(), canvas_rect);

                for edge_file in node.edges() {
                    if let Some(j) = self
                        .graph_nodes
                        .iter()
                        .position(|n| n.data().file() == edge_file)
                    {
                        let to_pos = self
                            .camera
                            .world_to_screen(self.node_positions[j].to_pos2(), canvas_rect);

                        // Only draw if both nodes are visible
                        if canvas_rect.contains(egui::pos2(from_pos.x, from_pos.y))
                            || canvas_rect.contains(egui::pos2(to_pos.x, to_pos.y))
                        {
                            let edge_color =
                                if Some(i) == self.selected_node || Some(j) == self.selected_node {
                                    egui::Color32::from_rgb(255, 150, 50)
                                } else {
                                    egui::Color32::from_rgba_premultiplied(100, 150, 200, 80)
                                };

                            // Draw the main line
                            painter.line_segment(
                                [
                                    egui::pos2(from_pos.x, from_pos.y),
                                    egui::pos2(to_pos.x, to_pos.y),
                                ],
                                egui::Stroke::new(
                                    2.0 * self.camera.zoom_level().sqrt(),
                                    edge_color,
                                ),
                            );

                            // Calculate arrow direction
                            let dir = (to_pos - from_pos).normalized();
                            let arrow_size = 10.0 * self.camera.zoom_level().sqrt();
                            let arrow_angle: f32 = 0.5; // ~30 degrees in radians

                            // Calculate arrowhead points
                            let arrow_end = to_pos - dir * (20.0 * self.camera.zoom_level().sqrt()); // Pull back from the end
                            let left = arrow_end
                                + arrow_size
                                    * vec2(
                                        -dir.x * arrow_angle.cos() + dir.y * arrow_angle.sin(),
                                        -dir.x * arrow_angle.sin() - dir.y * arrow_angle.cos(),
                                    );
                            let right = arrow_end
                                + arrow_size
                                    * vec2(
                                        -dir.x * arrow_angle.cos() - dir.y * arrow_angle.sin(),
                                        dir.x * arrow_angle.sin() - dir.y * arrow_angle.cos(),
                                    );

                            // Draw arrowhead
                            painter.add(egui::Shape::convex_polygon(
                                vec![
                                    pos2(to_pos.x, to_pos.y),
                                    pos2(left.x, left.y),
                                    pos2(right.x, right.y),
                                ],
                                edge_color,
                                egui::Stroke::new(1.0, edge_color),
                            ));
                        }
                    }
                }
            }
        }

        // Draw nodes
        for (i, node) in self.graph_nodes.iter().enumerate() {
            let screen_pos = self
                .camera
                .world_to_screen(self.node_positions[i].to_pos2(), &canvas_rect);

            // Only draw visible nodes
            let betweenness_score = self
                .graph_analysis
                .as_ref()
                .and_then(|analysis| analysis.get_betweenness_centrality(NodeIndex::new(i)));

            let base_radius = self.graph_nodes[i].calculate_size(
                self.min_loc,
                self.max_loc,
                self.min_node_radius,
                self.max_node_radius,
                betweenness_score,
            );
            let node_radius = base_radius * self.camera.zoom_level();
            if !canvas_rect.expand(node_radius).contains(screen_pos) {
                continue;
            }

            let color = self.get_node_color(i);

            // Node circle with subtle shadow
            painter.circle_filled(
                screen_pos + vec2(2.0, 2.0) * self.camera.zoom_level(),
                node_radius,
                egui::Color32::from_black_alpha(30),
            );
            painter.circle_filled(screen_pos, node_radius, color);

            // Node border
            let border_color = if Some(i) == self.selected_node {
                egui::Color32::from_rgb(255, 100, 0)
            } else {
                egui::Color32::from_rgb(60, 60, 60)
            };
            painter.circle_stroke(
                screen_pos,
                node_radius,
                egui::Stroke::new(2.0 * self.camera.zoom_level().sqrt(), border_color),
            );

            // Node label with background for better readability
            if self.show_labels
                && self.camera.zoom_level() > 0.3
                && let Some(name) = node.data().file().file_stem().and_then(|s| s.to_str())
            {
                let font_size = (12.0 * self.camera.zoom_level()).clamp(8.0, 16.0);

                // Measure text to create appropriate background
                let font_id = egui::FontId::proportional(font_size);
                let text_galley =
                    painter.layout_no_wrap(name.to_string(), font_id.clone(), egui::Color32::WHITE);
                let text_rect = egui::Rect::from_center_size(
                    screen_pos,
                    text_galley.size() + vec2(6.0, 4.0) * self.camera.zoom_level(),
                );

                // Only draw background if text is wider than the node
                if text_galley.size().x > node_radius * 1.5 {
                    painter.rect_filled(
                        text_rect,
                        4.0 * self.camera.zoom_level(),
                        egui::Color32::from_black_alpha(180),
                    );
                    painter.rect_stroke(
                        text_rect,
                        4.0 * self.camera.zoom_level(),
                        egui::Stroke::new(
                            1.0 * self.camera.zoom_level(),
                            egui::Color32::from_gray(100),
                        ),
                        egui::StrokeKind::Middle,
                    );
                }

                // Draw text - always white for contrast against dark backgrounds
                let text_color = if text_galley.size().x > node_radius * 1.5 {
                    egui::Color32::WHITE // White text on dark background
                } else {
                    // Text fits in node - use contrast-based color
                    if (color.r() as u16) + (color.g() as u16) + (color.b() as u16) > 400 {
                        egui::Color32::BLACK
                    } else {
                        egui::Color32::WHITE
                    }
                };

                painter.text(
                    screen_pos,
                    egui::Align2::CENTER_CENTER,
                    name,
                    font_id,
                    text_color,
                );
            }
        }
    }

    ////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////
    /// Interaction Handling
    ////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////

    fn handle_interaction(&mut self, ui: &mut Ui, response: &mut Response, canvas_rect: &Rect) {
        self.handle_hover(response, canvas_rect);
        self.handle_zoom(ui, response, canvas_rect);
        self.handle_pan(response, canvas_rect);
        self.handle_click(response, canvas_rect);
    }

    /// Handle highlighting of nodes hovered over.
    fn handle_hover(&mut self, response: &mut Response, canvas_rect: &Rect) {
        if !response.hovered() {
            return;
        }

        if let Some(cursor_pos) = response.hover_pos() {
            let world_mouse = self.camera.screen_to_world(cursor_pos, canvas_rect);
            for (i, _) in self.graph_nodes.iter().enumerate() {
                let dist = (world_mouse - self.node_positions[i]).to_vec2().length();

                // TODO: insanely inefficient, please change this someday
                let betweenness_score = self
                    .graph_analysis
                    .as_ref()
                    .and_then(|analysis| analysis.get_betweenness_centrality(NodeIndex::new(i)));

                let node_radius = self.graph_nodes[i].calculate_size(
                    self.min_loc,
                    self.max_loc,
                    self.min_node_radius,
                    self.max_node_radius,
                    betweenness_score,
                );
                if dist < node_radius {
                    self.hovered_node = Some(i);
                    return;
                }
            }
        }

        self.hovered_node = None;
        response.mark_changed();
    }

    /// Handle zoom via mouse scroll.
    fn handle_zoom(&mut self, ui: &mut Ui, response: &mut Response, canvas_rect: &Rect) {
        if !response.hovered() {
            return;
        }

        let scroll_delta = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll_delta == 0.0 {
            return;
        }

        if let Some(cursor_pos) = response.hover_pos() {
            let zoom_factor = 1.0 + scroll_delta * 0.001;
            self.camera.zoom_at(zoom_factor, cursor_pos, canvas_rect);
            response.mark_changed();
        }
    }

    /// Handle pan interaction for viewport panning and node movement.
    fn handle_pan(&mut self, response: &mut Response, canvas_rect: &Rect) {
        if !response.dragged() {
            return;
        }

        let drag_delta = response.drag_delta();

        // TODO: perform hit test here instead

        if let Some(cursor_pos) = response.hover_pos() {
            let world_mouse = self.camera.screen_to_world(cursor_pos, canvas_rect);

            for (i, _) in self.graph_nodes.iter().enumerate() {
                let dist = (world_mouse - self.node_positions[i]).to_vec2().length();

                // TODO: insanely inefficient, please change this someday
                let betweenness_score = self
                    .graph_analysis
                    .as_ref()
                    .and_then(|analysis| analysis.get_betweenness_centrality(NodeIndex::new(i)));

                let node_radius = self.graph_nodes[i].calculate_size(
                    self.min_loc,
                    self.max_loc,
                    self.min_node_radius,
                    self.max_node_radius,
                    betweenness_score,
                );
                if dist < node_radius {
                    response.dnd_set_drag_payload(i);
                    break;
                }
            }

            if let Some(node) = response.dnd_hover_payload::<usize>() {
                self.node_positions[*node] = world_mouse.to_vec2();
            } else {
                response.dnd_release_payload::<usize>();
                self.camera.pan(drag_delta, canvas_rect);
            }
        } else {
            response.dnd_release_payload::<usize>();
            self.camera.pan(drag_delta, canvas_rect);
        }

        response.mark_changed();
    }

    /// Handle click interaction for node selection/deselection.
    fn handle_click(&mut self, response: &mut Response, canvas_rect: &Rect) {
        if !(response.clicked() || response.double_clicked()) {
            return;
        }

        if let Some(click_pos) = response.interact_pointer_pos() {
            let click_world_pos = self.camera.screen_to_world(click_pos, canvas_rect);

            for (i, _) in self.graph_nodes.iter().enumerate() {
                let dist = (click_world_pos - self.node_positions[i])
                    .to_vec2()
                    .length();

                // TODO: insanely inefficient, please change this someday
                let betweenness_score = self
                    .graph_analysis
                    .as_ref()
                    .and_then(|analysis| analysis.get_betweenness_centrality(NodeIndex::new(i)));

                let node_radius = self.graph_nodes[i].calculate_size(
                    self.min_loc,
                    self.max_loc,
                    self.min_node_radius,
                    self.max_node_radius,
                    betweenness_score,
                );
                if dist < node_radius {
                    self.selected_node = Some(i);
                    return;
                }
            }

            // clicked on empty space
            self.selected_node = None;
        }

        response.mark_changed();
    }

    ////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////
    /// Panels
    ////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////

    fn render_analysis_panel(&mut self, ui: &mut Ui) {
        ui.heading("Graph Analysis");
        ui.add_space(8.0);

        if let Some(analysis) = &self.graph_analysis {
            // SCCs summary
            ui.collapsing("Strongly Connected Components", |ui| {
                ui.label(format!("Total SCCs: {}", analysis.scc_sizes.len()));
                ui.label(format!("Largest SCC size: {}", analysis.largest_scc_size));

                ui.add_space(4.0);
                ui.label("Files in largest SCC:");
                egui::ScrollArea::vertical()
                    .max_height(150.0)
                    .show(ui, |ui| {
                        for node_idx in &analysis.largest_scc_nodes {
                            let node = &self.graph_nodes[node_idx.index()];
                            ui.label(format!(
                                "• {}",
                                node.data()
                                    .file()
                                    .file_name()
                                    .unwrap_or_default()
                                    .to_string_lossy()
                            ));
                        }
                    });

                // Show SCC size distribution
                ui.add_space(8.0);
                ui.label("SCC Size Distribution:");
                let mut size_dist = HashMap::new();
                for &size in &analysis.scc_sizes {
                    *size_dist.entry(size).or_insert(0) += 1;
                }
                let mut sizes: Vec<_> = size_dist.into_iter().collect();
                sizes.sort_by_key(|&(size, _)| size);
                for (size, count) in sizes {
                    ui.label(format!("{} nodes: {} SCCs", size, count));
                }
            });

            // Highlight options
            if analysis.largest_scc_size > 1
                && ui.button("Highlight Largest SCC").clicked()
                && let Some(selected) = self.selected_node
                && !analysis.is_in_largest_scc(NodeIndex::new(selected))
            {
                self.selected_node = None;
            }

            // Show top 5 betweenness centrality nodes
            ui.collapsing("Top Dependency Chokepoints", |ui| {
                ui.label("(Highest betweenness centrality)");

                // Get all nodes with scores
                let mut nodes: Vec<_> = (0..self.graph_nodes.len())
                    .filter_map(|idx| {
                        analysis
                            .get_betweenness_centrality(NodeIndex::new(idx))
                            .map(|score| (idx, score))
                    })
                    .collect();

                // Sort by score descending
                nodes.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

                // Show top 5
                for (idx, score) in nodes.iter().take(5) {
                    let node = &self.graph_nodes[*idx];
                    let name = node
                        .data()
                        .file()
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("unknown");

                    if ui
                        .selectable_label(
                            Some(*idx) == self.selected_node,
                            format!("{} ({:.3})", name, score),
                        )
                        .clicked()
                    {
                        self.selected_node = Some(*idx);
                    }
                }
            });
        }
    }

    /// Renders the controls panel on the top of the window.
    /// Shows things like layout types, show/hide options, and zoom level.
    fn render_controls_panel(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.heading("Project Structure Graph");

            ui.separator();

            ui.horizontal(|ui| {
                ui.checkbox(&mut self.show_labels, "Show Labels");
                ui.checkbox(&mut self.show_dependencies, "Show Dependencies");
            });

            ui.separator();

            egui::ComboBox::from_label("Layout")
                .selected_text(match self.layout_type {
                    LayoutType::Circular => "Circular",
                    LayoutType::Sugiyama => "Sugiyama",
                })
                .show_ui(ui, |ui| {
                    let mut changed = false;
                    changed |= ui
                        .selectable_value(&mut self.layout_type, LayoutType::Circular, "Circular")
                        .clicked();
                    changed |= ui
                        .selectable_value(&mut self.layout_type, LayoutType::Sugiyama, "Sugiyama")
                        .clicked();
                    if changed {
                        self.initialize_positions();
                    }
                });

            ui.separator();

            ui.label(format!("Nodes: {}", self.graph_nodes.len()));

            ui.separator();

            ui.label(format!("Zoom: {:.1}x", self.camera.zoom_level()));
        });
    }

    fn render_details_panel(&mut self, ui: &mut Ui, selected_idx: usize) {
        ui.heading("Node Details");
        let node = &self.graph_nodes[selected_idx].data();

        ui.group(|ui| {
            ui.strong("File Information");
            ui.label(format!("📁 {}", node.file().display()));
            ui.label(format!("🔧 {}", node.language().to_string()));
            ui.label(format!("📊 {} lines", node.loc()));

            // Add betweenness centrality score if available
            if let Some(analysis) = &self.graph_analysis
                && let Some(score) =
                    analysis.get_betweenness_centrality(NodeIndex::new(selected_idx))
            {
                ui.label(format!("🔄 Betweenness: {:.3}", score));
            }
        });

        ui.separator();

        ui.group(|ui| {
            ui.strong("Dependencies");

            let incoming: Vec<_> = self
                .graph_nodes
                .iter()
                .enumerate()
                .filter(|(_, n)| n.edges().contains(node.file()))
                .collect();
            let outgoing = self.graph_nodes[selected_idx].edges();

            ui.collapsing(format!("📥 Incoming ({})", incoming.len()), |ui| {
                for (idx, dep_node) in incoming {
                    let name = dep_node
                        .data()
                        .file()
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("unknown");
                    if ui.selectable_label(false, name).clicked() {
                        self.selected_node = Some(idx);
                    }
                }
            });

            ui.collapsing(format!("📤 Outgoing ({})", outgoing.len()), |ui| {
                for edge in outgoing {
                    if let Some(idx) = self
                        .graph_nodes
                        .iter()
                        .position(|n| n.data().file() == edge)
                    {
                        let name = edge
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("unknown");
                        if ui.selectable_label(false, name).clicked() {
                            self.selected_node = Some(idx);
                        }
                    }
                }
            });
        });

        ui.separator();

        // Code structure
        ui.group(|ui| {
            ui.strong("Code Structure");

            if !node.functions().is_empty() {
                ui.collapsing(
                    format!("🔧 Functions ({})", node.functions().len()),
                    |ui| {
                        for func in node.functions() {
                            ui.monospace(func);
                        }
                    },
                );
            }

            if !node.containers().is_empty() {
                ui.collapsing(
                    format!("📦 Containers ({})", node.containers().len()),
                    |ui| {
                        for container in node.containers() {
                            ui.monospace(container);
                        }
                    },
                );
            }
        });

        // Imports
        if !node.imports().is_empty() {
            ui.separator();
            ui.group(|ui| {
                ui.strong("Imports");
                let (local, external): (Vec<_>, Vec<_>) =
                    node.imports().iter().partition(|imp| imp.is_local());

                if !local.is_empty() {
                    ui.collapsing(format!("🏠 Local ({})", local.len()), |ui| {
                        for imp in local {
                            ui.monospace(imp.path());
                        }
                    });
                }

                if !external.is_empty() {
                    ui.collapsing(format!("🌐 External ({})", external.len()), |ui| {
                        for imp in external {
                            ui.monospace(imp.path());
                        }
                    });
                }
            });
        }
    }

    /// Render the main graph view
    fn render_viewport(&mut self, ui: &mut Ui) {
        let (canvas_rect, mut response) =
            ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());

        if self.graph_nodes.is_empty() {
            ui.centered_and_justified(|ui| {
                ui.heading("No files found to display");
                ui.label("Try running the analyzer on a project directory with supported files.");
            });
            return;
        }

        self.handle_interaction(ui, &mut response, &canvas_rect);
        self.draw_graph(ui, &canvas_rect);

        // Instructions overlay
        if self.selected_node.is_none() {
            ui.scope_builder(egui::UiBuilder::new(), |ui| {
                ui.set_clip_rect(canvas_rect);
                ui.allocate_space(egui::Vec2::new(
                    canvas_rect.width() - 260.0,
                    canvas_rect.height() - 100.0,
                ));
                ui.group(|ui| {
                    ui.set_max_width(250.0);
                    ui.label("💡 Tips:");
                    ui.label("• Click nodes to see details");
                    ui.label("• Drag nodes to reposition");
                    ui.label("• Scroll to zoom");
                    ui.label("• Drag empty space to pan");
                });
            });
        }
    }
}

impl eframe::App for SeiriGraph {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Controls panel
        egui::TopBottomPanel::top("controls").show(ctx, |ui| {
            self.render_controls_panel(ui);
        });

        // Details panel for selected node
        if let Some(selected_idx) = self.selected_node {
            egui::SidePanel::right("details")
                .resizable(true)
                .default_width(300.0)
                .show(ctx, |ui| {
                    self.render_details_panel(ui, selected_idx);
                });
        } else {
            // Analysis panel on the right if no node is selected
            egui::SidePanel::right("analysis_panel")
                .resizable(true)
                .default_width(300.0)
                .show(ctx, |ui| {
                    self.render_analysis_panel(ui);
                });
        }

        // Main graph view
        egui::CentralPanel::default().show(ctx, |ui| {
            self.render_viewport(ui);
        });

        ctx.request_repaint();
    }
}

pub fn run_gui(graph_nodes: Vec<GraphNode>) {
    let app = SeiriGraph::new(graph_nodes);
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0])
            .with_resizable(true)
            .with_title("seiri - Project Structure Graph"),
        ..Default::default()
    };

    let _ = eframe::run_native(
        "seiri - Project Structure Graph",
        native_options,
        Box::new(|_cc| Ok(Box::new(app))),
    );
}
