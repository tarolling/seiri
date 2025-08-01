use crate::core::defs::{FileNode, GraphNode, Language};
use crate::core::resolvers::rust::RustResolver;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

pub mod python;
pub mod rust;

/// Module resolution trait
pub trait LanguageResolver {
    /// Build module mapping for this language
    fn build_module_map(&mut self, files: &[PathBuf], project_root: &Path);

    /// Resolve an import path to a file path for this language
    fn resolve_import(&self, import_path: &str, from_file: &Path) -> Option<PathBuf>;

    /// Get additional edges from external references
    fn resolve_external_references(
        &self,
        references: &HashSet<String>,
        from_file: &Path,
    ) -> Vec<PathBuf>;
}

/// Multi-language graph builder
pub struct GraphBuilder {
    resolvers: HashMap<Language, Box<dyn LanguageResolver>>,
}

impl GraphBuilder {
    pub fn new() -> Self {
        let mut resolvers: HashMap<Language, Box<dyn LanguageResolver>> = HashMap::new();
        resolvers.insert(Language::Rust, Box::new(RustResolver::new()));
        Self { resolvers }
    }

    /// Build graph edges for all languages
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

        // Build edges for each node
        let mut graph_nodes = Vec::new();
        for (file_path, node) in node_map {
            let mut edges = Vec::new();
            let mut resolved_imports = HashSet::new();

            // Use language-specific resolver
            if let Some(resolver) = self.resolvers.get(node.language()) {
                for import in node.imports() {
                    if !import.is_local() {
                        continue; // Skip non-local imports for now
                    }
                    if let Some(target_file) = resolver.resolve_import(import.path(), file_path) {
                        if target_file != *file_path && !resolved_imports.contains(&target_file) {
                            edges.push(target_file.clone());
                            resolved_imports.insert(target_file);
                        }
                    }
                }

                // Process external references
                let ext_refs =
                    resolver.resolve_external_references(node.external_references(), file_path);
                for target_file in ext_refs {
                    if target_file != *file_path && !resolved_imports.contains(&target_file) {
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
