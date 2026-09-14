use clap::{Parser, crate_name, crate_version};
use indicatif::{ProgressBar, ProgressStyle};
use seiri_cli::core::defs::{FileNode, Language};
use seiri_cli::core::resolvers::GraphBuilder;
use seiri_cli::discovery::{detect_project_languages, walk_directory};
use seiri_cli::export;
use seiri_cli::gui::run_gui;
use seiri_cli::parsers;
use seiri_cli::update;
use std::collections::HashMap;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

/// Projects at or below this many files parse faster than a progress bar is
/// worth drawing.
const PROGRESS_BAR_MIN_FILES: usize = 10;

#[derive(Parser)]
struct Cli {
    /// Path to the project directory or file to parse
    project_path: Option<PathBuf>,
    /// Name of desired output file
    #[arg(value_name = "gui | *.png | *.svg | *.jpg | *.jpeg")]
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
    /// Overwrite the output file without prompting if it already exists
    #[arg(short, long)]
    force: bool,
    /// Update the binary to the latest GitHub release
    #[arg(long)]
    update: bool,
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
    let already_updating = args.update;

    match run(args) {
        Ok(_) => {
            if verbose {
                println!("Operation completed successfully.");
            }
            if !already_updating {
                update::notify_if_update_available();
            }
        }
        Err(msg) => {
            eprintln!("Error: {msg}");
            std::process::exit(1);
        }
    }
}

/// Parses every detected file in parallel, returning the successfully parsed
/// files indexed by path.
///
/// A progress bar is shown unless `verbose` is set or the project is too small
/// for one to be worth drawing; `verbose` instead prints the sorted list of
/// parsed files once the parse has finished, since printing from worker threads
/// would interleave lines mid-write.
fn parse_project_files(
    language_files: &HashMap<PathBuf, Language>,
    verbose: bool,
) -> HashMap<PathBuf, FileNode> {
    let progress = if !verbose && language_files.len() > PROGRESS_BAR_MIN_FILES {
        let bar = ProgressBar::new(language_files.len() as u64);
        bar.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} [{bar:40.cyan/blue}] {pos}/{len} {elapsed}")
                .expect("progress bar template is valid"),
        );
        bar
    } else {
        // A hidden bar keeps this branch-free: advancing it is a no-op.
        ProgressBar::hidden()
    };

    let node_map = parsers::parse_all_parallel(language_files, || progress.inc(1));
    progress.finish_with_message("parsing complete");

    if verbose {
        let mut parsed_paths: Vec<&PathBuf> = node_map.keys().collect();
        parsed_paths.sort();
        for path in parsed_paths {
            let language = language_files
                .get(path)
                .map(Language::to_string)
                .unwrap_or("unknown");
            eprintln!("Parsed {language} file: {}", path.display());
        }
    }

    node_map
}

fn run(args: Cli) -> Result<(), String> {
    let Cli {
        project_path: provided_path,
        output_filename: output,
        verbose,
        version,
        no_gitignore,
        force,
        update,
    } = args;

    if version {
        println!("{} | version {}", crate_name!(), crate_version!());
        return Ok(());
    }

    if update {
        return update::run_self_update(verbose);
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
    let node_map = parse_project_files(&language_files, verbose);

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
                run_gui(graph_nodes).map_err(|e| format!("Failed to launch GUI: {e}"))?;
                return Ok(());
            }
            filename if filename.ends_with(".svg") => {
                confirm_overwrite(Path::new(filename), force, &mut io::stdin().lock())?;
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
                confirm_overwrite(Path::new(filename), force, &mut io::stdin().lock())?;
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
            filename if filename.ends_with(".jpg") || filename.ends_with(".jpeg") => {
                confirm_overwrite(Path::new(filename), force, &mut io::stdin().lock())?;
                if verbose {
                    println!("Exporting graph to JPEG: {filename}");
                }
                export::export_graph_as_jpeg(
                    &graph_nodes,
                    &PathBuf::from(filename),
                    detected_languages,
                )
                .map_err(|e| format!("Failed to export JPEG: {e}"))?;
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
            run_gui(graph_nodes).map_err(|e| format!("Failed to launch GUI: {e}"))?;
        }
        return Ok(());
    }

    Ok(())
}

