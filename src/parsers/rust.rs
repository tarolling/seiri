use crate::core::defs::{FileNode, Import, Language};
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use tree_sitter::Parser;
use tree_sitter_rust as ts_rust;

/// Get node text
fn get_text(n: tree_sitter::Node, code: &str) -> String {
    n.utf8_text(code.as_bytes()).unwrap_or("").to_string()
}

/// Determine if an import is local (starts with crate/self/super or current mod)
fn is_local_import(import_path: &str, file_path: &Path) -> bool {
    import_path.starts_with("crate::")
        || import_path.starts_with("self::")
        || import_path.starts_with("super::")
        || {
            // Also treat module-relative imports as local (e.g., modname::foo)
            if let Some(stem) = file_path.file_stem().and_then(|s| s.to_str()) {
                import_path.starts_with(&format!("{stem}::"))
            } else {
                false
            }
        }
}

// Returns the qualifed name of the scoped identifier
// fn parse_scoped_identifier(node: tree_sitter::Node, code: &str) -> str {
//     let scoped_id
//     let mut cursor = node.walk();
//     for child in node.children(&mut cursor) {

//     }

// }

/// Extract all import paths from a use declaration, handling use lists
fn extract_use_paths(node: tree_sitter::Node, code: &str) -> Vec<String> {
    let mut paths = Vec::new();
    extract_paths_from_use_clause(node, code, &mut paths);
    paths
}

fn parse_use_list(node: tree_sitter::Node, code: &str, _prefix: &str) -> Vec<String> {
    let mut cursor = node.walk();
    let mut imports: Vec<String> = vec![];

    for child in node.children(&mut cursor) {
        match child.kind() {
            "identifier" | "scoped_identifier" => {
                imports.push(get_text(child, code));
            }
            "scoped_use_list" => {
                for s in parse_scoped_use_list(child, code, "") {
                    imports.push(s.to_string());
                }
            }
            _ => {}
        }
    }

    imports
}

fn parse_scoped_use_list(node: tree_sitter::Node, code: &str, prefix: &str) -> Vec<String> {
    let mut imports: Vec<String> = vec![];
    let mut new_prefix = prefix.to_string();
    let mut cursor = node.walk();

    for child in node.children(&mut cursor) {
        match child.kind() {
            "crate" | "identifier" | "scoped_identifier" => {
                new_prefix.push_str(&(get_text(child, code) + "::"));
            }
            "use_list" => {
                parse_use_list(child, code, &new_prefix)
                    .iter()
                    .for_each(|s| imports.push(new_prefix.clone() + s));
            }
            "scoped_use_list" => {
                parse_scoped_use_list(child, code, &new_prefix)
                    .iter()
                    .for_each(|s| imports.push(new_prefix.clone() + s));
            }
            "::" => {} // skip
            _ => {}
        }
    }

    imports
}

/// Recursively extract paths from a use clause, building up the prefix
fn extract_paths_from_use_clause(node: tree_sitter::Node, code: &str, paths: &mut Vec<String>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "use" | ";" => {
                continue;
            }
            "scoped_identifier" => {
                paths.push(get_text(child, code));
            }
            "scoped_use_list" => {
                parse_scoped_use_list(child, code, "")
                    .iter()
                    .for_each(|s| paths.push(s.to_string()));
            }
            "use_as_clause" => {
                // Handle `foo as bar` - we want the original name (foo)
                if let Some(import_path) = child.child(0)
                    && (import_path.kind() == "identifier"
                        || import_path.kind() == "scoped_identifier")
                {
                    paths.push(get_text(import_path, code));
                }
            }
            // "use_wildcard" => {
            //     // Handle `foo::*` - just add the prefix
            //     if !prefix.is_empty() {
            //         paths.push(format!("{}::*", prefix));
            //     }
            // }
            _ => {}
        }
    }
}

