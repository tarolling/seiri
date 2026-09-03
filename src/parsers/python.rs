use crate::core::defs::{FileNode, Import, Language};
use crate::parsers::get_text;
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use tree_sitter::Parser;
use tree_sitter_python as ts_python;

/// Determine if an import is local. In Python, local imports are typically relative (starting with .) or
/// match the project's package structure.
fn is_local_import(import_path: &str, file_path: &Path) -> bool {
    if import_path.starts_with('.') {
        return true;
    }

    // Check if the import matches the current directory structure
    if let Some(parent) = file_path.parent() {
        let parts: Vec<_> = import_path.split('.').collect();
        let mut current_dir = parent.to_path_buf();

        for part in parts {
            current_dir.push(part);
            if current_dir.with_extension("py").exists() || current_dir.join("__init__.py").exists()
            {
                return true;
            }
            current_dir.pop();
        }
    }

    false
}

/// Recursively extract the callee/attribute path of a `call` or `attribute` node, stripping
/// any call-argument text so e.g. `foo(1)` and `obj.method(a, b)` normalize to `foo` and
/// `obj.method` instead of one distinct "reference" per call site/argument list.
fn callee_text(node: tree_sitter::Node, code: &str) -> String {
    match node.kind() {
        "call" => node
            .child_by_field_name("function")
            .map(|function| callee_text(function, code))
            .unwrap_or_default(),
        "attribute" => {
            let object = node
                .child_by_field_name("object")
                .map(|object| callee_text(object, code))
                .unwrap_or_default();
            let attribute = node
                .child_by_field_name("attribute")
                .map(|attribute| get_text(attribute, code))
                .unwrap_or_default();
            if object.is_empty() {
                attribute
            } else {
                format!("{object}.{attribute}")
            }
        }
        _ => get_text(node, code),
    }
}

/// Join the `identifier` children of a `dotted_name` node with `.`.
fn dotted_name_text(node: tree_sitter::Node, code: &str) -> String {
    let mut parts = Vec::new();
    let mut cursor = node.walk();
    for part in node.children(&mut cursor) {
        if part.kind() == "identifier" {
            parts.push(get_text(part, code));
        }
    }
    parts.join(".")
}

/// Extract import path from an import statement.
fn extract_import_path(node: tree_sitter::Node, code: &str) -> Vec<String> {
    let mut imports = Vec::new();
    let mut cursor = node.walk();

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
                                path.push(get_text(name_part, code));
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
                                    path.push(get_text(name_part, code));
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
            // handle "from x.y.z import a, b, c" and "from . import x"
            let mut dot_count = 0;
            let mut base_path = String::new();

            if let Some(module_name) = node.child_by_field_name("module_name") {
                match module_name.kind() {
                    "relative_import" => {
                        let mut rel_cursor = module_name.walk();
                        for child in module_name.children(&mut rel_cursor) {
                            match child.kind() {
                                "import_prefix" => {
                                    dot_count =
                                        get_text(child, code).chars().filter(|&c| c == '.').count();
                                }
                                "dotted_name" => {
                                    base_path = dotted_name_text(child, code);
                                }
                                _ => {}
                            }
                        }
                    }
                    "dotted_name" => {
                        base_path = dotted_name_text(module_name, code);
                    }
                    _ => {}
                }
            }

            if dot_count > 0 && base_path.is_empty() {
                // "from . import x, y" (and "from .. import x", etc.) - there is no
                // module after the dots, so each imported name is itself a module
                // that lives `dot_count` levels up from the current package
                let mut name_cursor = node.walk();
                for name_node in node.children_by_field_name("name", &mut name_cursor) {
                    let name_text = match name_node.kind() {
                        "dotted_name" => dotted_name_text(name_node, code),
                        "aliased_import" => name_node
                            .child_by_field_name("name")
                            .map(|n| dotted_name_text(n, code))
                            .unwrap_or_default(),
                        _ => String::new(),
                    };
                    if !name_text.is_empty() {
                        imports.push(format!("{}{}", ".".repeat(dot_count), name_text));
                    }
                }
            } else if dot_count > 0 {
                imports.push(format!("{}{}", ".".repeat(dot_count), base_path));
            } else if !base_path.is_empty() {
                imports.push(base_path);
            }
        }
        _ => {}
    }

    imports
}

