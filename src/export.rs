use crate::core::defs::{GraphNode, Language};
use fontdue::{Font, FontSettings};
use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use svg::Document;
use svg::node::element::{Circle, Line, Text, Title};
use tiny_skia::*;

const CANVAS_WIDTH: f32 = 1200.0;
const CANVAS_HEIGHT: f32 = 900.0;
const MIN_NODE_RADIUS: f32 = 20.0;
const MAX_NODE_RADIUS: f32 = 40.0;
const MARGIN: f32 = 50.0;

static FONT: &[u8] = include_bytes!("../font/arial.ttf");

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

pub fn export_graph_as_png(graph_nodes: &[GraphNode], output_path: &Path) -> Result<(), String> {
    if graph_nodes.is_empty() {
        return Ok(());
    }

    // Layout math (unchanged from SVG version)
    let radius = (CANVAS_HEIGHT - 2.0 * MARGIN).min(CANVAS_WIDTH - 2.0 * MARGIN) * 0.4;
    let center_x = CANVAS_WIDTH / 2.0;
    let center_y = CANVAS_HEIGHT / 2.0;
    let n = graph_nodes.len();

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

    let mut positions = std::collections::HashMap::new();
    for (i, node) in graph_nodes.iter().enumerate() {
        let angle = (i as f32) * (2.0 * std::f32::consts::PI / n as f32);
        let x = center_x + radius * angle.cos();
        let y = center_y + radius * angle.sin();
        positions.insert(node.data().file(), (x, y));
    }

    // Create pixmap (white background)
    let mut pixmap =
        Pixmap::new(CANVAS_WIDTH as u32, CANVAS_HEIGHT as u32).ok_or("Failed to create pixmap")?;
    pixmap.fill(Color::WHITE);

    // Stroke paint for edges
    let mut edge_paint = Paint::default();
    edge_paint.set_color(Color::from_rgba8(173, 216, 230, 255));
    edge_paint.anti_alias = true;

    let stroke = Stroke {
        width: 2.0,
        ..Default::default()
    };

    // Draw edges
    for node in graph_nodes {
        let (sx, sy) = positions[node.data().file()];
        for edge in node.edges() {
            if let Some(&(ex, ey)) = positions.get(edge) {
                let mut pb = PathBuilder::new();
                pb.move_to(sx, sy);
                pb.line_to(ex, ey);
                let path = pb.finish().unwrap();
                pixmap.stroke_path(&path, &edge_paint, &stroke, Transform::identity(), None);
            }
        }
    }

    // Draw nodes
    for node in graph_nodes {
        let (x, y) = positions[node.data().file()];
        let node_radius = node.calculate_size(min_loc, max_loc, MIN_NODE_RADIUS, MAX_NODE_RADIUS);

        // Circle fill
        let mut fill_paint = Paint::default();
        fill_paint.set_color(node.data().language().color_rgba());
        fill_paint.anti_alias = true;

        let circle_path = PathBuilder::from_circle(x, y, node_radius).unwrap();
        pixmap.fill_path(
            &circle_path,
            &fill_paint,
            FillRule::Winding,
            Transform::identity(),
            None,
        );

        // Circle stroke
        let mut stroke_paint = Paint::default();
        stroke_paint.set_color(Color::BLACK);
        pixmap.stroke_path(
            &circle_path,
            &stroke_paint,
            &Stroke {
                width: 2.0,
                ..Default::default()
            },
            Transform::identity(),
            None,
        );

        // Node label
        if let Some(name) = node.data().file().file_stem().and_then(|s| s.to_str()) {
            draw_text(&mut pixmap, name, x, y, 12.0, false);
        }
    }

    let legend_x = MARGIN;
    let legend_y = MARGIN;
    let legend_spacing = 25.0;

    for (i, lang) in [Language::Python, Language::Rust, Language::TypeScript]
        .iter()
        .enumerate()
    {
        let y = legend_y + (i as f32 * legend_spacing);

        // Legend dot
        let dot_path = PathBuilder::from_circle(legend_x, y, 6.0).unwrap();
        let mut dot_paint = Paint::default();
        dot_paint.set_color(lang.color_rgba());
        pixmap.fill_path(
            &dot_path,
            &dot_paint,
            FillRule::Winding,
            Transform::identity(),
            None,
        );

        // Dot border
        let mut border_paint = Paint::default();
        border_paint.set_color(Color::BLACK);
        pixmap.stroke_path(
            &dot_path,
            &border_paint,
            &Stroke {
                width: 1.0,
                ..Default::default()
            },
            Transform::identity(),
            None,
        );

        // Legend label
        draw_text(
            &mut pixmap,
            lang.to_string(),
            legend_x + 15.0,
            y,
            12.0,
            true,
        );
    }

    // Save PNG
    pixmap.save_png(output_path).map_err(|e| e.to_string())?;
    Ok(())
}

fn draw_text(pixmap: &mut Pixmap, text: &str, x: f32, y: f32, size: f32, legends: bool) {
    let font = Font::from_bytes(FONT, FontSettings::default()).unwrap();

    let total_width: f32 = text
        .chars()
        .map(|ch| font.metrics(ch, size).advance_width)
        .sum();

    let max_height = text
        .chars()
        .map(|ch| font.metrics(ch, size).height)
        .max()
        .unwrap_or(0) as f32;

    // Try to center the text
    let mut cursor_x = x - total_width / 2.0;
    let mut baseline_y = y + max_height / 2.0;

    // No centering needed for legends
    if legends {
        cursor_x = x - 5.0;
        baseline_y = y + 5.0;
    }

    for ch in text.chars() {
        let (metrics, bitmap) = font.rasterize(ch, size);

        // Position for each glyph
        let draw_x = cursor_x + metrics.xmin as f32;
        let draw_y = baseline_y - (metrics.height as f32 + metrics.ymin as f32);

        for (i, alpha) in bitmap.iter().enumerate() {
            let px = (i % metrics.width) as f32;
            let py = (i / metrics.width) as f32;
            if *alpha > 0 {
                let color = Color::from_rgba(0.0, 0.0, 0.0, *alpha as f32 / 255.0).unwrap();
                pixmap.fill_rect(
                    Rect::from_xywh(draw_x + px, draw_y + py, 1.0, 1.0).unwrap(),
                    &Paint {
                        shader: Shader::SolidColor(color),
                        ..Default::default()
                    },
                    Transform::identity(),
                    None,
                );
            }
        }

        cursor_x += metrics.advance_width;
    }
}
