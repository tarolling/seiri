use super::LanguageResolver;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

pub struct RustResolver {
    /// Maps module paths (like "crate::parser::rust") to actual file paths
    module_to_file: HashMap<String, PathBuf>,
    /// Maps file paths to their module paths
    file_to_module: HashMap<PathBuf, String>,
    /// Project root directory
    project_root: PathBuf,
}

impl RustResolver {
    pub fn new() -> Self {
        Self {
            module_to_file: HashMap::new(),
            file_to_module: HashMap::new(),
            project_root: PathBuf::new(),
        }
    }

    /// Convert a file path to its module path (e.g., src/parser/rust.rs -> crate::parser::rust)
    fn file_path_to_module_path(&self, file_path: &Path) -> Option<String> {
        let relative_path = file_path.strip_prefix(&self.project_root).ok()?;

        // Handle different Rust file patterns
        let module_parts: Vec<String> = if relative_path.starts_with("src") {
            // Standard src/ layout
            let without_src = relative_path.strip_prefix("src").ok()?;
            self.path_to_module_parts(without_src)
        } else {
            // Other layouts (like lib.rs in root, etc.)
            self.path_to_module_parts(relative_path)
        }?;

        if module_parts.is_empty() {
            return Some("crate".to_string()); // Root module
        }

        Some(format!("crate::{}", module_parts.join("::")))
    }

    /// Convert path components to module parts
    fn path_to_module_parts(&self, path: &Path) -> Option<Vec<String>> {
        let mut parts = Vec::new();

        for component in path.components() {
            if let std::path::Component::Normal(os_str) = component {
                if let Some(part) = os_str.to_str() {
                    if part.ends_with(".rs") {
                        let module_name = part.strip_suffix(".rs").unwrap();
                        // Skip main.rs and lib.rs as they don't add module components
                        if module_name != "main" && module_name != "lib" {
                            parts.push(module_name.to_string());
                        }
                    } else {
                        // Directory name becomes part of module path
                        parts.push(part.to_string());
                    }
                }
            }
        }

        Some(parts)
    }

    /// Resolve a module declaration (like "parsers" from "pub mod parsers") to its file path
    fn resolve_module_declaration(&self, module_name: &str, from_file: &Path) -> Option<PathBuf> {
        let from_dir = from_file.parent()?;

        // Try different patterns for module files:
        // 1. module_name.rs in the same directory
        let module_file = from_dir.join(format!("{}.rs", module_name));
        if module_file.exists() {
            return Some(module_file);
        }

        // 2. module_name/mod.rs in the same directory
        let mod_rs_file = from_dir.join(module_name).join("mod.rs");
        if mod_rs_file.exists() {
            return Some(mod_rs_file);
        }

        // 3. Check if we have it in our module map
        if let Some(current_module) = self.file_to_module.get(from_file) {
            let target_module = if current_module == "crate" {
                format!("crate::{}", module_name)
            } else {
                format!("{}::{}", current_module, module_name)
            };

            if let Some(target_file) = self.module_to_file.get(&target_module) {
                return Some(target_file.clone());
            }
        }

        None
    }
}

impl LanguageResolver for RustResolver {
    fn build_module_map(&mut self, files: &[PathBuf], project_root: &Path) {
        self.project_root = project_root.to_path_buf();

        for file_path in files {
            if let Some(module_path) = self.file_path_to_module_path(file_path) {
                self.module_to_file
                    .insert(module_path.clone(), file_path.clone());
                self.file_to_module.insert(file_path.clone(), module_path);
            }
        }
    }

    fn resolve_import(&self, import_path: &str, from_file: &Path) -> Option<PathBuf> {
        // Check if this is a module declaration (no :: in the path suggests it might be)
        if !import_path.contains("::") && !import_path.starts_with("crate") {
            // Try to resolve as a module declaration first
            if let Some(module_file) = self.resolve_module_declaration(import_path, from_file) {
                return Some(module_file);
            }
        }

        if import_path.starts_with("crate::") {
            // Absolute crate import
            self.module_to_file.get(import_path).cloned()
        } else if import_path.starts_with("super::") {
            // Super import - go up one module level
            if let Some(current_module) = self.file_to_module.get(from_file) {
                let super_import = import_path.strip_prefix("super::").unwrap();
                let current_parts: Vec<&str> = current_module.split("::").collect();
                if current_parts.len() > 1 {
                    let mut new_parts = current_parts[..current_parts.len() - 1].to_vec();
                    new_parts.extend(super_import.split("::"));
                    let resolved_path = new_parts.join("::");
                    self.module_to_file.get(&resolved_path).cloned()
                } else {
                    None
                }
            } else {
                None
            }
        } else if import_path.starts_with("self::") {
            // Self import - same module level
            if let Some(current_module) = self.file_to_module.get(from_file) {
                let self_import = import_path.strip_prefix("self::").unwrap();
                let current_parts: Vec<&str> = current_module.split("::").collect();
                if current_parts.len() > 1 {
                    let mut new_parts = current_parts[..current_parts.len() - 1].to_vec();
                    new_parts.extend(self_import.split("::"));
                    let resolved_path = new_parts.join("::");
                    self.module_to_file.get(&resolved_path).cloned()
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            // Try to resolve as relative import or direct module name
            if let Some(current_module) = self.file_to_module.get(from_file) {
                let current_parts: Vec<&str> = current_module.split("::").collect();
                if current_parts.len() <= 1 {
                    return None;
                }

                // Try as sibling module
                let mut new_parts = current_parts[..current_parts.len() - 1].to_vec();
                new_parts.extend(import_path.split("::"));
                let resolved_path = new_parts.join("::");
                self.module_to_file.get(&resolved_path).cloned()
            } else {
                None
            }
        }
    }

    fn resolve_external_references(
        &self,
        references: &HashSet<String>,
        _from_file: &Path,
    ) -> Vec<PathBuf> {
        let mut resolved = Vec::new();

        for ext_ref in references {
            // Try to resolve external references as module paths
            let potential_module = if ext_ref.contains("::") {
                format!("crate::{}", ext_ref)
            } else {
                format!("crate::{}", ext_ref)
            };

            if let Some(target_file) = self.module_to_file.get(&potential_module) {
                resolved.push(target_file.clone());
            }
        }

        resolved
    }
}
