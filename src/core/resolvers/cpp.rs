use super::{LanguageResolver, is_within_project};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// C++ include resolver with caching
#[derive(Default)]
pub struct CppResolver {
    /// Maps normalized include paths to actual file paths
    include_to_file: HashMap<String, PathBuf>,
    /// Project root directory
    project_root: PathBuf,
}

impl CppResolver {
    pub fn new() -> Self {
        Self::default()
    }

    /// Normalize an include path, resolving `.` and `..` components
    fn normalize_path(&self, path: &str) -> String {
        // First, normalize separators to forward slashes
        let path = path.replace('\\', "/");

        // Split into components and process them
        let components: Vec<&str> = path.split('/').collect();
        let mut normalized: Vec<String> = Vec::new();

        for component in components {
            match component {
                "" | "." => {
                    // Empty component or current directory - skip unless it's the first component
                    if normalized.is_empty() && component.is_empty() {
                        // Leading slash, preserve it
                        continue;
                    }
                }
                ".." => {
                    // Parent directory - pop if possible
                    if !normalized.is_empty() && normalized.last().is_some_and(|c| c != "..") {
                        normalized.pop();
                    } else if normalized.is_empty() {
                        // Preserve .. at the start for relative paths
                        normalized.push("..".to_string());
                    } else {
                        // Keep consecutive ..
                        normalized.push("..".to_string());
                    }
                }
                _ => {
                    normalized.push(component.to_string());
                }
            }
        }

        // Join back together, preserving leading slash if present
        let result = if path.starts_with('/') {
            format!("/{}", normalized.join("/"))
        } else {
            normalized.join("/")
        };

        // Handle the case where result is empty
        if result.is_empty() {
            ".".to_string()
        } else {
            result
        }
    }

    /// Try to find a file in common include directories.
    ///
    /// The caller (GraphBuilder::build_graph_edges) only invokes this for
    /// imports the parser already classified as local (quoted includes),
    /// so we trust that classification here rather than re-filtering by
    /// name against the stdlib/external prefix lists; a project's own
    /// header can legitimately share a name with a stdlib/system header
    /// (e.g. "windows.h", "string.h").
    fn find_include_file(&self, include_path: &str, from_file: &Path) -> Option<PathBuf> {
        let normalized = self.normalize_path(include_path);

        // Build search directories, ordered by priority
        let mut search_dirs = Vec::new();

        // 1. Same directory as the including file (highest priority for local includes)
        if let Some(parent) = from_file.parent() {
            search_dirs.push(parent.to_path_buf());
        }

        // 2. Project root
        search_dirs.push(self.project_root.clone());

        // 3. Common include directories
        let common_include_dirs = vec![
            "include",
            "include/public",
            "include/internal",
            "src",
            "src/include",
            "public",
            "private",
            "headers",
            "inc",
        ];

        for dir_name in common_include_dirs {
            let dir = self.project_root.join(dir_name);
            if !search_dirs.contains(&dir) {
                search_dirs.push(dir);
            }
        }

        // 4. Parent directories (for multi-level projects)
        if let Some(parent) = from_file.parent() {
            let mut current_parent = parent.to_path_buf();
            let mut depth = 0;
            loop {
                if depth >= 5 {
                    break;
                }
                if let Some(new_parent) = current_parent.parent() {
                    if new_parent == current_parent {
                        break;
                    }
                    if !search_dirs.contains(&current_parent) {
                        search_dirs.push(current_parent.clone());
                    }
                    current_parent = new_parent.to_path_buf();
                } else {
                    break;
                }
                depth += 1;
            }
        }

        // Search for the include file in order of priority
        for search_dir in search_dirs {
            let candidate = search_dir.join(&normalized);
            if candidate.is_file() && is_within_project(&candidate, &self.project_root) {
                return Some(candidate);
            }

            // Also check with different extensions for header files without extension
            if !normalized.contains('.') {
                let extensions = vec![".h", ".hpp", ".hxx", ".h++", ".cc", ".cpp", ".cxx", ".c++"];
                for ext in extensions {
                    let with_ext = search_dir.join(format!("{}{}", normalized, ext));
                    if with_ext.is_file() && is_within_project(&with_ext, &self.project_root) {
                        return Some(with_ext);
                    }
                }
            }
        }

        None
    }
}

impl LanguageResolver for CppResolver {
    fn build_module_map(&mut self, files: &[PathBuf], project_root: &Path) {
        self.project_root = project_root.to_path_buf();

        for file_path in files {
            // Map relative paths from project root to file paths
            if let Ok(relative_path) = file_path.strip_prefix(&self.project_root) {
                let normalized = self.normalize_path(relative_path.to_string_lossy().as_ref());
                self.include_to_file.insert(normalized, file_path.clone());
            }
        }
    }

    fn resolve_import(&self, import_path: &str, from_file: &Path) -> Option<PathBuf> {
        self.find_include_file(import_path, from_file)
    }

