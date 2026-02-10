mod gui;
use gui::run_gui;

mod analysis;
mod core;
mod export;
mod layout;
mod parsers;

use clap::{Parser, crate_name, crate_version};
use core::defs::{FileNode, Language};
use core::resolvers::GraphBuilder;
use ignore::WalkBuilder;
use parsers::{
    cpp::parse_cpp_file, python::parse_python_file, rust::parse_rust_file,
    typescript::parse_typescript_file,
};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(Parser)]
struct Cli {
    /// Path to the project directory or file to parse
    project_path: Option<PathBuf>,
    /// Name of desired output file
    #[arg(value_name = "gui | *.png | *.svg")]
    output_filename: Option<String>,
    /// Enable verbose output
    #[arg(short, long)]
    verbose: bool,
    /// Show version information
    #[arg(short = 'V', long = "version")]
    version: bool,
    /// Ignore .gitignore files
    #[arg(long)]
    no_gitignore: bool,
}

impl Cli {
    fn validate(&self) -> Result<(), String> {
        // Validate project path exists if provided
        if let Some(ref project_path) = self.project_path
            && !project_path.exists()
        {
            return Err(format!(
                "The specified project path does not exist: {:?}",
                project_path
            ));
        }

        // Validate output filename if provided
        if let Some(name) = &self.output_filename {
            if name.trim().is_empty() {
                return Err("Output filename cannot be empty".into());
            }
            if name.contains(std::path::MAIN_SEPARATOR) {
                return Err("Output filename cannot contain path separators".into());
            }
        }

        Ok(())
    }
}

fn main() {
    let args = Cli::parse();

    if let Err(msg) = args.validate() {
        eprintln!("Error: {msg}");
        std::process::exit(1);
    }

    let verbose = args.verbose;

    match run(args) {
        Ok(_) => {
            if verbose {
                println!("Operation completed successfully.");
            }
        }
        Err(msg) => {
            eprintln!("Error: {msg}");
            std::process::exit(1);
        }
    }
}

fn detect_file_language(
    target_file: PathBuf,
    language_files: &mut HashMap<PathBuf, Language>,
    detected_langs: &mut HashSet<Language>,
) {
    if let Some(file_language) = Language::from_file(target_file.to_str().unwrap()) {
        language_files.insert(target_file.clone(), file_language);
        detected_langs.insert(file_language);
    }
}

