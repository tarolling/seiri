use super::LanguageResolver;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// C++ include resolver
#[derive(Default)]
pub struct CppResolver {
    /// Maps normalized include paths to actual file paths
    include_to_file: HashMap<String, PathBuf>,
    /// Project root directory
    project_root: PathBuf,
    /// Standard library headers to exclude
    stdlib_headers: HashSet<String>,
}

impl CppResolver {
    pub fn new() -> Self {
        let mut resolver = Self::default();
        resolver.init_stdlib_headers();
        resolver
    }

    fn init_stdlib_headers(&mut self) {
        let headers = vec![
            "iostream",
            "fstream",
            "sstream",
            "iomanip",
            "vector",
            "list",
            "deque",
            "queue",
            "stack",
            "map",
            "set",
            "unordered_map",
            "unordered_set",
            "algorithm",
            "numeric",
            "functional",
            "iterator",
            "string",
            "cstring",
            "cctype",
            "cmath",
            "memory",
            "utility",
            "stdexcept",
            "initializer_list",
            "cassert",
            "cerrno",
            "cfloat",
            "climits",
            "cstddef",
            "cstdint",
            "cstdio",
            "cstdlib",
            "ctime",
            "cwchar",
            "thread",
            "mutex",
            "condition_variable",
            "atomic",
            "future",
            "chrono",
            "ratio",
            "regex",
            "random",
            "complex",
            "valarray",
            "bitset",
            "ostream",
            "istream",
            "streambuf",
            "ios",
        ];

        for header in headers {
            self.stdlib_headers.insert(header.to_string());
        }
    }

    /// Check if a header is a standard library header
    fn is_stdlib_header(&self, header_name: &str) -> bool {
        let normalized = header_name
            .trim_end_matches(".h")
            .trim_end_matches(".hpp")
            .trim_end_matches(".hxx");
        self.stdlib_headers.contains(normalized)
    }

    /// Normalize an include path
    fn normalize_path(&self, path: &str) -> String {
        path.replace('\\', "/")
    }

    /// Try to find a file in common include directories
    fn find_include_file(&self, include_path: &str, from_file: &Path) -> Option<PathBuf> {
        let normalized = self.normalize_path(include_path);

        // First, check same directory as including file
        if let Some(parent) = from_file.parent() {
            let candidate = parent.join(&normalized);
            if candidate.exists() {
                return Some(candidate);
            }
        }

        // Check common include directories
        let search_dirs = vec![
            self.project_root.clone(),
            self.project_root.join("include"),
            self.project_root.join("src"),
            self.project_root.join("include").join("public"),
        ];

        for search_dir in search_dirs {
            let candidate = search_dir.join(&normalized);
            if candidate.exists() {
                return Some(candidate);
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
        // Check if this is a standard library header
        if self.is_stdlib_header(import_path) {
            return None; // Don't resolve stdlib headers as they're external
        }

        // Try to find the include file
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
    fn test_stdlib_header_detection() {
        let resolver = CppResolver::new();
        assert!(resolver.is_stdlib_header("iostream"));
        assert!(resolver.is_stdlib_header("vector"));
        assert!(resolver.is_stdlib_header("string"));
        assert!(!resolver.is_stdlib_header("myheader"));
    }

    #[test]
    fn test_path_normalization() {
        let resolver = CppResolver::new();
        let result = resolver.normalize_path("path\\to\\file.h");
        assert_eq!(result, "path/to/file.h");
    }
}
