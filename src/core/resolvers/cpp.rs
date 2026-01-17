use super::LanguageResolver;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// C++ include resolver with caching
#[derive(Default)]
pub struct CppResolver {
    /// Maps normalized include paths to actual file paths
    include_to_file: HashMap<String, PathBuf>,
    /// Project root directory
    project_root: PathBuf,
    /// Standard library headers to exclude
    stdlib_headers: HashSet<String>,
    /// External library prefixes to exclude
    external_lib_prefixes: HashSet<String>,
}

impl CppResolver {
    pub fn new() -> Self {
        let mut resolver = Self::default();
        resolver.init_stdlib_headers();
        resolver.init_external_lib_prefixes();
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

    fn init_external_lib_prefixes(&mut self) {
        let prefixes = vec![
            // Boost library
            "boost",
            // Qt framework
            "qt",
            "QT",
            "Qt",
            // OpenGL and graphics
            "gl",
            "GL",
            "glm",
            "GLM",
            "glfw",
            "GLFW",
            "sdl",
            "SDL",
            // CUDA and GPU
            "cuda",
            "CUDA",
            "cudart",
            // System headers
            "sys",
            "windows",
            "windows.h",
            "unistd",
            "pthread",
            "pthread.h",
            "dirent",
            "fcntl",
            // Networking
            "sys/socket",
            "netinet/in",
            "arpa/inet",
            // Third-party libraries
            "libxml",
            "libcurl",
            "curl",
            "openssl",
            "zlib",
            "bzip2",
            // Compiler specific
            "intrin",
            "immintrin",
            "__builtin",
        ];

        for prefix in prefixes {
            self.external_lib_prefixes.insert(prefix.to_string());
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

    /// Check if an include is from an external library (system or third-party)
    fn is_external_library_include(&self, header_name: &str) -> bool {
        let lower = header_name.to_lowercase();
        
        // Check for known external library prefixes
        for prefix in &self.external_lib_prefixes {
            let prefix_lower = prefix.to_lowercase();
            if lower.starts_with(&prefix_lower) {
                // Check if it's actually a prefix match (followed by / or _)
                if lower.len() > prefix_lower.len() {
                    let next_char = &lower[prefix_lower.len()..].chars().next();
                    if matches!(next_char, Some('/') | Some('_') | Some('.')) {
                        return true;
                    }
                } else {
                    return true;
                }
            }
        }

        // Check for path-like patterns common in system includes
        if header_name.contains("sys/") || header_name.contains("arpa/") || 
           header_name.contains("netinet/") || header_name.contains("linux/") ||
           header_name.contains("asm/") {
            return true;
        }

        false
    }

    /// Check if an include should be filtered (not resolved as project dependency)
    fn should_filter_include(&self, header_name: &str) -> bool {
        self.is_stdlib_header(header_name) || self.is_external_library_include(header_name)
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
                    if normalized.is_empty() && component == "" {
                        // Leading slash, preserve it
                        continue;
                    }
                }
                ".." => {
                    // Parent directory - pop if possible
                    if !normalized.is_empty() && normalized.last().map_or(false, |c| c != "..") {
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

    /// Try to find a file in common include directories
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
            if candidate.exists() && candidate.is_file() {
                return Some(candidate);
            }

            // Also check with different extensions for header files without extension
            if !normalized.contains('.') {
                let extensions = vec![".h", ".hpp", ".hxx", ".h++", ".cc", ".cpp", ".cxx", ".c++"];
                for ext in extensions {
                    let with_ext = search_dir.join(format!("{}{}", &normalized, ext));
                    if with_ext.exists() && with_ext.is_file() {
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
        // Note: This is an immutable reference, so we can't update cache stats.
        // In a real implementation with RefCell, we could track cache performance.
        
        // Check if this should be filtered (stdlib or external library)
        if self.should_filter_include(import_path) {
            return None; // Don't resolve system/external headers as they're external
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
        assert_eq!(resolver.normalize_path("/path/to/file.h"), "/path/to/file.h");
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
        assert_eq!(resolver.normalize_path("path//to///file.h"), "path/to/file.h");
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
        resolver.build_module_map(&[include_file.clone()], project_root);

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
        resolver.build_module_map(&[include_file.clone()], project_root);

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
        resolver.build_module_map(&[include_file.clone()], project_root);

        // Request without extension - resolver should find it with .hpp
        let result = resolver.find_include_file("include/helper", &source_file);
        assert!(result.is_some());
    }

    #[test]
    fn test_external_library_detection_boost() {
        let resolver = CppResolver::new();
        assert!(resolver.is_external_library_include("boost/algorithm.hpp"));
        assert!(resolver.is_external_library_include("boost/shared_ptr.hpp"));
    }

    #[test]
    fn test_external_library_detection_qt() {
        let resolver = CppResolver::new();
        assert!(resolver.is_external_library_include("qt/QApplication"));
        assert!(resolver.is_external_library_include("Qt/QWidget.h"));
        assert!(resolver.is_external_library_include("QT/QMainWindow"));
    }

    #[test]
    fn test_external_library_detection_opengl() {
        let resolver = CppResolver::new();
        assert!(resolver.is_external_library_include("GL/glew.h"));
        assert!(resolver.is_external_library_include("glm/vec3.hpp"));
        assert!(resolver.is_external_library_include("glfw/glfw3.h"));
        assert!(resolver.is_external_library_include("SDL/SDL.h"));
    }

    #[test]
    fn test_external_library_detection_system() {
        let resolver = CppResolver::new();
        assert!(resolver.is_external_library_include("sys/socket.h"));
        assert!(resolver.is_external_library_include("unistd.h"));
        assert!(resolver.is_external_library_include("pthread.h"));
        assert!(resolver.is_external_library_include("windows.h"));
    }

    #[test]
    fn test_external_library_detection_third_party() {
        let resolver = CppResolver::new();
        assert!(resolver.is_external_library_include("libxml/parser.h"));
        assert!(resolver.is_external_library_include("curl/curl.h"));
        assert!(resolver.is_external_library_include("openssl/ssl.h"));
        assert!(resolver.is_external_library_include("zlib.h"));
    }

    #[test]
    fn test_external_library_detection_negative() {
        let resolver = CppResolver::new();
        assert!(!resolver.is_external_library_include("myheader.h"));
        assert!(!resolver.is_external_library_include("utils/helper.hpp"));
        assert!(!resolver.is_external_library_include("project/config.h"));
    }

    #[test]
    fn test_should_filter_include() {
        let resolver = CppResolver::new();
        // Should filter stdlib
        assert!(resolver.should_filter_include("iostream"));
        assert!(resolver.should_filter_include("vector"));
        // Should filter external libraries
        assert!(resolver.should_filter_include("boost/shared_ptr.hpp"));
        assert!(resolver.should_filter_include("GL/glew.h"));
        // Should not filter project includes
        assert!(!resolver.should_filter_include("myheader.h"));
        assert!(!resolver.should_filter_include("utils/helper.hpp"));
    }
}