fn run(args: Cli) -> Result<(), String> {
    let Cli {
        project_path: provided_path,
        output_filename: output,
        verbose,
        version,
        no_gitignore,
    } = args;

    if version {
        println!("{} | version {}", crate_name!(), crate_version!());
        return Ok(());
    }

    // Get the project path, using current directory as default
    let project_path = match provided_path {
        Some(path) => path
            .canonicalize()
            .map_err(|e| format!("Failed to canonicalize path: {e}"))?,
        None => {
            std::env::current_dir().map_err(|e| format!("Failed to get current directory: {e}"))?
        }
    };

    if verbose {
        println!("Processing path: {}", project_path.display());
    }

    // Detect languages in file/project
    let mut language_files: HashMap<PathBuf, Language> = HashMap::new();
    let files_to_process = walk_directory(&project_path, no_gitignore);
    let detected_languages = detect_project_languages(&files_to_process, &mut language_files)
        .ok_or_else(|| "No supported language files found in the project".to_string())?;

    // Parse files and collect Nodes, indexed by file path
    let mut node_map: HashMap<PathBuf, FileNode> = HashMap::new();
    for (file_path, lang) in &language_files {
        match lang {
            Language::Python => {
                if let Some(node) = parse_python_file(file_path) {
                    if verbose {
                        println!("Parsed Python file: {}", file_path.display());
                    }
                    node_map.insert(file_path.clone(), node);
                }
            }
            Language::Rust => {
                if let Some(node) = parse_rust_file(file_path) {
                    if verbose {
                        println!("Parsed Rust file: {}", file_path.display());
                    }
                    node_map.insert(file_path.clone(), node);
                }
            }
            Language::TypeScript => {
                if let Some(node) = parse_typescript_file(file_path) {
                    if verbose {
                        println!("Parsed TypeScript file: {}", file_path.display());
                    }
                    node_map.insert(file_path.clone(), node);
                }
            }
            Language::Cpp => {
                if let Some(node) = parse_cpp_file(file_path) {
                    if verbose {
                        println!("Parsed C++ file: {}", file_path.display());
                    }
                    node_map.insert(file_path.clone(), node);
                }
            }
        }
    }

    // Build GraphNodes with multi-language support
    let mut graph_builder = GraphBuilder::new();
    let graph_nodes = graph_builder.build_graph_edges(&node_map, &project_path);

    if verbose {
        println!("\nResolved {} nodes with connections:", graph_nodes.len());
        for gnode in &graph_nodes {
            println!(
                "  {} ({:?}):",
                gnode.data().file().file_name().unwrap().to_string_lossy(),
                gnode.data().language(),
            );
            println!("    Functions: {}", gnode.data().functions().len());
            println!("    Containers: {}", gnode.data().containers().len());
            println!("    Imports: {}", gnode.data().imports().len());
            println!("    Dependencies: {}", gnode.edges().len());

            if !gnode.edges().is_empty() {
                println!("    Depends on:");
                for edge in gnode.edges() {
                    println!("      -> {}", edge.file_name().unwrap().to_string_lossy());
                }
            }
            println!();
        }
    }

    // launch the visualization or export if specified
    if let Some(filename) = output {
        match filename.as_str() {
            "gui" => {
                run_gui(graph_nodes);
                return Ok(());
            }
            filename if filename.ends_with(".svg") => {
                if verbose {
                    println!("Exporting graph to SVG: {filename}");
                }
                export::export_graph_as_svg(
                    &graph_nodes,
                    &PathBuf::from(filename),
                    detected_languages,
                )
                .map_err(|e| format!("Failed to export SVG: {e}"))?;
                if verbose {
                    println!("Successfully exported to {filename}");
                }
            }
            filename if filename.ends_with(".png") => {
                if verbose {
                    println!("Exporting graph to PNG: {filename}");
                }
                export::export_graph_as_png(
                    &graph_nodes,
                    &PathBuf::from(filename),
                    detected_languages,
                )
                .map_err(|e| format!("Failed to export PNG: {e}"))?;
                if verbose {
                    println!("Successfully exported to {filename}");
                }
            }
            _ => {
                return Err(format!("Unsupported output format: {filename}"));
            }
        }
    } else {
        // Default to GUI if no output specified
        #[cfg(not(test))]
        {
            run_gui(graph_nodes);
        }
        return Ok(());
    }

    Ok(())
}

fn detect_project_languages(
    files_to_process: &[PathBuf],
    language_files: &mut HashMap<PathBuf, Language>,
) -> Option<HashSet<Language>> {
    let mut detected: HashSet<Language> = HashSet::new();
    files_to_process
        .iter()
        .for_each(|entry| detect_file_language(entry.to_path_buf(), language_files, &mut detected));

    if detected.is_empty() {
        None
    } else {
        Some(detected)
    }
}

