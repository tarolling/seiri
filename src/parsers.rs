use crate::core::defs::{FileNode, Language};
use rayon::prelude::*;
use std::cell::RefCell;
use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use tree_sitter::{Node, Parser, TreeCursor};

pub mod cache;
pub mod cpp;
pub mod python;
pub mod rust;
pub mod typescript;

pub use cache::ParseCache;

/// Helper function to extract text from a node.
///
/// Returns an empty string when the node's range is not a valid slice of `code`.
#[inline]
pub fn get_text(n: Node, code: &str) -> String {
    text_of(n, code).to_string()
}

/// The text of `n` borrowed straight from `code`, without the copy [`get_text`]
/// makes. Returns an empty string when the range is not a valid slice of `code`.
#[inline]
pub fn text_of<'code>(n: Node, code: &'code str) -> &'code str {
    code.get(n.byte_range()).unwrap_or("")
}

/// Records the text of `n` in `set`, unless the set already holds it.
///
/// The tree walk meets the same name once per use, and a large file has
/// thousands of uses of its few dozen names, so looking the text up before
/// copying it keeps the walk from allocating a `String` per occurrence.
#[inline]
pub fn insert_text(set: &mut HashSet<String>, n: Node, code: &str) {
    let text = text_of(n, code);
    if !set.contains(text) {
        set.insert(text.to_string());
    }
}

/// The tree-sitter grammar for `language`.
fn grammar(language: Language) -> tree_sitter::Language {
    match language {
        Language::Python => tree_sitter_python::LANGUAGE.into(),
        Language::Rust => tree_sitter_rust::LANGUAGE.into(),
        Language::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        Language::Cpp => tree_sitter_cpp::LANGUAGE.into(),
    }
}

thread_local! {
    /// One parser per language, per thread. Parsing is the whole cost of a scan
    /// and a parser carries a parse stack and a subtree pool that grow to fit the
    /// largest file it has seen, so reusing one beats building a fresh parser for
    /// every file.
    static PARSERS: RefCell<HashMap<Language, Parser>> = RefCell::new(HashMap::new());
}

/// Runs `parse` with the calling thread's parser for `language`, creating one on
/// first use.
///
/// Returns `None` only when the grammar cannot be configured on a new parser.
pub(crate) fn with_parser<R>(
    language: Language,
    parse: impl FnOnce(&mut Parser) -> R,
) -> Option<R> {
    PARSERS.with(|parsers| {
        let mut parsers = parsers.borrow_mut();
        let parser = match parsers.entry(language) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => {
                let mut parser = Parser::new();
                parser.set_language(&grammar(language)).ok()?;
                entry.insert(parser)
            }
        };
        Some(parse(parser))
    })
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
    parse_all_parallel_with(language_files, parse_file, on_file_parsed)
}

/// Parses every detected file, taking unchanged files from `cache` and adding
/// the ones it parses.
///
/// `on_file_parsed` runs once for every file that produced a node, whether the
/// cache had it or not, so a progress bar still fills.
///
/// The caller owns the cache's lifetime and decides when to write it back with
/// [`ParseCache::save`].
pub fn parse_all_parallel_cached<F>(
    language_files: &HashMap<PathBuf, Language>,
    cache: &mut ParseCache,
    on_file_parsed: F,
) -> ParseOutcome
where
    F: Fn() + Sync + Send,
{
    // The lookup is serial on purpose: one `stat` per file is nothing next to
    // a parse, and keeping the cache out of the parallel closure means it needs
    // no locking, so nothing is contended across the thread pool.
    let mut results = Vec::with_capacity(language_files.len());
    let mut missing = Vec::new();
    for path in language_files.keys() {
        match cache.get(path) {
            Some(node) => {
                on_file_parsed();
                results.push((path.to_path_buf(), Some(node)));
            }
            None => missing.push(path),
        }
    }

    let parsed = missing
        .par_iter()
        .map(|&path| {
            let node = parse_file(path, language_files[path]);
            if node.is_some() {
                on_file_parsed();
            }
            (path.to_path_buf(), node)
        })
        .collect::<Vec<_>>();

    for (path, node) in &parsed {
        if let Some(node) = node {
            cache.record(path, node);
        }
    }
    results.extend(parsed);

    split_results(results)
}

