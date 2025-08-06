use super::LanguageResolver;
use crate::core::defs::Language;
use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};

#[derive(Default)]
pub struct TypeScriptResolver;

impl TypeScriptResolver {
    pub fn new() -> Self {
        Self
    }

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use tempfile::TempDir;

    /// Creates a mock TS project:
    /// /
    /// ├── main.ts
    /// ├── components/
    /// │   ├── button.ts
    /// │   └── index.ts // we do not currently support TSX
    /// └── utils.ts
    fn setup_test_project(dir: &TempDir) {
        let root = dir.path();
        fs::create_dir_all(root.join("components")).unwrap();

        File::create(root.join("main.ts")).unwrap();
        File::create(root.join("utils.ts")).unwrap();
        File::create(root.join("components/button.ts")).unwrap();
        File::create(root.join("components/index.ts")).unwrap();
    }

    #[test]
    fn test_ts_resolver_sibling_and_parent() {
        let temp_dir = TempDir::new().unwrap();
        setup_test_project(&temp_dir);
        let root = temp_dir.path();
        let resolver = TypeScriptResolver::new();

        // From `main.ts`, import a sibling file `./utils`
        let from_file = root.join("main.ts");
        let resolved = resolver.resolve_import("./utils", &from_file);
        assert_eq!(resolved, Some(root.join("utils.ts")));

        // From `components/Button.tsx`, import a file in the parent directory `../utils`
        let from_file = root.join("components/Button.tsx");
        let resolved = resolver.resolve_import("../utils", &from_file);
        assert_eq!(resolved, Some(root.join("utils.ts")));
    }

    #[test]
    fn test_ts_resolver_directory_and_extension() {
        let temp_dir = TempDir::new().unwrap();
        setup_test_project(&temp_dir);
        let root = temp_dir.path();
        let resolver = TypeScriptResolver::new();
        let from_file = root.join("main.ts");

        // Import a directory `./components`, which should resolve to `components/index.ts`
        let resolved = resolver.resolve_import("./components", &from_file);
        assert_eq!(resolved, Some(root.join("components/index.ts")));

        // Import a .ts file directly
        let resolved = resolver.resolve_import("./components/button", &from_file);
        assert_eq!(resolved, Some(root.join("components/button.ts")));
    }

    #[test]
    fn test_ts_resolver_non_existent() {
        let temp_dir = TempDir::new().unwrap();
        setup_test_project(&temp_dir);
        let root = temp_dir.path();
        let resolver = TypeScriptResolver::new();

        let from_file = root.join("main.ts");
        let resolved = resolver.resolve_import("./non-existent", &from_file);
        assert!(resolved.is_none());

        // Test non-relative path which should be ignored
        let resolved = resolver.resolve_import("react", &from_file);
        assert!(resolved.is_none());
    }
}