/// Checks whether `path` already exists and, if so, either rejects the export outright
/// (when `force` is true, overwriting is allowed unconditionally) or asks the user to
/// confirm the overwrite via `reader`. Returns an error if the user declines or `force`
/// is not set and confirmation is not given.
fn confirm_overwrite<R: BufRead>(path: &Path, force: bool, reader: &mut R) -> Result<(), String> {
    if force || !path.exists() {
        return Ok(());
    }

    eprint!(
        "Warning: output file {} already exists. Overwrite? [y/N]: ",
        path.display()
    );
    io::stderr().flush().map_err(|e| e.to_string())?;

    let mut input = String::new();
    reader
        .read_line(&mut input)
        .map_err(|e| format!("Failed to read confirmation: {e}"))?;

    match input.trim().to_lowercase().as_str() {
        "y" | "yes" => Ok(()),
        _ => Err(format!(
            "Aborted: not overwriting existing file {} (use --force to skip this prompt)",
            path.display()
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use seiri_cli::layout::{self, Layout};
    use seiri_cli::parsers::cpp::parse_cpp_file;
    use std::fs;
    use std::fs::File;
    use std::io::Cursor;
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
            force: false,
            update: false,
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
            force: false,
            update: false,
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
            force: false,
            update: false,
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
            force: false,
            update: false,
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
            force: false,
            update: false,
        };

        let result = run(args);

        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_project_files_parses_every_supported_file() {
        let temp_dir = TempDir::new().unwrap();
        let rust_file = temp_dir.path().join("lib.rs");
        File::create(&rust_file).unwrap();
        let rust_contents = "pub fn helper() -> u32 { 42 }\n";
        fs::write(&rust_file, rust_contents).unwrap();

        let python_file = temp_dir.path().join("script.py");
        fs::write(&python_file, "def main():\n    return 0\n").unwrap();

        let mut language_files: HashMap<PathBuf, Language> = HashMap::new();
        let files_to_process = walk_directory(temp_dir.path(), true);
        detect_project_languages(&files_to_process, &mut language_files);

        let parsed = parse_project_files(&language_files, false);

        assert_eq!(parsed.len(), language_files.len());
        // LOC is counted as newlines plus one, since the last line may be unterminated.
        assert_eq!(
            parsed.get(&rust_file).map(|node| node.loc()),
            Some(rust_contents.matches('\n').count() as u32 + 1)
        );
    }

    #[test]
    fn test_update_flag_parses_without_project_path() {
        let args = Cli::try_parse_from(["seiri", "--update"]).unwrap();

        assert!(args.update);
        assert!(args.project_path.is_none());
        assert!(args.validate().is_ok());
    }

    #[test]
    fn test_force_flag_parses() {
        let args = Cli::try_parse_from(["seiri", "--force"]).unwrap();
        assert!(args.force);

        let args = Cli::try_parse_from(["seiri", "-f"]).unwrap();
        assert!(args.force);

        let args = Cli::try_parse_from(["seiri"]).unwrap();
        assert!(!args.force);
    }

    #[test]
    fn test_confirm_overwrite_allows_nonexistent_path() {
        let temp_dir = TempDir::new().unwrap();
        let output_path = temp_dir.path().join("does_not_exist.svg");
        let mut reader = Cursor::new(Vec::new());

        assert!(confirm_overwrite(&output_path, false, &mut reader).is_ok());
    }

    #[test]
    fn test_confirm_overwrite_force_skips_prompt() {
        let temp_dir = TempDir::new().unwrap();
        let output_path = temp_dir.path().join("existing.svg");
        File::create(&output_path).unwrap();
        // empty reader: if force didn't short-circuit, read_line would return an empty
        // string, which is treated as "no" and would cause an error
        let mut reader = Cursor::new(Vec::new());

        assert!(confirm_overwrite(&output_path, true, &mut reader).is_ok());
    }

    #[test]
    fn test_confirm_overwrite_prompt_accepts_yes() {
        let temp_dir = TempDir::new().unwrap();
        let output_path = temp_dir.path().join("existing.svg");
        File::create(&output_path).unwrap();
        let mut reader = Cursor::new(b"y\n".to_vec());

        assert!(confirm_overwrite(&output_path, false, &mut reader).is_ok());
    }

    #[test]
    fn test_confirm_overwrite_prompt_rejects_no() {
        let temp_dir = TempDir::new().unwrap();
        let output_path = temp_dir.path().join("existing.svg");
        File::create(&output_path).unwrap();
        let mut reader = Cursor::new(b"n\n".to_vec());

        let result = confirm_overwrite(&output_path, false, &mut reader);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Aborted"));
    }

    #[test]
    fn test_confirm_overwrite_prompt_defaults_to_no() {
        let temp_dir = TempDir::new().unwrap();
        let output_path = temp_dir.path().join("existing.svg");
        File::create(&output_path).unwrap();
        let mut reader = Cursor::new(b"\n".to_vec());

        assert!(confirm_overwrite(&output_path, false, &mut reader).is_err());
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
        for (x, y) in positions_sugiyama.values() {
            assert!(
                x.is_finite() && y.is_finite(),
                "Sugiyama layout position should have finite coordinates: ({}, {})",
                x,
                y
            );
        }

        for (x, y) in positions_circular.values() {
            assert!(
                x.is_finite() && y.is_finite(),
                "Circular layout position should have finite coordinates: ({}, {})",
                x,
                y
            );
        }
    }

    /// Test T020: Verify C++ Export (SVG, PNG, and JPEG)
    /// Tests that C++ graphs export correctly to SVG, PNG, and JPEG formats
    #[test]
    fn test_cpp_export_svg_and_png() {
        let temp_dir = TempDir::new().unwrap();
        let output_svg = temp_dir.path().join("test_output.svg");
        let output_png = temp_dir.path().join("test_output.png");
        let output_jpeg = temp_dir.path().join("test_output.jpg");

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
        let png_result =
            export::export_graph_as_png(&graph_nodes, &output_png, detected_languages.clone());
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

        // Test JPEG export
        let jpeg_result =
            export::export_graph_as_jpeg(&graph_nodes, &output_jpeg, detected_languages);
        assert!(
            jpeg_result.is_ok(),
            "JPEG export should succeed, got: {:?}",
            jpeg_result
        );
        assert!(output_jpeg.exists(), "JPEG output file should be created");

        // Verify JPEG file has content and starts with the JPEG SOI marker
        let jpeg_bytes = fs::read(&output_jpeg).expect("Should read JPEG file");
        assert!(
            jpeg_bytes.len() > 100,
            "JPEG file should have substantial size"
        );
        assert_eq!(
            &jpeg_bytes[0..2],
            &[0xFF, 0xD8],
            "JPEG file should start with the SOI marker"
        );
    }
}