/// Parses every detected file across rayon's thread pool, reading each file
/// through `parse`.
fn parse_all_parallel_with<P, F>(
    language_files: &HashMap<PathBuf, Language>,
    parse: P,
    on_file_parsed: F,
) -> ParseOutcome
where
    P: Fn(&Path, Language) -> Option<FileNode> + Sync + Send,
    F: Fn() + Sync + Send,
{
    let results = language_files
        .par_iter()
        .map(|(path, &language)| {
            let node = parse(path, language);
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

    #[test]
    fn cached_parse_matches_uncached_parse() {
        let dir = TempDir::new().unwrap();
        let language_files = write_project(dir.path());
        let cache_dir = dir.path().join("cache");

        let mut cache = ParseCache::load_from(dir.path(), &cache_dir);
        let cold = parse_all_parallel_cached(&language_files, &mut cache, || {}).into_nodes();
        cache.save();
        assert_eq!(cache.hits(), 0, "nothing was cached yet");

        let mut warm = ParseCache::load_from(dir.path(), &cache_dir);
        let cached = parse_all_parallel_cached(&language_files, &mut warm, || {}).into_nodes();

        assert_eq!(warm.hits(), language_files.len(), "every file was reused");
        assert_eq!(cold.len(), cached.len());
        for (path, cold_node) in &cold {
            let cached_node = cached.get(path).expect("same files come back");
            assert_eq!(cold_node.loc(), cached_node.loc());
            assert_eq!(cold_node.language(), cached_node.language());
            assert_eq!(cold_node.imports(), cached_node.imports());
            assert_eq!(cold_node.functions(), cached_node.functions());
            assert_eq!(cold_node.containers(), cached_node.containers());
            assert_eq!(
                cold_node.external_references(),
                cached_node.external_references()
            );
        }
    }

    #[test]
    fn cached_parse_reports_a_file_parsed_exactly_once() {
        let dir = TempDir::new().unwrap();
        let language_files = write_project(dir.path());
        let cache_dir = dir.path().join("cache");

        for _ in 0..3 {
            let reported = AtomicUsize::new(0);
            let mut cache = ParseCache::load_from(dir.path(), &cache_dir);
            let outcome = parse_all_parallel_cached(&language_files, &mut cache, || {
                reported.fetch_add(1, Ordering::Relaxed);
            });
            cache.save();

            assert_eq!(
                reported.load(Ordering::Relaxed),
                language_files.len(),
                "every file counts once whether cached or parsed"
            );
            assert_eq!(outcome.nodes().len(), language_files.len());
        }
    }

    #[test]
    fn cached_parse_reparses_a_file_that_changed() {
        let dir = TempDir::new().unwrap();
        let language_files = write_project(dir.path());
        let cache_dir = dir.path().join("cache");

        let mut cache = ParseCache::load_from(dir.path(), &cache_dir);
        parse_all_parallel_cached(&language_files, &mut cache, || {});
        cache.save();

        let edited = dir.path().join("gamma.py");
        fs::write(
            &edited,
            "import os\n\ndef renamed():\n    return os.getcwd()\n",
        )
        .unwrap();

        let mut warm = ParseCache::load_from(dir.path(), &cache_dir);
        let outcome = parse_all_parallel_cached(&language_files, &mut warm, || {});

        assert_eq!(
            warm.hits(),
            language_files.len() - 1,
            "only the edit re-parsed"
        );
        let node = outcome
            .nodes()
            .get(&edited)
            .expect("the edited file is present");
        assert!(node.functions().contains("renamed"));
        assert!(!node.functions().contains("gamma"));
    }

    #[test]
    fn uncached_parse_ignores_an_existing_cache() {
        // the parsing benchmark measures `parse_all_parallel`, so it must keep
        // measuring real parses even when a cache exists
        let dir = TempDir::new().unwrap();
        let language_files = write_project(dir.path());

        let mut cache = ParseCache::load_from(dir.path(), &dir.path().join("cache"));
        parse_all_parallel_cached(&language_files, &mut cache, || {});
        cache.save();

        let parsed = parse_all_parallel(&language_files, || {});
        assert_eq!(parsed.nodes().len(), language_files.len());
    }

    #[test]
    fn cached_parse_reports_files_it_could_not_read() {
        let dir = TempDir::new().unwrap();
        let mut language_files = write_project(dir.path());
        let missing = dir.path().join("missing.rs");
        language_files.insert(missing.clone(), Language::Rust);
        let cache_dir = dir.path().join("cache");

        let mut cache = ParseCache::load_from(dir.path(), &cache_dir);
        let outcome = parse_all_parallel_cached(&language_files, &mut cache, || {});
        assert_eq!(outcome.failed(), std::slice::from_ref(&missing));
        cache.save();

        // a file that never parsed must not be cached as if it had
        let mut warm = ParseCache::load_from(dir.path(), &cache_dir);
        assert!(warm.get(&missing).is_none(), "a missing file was cached");
        assert_eq!(warm.len(), language_files.len() - 1);
    }

    #[test]
    fn text_of_borrows_from_the_source() {
        let code = "let alpha = 1;\n";
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(code, None).unwrap();
        let name = tree.root_node().child(0).unwrap().child(1).unwrap();

        let text = text_of(name, code);
        assert_eq!(text, "alpha");
        assert!(std::ptr::eq(
            text.as_ptr(),
            code[name.start_byte()..].as_ptr()
        ));
        // an out-of-range node still yields empty text rather than panicking
        assert_eq!(text_of(name, "let"), "");
    }

    #[test]
    fn insert_text_deduplicates_repeated_node_text() {
        let code = "fn alpha() { helper(); }\nfn beta() {}\n";
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(code, None).unwrap();

        // every node the walk visits, with duplicates collapsed
        let mut expected = HashSet::new();
        let mut cursor = tree.root_node().walk();
        loop {
            expected.insert(text_of(cursor.node(), code).to_string());
            if !advance(&mut cursor) {
                break;
            }
        }

        let mut set = HashSet::new();
        let mut cursor = tree.root_node().walk();
        loop {
            insert_text(&mut set, cursor.node(), code);
            if !advance(&mut cursor) {
                break;
            }
        }

        assert_eq!(set, expected);
        assert!(set.contains("alpha"));
        assert!(set.contains("beta"));
        // a name used twice is still recorded once
        assert!(code.matches("fn").count() > set.contains("fn") as usize);
    }

    /// Each thread keeps one parser per language, so a reused parser must not
    /// leak anything from the file it parsed before.
    #[test]
    fn reused_parsers_do_not_leak_between_files() {
        let dir = TempDir::new().unwrap();
        let first = dir.path().join("first.py");
        fs::write(
            &first,
            "import os\n\ndef alpha():\n    return os.getcwd()\n",
        )
        .unwrap();
        let second = dir.path().join("second.py");
        fs::write(&second, "import sys\n\ndef beta():\n    return sys.argv\n").unwrap();

        let language = Language::Python;
        let alpha = parse_file(&first, language).expect("first should parse");
        let beta = parse_file(&second, language).expect("second should parse");

        assert!(alpha.functions().contains("alpha"));
        assert!(!alpha.functions().contains("beta"));
        assert!(beta.functions().contains("beta"));
        assert!(!beta.functions().contains("alpha"));
        assert!(
            alpha.imports().iter().any(|i| i.path() == "os"),
            "imports: {:?}",
            alpha.imports()
        );
        assert!(
            !beta.imports().iter().any(|i| i.path() == "os"),
            "imports: {:?}",
            beta.imports()
        );
    }
}
