use crate::core::defs::GraphNode;
use eframe::egui;

pub struct GraphApp {
    pub graph_nodes: Vec<GraphNode>,
    pan: egui::Vec2,
    zoom: f32,
    dragging: bool,
    last_drag_pos: Option<egui::Pos2>,
    selected: Option<usize>,
}

impl eframe::App for GraphApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Show details panel if a node is selected
        if let Some(selected_idx) = self.selected {
            egui::SidePanel::right("details_panel").show(ctx, |ui| {
                ui.heading("Node Details");
                let node = &self.graph_nodes[selected_idx].node;
                
                // Basic file info
                ui.label(format!("File: {}", node.file.display()));
                ui.label(format!("Language: {:?}", node.language));
                ui.separator();

                // Dependencies
                let incoming = self.graph_nodes.iter()
                    .filter(|n| n.edges.contains(&node.file))
                    .count();
                let outgoing = self.graph_nodes[selected_idx].edges.len();
                ui.label(format!("Incoming dependencies: {}", incoming));
                ui.label(format!("Outgoing dependencies: {}", outgoing));
                ui.label(format!("Total dependencies: {}", incoming + outgoing));
                ui.separator();

                // Graph metrics
                ui.label(format!("Degree centrality: {:.2}", (incoming + outgoing) as f32 / (self.graph_nodes.len() - 1) as f32));
                
                // Declarations
                if !node.functions.is_empty() {
                    ui.collapsing("Functions", |ui| {
                        for func in &node.functions {
                            ui.label(func);
                        }
                    });
                }
                if !node.containers.is_empty() {
                    ui.collapsing("Containers", |ui| {
                        for container in &node.containers {
                            ui.label(container);
                        }
                    });
                }

                // Imports classification
                if !node.imports.is_empty() {
                    ui.collapsing("Imports", |ui| {
                        let (local, external): (Vec<_>, Vec<_>) = node.imports
                            .iter()
                            .partition(|imp| imp.is_local);
                        
                        if !local.is_empty() {
                            ui.label("Local:");
                            for imp in local {
                                ui.label(format!("  {}", imp.path));
                            }
                        }
                        if !external.is_empty() {
                            ui.label("External:");
                            for imp in external {
                                ui.label(format!("  {}", imp.path));
                            }
                        }
                    });
                }
            });
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Project Structure Graph");
            let n = self.graph_nodes.len();
            if n == 0 {
                ui.label("No nodes to display.");
                return;
            }

            // Pan/zoom interaction
            let response = ui.allocate_response(egui::Vec2::new(800.0, 600.0), egui::Sense::drag());
            let painter = ui.painter_at(response.rect);

            // Zoom with scroll
            if response.hovered() {
                let scroll = ui.input(|i| i.raw_scroll_delta.y);
                if scroll != 0.0 {
                    let zoom_factor = (self.zoom * 0.1).max(0.1);
                    self.zoom = (self.zoom + scroll.signum() * zoom_factor).clamp(0.2, 5.0);
                }
            }

            // Pan with drag
            if response.dragged() {
                if let Some(pos) = ui.ctx().pointer_interact_pos() {
                    if let Some(last) = self.last_drag_pos {
                        self.pan += pos - last;
                    }
                    self.last_drag_pos = Some(pos);
                    self.dragging = true;
                }
            } else {
                self.dragging = false;
                self.last_drag_pos = None;
            }

            // Layout nodes in a circle
            let radius = 200.0 * self.zoom;
            let center = egui::Vec2::new(400.0, 300.0) + self.pan;
            let node_radius = 24.0 * self.zoom;
            let mut positions = Vec::with_capacity(n);
            for i in 0..n {
                let angle = i as f32 * (std::f32::consts::TAU / n as f32);
                let pos = center + egui::vec2(angle.cos(), angle.sin()) * radius;
                positions.push(pos);
            }

            // Node selection
            if response.clicked() {
                if let Some(mouse_pos) = ui.ctx().pointer_interact_pos() {
                    for (i, pos) in positions.iter().enumerate() {
                        let node_screen_pos = response.rect.left_top() + *pos;
                        let d = (mouse_pos - node_screen_pos).length();
                        if d < node_radius {
                            self.selected = Some(i);
                            break;
                        }
                    }
                }
            }

            // Draw edges as lines
            for (i, node) in self.graph_nodes.iter().enumerate() {
                for edge in &node.edges {
                    if let Some(j) = self.graph_nodes.iter().position(|n| &n.node.file == edge) {
                        let p1 = response.rect.left_top() + positions[i];
                        let p2 = response.rect.left_top() + positions[j];
                        painter.line_segment([
                            p1,
                            p2
                        ], egui::Stroke::new(2.0, egui::Color32::LIGHT_BLUE));
                    }
                }
            }

            // Draw nodes as circles
            for (i, node) in self.graph_nodes.iter().enumerate() {
                let pos = response.rect.left_top() + positions[i];
                let color = if Some(i) == self.selected {
                    egui::Color32::YELLOW
                } else {
                    egui::Color32::from_rgb(120, 180, 255)
                };
                painter.circle_filled(pos, node_radius, color);
                painter.circle_stroke(pos, node_radius, egui::Stroke::new(2.0, egui::Color32::BLACK));
                // Draw file name (just the file stem)
                if let Some(stem) = node.node.file.file_stem().and_then(|s| s.to_str()) {
                    painter.text(
                        pos,
                        egui::Align2::CENTER_CENTER,
                        stem,
                        egui::FontId::proportional(16.0 * self.zoom),
                        egui::Color32::BLACK,
                    );
                }
            }
        });
    }
}

pub fn run_gui(graph_nodes: Vec<GraphNode>) {
    let app = GraphApp {
        graph_nodes,
        pan: egui::Vec2::ZERO,
        zoom: 1.0,
        dragging: false,
        last_drag_pos: None,
        selected: None,
    };
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([800.0, 600.0])
            .with_resizable(true),
        ..Default::default()
    };
    let _ = eframe::run_native(
        "Project Structure Graph",
        native_options,
        Box::new(|_cc| Ok(Box::new(app))),
    );
}