fn parser_loop<P: AsRef<Path>>(
    path: P,
    code: &str,
    root_node: tree_sitter::Node<'_>,
) -> Option<FileNode> {
    let loc = code.matches("\n").count() as u32 + 1; // count number of newlines bc code.lines() has failed me

    let mut imports = Vec::new();
    let mut functions = Vec::new();
    let mut containers = Vec::new();
    let mut external_references = HashSet::new();

    let mut cursor = root_node.walk();
    let mut stack = vec![root_node];

    while let Some(node) = stack.pop() {
        match node.kind() {
            "use_declaration" => {
                let import_paths = extract_use_paths(node, code);
                for import_path in import_paths {
                    if !import_path.is_empty() {
                        let is_local = is_local_import(&import_path, path.as_ref());
                        imports.push(Import::new(import_path, is_local));
                    }
                }
            }
            "mod_item" => {
                // Handle module declarations like "pub mod python;" or "mod utils;"
                let mut mod_name = String::new();
                let mut is_declaration = false;

                for child in node.children(&mut cursor) {
                    if child.kind() == "identifier" {
                        mod_name = get_text(child, code);
                    } else if child.kind() == ";" {
                        // If we find a semicolon, this is a module declaration (not inline definition)
                        is_declaration = true;
                    }
                }

                // Only add as import if it's a declaration (has semicolon)
                if !mod_name.is_empty() && is_declaration {
                    imports.push(Import::new(mod_name, true));
                }
            }
            "function_item" | "function_signature_item" => {
                // Get function name - look for the first identifier after any visibility modifiers
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() == "identifier" {
                        let name = get_text(child, code);
                        functions.push(name);
                    }
                }
            }
            "struct_item" | "enum_item" | "trait_item" | "impl_item" => {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() == "type_identifier" {
                        let name = get_text(child, code);
                        containers.push(name);
                    }
                }
            }
            // For external references, look for scoped identifiers (e.g., foo::bar)
            "scoped_identifier" => {
                let text = get_text(node, code);
                external_references.insert(text);
            }
            _ => {}
        }

        // push children onto stack
        let mut child_cursor = node.walk();
        for child in node.children(&mut child_cursor) {
            stack.push(child);
        }
    }

    Some(FileNode::new(
        path.as_ref().to_path_buf(),
        loc,
        Language::Rust,
        imports,
        functions,
        containers,
        external_references,
    ))
}