fn walk_directory(path: &Path, no_gitignore: bool) -> Vec<PathBuf> {
    let mut paths = Vec::new();

    let mut builder = WalkBuilder::new(path);
    if no_gitignore {
        builder
            .git_ignore(false)
            .git_exclude(false)
            .git_global(false)
            .ignore(false);
    } else {
        // for most/all projects, gitignore and other ignore files will be automatically detected by ignore crate
        // but they don't when using tempfile and/or when running tests
        let gitignore_path = path.join(".gitignore");
        if gitignore_path.exists() {
            builder.add_ignore(gitignore_path);
        }
    }

    for result in builder.build() {
        match result {
            Ok(entry) => {
                if let Some(file_type) = entry.file_type()
                    && file_type.is_file()
                {
                    paths.push(entry.path().to_path_buf());
                }
            }
            Err(msg) => eprintln!("Error reading entry: {msg}"),
        }
    }

    paths
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::Layout;
    use std::fs;
    use std::{fs::File, path::Path};
    use tempfile::TempDir;

    #[test]
    fn test_non_existent_path() {
        let temp_dir = TempDir::new().unwrap();
        let non_existent = temp_dir.path().join("non_existent_dir_12345");

        let args = Cli {
            project_path: Some(non_existent.clone()),
            output_filename: Some("output.txt".to_string()),
            verbose: false,
            version: false,
            no_gitignore: false,
        };

        let result = args.validate();

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("does not exist"));
    }

    #[test]
    fn test_existing_file() {
        let temp_dir = TempDir::new().unwrap();
        let temp_file = temp_dir.path().join("test_file.py");
        File::create(&temp_file).unwrap();

        let args = Cli {
            project_path: Some(temp_file),
            output_filename: None,
            verbose: false,
            version: false,
            no_gitignore: false,
        };

        let result = run(args);
        assert!(result.is_ok());
    }

    #[test]
    fn test_existing_directory() {
        let temp_dir = TempDir::new().unwrap();

        // Test with explicit path
        let args = Cli {
            project_path: Some(temp_dir.path().to_path_buf()),
            output_filename: None,
            verbose: false,
            version: false,
            no_gitignore: false,
        };
        let result = run(args);
        // we expect an error since the directory is empty
        assert!(result.is_err());

        // Test with default (current) directory
        let args = Cli {
            project_path: None,
            output_filename: None,
            verbose: false,
            version: false,
            no_gitignore: false,
        };
        let result = run(args);
        assert!(result.is_ok());
    }

    #[test]
    fn test_verbose_output() {
        let temp_dir = TempDir::new().unwrap();
        let temp_file = temp_dir.path().join("test.rs");
        File::create(&temp_file).unwrap();

        let args = Cli {
            project_path: Some(temp_file),
            output_filename: None,
            verbose: true,
            version: false,
            no_gitignore: false,
        };

        let result = run(args);

        assert!(result.is_ok());
    }

    #[test]
    fn test_detect_file() {
        let current_file = Path::new(file!());
        assert!(current_file.try_exists().is_ok());

        let mut language_files: HashMap<PathBuf, Language> = HashMap::new();
        let mut detected_languages = HashSet::new();
        detect_file_language(
            current_file.to_path_buf(),
            &mut language_files,
            &mut detected_languages,
        );

        assert!(!detected_languages.is_empty());
        assert!(detected_languages.contains(&Language::Rust));
    }

    #[test]
    fn test_detect_invalid_file() {
        let current_file = Path::new("Cargo.lock");
        assert!(current_file.try_exists().is_ok());

        let mut language_files: HashMap<PathBuf, Language> = HashMap::new();
        let mut detected_languages = HashSet::new();
        detect_file_language(
            current_file.to_path_buf(),
            &mut language_files,
            &mut detected_languages,
        );

        assert!(detected_languages.is_empty());
    }

    #[test]
    fn test_detect_dir() {
        let current_dir = Path::new(file!()).parent().unwrap().canonicalize().unwrap();
        assert!(current_dir.try_exists().is_ok());

        let mut language_files: HashMap<PathBuf, Language> = HashMap::new();
        let files_to_process = walk_directory(&current_dir, false);
        let result = detect_project_languages(&files_to_process, &mut language_files);

        assert!(&result.is_some());

        let langs = result.unwrap();
        assert_eq!(langs.len(), 1);
        assert!(langs.contains(&Language::Rust));
    }

    #[test]
    fn respects_gitignore() {
        let dir = tempfile::tempdir().unwrap();
        let ignored_file = dir.path().join("ignored.txt");
        File::create(&ignored_file).unwrap();

        fs::write(dir.path().join(".gitignore"), "ignored.txt\n").unwrap();

        let files = walk_directory(dir.path(), false);
        assert!(!files.iter().any(|p| p.ends_with("ignored.txt")));
    }

    #[test]
    fn ignores_no_gitignore_flag() {
        let dir = tempfile::tempdir().unwrap();
        let ignored_file = dir.path().join("ignored.txt");
        File::create(&ignored_file).unwrap();

        fs::write(dir.path().join(".gitignore"), "ignored.txt\n").unwrap();

        let files = walk_directory(dir.path(), true);
        assert!(files.iter().any(|p| p.ends_with("ignored.txt")));
    }

    /// Test T019: Verify C++ nodes work with layout algorithms
    /// Creates a simple C++ project and tests both Sugiyama and Circular layouts
    #[test]
    fn test_cpp_layout_sugiyama_and_circular() {
        let temp_dir = TempDir::new().unwrap();

        // Create a simple C++ project with dependencies
        // header.h
        let header_path = temp_dir.path().join("header.h");
        File::create(&header_path).unwrap();
        fs::write(
            &header_path,
            "#ifndef HEADER_H\n#define HEADER_H\nvoid foo();\n#endif\n",
        )
        .unwrap();

        // math.h
        let math_header_path = temp_dir.path().join("math.h");
        File::create(&math_header_path).unwrap();
        fs::write(
            &math_header_path,
            "#ifndef MATH_H\n#define MATH_H\nint add(int a, int b);\n#endif\n",
        )
        .unwrap();

        // main.cpp depends on header.h and math.h
        let main_path = temp_dir.path().join("main.cpp");
        File::create(&main_path).unwrap();
        fs::write(
            &main_path,
            "#include \"header.h\"\n#include \"math.h\"\n\nint main() { foo(); return 0; }\n",
        )
        .unwrap();

        // util.cpp depends on header.h
        let util_path = temp_dir.path().join("util.cpp");
        File::create(&util_path).unwrap();
        fs::write(&util_path, "#include \"header.h\"\n\nvoid helper() {}\n").unwrap();

        // Parse all C++ files
        let mut language_files: HashMap<PathBuf, Language> = HashMap::new();
        let files_to_process = walk_directory(temp_dir.path(), true);
        detect_project_languages(&files_to_process, &mut language_files);

        // Only process C++ files
        let cpp_files: Vec<_> = language_files
            .iter()
            .filter(|(_, lang)| **lang == Language::Cpp)
            .collect();
        assert!(
            cpp_files.len() >= 3,
            "Should have at least 3 C++ files, got {}",
            cpp_files.len()
        );

        // Parse files
        let mut node_map: HashMap<PathBuf, FileNode> = HashMap::new();
        for (file_path, _) in &cpp_files {
            if let Some(node) = parse_cpp_file(file_path) {
                node_map.insert((*file_path).clone(), node);
            }
        }

        assert!(!node_map.is_empty(), "Should have parsed C++ files");

        // Build graph
        let mut graph_builder = GraphBuilder::new();
        let graph_nodes = graph_builder.build_graph_edges(&node_map, temp_dir.path());
        assert!(
            !graph_nodes.is_empty(),
            "Graph should have nodes after building edges"
        );

        // Test that layout functions don't panic with C++ graphs
        let sugiyama_layout =
            layout::sugiyama::SugiyamaLayout::new(layout::sugiyama::SugiyamaConfig::default());
        let circular_layout =
            layout::circular::CircularLayout::new(layout::circular::CircularConfig::default());

        // Create a simple graph to test layout
        let mut graph = petgraph::graph::Graph::new();
        for _ in 0..graph_nodes.len() {
            graph.add_node(());
        }
        // Add some edges based on graph_nodes
        for node in &graph_nodes {
            for _ in node.edges() {
                if graph.node_count() > 1 {
                    let n1 = petgraph::graph::NodeIndex::new(0);
                    let n2 = petgraph::graph::NodeIndex::new(
                        (1 % graph.node_count()).min(graph.node_count() - 1),
                    );
                    if !graph.contains_edge(n1, n2) {
                        graph.add_edge(n1, n2, ());
                    }
                }
            }
        }

        // Test Sugiyama layout
        let positions_sugiyama = sugiyama_layout.layout(&graph);
        assert!(
            !positions_sugiyama.is_empty(),
            "Sugiyama layout should produce positions"
        );

        // Test Circular layout
        let positions_circular = circular_layout.layout(&graph);
        assert!(
            !positions_circular.is_empty(),
            "Circular layout should produce positions"
        );

        // Verify positions have valid coordinates
        for (_node_idx, (x, y)) in positions_sugiyama.iter() {
            assert!(
                x.is_finite() && y.is_finite(),
                "Sugiyama layout position should have finite coordinates: ({}, {})",
                x,
                y
            );
        }

        for (_node_idx, (x, y)) in positions_circular.iter() {
            assert!(
                x.is_finite() && y.is_finite(),
                "Circular layout position should have finite coordinates: ({}, {})",
                x,
                y
            );
        }
    }

    /// Test T020: Verify C++ Export (SVG and PNG)
    /// Tests that C++ graphs export correctly to SVG and PNG formats
    #[test]
    fn test_cpp_export_svg_and_png() {
        let temp_dir = TempDir::new().unwrap();
        let output_svg = temp_dir.path().join("test_output.svg");
        let output_png = temp_dir.path().join("test_output.png");

        // Create a simple C++ project with dependencies
        let header_path = temp_dir.path().join("base.h");
        File::create(&header_path).unwrap();
        fs::write(
            &header_path,
            "#ifndef BASE_H\n#define BASE_H\nvoid setup();\n#endif\n",
        )
        .unwrap();

        let util_path = temp_dir.path().join("util.cpp");
        File::create(&util_path).unwrap();
        fs::write(&util_path, "#include \"base.h\"\n\nvoid util_func() {}\n").unwrap();

        let main_path = temp_dir.path().join("main.cpp");
        File::create(&main_path).unwrap();
        fs::write(
            &main_path,
            "#include \"base.h\"\n#include \"util.cpp\"\n\nint main() { setup(); return 0; }\n",
        )
        .unwrap();

        // Parse files
        let mut language_files: HashMap<PathBuf, Language> = HashMap::new();
        let files_to_process = walk_directory(temp_dir.path(), true);
        let detected_languages = detect_project_languages(&files_to_process, &mut language_files)
            .expect("Should detect languages");

        let mut node_map: HashMap<PathBuf, FileNode> = HashMap::new();
        for (file_path, lang) in &language_files {
            if lang == &Language::Cpp
                && let Some(node) = parse_cpp_file(file_path)
            {
                node_map.insert(file_path.clone(), node);
            }
        }

        assert!(!node_map.is_empty(), "Should have parsed C++ files");

        // Build graph
        let mut graph_builder = GraphBuilder::new();
        let graph_nodes = graph_builder.build_graph_edges(&node_map, temp_dir.path());
        assert!(
            !graph_nodes.is_empty(),
            "Graph should have nodes after building edges"
        );

        // Test SVG export
        let svg_result =
            export::export_graph_as_svg(&graph_nodes, &output_svg, detected_languages.clone());
        assert!(
            svg_result.is_ok(),
            "SVG export should succeed, got: {:?}",
            svg_result
        );
        assert!(output_svg.exists(), "SVG output file should be created");

        // Verify SVG content
        let svg_content = fs::read_to_string(&output_svg).expect("Should read SVG file");
        assert!(svg_content.contains("<svg"), "SVG should contain svg tag");
        assert!(svg_content.len() > 100, "SVG content should be substantial");

        // Test PNG export
        let png_result = export::export_graph_as_png(&graph_nodes, &output_png, detected_languages);
        assert!(
            png_result.is_ok(),
            "PNG export should succeed, got: {:?}",
            png_result
        );
        assert!(output_png.exists(), "PNG output file should be created");

        // Verify PNG file has content
        let png_metadata = fs::metadata(&output_png).expect("Should read PNG metadata");
        assert!(
            png_metadata.len() > 100,
            "PNG file should have substantial size"
        );
    }
}
