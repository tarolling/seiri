use crate::core::defs::{FileNode, Language};
use rayon::prelude::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub mod cpp;
pub mod python;
pub mod rust;
pub mod typescript;

/// Helper function to extract text from a node.
#[inline]
pub fn get_text(n: tree_sitter::Node, code: &str) -> String {
    n.utf8_text(code.as_bytes()).unwrap_or("").to_string()
}

/// Parses a single file with the parser matching `language`.
///
/// Returns `None` when the file cannot be read or parsed.
pub fn parse_file(path: &Path, language: Language) -> Option<FileNode> {
    match language {
        Language::Python => python::parse_python_file(path),
        Language::Rust => rust::parse_rust_file(path),
        Language::TypeScript => typescript::parse_typescript_file(path),
        Language::Cpp => cpp::parse_cpp_file(path),
    }
}

/// Parses every detected file one at a time.
///
/// This is the reference implementation that [`parse_all_parallel`] is checked
/// against, and the baseline `benches/parsing.rs` measures the parallel speedup
/// from.
pub fn parse_all_sequential(
    language_files: &HashMap<PathBuf, Language>,
) -> HashMap<PathBuf, FileNode> {
    language_files
        .iter()
        .filter_map(|(path, &language)| parse_file(path, language).map(|node| (path.clone(), node)))
        .collect()
}

/// Parses every detected file across rayon's thread pool.
///
/// `on_file_parsed` runs on the worker thread that finished each file, so a
/// caller can advance a progress bar without any worker printing for itself.
/// Each parser constructs its own `tree_sitter::Parser` and nothing else is
/// shared, so the calls are safe to run concurrently.
pub fn parse_all_parallel<F>(
    language_files: &HashMap<PathBuf, Language>,
    on_file_parsed: F,
) -> HashMap<PathBuf, FileNode>
where
    F: Fn() + Sync + Send,
{
    language_files
        .par_iter()
        .filter_map(|(path, &language)| {
            let node = parse_file(path, language);
            on_file_parsed();
            node.map(|node| (path.clone(), node))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tempfile::TempDir;

    /// Builds a small project with one file per supported language, each with a
    /// dependency worth resolving.
    fn write_project(dir: &Path) -> HashMap<PathBuf, Language> {
        let files: [(&str, Language, &str); 5] = [
            (
                "alpha.rs",
                Language::Rust,
                "use crate::beta::Thing;\npub fn alpha() -> i32 { 1 }\n",
            ),
            (
                "beta.rs",
                Language::Rust,
                "pub struct Thing { pub value: i32 }\n",
            ),
            (
                "gamma.py",
                Language::Python,
                "import os\n\ndef gamma():\n    return os.getcwd()\n",
            ),
            (
                "delta.ts",
                Language::TypeScript,
                "export function delta(): number { return 1; }\n",
            ),
            (
                "epsilon.cpp",
                Language::Cpp,
                "#include <vector>\nint epsilon() { return 0; }\n",
            ),
        ];

        let mut language_files = HashMap::new();
        for (name, language, contents) in files {
            let path = dir.join(name);
            fs::write(&path, contents).unwrap();
            language_files.insert(path, language);
        }
        language_files
    }

    /// Asserts both maps contain the same files with identical parse results.
    fn assert_equivalent(
        sequential: &HashMap<PathBuf, FileNode>,
        parallel: &HashMap<PathBuf, FileNode>,
    ) {
        assert_eq!(sequential.len(), parallel.len());
        for (path, expected) in sequential {
            let actual = parallel
                .get(path)
                .unwrap_or_else(|| panic!("parallel parse dropped {}", path.display()));
            assert_eq!(expected.loc(), actual.loc(), "{}: LOC", path.display());
            assert_eq!(
                expected.language(),
                actual.language(),
                "{}: language",
                path.display()
            );
            assert_eq!(
                expected.imports(),
                actual.imports(),
                "{}: imports",
                path.display()
            );
            assert_eq!(
                expected.functions(),
                actual.functions(),
                "{}: functions",
                path.display()
            );
            assert_eq!(
                expected.containers(),
                actual.containers(),
                "{}: containers",
                path.display()
            );
            assert_eq!(
                expected.external_references(),
                actual.external_references(),
                "{}: external references",
                path.display()
            );
        }
    }

    #[test]
    fn parallel_parse_matches_sequential_parse_across_languages() {
        let dir = TempDir::new().unwrap();
        let language_files = write_project(dir.path());

        let sequential = parse_all_sequential(&language_files);
        let parallel = parse_all_parallel(&language_files, || {});

        assert_eq!(sequential.len(), language_files.len());
        assert_equivalent(&sequential, &parallel);
    }

    #[test]
    fn parse_all_parallel_reports_progress_for_every_file() {
        let dir = TempDir::new().unwrap();
        let language_files = write_project(dir.path());
        let parsed_count = AtomicUsize::new(0);

        let parsed = parse_all_parallel(&language_files, || {
            parsed_count.fetch_add(1, Ordering::Relaxed);
        });

        assert_eq!(parsed.len(), language_files.len());
        assert_eq!(parsed_count.load(Ordering::Relaxed), language_files.len());
    }

    #[test]
    fn parse_all_sequential_and_parallel_handle_empty_input() {
        let no_files = HashMap::new();

        assert!(parse_all_sequential(&no_files).is_empty());
        assert!(parse_all_parallel(&no_files, || {}).is_empty());
    }

    #[test]
    fn parse_file_dispatches_by_language() {
        let dir = TempDir::new().unwrap();
        let language_files = write_project(dir.path());

        for (path, language) in &language_files {
            let node = parse_file(path, *language).expect("file should parse");
            assert_eq!(node.language(), language);
        }
    }
}