/// Parse a Rust file and extract its structure
pub fn parse_rust_file<P: AsRef<Path>>(path: P) -> Option<FileNode> {
    let code = fs::read_to_string(&path).ok()?;

    let mut parser = Parser::new();
    parser
        .set_language(&ts_rust::LANGUAGE.into())
        .expect("Error loading Rust grammar");
    let tree = parser.parse(&code, None)?;
    let root_node = tree.root_node();

    parser_loop(path, &code, root_node)
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
    fn test_external_import() {
        let temp_dir = TempDir::new().unwrap();
        let content = r#"use std::path::Path;"#;
        let file_path = create_test_file(&temp_dir, "test.rs", content);

        let result = parse_rust_file(&file_path).unwrap();
        let imports: Vec<_> = result.imports().iter().collect();

        assert!(
            imports
                .iter()
                .any(|i| i.path() == "std::path::Path" && !i.is_local())
        );
    }

    #[test]
    fn test_crate_import() {
        let temp_dir = TempDir::new().unwrap();
        let content = r#"use crate::core::defs::FileNode;"#;
        let file_path = create_test_file(&temp_dir, "test.rs", content);

        let result = parse_rust_file(&file_path).unwrap();
        let imports: Vec<_> = result.imports().iter().collect();

        assert!(
            imports
                .iter()
                .any(|i| i.path() == "crate::core::defs::FileNode" && i.is_local())
        );
    }

    #[test]
    fn test_self_import() {
        let temp_dir = TempDir::new().unwrap();
        let content = r#"use self::internal::stuff;"#;
        let file_path = create_test_file(&temp_dir, "test.rs", content);

        let result = parse_rust_file(&file_path).unwrap();
        let imports: Vec<_> = result.imports().iter().collect();

        assert!(
            imports
                .iter()
                .any(|i| i.path() == "self::internal::stuff" && i.is_local())
        );
    }

    #[test]
    fn test_super_import() {
        let temp_dir = TempDir::new().unwrap();
        let content = r#"use super::internal::stuff;"#;
        let file_path = create_test_file(&temp_dir, "test.rs", content);

        let result = parse_rust_file(&file_path).unwrap();
        let imports: Vec<_> = result.imports().iter().collect();

        assert!(
            imports
                .iter()
                .any(|i| i.path() == "super::internal::stuff" && i.is_local())
        );
    }

    #[test]
    fn test_use_list_import() {
        let temp_dir = TempDir::new().unwrap();
        let content = r#"use std::{fs::File, io::Write};"#;
        let file_path = create_test_file(&temp_dir, "test.rs", content);

        let result = parse_rust_file(&file_path).unwrap();
        let imports: Vec<_> = result.imports().iter().collect();

        assert!(
            imports
                .iter()
                .any(|i| i.path() == "std::fs::File" && !i.is_local())
        );
        assert!(
            imports
                .iter()
                .any(|i| i.path() == "std::io::Write" && !i.is_local())
        );
    }

    #[test]
    fn test_basic_imports() {
        let temp_dir = TempDir::new().unwrap();
        let content = r#"
use std::fs;
use std::path::Path;
use crate::core::defs::FileNode;
use super::utils::helper;
use self::internal::stuff;
use tree_sitter as ts;
        "#;
        let file_path = create_test_file(&temp_dir, "test.rs", content);

        let result = parse_rust_file(&file_path).unwrap();
        let imports: Vec<_> = result.imports().iter().collect();

        // Test standard library imports
        assert!(
            imports
                .iter()
                .any(|i| i.path() == "std::fs" && !i.is_local())
        );
        assert!(
            imports
                .iter()
                .any(|i| i.path() == "std::path::Path" && !i.is_local())
        );

        // Test crate-relative imports
        assert!(
            imports
                .iter()
                .any(|i| i.path() == "crate::core::defs::FileNode" && i.is_local())
        );

        // Test super/self imports
        assert!(
            imports
                .iter()
                .any(|i| i.path() == "super::utils::helper" && i.is_local())
        );
        assert!(
            imports
                .iter()
                .any(|i| i.path() == "self::internal::stuff" && i.is_local())
        );

        // Test aliased imports
        assert!(
            imports
                .iter()
                .any(|i| i.path() == "tree_sitter" && !i.is_local())
        );
    }

    #[test]
    fn test_nested_list_imports() {
        let temp_dir = TempDir::new().unwrap();
        let content = r#"
use std::{
    fs::File,
    path::{Path, PathBuf},
    io::Write,
};
use crate::{
    core::defs::{FileNode, Import},
    utils::{helper, tools},
};
        "#;
        let file_path = create_test_file(&temp_dir, "test.rs", content);

        let result = parse_rust_file(&file_path).unwrap();
        let imports: Vec<_> = result.imports().iter().collect();

        println!("IMPORTS: {:?}", imports);

        // Test std imports
        assert!(
            imports
                .iter()
                .any(|i| i.path() == "std::fs::File" && !i.is_local())
        );
        assert!(
            imports
                .iter()
                .any(|i| i.path() == "std::path::Path" && !i.is_local())
        );
        assert!(
            imports
                .iter()
                .any(|i| i.path() == "std::path::PathBuf" && !i.is_local())
        );
        assert!(
            imports
                .iter()
                .any(|i| i.path() == "std::io::Write" && !i.is_local())
        );

        // Test crate imports
        assert!(
            imports
                .iter()
                .any(|i| i.path() == "crate::core::defs::FileNode" && i.is_local())
        );
        assert!(
            imports
                .iter()
                .any(|i| i.path() == "crate::core::defs::Import" && i.is_local())
        );
        assert!(
            imports
                .iter()
                .any(|i| i.path() == "crate::utils::helper" && i.is_local())
        );
        assert!(
            imports
                .iter()
                .any(|i| i.path() == "crate::utils::tools" && i.is_local())
        );
    }

    #[test]
    fn test_functions_and_containers() {
        let temp_dir = TempDir::new().unwrap();
        let content = r#"
pub fn public_function() {}
fn private_function() {}

pub(crate) struct MyStruct {
    field: i32,
}

enum MyEnum {
    Variant1,
    Variant2,
}

trait MyTrait {
    fn trait_method(&self);
}

impl MyStruct {
    fn impl_method(&self) {}
}
        "#;
        let file_path = create_test_file(&temp_dir, "test.rs", content);

        let result = parse_rust_file(&file_path).unwrap();

        // Check functions
        let functions: Vec<_> = result.functions().iter().collect();
        assert!(functions.iter().any(|f| *f == "public_function"));
        assert!(functions.iter().any(|f| *f == "private_function"));
        assert!(functions.iter().any(|f| *f == "impl_method"));
        assert!(functions.iter().any(|f| *f == "trait_method"));

        // Check containers
        let containers: Vec<_> = result.containers().iter().collect();
        assert!(containers.iter().any(|c| *c == "MyStruct"));
        assert!(containers.iter().any(|c| *c == "MyEnum"));
        assert!(containers.iter().any(|c| *c == "MyTrait"));
    }

    #[test]
    fn test_external_references() {
        let temp_dir = TempDir::new().unwrap();
        let content = r#"
fn test_function() {
    let path = std::path::PathBuf::new();
    fs::File::create(path).unwrap();
    some_module::some_function();
}
        "#;
        let file_path = create_test_file(&temp_dir, "test.rs", content);

        let result = parse_rust_file(&file_path).unwrap();
        let refs: Vec<_> = result.external_references().iter().collect();

        assert!(refs.iter().any(|r| *r == "std::path::PathBuf"));
        assert!(refs.iter().any(|r| *r == "some_module::some_function"));
    }

    #[test]
    fn test_lines_of_code() {
        let temp_dir = TempDir::new().unwrap();
        let content = r#"// This is a comment
use std::fs;

fn function1() {
    println!("Hello");
}

fn function2() {
    // Another comment
    println!("World");
}"#;
        let file_path = create_test_file(&temp_dir, "test.rs", content);

        let result = parse_rust_file(&file_path).unwrap();

        assert_eq!(result.loc(), 11);
    }

    #[test]
    fn test_lines_of_code_newlines() {
        let temp_dir = TempDir::new().unwrap();
        let content = r#"
// This is a comment
use std::fs;

fn function1() {
    println!("Hello");
}

fn function2() {
    // Another comment
    println!("World");
}
"#;
        let file_path = create_test_file(&temp_dir, "test.rs", content);

        let result = parse_rust_file(&file_path).unwrap();

        assert_eq!(result.loc(), 13);
    }

    #[test]
    fn test_module_declarations() {
        let temp_dir = TempDir::new().unwrap();
        let content = r#"
pub mod utils;  // External module
mod internal {  // Inline module
    fn internal_function() {}
}
mod tests;  // External test module
        "#;
        let file_path = create_test_file(&temp_dir, "test.rs", content);

        let result = parse_rust_file(&file_path).unwrap();
        let imports: Vec<_> = result.imports().iter().collect();

        // Only external module declarations should be treated as imports
        assert!(imports.iter().any(|i| i.path() == "utils" && i.is_local()));
        assert!(imports.iter().any(|i| i.path() == "tests" && i.is_local()));
        assert_eq!(imports.len(), 2); // internal module should not be included
    }
}
