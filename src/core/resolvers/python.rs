use super::LanguageResolver;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Default)]
pub struct PythonResolver {
    /// Project root directory
    project_root: PathBuf,
}

impl PythonResolver {
    pub fn new() -> Self {
        Self::default()
    }

    /// Resolves an absolute import path (e.g., `my_app.utils`) from the project root
    fn resolve_absolute(&self, import_path: &str) -> Option<PathBuf> {
        // Convert 'my.module.name' to an OS-specific path 'my/module/name'
        let relative_path = PathBuf::from(import_path.replace('.', std::path::MAIN_SEPARATOR_STR));
        let mut potential_path = self.project_root.join(relative_path);

        // 1. Check if it's a '.py' file (e.g., /root/my/module/name.py)
        potential_path.set_extension("py");
        if potential_path.is_file() {
            return Some(potential_path);
        }

        // 2. Check if it's a package (e.g., /root/my/module/name/__init__.py)
        potential_path.set_extension(""); // Unset '.py' before joining
        let init_path = potential_path.join("__init__.py");
        if init_path.is_file() {
            return Some(init_path);
        }

        None
    }

    /// Resolves a relative import path (e.g., `.utils` or `..api.routes`) from the file's location
    fn resolve_relative(&self, import_path: &str, from_file: &Path) -> Option<PathBuf> {
        let mut relative_level = 0;
        let mut module_spec = import_path;

        // Count leading dots to determine how many levels to go up
        while module_spec.starts_with('.') {
            relative_level += 1;
            module_spec = &module_spec[1..];
        }

        // Determine the base directory for the search
        let mut base_dir = from_file.parent()?.to_path_buf();
        for _ in 1..relative_level {
            base_dir.pop();
        }

        // If module_spec is empty, we are importing the package itself (e.g., from . import foo)
        if module_spec.is_empty() {
            let init_path = base_dir.join("__init__.py");
            return if init_path.is_file() {
                Some(init_path)
            } else {
                None
            };
        }

        // Convert the rest of the module path
        let module_path = PathBuf::from(module_spec.replace('.', std::path::MAIN_SEPARATOR_STR));
        let mut target_path = base_dir.join(module_path);

        // Check for .py file or package
        target_path.set_extension("py");
        if target_path.is_file() {
            return Some(target_path);
        }
        target_path.set_extension("");
        let init_path = target_path.join("__init__.py");
        if init_path.is_file() {
            return Some(init_path);
        }

        None
    }
}

impl LanguageResolver for PythonResolver {
    fn build_module_map(&mut self, _files: &[PathBuf], project_root: &Path) {
        self.project_root = project_root.to_path_buf();
    }

    fn resolve_import(&self, import_path: &str, from_file: &Path) -> Option<PathBuf> {
        if import_path.starts_with('.') {
            self.resolve_relative(import_path, from_file)
        } else {
            self.resolve_absolute(import_path)
        }
    }

    fn resolve_external_references(
        &self,
        _references: &HashSet<String>,
        _from_file: &Path,
    ) -> Vec<PathBuf> {
        // We rely on explicit imports for the dependency graph
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use tempfile::TempDir;

    /// Helper to create a mock Python project:
    /// /
    /// ├── main.py
    /// ├── utils.py
    /// └── api/
    ///     ├── __init__.py
    ///     └── routes.py
    fn setup_test_project(dir: &TempDir) {
        let root = dir.path();
        fs::create_dir(root.join("api")).unwrap();
        File::create(root.join("main.py")).unwrap();
        File::create(root.join("api/__init__.py")).unwrap();
        File::create(root.join("api/routes.py")).unwrap();
        File::create(root.join("utils.py")).unwrap();
    }

    #[test]
    fn test_resolve_absolute_imports() {
        let temp_dir = TempDir::new().unwrap();
        setup_test_project(&temp_dir);
        let root = temp_dir.path();

        let mut resolver = PythonResolver::new();
        resolver.build_module_map(&[], root);
        let from_file = root.join("main.py");

        // Test importing a top-level module
        let resolved = resolver.resolve_import("utils", &from_file).unwrap();
        assert_eq!(resolved, root.join("utils.py"));

        // Test importing a package
        let resolved = resolver.resolve_import("api", &from_file).unwrap();
        assert_eq!(resolved, root.join("api/__init__.py"));

        // Test importing a submodule
        let resolved = resolver.resolve_import("api.routes", &from_file).unwrap();
        assert_eq!(resolved, root.join("api/routes.py"));
    }

    #[test]
    fn test_resolve_relative_imports() {
        let temp_dir = TempDir::new().unwrap();
        setup_test_project(&temp_dir);
        let root = temp_dir.path();

        let mut resolver = PythonResolver::new();
        resolver.build_module_map(&[], root);
        let from_file = root.join("api/routes.py");

        // Test `from . import ...` which should resolve to the current package's __init__.py
        let resolved = resolver.resolve_import(".", &from_file).unwrap();
        assert_eq!(resolved, root.join("api/__init__.py"));

        // Test `from ..utils import ...`
        let resolved = resolver.resolve_import("..utils", &from_file).unwrap();
        assert_eq!(resolved, root.join("utils.py"));
    }

    #[test]
    fn test_import_non_existent_module() {
        let temp_dir = TempDir::new().unwrap();
        setup_test_project(&temp_dir);
        let root = temp_dir.path();

        let mut resolver = PythonResolver::new();
        resolver.build_module_map(&[], root);
        let from_file = root.join("main.py");

        // Absolute
        let resolved = resolver.resolve_import("non_existent.module", &from_file);
        assert!(resolved.is_none());

        // Relative
        let resolved = resolver.resolve_import(".non_existent", &from_file);
        assert!(resolved.is_none());
    }
}