pub fn parse_python_file<P: AsRef<Path>>(path: P) -> Option<FileNode> {
    let code = fs::read_to_string(&path).ok()?;
    let loc = code.matches("\n").count() as u32 + 1; // count number of newlines bc code.lines() has failed me

    let mut parser = Parser::new();
    parser.set_language(&ts_python::LANGUAGE.into()).ok()?;
    let tree = parser.parse(&code, None)?;
    let root_node = tree.root_node();

    let mut imports = HashSet::new();
    let mut functions = HashSet::new();
    let mut containers = HashSet::new();
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
                    imports.insert(Import::new(import_path, is_local));
                }
            }
            "function_definition" => {
                // Get function name
                if let Some(name_node) = node
                    .children(&mut cursor)
                    .find(|n| n.kind() == "identifier")
                {
                    let name = get_text(name_node, &code);
                    let in_function = node.parent().is_some_and(|p| p.kind() == "block")
                        && node
                            .parent()
                            .unwrap()
                            .parent()
                            .is_some_and(|p| p.kind() == "function_definition");
                    if (!name.starts_with('_') || name.starts_with("__")) && !in_function {
                        functions.insert(name);
                    }
                }
            }
            "class_definition" => {
                if let Some(name_node) = node
                    .children(&mut cursor)
                    .find(|n| n.kind() == "identifier")
                {
                    containers.insert(get_text(name_node, &code));
                }
            }
            "attribute" | "call" => {
                // Collect external references from attribute access and function calls,
                // normalized to the callee/attribute path with argument text stripped
                let text = callee_text(node, &code);
                if !text.is_empty() && !text.starts_with('_') {
                    // Only include public attributes/calls
                    external_references.insert(text);
                }
            }
            _ => {}
        }

        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }

    Some(FileNode::new(
        path.as_ref().to_path_buf(),
        loc,
        Language::Python,
        imports,
        functions,
        containers,
        external_references,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::TempDir;

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
        let import_paths: Vec<_> = result.imports().iter().map(|i| i.path()).collect();

        assert!(import_paths.contains(&"os"));
        assert!(import_paths.contains(&"sys"));
        assert!(import_paths.contains(&"pathlib"));
        assert!(import_paths.contains(&"datetime"));
        assert!(import_paths.contains(&".local_module"));
        assert!(import_paths.contains(&"..parent_module"));
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
        let local_imports: Vec<_> = result
            .imports()
            .iter()
            .filter(|i| i.is_local())
            .map(|i| i.path())
            .collect();
        let external_imports: Vec<_> = result
            .imports()
            .iter()
            .filter(|i| !i.is_local())
            .map(|i| i.path())
            .collect();

        assert!(local_imports.contains(&"mypackage.module"));
        assert!(local_imports.contains(&".relative_module"));
        assert!(external_imports.contains(&"sys"));
    }

    #[test]
    fn test_relative_import_dot_counting() {
        let temp_dir = TempDir::new().unwrap();
        let content = r#"
from . import sibling
from .. import cousin
from ... import great_uncle
from .pkg import thing
from ..pkg.sub import other_thing
from . import a as b
        "#;
        let file_path = create_test_file(&temp_dir, "test.py", content);

        let result = parse_python_file(&file_path).unwrap();
        let import_paths: Vec<_> = result.imports().iter().map(|i| i.path()).collect();

        // "from . import x" has no module name, so the imported name IS the
        // sibling module being referenced; it must not collapse to ""
        assert!(import_paths.contains(&".sibling"));
        assert!(!import_paths.contains(&""));

        // dot count must reflect the actual number of leading dots, not the
        // relative_import node's child count
        assert!(import_paths.contains(&"..cousin"));
        assert!(import_paths.contains(&"...great_uncle"));

        // when a module name follows the dots, that module is the edge;
        // imported symbol names are not appended
        assert!(import_paths.contains(&".pkg"));
        assert!(import_paths.contains(&"..pkg.sub"));

        // aliased relative imports use the real name, not the alias
        assert!(import_paths.contains(&".a"));
        assert!(!import_paths.contains(&".b"));
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
        assert!(result.functions().contains(&"public_function".to_string()));
        assert!(
            !result
                .functions()
                .contains(&"_private_function".to_string())
        );
        assert!(
            result
                .functions()
                .contains(&"__dunder_method__".to_string())
        );

        // Check classes
        assert!(result.containers().contains(&"PublicClass".to_string()));
        assert!(result.containers().contains(&"_PrivateClass".to_string()));
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
        let import_paths: Vec<_> = result.imports().iter().map(|i| i.path()).collect();

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

        assert!(result.containers().contains(&"OuterClass".to_string()));
        assert!(result.containers().contains(&"InnerClass".to_string()));
        assert!(result.functions().contains(&"outer_method".to_string()));
        assert!(result.functions().contains(&"inner_method".to_string()));
        // local_function is not captured as it's a nested function
        assert!(!result.functions().contains(&"local_function".to_string()));
    }

    #[test]
    fn test_call_references_normalize_callee() {
        let temp_dir = TempDir::new().unwrap();
        let content = r#"
foo(1)
foo(2)
obj.method(arg)
chained.call().another(1, 2)
"#;
        let file_path = create_test_file(&temp_dir, "test.py", content);

        let result = parse_python_file(&file_path).unwrap();
        let refs = result.external_references();

        // calling the same function with different arguments should normalize
        // to a single callee reference, not one entry per call site
        assert!(refs.contains("foo"));
        assert!(!refs.iter().any(|r| r.contains('(')));
        assert!(!refs.contains("foo(1)"));
        assert!(!refs.contains("foo(2)"));

        // attribute-call callees normalize to the dotted attribute path
        assert!(refs.contains("obj.method"));
        assert!(!refs.contains("obj.method(arg)"));

        // nested/chained calls strip argument text at every level
        assert!(refs.contains("chained.call.another"));
    }

    #[test]
    fn test_lines_of_code() {
        let temp_dir = TempDir::new().unwrap();
        let content = r#"# This is a comment
import os
import sys

def example_function():
    # Another comment
    print("Hello, World!")  # Inline comment

def another_function():
    pass

class ExampleClass:
    def method(self):
        pass
"#;
        let file_path = create_test_file(&temp_dir, "test.py", content);

        let result = parse_python_file(&file_path).unwrap();

        assert_eq!(result.loc(), 15);
    }
}
