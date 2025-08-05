use crate::core::defs::GraphNode;
use eframe::egui;

pub struct SeiriGraph {
    pub graph_nodes: Vec<GraphNode>,

    // View state
    camera_pos: egui::Vec2,
    zoom: f32,

    // Node layout
    node_positions: Vec<egui::Vec2>,
    node_velocities: Vec<egui::Vec2>,

    // Interaction state
    selected_node: Option<usize>,
    hovered_node: Option<usize>,
    dragging_node: Option<usize>,
    panning: bool,
    last_mouse_pos: Option<egui::Pos2>,
    drag_start_pos: Option<egui::Pos2>,

    // Layout animation
    layout_iterations: usize,
    auto_layout: bool,

    // Visual settings
    min_node_radius: f32,
    max_node_radius: f32,
    show_labels: bool,
    show_dependencies: bool,
    layout_strength: f32,

    // Node size calculation
    min_loc: u32,
    max_loc: u32,
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
            camera_pos: egui::Vec2::ZERO,
            zoom: 1.0,
            node_positions: vec![egui::Vec2::ZERO; n],
            node_velocities: vec![egui::Vec2::ZERO; n],
            selected_node: None,
            hovered_node: None,
            dragging_node: None,
            panning: false,
            last_mouse_pos: None,
            drag_start_pos: None,
            layout_iterations: 0,
            auto_layout: true,
            min_node_radius: 20.0,
            max_node_radius: 40.0,
            show_labels: true,
            show_dependencies: true,
            layout_strength: 1.0,
            min_loc,
            max_loc,
        };
        app.initialize_positions();
        app
    }

    fn initialize_positions(&mut self) {
        let n = self.graph_nodes.len();
        if n == 0 {
            return;
        }

        // Start with a circle layout
        let radius = 150.0;
        for i in 0..n {
            let angle = i as f32 * std::f32::consts::TAU / n as f32;
            self.node_positions[i] = egui::vec2(angle.cos() * radius, angle.sin() * radius);
        }
    }

    fn apply_force_layout(&mut self) {
        if !self.auto_layout || self.graph_nodes.len() < 2 {
            return;
        }

        let dt = 0.02;
        let damping = 0.9;
        let repulsion_strength = 8000.0 * self.layout_strength;
        let attraction_strength = 0.1 * self.layout_strength;
        let center_attraction = 0.001;

        // Reset velocities if dragging
        if let Some(drag_idx) = self.dragging_node {
            self.node_velocities[drag_idx] = egui::Vec2::ZERO;
        }

        // Apply forces
        for i in 0..self.graph_nodes.len() {
            if Some(i) == self.dragging_node {
                continue;
            }

            let mut force = egui::Vec2::ZERO;

            // Repulsion from other nodes
            for j in 0..self.graph_nodes.len() {
                if i == j {
                    continue;
                }
                let diff = self.node_positions[i] - self.node_positions[j];
                let dist_sq = diff.length_sq().max(100.0); // Avoid division by zero
                let repulsion = repulsion_strength / dist_sq;
                force += diff.normalized() * repulsion;
            }

            // Attraction along edges
            for edge_file in self.graph_nodes[i].edges() {
                if let Some(j) = self
                    .graph_nodes
                    .iter()
                    .position(|n| n.data().file() == edge_file)
                {
                    let diff = self.node_positions[j] - self.node_positions[i];
                    let dist = diff.length();
                    let target_dist = 120.0;
                    let spring_force = (dist - target_dist) * attraction_strength;
                    force += diff.normalized() * spring_force;
                }
            }

            // Gentle center attraction
            force -= self.node_positions[i] * center_attraction;

            // Update velocity and position
            self.node_velocities[i] = (self.node_velocities[i] + force * dt) * damping;
            self.node_positions[i] += self.node_velocities[i] * dt;
        }

        self.layout_iterations += 1;
    }

    fn world_to_screen(&self, world_pos: egui::Vec2, canvas_center: egui::Vec2) -> egui::Vec2 {
        (world_pos - self.camera_pos) * self.zoom + canvas_center
    }

    fn screen_to_world(&self, screen_pos: egui::Vec2, canvas_center: egui::Vec2) -> egui::Vec2 {
        (screen_pos - canvas_center) / self.zoom + self.camera_pos
    }

    fn get_node_color(&self, index: usize) -> egui::Color32 {
        let node = &self.graph_nodes[index];
        let is_external = !node.data().file().exists();

        // change base color based on internal or external node
        let base_color = if is_external {
            egui::Color32::from_hex(node.data().language().color())
        } else {
            Ok(egui::Color32::from_hex(node.data().language().color())
                .expect("Error parsing color hex code")
                .gamma_multiply(0.5)) // Internal project files
        };

        if Some(index) == self.selected_node {
            egui::Color32::ORANGE
        } else if Some(index) == self.hovered_node {
            egui::Color32::LIGHT_BLUE
        } else {
            base_color.expect("Error parsing color hex code")
        }
    }

    fn draw_graph(&mut self, ui: &mut egui::Ui, canvas_rect: egui::Rect) {
        let painter = ui.painter_at(canvas_rect);
        let canvas_center = canvas_rect.center().to_vec2();

        // Draw edges first (behind nodes)
        if self.show_dependencies {
            for (i, node) in self.graph_nodes.iter().enumerate() {
                let from_pos = self.world_to_screen(self.node_positions[i], canvas_center);

                for edge_file in node.edges() {
                    if let Some(j) = self
                        .graph_nodes
                        .iter()
                        .position(|n| n.data().file() == edge_file)
                    {
                        let to_pos = self.world_to_screen(self.node_positions[j], canvas_center);

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

                            painter.line_segment(
                                [
                                    egui::pos2(from_pos.x, from_pos.y),
                                    egui::pos2(to_pos.x, to_pos.y),
                                ],
                                egui::Stroke::new(2.0 * self.zoom.sqrt(), edge_color),
                            );
                        }
                    }
                }
            }
        }

        // Draw nodes
        for (i, node) in self.graph_nodes.iter().enumerate() {
            let screen_pos = self.world_to_screen(self.node_positions[i], canvas_center);
            let screen_pos = egui::pos2(screen_pos.x, screen_pos.y);

            // Only draw visible nodes
            let base_radius = self.graph_nodes[i].calculate_size(
                self.min_loc,
                self.max_loc,
                self.min_node_radius,
                self.max_node_radius,
            );
            let node_radius = base_radius * self.zoom;
            if !canvas_rect.expand(node_radius).contains(screen_pos) {
                continue;
            }

            let color = self.get_node_color(i);

            // Node circle with subtle shadow
            painter.circle_filled(
                screen_pos + egui::vec2(2.0, 2.0) * self.zoom,
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
                egui::Stroke::new(2.0 * self.zoom.sqrt(), border_color),
            );

            // Node label with background for better readability
            if self.show_labels && self.zoom > 0.3 {
                if let Some(name) = node.data().file().file_stem().and_then(|s| s.to_str()) {
                    let font_size = (12.0 * self.zoom).clamp(8.0, 16.0);

                    // Measure text to create appropriate background
                    let font_id = egui::FontId::proportional(font_size);
                    let text_galley = painter.layout_no_wrap(
                        name.to_string(),
                        font_id.clone(),
                        egui::Color32::WHITE,
                    );
                    let text_rect = egui::Rect::from_center_size(
                        screen_pos,
                        text_galley.size() + egui::vec2(6.0, 4.0) * self.zoom,
                    );

                    // Only draw background if text is wider than the node
                    if text_galley.size().x > node_radius * 1.5 {
                        painter.rect_filled(
                            text_rect,
                            4.0 * self.zoom,
                            egui::Color32::from_black_alpha(180),
                        );
                        painter.rect_stroke(
                            text_rect,
                            4.0 * self.zoom,
                            egui::Stroke::new(1.0 * self.zoom, egui::Color32::from_gray(100)),
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
    }

    fn handle_graph_interaction(&mut self, ui: &mut egui::Ui, canvas_rect: egui::Rect) {
        let canvas_center = canvas_rect.center().to_vec2();

        if let Some(mouse_pos) = ui.ctx().pointer_interact_pos() {
            if canvas_rect.contains(mouse_pos) {
                let world_mouse = self.screen_to_world(mouse_pos.to_vec2(), canvas_center);

                // Find hovered node
                self.hovered_node = None;
                for (i, _) in self.graph_nodes.iter().enumerate() {
                    let dist = (world_mouse - self.node_positions[i]).length();
                    let node_radius = self.graph_nodes[i].calculate_size(
                        self.min_loc,
                        self.max_loc,
                        self.min_node_radius,
                        self.max_node_radius,
                    );
                    if dist < node_radius {
                        self.hovered_node = Some(i);
                        break;
                    }
                }

                // Handle mouse input
                let mouse_input = ui.input(|i| {
                    (
                        i.pointer.primary_clicked(),
                        i.pointer.primary_down(),
                        i.pointer.primary_released(),
                    )
                });

                match mouse_input {
                    (true, _, _) => {
                        // Click
                        // Always stop any current dragging first
                        self.dragging_node = None;
                        self.panning = false;
                        self.drag_start_pos = Some(mouse_pos);

                        if let Some(hovered) = self.hovered_node {
                            self.selected_node = if self.selected_node == Some(hovered) {
                                None // Deselect if clicking same node
                            } else {
                                Some(hovered) // Select new node
                            };
                            // Only start dragging if we're clicking the same node that's already selected
                            if self.selected_node == Some(hovered) {
                                self.dragging_node = Some(hovered);
                            }
                        } else {
                            self.selected_node = None;
                            self.panning = true;
                        }
                    }
                    (_, true, _) => {
                        // Drag
                        // Only process drag if mouse has moved significantly from start position
                        let should_drag = if let (Some(start_pos), Some(_)) =
                            (self.drag_start_pos, self.last_mouse_pos)
                        {
                            (mouse_pos - start_pos).length() > 5.0 // 5 pixel threshold
                        } else {
                            false
                        };

                        if should_drag {
                            if let Some(drag_idx) = self.dragging_node {
                                self.node_positions[drag_idx] = world_mouse;
                            } else if self.panning {
                                if let Some(last_pos) = self.last_mouse_pos {
                                    let delta = (mouse_pos - last_pos) / self.zoom;
                                    self.camera_pos -= delta;
                                }
                            }
                        }
                    }
                    (_, _, true) => {
                        // Release
                        self.dragging_node = None;
                        self.panning = false;
                        self.drag_start_pos = None;
                    }
                    _ => {}
                }

                self.last_mouse_pos = Some(mouse_pos);

                // Zoom with scroll
                let scroll = ui.input(|i| i.raw_scroll_delta.y);
                if scroll != 0.0 {
                    let zoom_factor = 1.0 + scroll * 0.001;
                    let new_zoom = (self.zoom * zoom_factor).clamp(0.1, 5.0);

                    // Zoom towards mouse position
                    let mouse_world_before =
                        self.screen_to_world(mouse_pos.to_vec2(), canvas_center);
                    self.zoom = new_zoom;
                    let mouse_world_after =
                        self.screen_to_world(mouse_pos.to_vec2(), canvas_center);
                    self.camera_pos += mouse_world_before - mouse_world_after;
                }
            }
        }
    }
}

impl eframe::App for SeiriGraph {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Controls panel
        egui::TopBottomPanel::top("controls").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Project Structure Graph");
                ui.separator();

                ui.checkbox(&mut self.auto_layout, "Auto Layout");
                if ui.button("Reset Layout").clicked() {
                    self.initialize_positions();
                    self.layout_iterations = 0;
                }

                ui.separator();
                ui.checkbox(&mut self.show_labels, "Show Labels");
                ui.checkbox(&mut self.show_dependencies, "Show Dependencies");

                ui.separator();
                ui.add(
                    egui::Slider::new(&mut self.layout_strength, 0.1..=2.0).text("Layout Force"),
                );

                ui.separator();
                ui.label(format!(
                    "Nodes: {} | Zoom: {:.1}x",
                    self.graph_nodes.len(),
                    self.zoom
                ));
            });
        });

        // Details panel for selected node
        if let Some(selected_idx) = self.selected_node {
            egui::SidePanel::right("details")
                .resizable(true)
                .default_width(350.0)
                .show(ctx, |ui| {
                    ui.heading("Node Details");
                    let node = &self.graph_nodes[selected_idx].data();

                    ui.group(|ui| {
                        ui.strong("File Information");
                        ui.label(format!("📁 {}", node.file().display()));
                        ui.label(format!("🔧 {:?}", node.language()));
                        ui.label(format!("📊 {} lines", node.loc()));
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
                });
        }

        // Main graph view
        egui::CentralPanel::default().show(ctx, |ui| {
            let canvas_rect = ui.max_rect();

            if self.graph_nodes.is_empty() {
                ui.centered_and_justified(|ui| {
                    ui.heading("No files found to display");
                    ui.label(
                        "Try running the analyzer on a project directory with supported files.",
                    );
                });
                return;
            }

            self.apply_force_layout();
            self.handle_graph_interaction(ui, canvas_rect);
            self.draw_graph(ui, canvas_rect);

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
        });

        if self.auto_layout && self.layout_iterations < 500 {
            ctx.request_repaint();
        }
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
