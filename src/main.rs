mod gui;
use gui::run_gui;

mod core;
mod parsers;

use clap::Parser;
use core::defs::{FileNode, Language};
use core::resolvers::GraphBuilder;
use parsers::rust::parse_rust_file;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use walkdir::WalkDir;

#[derive(Parser)]
struct Cli {
    /// Path to the project directory or file to parse
    project_path: PathBuf,
    /// Name of desired output file
    output_filename: Option<String>,
}

fn detect_file_language(
    target_file: PathBuf,
    language_files: &mut HashMap<PathBuf, Language>,
) -> Option<HashSet<Language>> {
    let file_language = Language::from_file(target_file.to_str().unwrap())?;

    let mut detected = HashSet::new();
    language_files.insert(target_file.clone(), file_language);
    detected.insert(file_language);
    Some(detected)
}

fn detect_project_languages(
    target_dir: PathBuf,
    language_files: &mut HashMap<PathBuf, Language>,
) -> Option<HashSet<Language>> {
    // TODO: Read .gitignore if it exists
    // let mut exclude_patterns = Vec::new();
    // let gitignore_path = target_dir.join(".gitignore");
    // if gitignore_path.exists() {
    //     if let Ok(content) = fs::read_to_string(gitignore_path) {
    //         exclude_patterns = content
    //             .lines()
    //             .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
    //             .map(|line| line.trim().to_string())
    //             .collect();
    //     }
    // }

    let mut detected: HashSet<Language> = HashSet::new();

    for entry in WalkDir::new(target_dir) {
        let entry = entry.unwrap();
        let path = entry.path();

        if path.is_file() {
            let file_language = Language::from_file(path.to_str().unwrap());

            if file_language.is_some() {
                let lang = file_language.unwrap();
                language_files.insert(path.to_path_buf(), lang);
                detected.insert(lang);
            }
        }
    }

    if detected.is_empty() {
        None
    } else {
        Some(detected)
    }
}

fn run(path: PathBuf, output: Option<String>) -> Result<(), String> {
    if !path.exists() {
        return Err(format!("The path you specified does not exist: {path:?}"));
    }
    if output.is_none() {
        println!("Processing path: {path:?} (no output)");
    } else {
        println!(
            "Processing path: {:?}, output: {}",
            path,
            output.as_ref().unwrap()
        );
    }

    let mut language_files: HashMap<PathBuf, Language> = HashMap::new();

    // Determine project root
    let project_root = if path.is_file() {
        path.parent().unwrap_or(&path).to_path_buf()
    } else {
        path.clone()
    };

    // Detect languages in file/project
    if path.is_file() {
        detect_file_language(path.clone(), &mut language_files);
    } else {
        detect_project_languages(path.clone(), &mut language_files);
    }

    // Parse files and collect Nodes, indexed by file path
    let mut node_map: HashMap<PathBuf, FileNode> = HashMap::new();
    for (file_path, lang) in &language_files {
        match lang {
            Language::Rust => {
                if let Some(node) = parse_rust_file(file_path) {
                    node_map.insert(file_path.clone(), node);
                }
            }
            // _ => {
            //     println!("Skipping unsupported language: {lang:?}");
            // }
        }
    }

    // Build GraphNodes with multi-language support
    let mut graph_builder = GraphBuilder::new();
    let graph_nodes = graph_builder.build_graph_edges(&node_map, &project_root);

    // Debug: Print resolved connections
    println!("Resolved {} nodes with connections:", graph_nodes.len());
    for gnode in &graph_nodes {
        if !gnode.edges.is_empty() {
            println!(
                "  {} ({:?}) -> {} dependencies",
                gnode.data.file.file_name().unwrap().to_string_lossy(),
                gnode.data.language,
                gnode.edges.len()
            );
            for edge in &gnode.edges {
                println!("    -> {}", edge.file_name().unwrap().to_string_lossy());
            }
        }
    }

    // Launch the visualization if requested
    if let Some(ref out) = output {
        if out == "gui" {
            run_gui(graph_nodes);
            return Ok(());
        }
    }

    Ok(())
}

fn main() {
    let args = Cli::parse();
    match run(args.project_path, args.output_filename) {
        Ok(_) => println!("Success!"),
        Err(e) => {
            eprintln!("seiri error: {e}");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs::File, path::Path};
    use tempfile::TempDir;

    #[test]
    fn test_non_existent_path() {
        let temp_dir = TempDir::new().unwrap();
        let non_existent = temp_dir.path().join("non_existent_dir_12345");

        let result = run(non_existent.clone(), Some("output.txt".to_string()));

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("does not exist"));
    }

    #[test]
    fn test_existing_file() {
        let temp_dir = TempDir::new().unwrap();
        let temp_file = temp_dir.path().join("test_file.txt");
        File::create(&temp_file).unwrap();

        let result = run(temp_file, Some("output.txt".to_string()));

        assert!(result.is_ok());
    }

    #[test]
    fn test_existing_directory() {
        let temp_dir = TempDir::new().unwrap();

        let result = run(
            temp_dir.path().to_path_buf(),
            Some("output.txt".to_string()),
        );

        assert!(result.is_ok());
    }

    #[test]
    fn test_detect_file() {
        let current_file = Path::new(file!());
        assert!(!current_file.try_exists().is_err());

        let mut language_files: HashMap<PathBuf, Language> = HashMap::new();
        let result = detect_file_language(current_file.to_path_buf(), &mut language_files);

        assert!(result.is_some());
        assert!(result.unwrap().contains(&Language::Rust));
    }

    #[test]
    fn test_detect_invalid_file() {
        let current_file = Path::new("Cargo.lock");
        assert!(!current_file.try_exists().is_err());

        let mut language_files: HashMap<PathBuf, Language> = HashMap::new();
        let result = detect_file_language(current_file.to_path_buf(), &mut language_files);

        assert!(result.is_none());
    }

    #[test]
    fn test_detect_dir() {
        let current_dir = Path::new(file!()).parent().unwrap().canonicalize().unwrap();
        assert!(!current_dir.try_exists().is_err());

        let mut language_files: HashMap<PathBuf, Language> = HashMap::new();
        let result = detect_project_languages(current_dir.to_path_buf(), &mut language_files);

        assert!(&result.is_some());

        let langs = result.unwrap();
        assert!(langs.len() == 1);
        assert!(langs.contains(&Language::Rust));
    }
}
