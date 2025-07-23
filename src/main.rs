mod config;
mod core;
mod parsers;

use crate::config::load_language_extensions;
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
struct CLI {
    /// Path to the project directory or file to parse
    project_path: std::path::PathBuf,
    /// Name of desired output file
    output_filename: Option<String>,
}

fn detect_file_language(file_path: PathBuf) -> Option<Vec<String>> {
    todo!()
}

fn detect_project_languages(dir_path: PathBuf) -> Option<Vec<String>> {
    todo!()

    // dir_path.iter().map(|f|)
}

fn run(path: PathBuf, output: Option<String>) -> Result<(), String> {
    if !path.exists() {
        return Err(format!("The path you specified does not exist: {:?}", path));
    }
    if output.is_none() {
        println!("Processing path: {:?} (no output)", path);
    } else {
        println!("Processing path: {:?}, output: {}", path, output.unwrap());
    }

    // Load supported language information
    // let language_extension_map = load_language_extensions();
    match load_language_extensions() {
        Ok(_) => Ok(()),
        Err(err) => Err(err),
    }

    // Detect languages in file/project
    // let languages = ternary!(
    //     path.is_file(),
    //     detect_file_language(path),
    //     detect_project_languages(path)
    // );

    // Ok(())
}

fn main() {
    let args = CLI::parse();
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
    use std::fs::File;
    use tempfile::TempDir;

    #[test]
    fn test_non_existent_path() {
        let temp_dir = std::env::temp_dir();
        let non_existent = temp_dir.join("non_existent_dir_12345");

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
}
