use crate::core::defs::{FileNode, Language};
use rayon::prelude::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tree_sitter::{Node, TreeCursor};

pub mod cpp;
pub mod python;
pub mod rust;
pub mod typescript;

/// Helper function to extract text from a node.
///
/// Returns an empty string when the node's range is not a valid slice of `code`.
#[inline]
pub fn get_text(n: Node, code: &str) -> String {
    code.get(n.byte_range()).unwrap_or("").to_string()
}

/// Advances `cursor` to the next node in depth-first pre-order, descending into
/// the current node's children. Returns false once the whole tree has been visited.
#[inline]
pub(crate) fn advance(cursor: &mut TreeCursor) -> bool {
    if cursor.goto_first_child() {
        return true;
    }
    skip_children(cursor)
}

/// Advances `cursor` to the next node after the current node's subtree, so the
/// cursor's children are never visited. Returns false past the end of the tree.
#[inline]
pub(crate) fn skip_children(cursor: &mut TreeCursor) -> bool {
    loop {
        if cursor.goto_next_sibling() {
            return true;
        }
        if !cursor.goto_parent() {
            return false;
        }
    }
}

/// Moves `cursor` onto `target`, one of the children of the node it currently
/// points at, skipping any siblings that come before it. Returns false when
/// `target` is not a child, leaving the cursor where it started.
#[inline]
pub(crate) fn descend_into(cursor: &mut TreeCursor, target: Node) -> bool {
    if !cursor.goto_first_child() {
        return false;
    }

    loop {
        if cursor.node() == target {
            return true;
        }
        if !cursor.goto_next_sibling() {
            cursor.goto_parent();
            return false;
        }
    }
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

/// The outcome of a parse pass: the files that parsed, plus the ones that did not.
#[derive(Debug, Default)]
pub struct ParseOutcome {
    nodes: HashMap<PathBuf, FileNode>,
    failed: Vec<PathBuf>,
}

impl ParseOutcome {
    /// Successfully parsed files, indexed by path.
    #[inline]
    pub fn nodes(&self) -> &HashMap<PathBuf, FileNode> {
        &self.nodes
    }

    /// Consumes the outcome, returning the successfully parsed files.
    #[inline]
    pub fn into_nodes(self) -> HashMap<PathBuf, FileNode> {
        self.nodes
    }

    /// Paths that could not be read or parsed, sorted by path.
    #[inline]
    pub fn failed(&self) -> &[PathBuf] {
        &self.failed
    }
}

/// Splits parsed files from the paths that failed, keeping failures sorted.
fn split_results(results: Vec<(PathBuf, Option<FileNode>)>) -> ParseOutcome {
    let mut nodes = HashMap::with_capacity(results.len());
    let mut failed = Vec::new();

    for (path, node) in results {
        match node {
            Some(node) => {
                nodes.insert(path, node);
            }
            None => failed.push(path),
        }
    }

    failed.sort();
    ParseOutcome { nodes, failed }
}

/// Parses every detected file across rayon's thread pool.
///
/// `on_file_parsed` runs once for every file that parsed, and never for one that failed.
pub fn parse_all_parallel<F>(
    language_files: &HashMap<PathBuf, Language>,
    on_file_parsed: F,
) -> ParseOutcome
where
    F: Fn() + Sync + Send,
{
    let results = language_files
        .par_iter()
        .map(|(path, &language)| {
            let node = parse_file(path, language);
            if node.is_some() {
                on_file_parsed();
            }
            (path.clone(), node)
        })
        .collect();

    split_results(results)
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

    #[test]
    fn cursor_walk_visits_every_node_and_skip_prunes_subtrees() {
        let code = "fn outer() { inner(); }\nfn second() {}\n";
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(code, None).unwrap();
        let root = tree.root_node();

        let mut visited = 0;
        let mut cursor = root.walk();
        loop {
            visited += 1;
            if !advance(&mut cursor) {
                break;
            }
        }
        assert_eq!(visited, count_nodes(root));

        // skipping children of the root leaves the whole tree unvisited
        let mut cursor = root.walk();
        let mut visited = vec![cursor.node().kind_id()];
        while skip_children(&mut cursor) {
            visited.push(cursor.node().kind_id());
        }
        assert_eq!(visited, vec![root.kind_id()]);
    }

    #[test]
    fn descend_into_moves_the_cursor_onto_a_child_and_back_off_it() {
        let code = "fn outer() { inner(); }\nfn second() {}\n";
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(code, None).unwrap();
        let root = tree.root_node();
        let first = root.child(0).unwrap();

        let mut cursor = root.walk();
        assert!(descend_into(&mut cursor, first));
        assert_eq!(cursor.node(), first);

        // a node that is not a direct child leaves the cursor untouched
        let deeper = first.child(0).unwrap();
        let mut cursor = root.walk();
        assert!(!descend_into(&mut cursor, deeper));
        assert_eq!(cursor.node(), root);
    }

    /// Counts every node in the tree, the same set the cursor walk visits.
    fn count_nodes(root: Node) -> usize {
        let mut count = 0;
        let mut cursor = root.walk();
        loop {
            count += 1;
            if !advance(&mut cursor) {
                return count;
            }
        }
    }

    /// Writes a file that cannot be decoded as UTF-8, which no parser can read.
    fn write_unreadable_file(path: &Path) {
        fs::write(path, [0xFF, 0xFE, 0x00, 0x41]).unwrap();
    }

    #[test]
    fn parse_all_parallel_reports_files_it_cannot_read() {
        let dir = TempDir::new().unwrap();
        let good = dir.path().join("good.py");
        fs::write(&good, "def good():\n    return 1\n").unwrap();
        let bad = dir.path().join("bad.py");
        write_unreadable_file(&bad);

        let language_files = HashMap::from([
            (good.clone(), Language::Python),
            (bad.clone(), Language::Python),
        ]);
        let parsed = AtomicUsize::new(0);
        let outcome = parse_all_parallel(&language_files, || {
            parsed.fetch_add(1, Ordering::Relaxed);
        });

        assert!(outcome.nodes().contains_key(&good));
        assert_eq!(outcome.failed(), [bad]);
        // progress is only reported for files that parsed
        assert_eq!(parsed.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn parse_all_parallel_reports_no_failures_for_readable_files() {
        let dir = TempDir::new().unwrap();
        let language_files = write_project(dir.path());

        let outcome = parse_all_parallel(&language_files, || {});

        assert!(outcome.failed().is_empty());
        assert_eq!(outcome.nodes().len(), language_files.len());
    }

    /// Asserts both parse maps contain the same files with identical results.
    fn assert_same_parse(
        expected: &HashMap<PathBuf, FileNode>,
        actual: &HashMap<PathBuf, FileNode>,
    ) {
        assert_eq!(expected.len(), actual.len());
        for (path, expected) in expected {
            let actual = actual
                .get(path)
                .unwrap_or_else(|| panic!("parse dropped {}", path.display()));
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
    fn parallel_parse_is_deterministic_across_runs() {
        let dir = TempDir::new().unwrap();
        let language_files = write_project(dir.path());

        let first = parse_all_parallel(&language_files, || {});
        let second = parse_all_parallel(&language_files, || {});

        assert_eq!(first.nodes().len(), language_files.len());
        assert_same_parse(first.nodes(), second.nodes());
    }

    #[test]
    fn parse_all_parallel_reports_progress_for_every_file() {
        let dir = TempDir::new().unwrap();
        let language_files = write_project(dir.path());
        let parsed_count = AtomicUsize::new(0);

        let outcome = parse_all_parallel(&language_files, || {
            parsed_count.fetch_add(1, Ordering::Relaxed);
        });

        assert_eq!(outcome.nodes().len(), language_files.len());
        assert_eq!(parsed_count.load(Ordering::Relaxed), language_files.len());
    }

    #[test]
    fn parse_all_parallel_handles_empty_input() {
        let no_files = HashMap::new();

        assert!(parse_all_parallel(&no_files, || {}).nodes().is_empty());
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
