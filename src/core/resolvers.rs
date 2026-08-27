use crate::core::defs::{FileNode, GraphNode, Import, Language};
use crate::core::resolvers::cpp::CppResolver;
use crate::core::resolvers::python::PythonResolver;
use crate::core::resolvers::rust::RustResolver;
use crate::core::resolvers::typescript::TypeScriptResolver;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

pub mod cpp;
pub mod python;
pub mod rust;
pub mod typescript;

fn is_within_project(candidate: &Path, project_root: &Path) -> bool {
    let Ok(candidate) = candidate.canonicalize() else {
        return false;
    };
    let Ok(project_root) = project_root.canonicalize() else {
        return false;
    };

    candidate.starts_with(project_root)
}

/// Module resolution trait
pub trait LanguageResolver {
    /// Build module mapping for this language. This will build a map of files
    /// to "modules", or expressions that are importable (e.g., `use crate::core`
    /// <-> `src/core.rs`)
    fn build_module_map(&mut self, files: &[PathBuf], project_root: &Path);

    /// Resolve an import path to a file path for this language.
    fn resolve_import(&self, import_path: &str, from_file: &Path) -> Option<PathBuf>;

    /// Get additional edges from external references.
    fn resolve_external_references(
        &self,
        references: &HashSet<String>,
        from_file: &Path,
    ) -> Vec<PathBuf>;
}

/// Multi-language graph builder.
pub struct GraphBuilder {
    resolvers: HashMap<Language, Box<dyn LanguageResolver>>,
}

impl GraphBuilder {
    pub fn new() -> Self {
        let mut resolvers: HashMap<Language, Box<dyn LanguageResolver>> = HashMap::new();
        resolvers.insert(Language::Python, Box::new(PythonResolver::new()));
        resolvers.insert(Language::Rust, Box::new(RustResolver::new()));
        resolvers.insert(Language::TypeScript, Box::new(TypeScriptResolver::new()));
        resolvers.insert(Language::Cpp, Box::new(CppResolver::new()));
        Self { resolvers }
    }

