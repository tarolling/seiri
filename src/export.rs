use crate::core::defs::{GraphNode, Language};
use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use svg::Document;
use svg::node::element::{Circle, Line, Text, Title};

const CANVAS_WIDTH: f32 = 1200.0;
const CANVAS_HEIGHT: f32 = 900.0;
const MIN_NODE_RADIUS: f32 = 20.0;
const MAX_NODE_RADIUS: f32 = 40.0;
const MARGIN: f32 = 50.0;

pub fn export_graph_as_svg(graph_nodes: &[GraphNode], output_path: &Path) -> Result<(), String> {
    if graph_nodes.is_empty() {
        return Ok(());
    }

    // Calculate layout (similar to GUI layout)
    let radius = (CANVAS_HEIGHT - 2.0 * MARGIN).min(CANVAS_WIDTH - 2.0 * MARGIN) * 0.4;
    let center_x = CANVAS_WIDTH / 2.0;
    let center_y = CANVAS_HEIGHT / 2.0;
    let n = graph_nodes.len();

    // Calculate min/max LOC for node size normalization
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

    // Calculate node positions
    let mut positions = HashMap::new();
    for (i, node) in graph_nodes.iter().enumerate() {
        let angle = (i as f32) * (2.0 * std::f32::consts::PI / n as f32);
        let x = center_x + radius * angle.cos();
        let y = center_y + radius * angle.sin();
        positions.insert(node.data().file(), (x, y));
    }

    // Create SVG document
    let mut document = Document::new()
        .set("width", CANVAS_WIDTH)
        .set("height", CANVAS_HEIGHT)
        .set("viewBox", (0, 0, CANVAS_WIDTH as i32, CANVAS_HEIGHT as i32))
        .set("style", "background-color: white");

    // Add edges first (so they appear under nodes)
    for node in graph_nodes {
        let (start_x, start_y) = positions.get(node.data().file()).unwrap();

        for edge in node.edges() {
            if let Some((end_x, end_y)) = positions.get(edge) {
                let edge = Line::new()
                    .set("x1", *start_x)
                    .set("y1", *start_y)
                    .set("x2", *end_x)
                    .set("y2", *end_y)
                    .set("stroke", "lightblue")
                    .set("stroke-width", 2);
                document = document.add(edge);
            }
        }
    }

    // Add nodes with labels
    for node in graph_nodes {
        let (x, y) = positions.get(node.data().file()).unwrap();
        let radius = node.calculate_size(min_loc, max_loc, MIN_NODE_RADIUS, MAX_NODE_RADIUS);

        // Node circle
        let circle = Circle::new()
            .set("cx", *x)
            .set("cy", *y)
            .set("r", radius)
            .set("fill", node.data().language().color())
            .set("stroke", "black")
            .set("stroke-width", 2);

        // Add title for hover tooltip
        let title = Title::new(node.data().file().file_name().unwrap().to_str().unwrap());
        let circle_with_title = circle.add(title);
        document = document.add(circle_with_title);

        // Node label
        if let Some(name) = node.data().file().file_stem()
            && let Some(name_str) = name.to_str()
        {
            let label = Text::new(name_str)
                .set("x", *x)
                .set("y", *y)
                .set("text-anchor", "middle")
                .set("dominant-baseline", "middle")
                .set("font-family", "Arial")
                .set("font-size", 12)
                .set("fill", "black");
            document = document.add(label);
        }
    }

    // Add legend
    let legend_y = MARGIN;
    let legend_x = MARGIN;
    let legend_spacing = 25.0;

    for (i, lang) in [Language::Python, Language::Rust, Language::TypeScript]
        .iter()
        .enumerate()
    {
        let y = legend_y + (i as f32 * legend_spacing);

        // Legend dot
        let dot = Circle::new()
            .set("cx", legend_x)
            .set("cy", y)
            .set("r", 6)
            .set("fill", lang.color())
            .set("stroke", "black")
            .set("stroke-width", 1);
        document = document.add(dot);

        // Legend text
        let text = Text::new(lang.to_string())
            .set("x", legend_x + 15.0)
            .set("y", y)
            .set("dominant-baseline", "middle")
            .set("font-family", "Arial")
            .set("font-size", 12);
        document = document.add(text);
    }

    // Save to file
    let mut file = File::create(output_path).map_err(|e| e.to_string())?;
    file.write_all(document.to_string().as_bytes())
        .map_err(|e| e.to_string())?;

    Ok(())
}
