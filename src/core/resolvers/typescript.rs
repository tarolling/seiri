use super::super::defs::Language;
use super::LanguageResolver;
use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};

pub struct TypeScriptResolver;

impl TypeScriptResolver {
    /// Resolves a relative import path into a full PathBuf
    fn resolve_relative_import(&self, import_path_str: &str, from_file: &Path) -> Option<PathBuf> {
        let from_dir = from_file.parent()?;
        let target_path = from_dir.join(import_path_str);

        // normalize the path to handle './' and '../' segments cleanly
        let mut components = Vec::new();
        for component in target_path.components() {
            match component {
                Component::ParentDir => {
                    if let Some(Component::Normal(_)) = components.last() {
                        components.pop();
                    } else {
                        components.push(component);
                    }
                }
                Component::CurDir => {}
                _ => {
                    components.push(component);
                }
            }
        }
        let normalized_path: PathBuf = components.iter().collect();

        for ext in Language::TypeScript.extensions() {
            let path_with_ext = normalized_path.with_extension(ext);
            if path_with_ext.is_file() {
                return Some(path_with_ext);
            }
        }

        // check for index file in directory (e.g., ./foo/index.ts)
        if normalized_path.is_dir() {
            for ext in Language::TypeScript.extensions() {
                let index_path = normalized_path.join(format!("index.{ext}"));
                if index_path.is_file() {
                    return Some(index_path);
                }
            }
        }

        None
    }
}

impl LanguageResolver for TypeScriptResolver {
    fn build_module_map(&mut self, _files: &[PathBuf], _project_root: &Path) {}

    fn resolve_import(&self, import_path: &str, from_file: &Path) -> Option<PathBuf> {
        if is_local_import(import_path) {
            self.resolve_relative_import(import_path, from_file)
        } else {
            None
        }
    }

    fn resolve_external_references(
        &self,
        _references: &HashSet<String>,
        _from_file: &Path,
    ) -> Vec<PathBuf> {
        // TODO: convert references to vec
        Vec::new()
    }
}

fn is_local_import(import_path: &str) -> bool {
    import_path.starts_with('.')
}
