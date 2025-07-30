use crate::core::defs::{FileNode, Import, Language};
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use tree_sitter::Parser;
use tree_sitter_python;

/// Helper to determine if an import is local
fn is_local_import(import_path: &str, file_path: &Path) -> bool {
    // In Python, local imports are typically relative (starting with .) or 
    // match the project's package structure
    if import_path.starts_with('.') {
        return true;
    }
    
    // Check if the import matches the current directory structure
    if let Some(parent) = file_path.parent() {
        let parts: Vec<_> = import_path.split('.').collect();
        let mut current_dir = parent.to_path_buf();
        
        for part in parts {
            current_dir.push(part);
            if current_dir.with_extension("py").exists() || current_dir.join("__init__.py").exists() {
                return true;
            }
            current_dir.pop();
        }
    }
    
    false
}

/// Extract import path from an import statement
fn extract_import_path(node: tree_sitter::Node, code: &str) -> Vec<String> {
    let mut imports = Vec::new();
    let mut cursor = node.walk();

    // Helper function to get node text
    let get_text = |n: tree_sitter::Node| -> String {
        n.utf8_text(code.as_bytes()).unwrap_or("").to_string()
    };

    match node.kind() {
        "import_statement" => {
            // Handle "import x.y.z" and "import x.y.z as w"
            for child in node.children(&mut cursor) {
                match child.kind() {
                    "dotted_name" => {
                        let mut path = Vec::new();
                        let mut name_cursor = child.walk();
                        for name_part in child.children(&mut name_cursor) {
                            if name_part.kind() == "identifier" {
                                path.push(get_text(name_part));
                            }
                        }
                        if !path.is_empty() {
                            imports.push(path.join("."));
                        }
                    }
                    "aliased_import" => {
                        if let Some(name_node) = child.child_by_field_name("name") {
                            let mut path = Vec::new();
                            let mut name_cursor = name_node.walk();
                            for name_part in name_node.children(&mut name_cursor) {
                                if name_part.kind() == "identifier" {
                                    path.push(get_text(name_part));
                                }
                            }
                            if !path.is_empty() {
                                imports.push(path.join("."));
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        "import_from_statement" => {
            // Handle "from x.y.z import a, b, c" and "from . import x"
            let mut from_path = String::new();
            let mut relative_dots = 0;

            for child in node.children(&mut cursor) {
                match child.kind() {
                    "dotted_name" => {
                        let mut parts = Vec::new();
                        let mut name_cursor = child.walk();
                        for name_part in child.children(&mut name_cursor) {
                            if name_part.kind() == "identifier" {
                                parts.push(get_text(name_part));
                            }
                        }
                        if !parts.is_empty() {
                            from_path = parts.join(".");
                        }
                    }
                    "import_from_level" => {
                        relative_dots = child.child_count();
                    }
                    "import_suffix" | "aliased_import" => {
                        let name = if child.kind() == "aliased_import" {
                            if let Some(name_node) = child.child_by_field_name("name") {
                                get_text(name_node)
                            } else {
                                continue;
                            }
                        } else {
                            get_text(child)
                        };

                        let prefix = if relative_dots > 0 {
                            ".".repeat(relative_dots)
                        } else if !from_path.is_empty() {
                            from_path.clone()
                        } else {
                            continue;
                        };

                        if !name.is_empty() {
                            imports.push(if relative_dots > 0 {
                                format!("{}{}", prefix, name)
                            } else {
                                prefix
                            });
                        }
                    }
                    _ => {}
                }
            }
        }
        _ => {}
    }

    imports
}

pub fn parse_python_file<P: AsRef<Path>>(path: P) -> Option<FileNode> {
    let code = fs::read_to_string(&path).ok()?;

    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_python::LANGUAGE.into())
        .expect("Error loading Python grammar");
    let tree = parser.parse(&code, None)?;
    let root_node = tree.root_node();

    let mut imports = Vec::new();
    let mut functions = Vec::new();
    let mut containers = Vec::new();
    let mut external_references = HashSet::new();

    // Traverse the syntax tree
    let mut cursor = root_node.walk();
    let mut stack = vec![root_node];

    while let Some(node) = stack.pop() {
        match node.kind() {
            "import_statement" | "import_from_statement" => {
                // Handle both "import foo" and "from foo import bar"
                let import_paths = extract_import_path(node, &code);
                for import_path in import_paths {
                    let is_local = is_local_import(&import_path, path.as_ref());
                    imports.push(Import {
                        path: import_path,
                        is_local,
                    });
                }
            }
            "function_definition" => {
                // Get function name
                if let Some(name_node) = node
                    .children(&mut cursor)
                    .find(|n| n.kind() == "identifier")
                {
                    let name = name_node
                        .utf8_text(code.as_bytes())
                        .unwrap_or("")
                        .to_string();
                    if !name.starts_with('_') || name.starts_with("__") {
                        // Include public functions and dunder methods
                        functions.push(name);
                    }
                }
            }
            "class_definition" => {
                // Get class name
                if let Some(name_node) = node
                    .children(&mut cursor)
                    .find(|n| n.kind() == "identifier")
                {
                    let name = name_node
                        .utf8_text(code.as_bytes())
                        .unwrap_or("")
                        .to_string();
                    containers.push(name);
                }
            }
            "attribute" | "call" => {
                // Collect external references from attribute access and function calls
                let text = node.utf8_text(code.as_bytes()).unwrap_or("").to_string();
                if !text.starts_with('_') {
                    // Only include public attributes/calls
                    external_references.insert(text);
                }
            }
            _ => {}
        }

        // Push children to stack
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }

    Some(FileNode {
        file: path.as_ref().to_path_buf(),
        language: Language::Python,
        imports,
        functions,
        containers,
        external_references,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::fs::File;
    use std::io::Write;

    fn create_test_file(dir: &TempDir, filename: &str, content: &str) -> std::path::PathBuf {
        let file_path = dir.path().join(filename);
        let mut file = File::create(&file_path).unwrap();
        file.write_all(content.as_bytes()).unwrap();
        file_path
    }

    #[test]
    fn test_basic_imports() {
        let temp_dir = TempDir::new().unwrap();
        let content = r#"
import os
import sys as system
from pathlib import Path
from datetime import datetime as dt
from .local_module import something
from ..parent_module import another_thing
        "#;
        let file_path = create_test_file(&temp_dir, "test.py", content);
        
        let result = parse_python_file(&file_path).unwrap();
        let import_paths: Vec<_> = result.imports.iter().map(|i| i.path.as_str()).collect();
        
        assert!(import_paths.contains(&"os"));
        assert!(import_paths.contains(&"sys"));
        assert!(import_paths.contains(&"pathlib"));
        assert!(import_paths.contains(&"datetime"));
        assert!(import_paths.contains(&"local_module"));
        assert!(import_paths.contains(&"parent_module"));
    }

    #[test]
    fn test_local_imports() {
        let temp_dir = TempDir::new().unwrap();
        
        // Create a local module
        std::fs::create_dir(temp_dir.path().join("mypackage")).unwrap();
        create_test_file(&temp_dir, "mypackage/__init__.py", "");
        create_test_file(&temp_dir, "mypackage/module.py", "");
        
        let content = r#"
from mypackage.module import thing
from .relative_module import other_thing
import sys
        "#;
        let file_path = create_test_file(&temp_dir, "test.py", content);
        
        let result = parse_python_file(&file_path).unwrap();
        let local_imports: Vec<_> = result.imports.iter()
            .filter(|i| i.is_local)
            .map(|i| i.path.as_str())
            .collect();
        let external_imports: Vec<_> = result.imports.iter()
            .filter(|i| !i.is_local)
            .map(|i| i.path.as_str())
            .collect();
        
        assert!(local_imports.contains(&"mypackage.module"));
        assert!(local_imports.contains(&"relative_module"));
        assert!(external_imports.contains(&"sys"));
    }

    #[test]
    fn test_functions_and_classes() {
        let temp_dir = TempDir::new().unwrap();
        let content = r#"
def public_function():
    pass

def _private_function():
    pass

def __dunder_method__():
    pass

class PublicClass:
    def method(self):
        pass

class _PrivateClass:
    pass
        "#;
        let file_path = create_test_file(&temp_dir, "test.py", content);
        
        let result = parse_python_file(&file_path).unwrap();
        
        // Check functions
        assert!(result.functions.contains(&"public_function".to_string()));
        assert!(!result.functions.contains(&"_private_function".to_string()));
        assert!(result.functions.contains(&"__dunder_method__".to_string()));
        
        // Check classes
        assert!(result.containers.contains(&"PublicClass".to_string()));
        assert!(result.containers.contains(&"_PrivateClass".to_string()));
    }

    #[test]
    fn test_external_references() {
        let temp_dir = TempDir::new().unwrap();
        let content = r#"
import os

def some_function():
    path = os.path.join("a", "b")
    os.makedirs(path)
    return path.exists()
        "#;
        let file_path = create_test_file(&temp_dir, "test.py", content);
        
        let result = parse_python_file(&file_path).unwrap();
        let refs: Vec<_> = result.external_references.iter().collect();
        
        assert!(refs.iter().any(|&r| r == "path"));
        assert!(refs.iter().any(|&r| r == "join"));
        assert!(refs.iter().any(|&r| r == "makedirs"));
        assert!(refs.iter().any(|&r| r == "exists"));
    }

    #[test]
    fn test_complex_imports() {
        let temp_dir = TempDir::new().unwrap();
        let content = r#"
from os import (
    path as p,
    makedirs,
    remove
)
from typing import (
    List,
    Optional as Opt,
    Dict
)
        "#;
        let file_path = create_test_file(&temp_dir, "test.py", content);
        
        let result = parse_python_file(&file_path).unwrap();
        let import_paths: Vec<_> = result.imports.iter().map(|i| i.path.as_str()).collect();
        
        assert!(import_paths.contains(&"os"));
        assert!(import_paths.contains(&"typing"));
    }

    #[test]
    fn test_nested_structures() {
        let temp_dir = TempDir::new().unwrap();
        let content = r#"
class OuterClass:
    class InnerClass:
        def inner_method(self):
            pass
    
    def outer_method(self):
        def local_function():
            pass
        return local_function
        "#;
        let file_path = create_test_file(&temp_dir, "test.py", content);
        
        let result = parse_python_file(&file_path).unwrap();
        
        assert!(result.containers.contains(&"OuterClass".to_string()));
        assert!(result.containers.contains(&"InnerClass".to_string()));
        assert!(result.functions.contains(&"outer_method".to_string()));
        assert!(result.functions.contains(&"inner_method".to_string()));
        // local_function is not captured as it's a nested function
        assert!(!result.functions.contains(&"local_function".to_string()));
    }
}