    fn resolve_external_references(
        &self,
        _references: &HashSet<String>,
        _from_file: &Path,
    ) -> Vec<PathBuf> {
        // For MVP, we don't resolve external references
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_path_normalization() {
        let resolver = CppResolver::new();
        let result = resolver.normalize_path("path\\to\\file.h");
        assert_eq!(result, "path/to/file.h");
    }

    #[test]
    fn test_path_normalization_current_dir() {
        let resolver = CppResolver::new();
        assert_eq!(resolver.normalize_path("./file.h"), "file.h");
        assert_eq!(resolver.normalize_path("path/./file.h"), "path/file.h");
    }

    #[test]
    fn test_path_normalization_parent_dir() {
        let resolver = CppResolver::new();
        assert_eq!(resolver.normalize_path("path/../file.h"), "file.h");
        assert_eq!(resolver.normalize_path("path/to/../file.h"), "path/file.h");
        assert_eq!(resolver.normalize_path("path/to/../../file.h"), "file.h");
    }

    #[test]
    fn test_path_normalization_relative_parent() {
        let resolver = CppResolver::new();
        assert_eq!(resolver.normalize_path("../file.h"), "../file.h");
        assert_eq!(resolver.normalize_path("../../file.h"), "../../file.h");
        assert_eq!(resolver.normalize_path("path/../../file.h"), "../file.h");
    }

    #[test]
    fn test_path_normalization_leading_slash() {
        let resolver = CppResolver::new();
        assert_eq!(
            resolver.normalize_path("/path/to/file.h"),
            "/path/to/file.h"
        );
        assert_eq!(resolver.normalize_path("/path/./file.h"), "/path/file.h");
        assert_eq!(resolver.normalize_path("/path/../file.h"), "/file.h");
    }

    #[test]
    fn test_path_normalization_idempotent() {
        let resolver = CppResolver::new();
        let path1 = "path/to/../../../file.h";
        let normalized1 = resolver.normalize_path(path1);
        let normalized2 = resolver.normalize_path(&normalized1);
        assert_eq!(normalized1, normalized2);
    }

    #[test]
    fn test_path_normalization_consecutive_slashes() {
        let resolver = CppResolver::new();
        assert_eq!(
            resolver.normalize_path("path//to///file.h"),
            "path/to/file.h"
        );
    }

    #[test]
    fn test_find_include_file_same_directory() {
        use std::fs;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let project_root = temp_dir.path();

        // Create a test include file
        let include_file = project_root.join("helper.h");
        fs::write(&include_file, "// helper").expect("Failed to write include file");

        // Create a source file
        let source_file = project_root.join("main.cpp");
        fs::write(&source_file, "#include \"helper.h\"").expect("Failed to write source file");

        let mut resolver = CppResolver::new();
        resolver.build_module_map(std::slice::from_ref(&include_file), project_root);

        let result = resolver.find_include_file("helper.h", &source_file);
        assert!(result.is_some());
        assert_eq!(result.unwrap(), include_file);
    }

    #[test]
    fn test_find_include_file_include_dir() {
        use std::fs;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let project_root = temp_dir.path();

        // Create include directory
        let include_dir = project_root.join("include");
        fs::create_dir(&include_dir).expect("Failed to create include dir");

        // Create a test include file
        let include_file = include_dir.join("helper.h");
        fs::write(&include_file, "// helper").expect("Failed to write include file");

        // Create a source file in project root
        let source_file = project_root.join("main.cpp");

        let mut resolver = CppResolver::new();
        resolver.build_module_map(std::slice::from_ref(&include_file), project_root);

        let result = resolver.find_include_file("include/helper.h", &source_file);
        assert!(result.is_some());
    }

    #[test]
    fn test_find_include_file_not_found() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let project_root = temp_dir.path();
        let source_file = project_root.join("main.cpp");

        let mut resolver = CppResolver::new();
        resolver.build_module_map(&[], project_root);

        let result = resolver.find_include_file("nonexistent.h", &source_file);
        assert!(result.is_none());
    }

    #[test]
    fn test_find_include_file_with_extension_inference() {
        use std::fs;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let project_root = temp_dir.path();

        // Create an include directory
        let include_dir = project_root.join("include");
        fs::create_dir(&include_dir).expect("Failed to create include dir");

        // Create a header file with .hpp extension
        let include_file = include_dir.join("helper.hpp");
        fs::write(&include_file, "// helper").expect("Failed to write include file");

        let source_file = project_root.join("main.cpp");

        let mut resolver = CppResolver::new();
        resolver.build_module_map(std::slice::from_ref(&include_file), project_root);

        // Request without extension - resolver should find it with .hpp
        let result = resolver.find_include_file("include/helper", &source_file);
        assert!(result.is_some());
    }

    #[test]
    fn test_find_include_file_rejects_parent_traversal() {
        use std::fs;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let project_root = temp_dir.path().join("project");
        let source_dir = project_root.join("src");
        fs::create_dir_all(&source_dir).expect("Failed to create source dir");

        let outside_file = temp_dir.path().join("outside.h");
        fs::write(&outside_file, "// outside").expect("Failed to write outside file");
        let source_file = source_dir.join("main.cpp");
        fs::write(&source_file, "#include \"../../outside.h\"")
            .expect("Failed to write source file");

        let mut resolver = CppResolver::new();
        resolver.build_module_map(&[], &project_root);

        assert_eq!(
            resolver.find_include_file("../../outside.h", &source_file),
            None
        );
    }

    #[test]
    fn test_resolve_import_does_not_filter_local_include_with_stdlib_name() {
        // regression test for #147: resolve_import is only ever called with
        // imports the parser already classified as local (see
        // GraphBuilder::build_graph_edges), so it must not re-reject a local
        // header just because its name collides with a stdlib/external
        // prefix (e.g. a project's own "windows.h" or "string.h")
        use std::fs;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let project_root = temp_dir.path();

        let include_file = project_root.join("windows.h");
        fs::write(&include_file, "// project-local windows.h")
            .expect("Failed to write include file");

        let source_file = project_root.join("main.cpp");
        fs::write(&source_file, "#include \"windows.h\"").expect("Failed to write source file");

        let mut resolver = CppResolver::new();
        resolver.build_module_map(std::slice::from_ref(&include_file), project_root);

        let result = resolver.resolve_import("windows.h", &source_file);
        assert_eq!(result, Some(include_file));
    }
}
