use crate::core::defs::{Import, Language, Node};
use std::fs;
use std::path::Path;
use tree_sitter::Parser;
use tree_sitter_rust;

/// Parses a Rust file and returns a Node with imports, functions, containers, and external references.

/// Helper to determine if an import is local (starts with crate/self/super or current mod)
fn is_local_import(import_path: &str, file_path: &Path) -> bool {
    import_path.starts_with("crate::")
        || import_path.starts_with("self::")
        || import_path.starts_with("super::")
        || {
            // Also treat module-relative imports as local (e.g., modname::foo)
            if let Some(stem) = file_path.file_stem().and_then(|s| s.to_str()) {
                import_path.starts_with(&format!("{}::", stem))
            } else {
                false
            }
        }
}

pub fn parse_rust_file<P: AsRef<Path>>(path: P) -> Option<Node> {
    let code = fs::read_to_string(&path).ok()?;

    let mut parser = Parser::new();
    parser.set_language(&tree_sitter_rust::LANGUAGE.into()).expect("Error loading Rust grammar");
    let tree = parser.parse(&code, None)?;
    let root_node = tree.root_node();

    let mut imports = Vec::new();
    let mut functions = Vec::new();
    let mut containers = Vec::new();
    let mut external_references = Vec::new();

    // Traverse the syntax tree
    let mut cursor = root_node.walk();
    let mut stack = vec![root_node];
    while let Some(node) = stack.pop() {
        match node.kind() {
            "use_declaration" => {
                // Try to extract the import path (e.g., use foo::bar;)
                let mut import_path = String::new();
                for child in node.children(&mut cursor) {
                    if child.kind() == "scoped_identifier" || child.kind() == "identifier" {
                        import_path = child.utf8_text(code.as_bytes()).unwrap_or("").to_string();
                        break;
                    }
                }
                if !import_path.is_empty() {
                    let is_local = is_local_import(&import_path, path.as_ref());
                    imports.push(Import {
                        path: import_path,
                        is_local,
                    });
                }
            }
            "function_item" => {
                // Get function name
                for child in node.children(&mut cursor) {
                    if child.kind() == "identifier" {
                        let name = child.utf8_text(code.as_bytes()).unwrap_or("").to_string();
                        functions.push(name);
                    }
                }
            }
            "struct_item" | "enum_item" | "trait_item" | "impl_item" => {
                // Get container name
                for child in node.children(&mut cursor) {
                    if child.kind() == "identifier" {
                        let name = child.utf8_text(code.as_bytes()).unwrap_or("").to_string();
                        containers.push(name);
                    }
                }
            }
            // For external references, look for scoped identifiers (e.g., foo::bar)
            "scoped_identifier" => {
                let text = node.utf8_text(code.as_bytes()).unwrap_or("").to_string();
                external_references.push(text);
            }
            _ => {}
        }
        // Push children to stack
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }

    Some(Node {
        file: path.as_ref().to_path_buf(),
        language: Language::Rust,
        imports,
        functions,
        containers,
        external_references,
    })
}