    /// Build graph edges for all languages.
    pub fn build_graph_edges(
        &mut self,
        node_map: &HashMap<PathBuf, FileNode>,
        project_root: &Path,
    ) -> Vec<GraphNode> {
        // Group files by language
        let mut files_by_language: HashMap<Language, Vec<PathBuf>> = HashMap::new();
        for (file_path, node) in node_map {
            files_by_language
                .entry(*node.language())
                .or_default()
                .push(file_path.clone());
        }

        // Build module maps for each language
        for (language, files) in &files_by_language {
            if let Some(resolver) = self.resolvers.get_mut(language) {
                resolver.build_module_map(files, project_root);
            }
        }

        // build edges for each node. iterate file paths in sorted order so the
        // resulting node/edge order is deterministic
        let mut file_paths: Vec<&PathBuf> = node_map.keys().collect();
        file_paths.sort();

        let mut graph_nodes = Vec::with_capacity(file_paths.len());
        for file_path in file_paths {
            let node = &node_map[file_path];
            let mut edges = Vec::new();
            let mut resolved_imports = HashSet::new();

            // Use language-specific resolver
            if let Some(resolver) = self.resolvers.get(node.language()) {
                let mut imports: Vec<&Import> = node.imports().iter().collect();
                imports.sort_by(|a, b| a.path().cmp(b.path()));

                for import in imports {
                    if !import.is_local() {
                        continue; // Skip non-local imports for now
                    }
                    if let Some(target_file) = resolver.resolve_import(import.path(), file_path)
                        && target_file != *file_path
                        && node_map.contains_key(&target_file)
                        && !resolved_imports.contains(&target_file)
                    {
                        edges.push(target_file.clone());
                        resolved_imports.insert(target_file);
                    }
                }

                let mut ext_refs =
                    resolver.resolve_external_references(node.external_references(), file_path);
                ext_refs.sort(); // just in case
                for target_file in ext_refs {
                    if target_file != *file_path
                        && node_map.contains_key(&target_file)
                        && !resolved_imports.contains(&target_file)
                    {
                        edges.push(target_file.clone());
                        resolved_imports.insert(target_file);
                    }
                }
            }

            graph_nodes.push(GraphNode::new(node.clone(), edges));
        }

        graph_nodes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rust_file(path: &str, imports: &[&str]) -> (PathBuf, FileNode) {
        let path = PathBuf::from(path);
        let imports: HashSet<Import> = imports
            .iter()
            .map(|i| Import::new(i.to_string(), true))
            .collect();
        let node = FileNode::new(
            path.clone(),
            10,
            Language::Rust,
            imports,
            HashSet::new(),
            HashSet::new(),
            HashSet::new(),
        );
        (path, node)
    }

    fn rust_file_with_external_refs(path: &str, external_refs: &[&str]) -> (PathBuf, FileNode) {
        let path = PathBuf::from(path);
        let external_references: HashSet<String> =
            external_refs.iter().map(|r| r.to_string()).collect();
        let node = FileNode::new(
            path.clone(),
            10,
            Language::Rust,
            HashSet::new(),
            HashSet::new(),
            HashSet::new(),
            external_references,
        );
        (path, node)
    }

    /// Resolver stub whose `resolve_external_references` always points at a
    /// file that was never part of the scanned `node_map` (e.g. it was
    /// gitignored, unsupported, or failed to parse).
    struct DanglingExternalRefResolver;

    impl LanguageResolver for DanglingExternalRefResolver {
        fn build_module_map(&mut self, _files: &[PathBuf], _project_root: &Path) {}

        fn resolve_import(&self, _import_path: &str, _from_file: &Path) -> Option<PathBuf> {
            None
        }

        fn resolve_external_references(
            &self,
            references: &HashSet<String>,
            _from_file: &Path,
        ) -> Vec<PathBuf> {
            references
                .iter()
                .map(|r| PathBuf::from(format!("/project/src/{r}.rs")))
                .collect()
        }
    }

    /// Regression test for issue #153: `build_graph_edges` pushes resolved
    /// external-reference targets as edges without checking they exist in
    /// `node_map`. The equivalent check exists for regular imports but was
    /// missing for external references, allowing dangling edges to files
    /// that were never scanned.
    #[test]
    fn build_graph_edges_filters_dangling_external_reference_edges() {
        let project_root = PathBuf::from("/project");

        let (path, node) = rust_file_with_external_refs("/project/src/a.rs", &["not_in_node_map"]);
        let mut node_map = HashMap::new();
        node_map.insert(path, node);

        let mut resolvers: HashMap<Language, Box<dyn LanguageResolver>> = HashMap::new();
        resolvers.insert(Language::Rust, Box::new(DanglingExternalRefResolver));
        let mut builder = GraphBuilder { resolvers };

        let graph_nodes = builder.build_graph_edges(&node_map, &project_root);

        assert_eq!(graph_nodes.len(), 1);
        assert!(
            graph_nodes[0].edges().is_empty(),
            "edge to a file not present in node_map should have been filtered out, got: {:?}",
            graph_nodes[0].edges()
        );
    }

    /// Regression test for issue #154: `build_graph_edges` used to iterate the
    /// `node_map: HashMap<PathBuf, FileNode>` directly, so the resulting node
    /// and edge order depended on HashMap iteration order rather than on the
    /// actual project contents. Build the same logical graph twice, inserting
    /// entries into the map in opposite orders, and assert the output is
    /// identical (and sorted by path) either way.
    #[test]
    fn build_graph_edges_is_deterministic_regardless_of_node_map_insertion_order() {
        let project_root = PathBuf::from("/project");

        let files = vec![
            rust_file("/project/src/a.rs", &["crate::b", "crate::c"]),
            rust_file("/project/src/b.rs", &[]),
            rust_file("/project/src/c.rs", &[]),
            rust_file("/project/src/main.rs", &["crate::a"]),
        ];

        let mut forward: HashMap<PathBuf, FileNode> = HashMap::new();
        for (path, node) in &files {
            forward.insert(path.clone(), node.clone());
        }

        let mut backward: HashMap<PathBuf, FileNode> = HashMap::new();
        for (path, node) in files.iter().rev() {
            backward.insert(path.clone(), node.clone());
        }

        let nodes_a = GraphBuilder::new().build_graph_edges(&forward, &project_root);
        let nodes_b = GraphBuilder::new().build_graph_edges(&backward, &project_root);

        let paths_a: Vec<_> = nodes_a.iter().map(|n| n.data().file().clone()).collect();
        let paths_b: Vec<_> = nodes_b.iter().map(|n| n.data().file().clone()).collect();

        let mut sorted_paths = paths_a.clone();
        sorted_paths.sort();
        assert_eq!(
            paths_a, sorted_paths,
            "graph nodes should be ordered by file path"
        );
        assert_eq!(
            paths_a, paths_b,
            "node order must not depend on HashMap insertion order"
        );

        let edges_a: Vec<_> = nodes_a.iter().map(|n| n.edges().clone()).collect();
        let edges_b: Vec<_> = nodes_b.iter().map(|n| n.edges().clone()).collect();
        assert_eq!(
            edges_a, edges_b,
            "edge order must not depend on HashMap insertion order"
        );

        // a.rs imports crate::b then crate::c (alphabetical), so its edges
        // should resolve in that same order.
        let a_edges = &edges_a[0];
        assert_eq!(
            a_edges,
            &vec![
                PathBuf::from("/project/src/b.rs"),
                PathBuf::from("/project/src/c.rs"),
            ]
        );
    }
}
