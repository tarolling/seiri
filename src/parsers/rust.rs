use crate::core::defs::{FileNode, Import, Language};
use crate::parsers::{advance, descend_into, get_text, skip_children};
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::sync::LazyLock;
use tree_sitter::Parser;
use tree_sitter_rust as ts_rust;

/// Node kinds this parser acts on, as numeric ids so the tree walk compares
/// `u16`s instead of node kind names.
struct RustKinds {
    identifier: u16,
    type_identifier: u16,
    semicolon: u16,
    crate_kw: u16,
    self_kw: u16,
    super_kw: u16,
    metavariable: u16,
    use_wildcard: u16,
    use_as_clause: u16,
    use_list: u16,
    scoped_use_list: u16,
    use_declaration: u16,
    mod_item: u16,
    function_item: u16,
    function_signature_item: u16,
    struct_item: u16,
    enum_item: u16,
    trait_item: u16,
    impl_item: u16,
    scoped_identifier: u16,
}

impl RustKinds {
    fn new() -> Self {
        let language: tree_sitter::Language = ts_rust::LANGUAGE.into();
        let id = |kind: &str| language.id_for_node_kind(kind, true);
        RustKinds {
            identifier: id("identifier"),
            type_identifier: id("type_identifier"),
            semicolon: language.id_for_node_kind(";", false),
            crate_kw: id("crate"),
            self_kw: id("self"),
            super_kw: id("super"),
            metavariable: id("metavariable"),
            use_wildcard: id("use_wildcard"),
            use_as_clause: id("use_as_clause"),
            use_list: id("use_list"),
            scoped_use_list: id("scoped_use_list"),
            use_declaration: id("use_declaration"),
            mod_item: id("mod_item"),
            function_item: id("function_item"),
            function_signature_item: id("function_signature_item"),
            struct_item: id("struct_item"),
            enum_item: id("enum_item"),
            trait_item: id("trait_item"),
            impl_item: id("impl_item"),
            scoped_identifier: id("scoped_identifier"),
        }
    }
}

/// Resolved once per process: looking ids up walks the grammar's symbol tables.
static KINDS: LazyLock<RustKinds> = LazyLock::new(RustKinds::new);

/// Determine if an import is local (starts with crate/self/super or current mod).
fn is_local_import(import_path: &str, file_path: &Path) -> bool {
    import_path.starts_with("crate::")
        || import_path.starts_with("self::")
        || import_path.starts_with("super::")
        || import_path == "crate"
        || import_path == "self"
        || import_path == "super"
        || file_path
            .file_stem()
            .and_then(|s| s.to_str())
            .is_some_and(|stem| import_path.starts_with(&format!("{stem}::")))
}

/// Extract all import paths from a use declaration, handling use lists.
fn extract_use_paths(node: tree_sitter::Node, code: &str) -> Vec<String> {
    let mut paths = Vec::new();
    if let Some(argument) = node.child_by_field_name("argument") {
        collect_use_paths(argument, code, &KINDS, "", &mut paths);
    }
    paths
}

/// Find the first named path component inside a `use_wildcard` node (the part before `::*`).
fn use_wildcard_prefix(node: tree_sitter::Node, code: &str) -> Option<String> {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .find(|c| c.is_named())
        .map(|c| get_text(c, code))
}

/// Recursively collect import paths from a use-clause argument node, accumulating `prefix`
/// as scopes are entered.
fn collect_use_paths(
    node: tree_sitter::Node,
    code: &str,
    kinds: &RustKinds,
    prefix: &str,
    paths: &mut Vec<String>,
) {
    let id = node.kind_id();
    if id == kinds.identifier
        || id == kinds.crate_kw
        || id == kinds.metavariable
        || id == kinds.scoped_identifier
    {
        paths.push(format!("{prefix}{}", get_text(node, code)));
        return;
    }
    if id == kinds.self_kw {
        // `foo::{self, bar}` imports the module `foo` itself; a bare `use self;`
        // refers to the current module.
        if prefix.is_empty() {
            paths.push("self".to_string());
        } else {
            paths.push(prefix.trim_end_matches("::").to_string());
        }
        return;
    }
    if id == kinds.super_kw {
        paths.push(format!("{prefix}super"));
        return;
    }
    if id == kinds.use_wildcard {
        match use_wildcard_prefix(node, code) {
            Some(inner) => paths.push(format!("{prefix}{inner}::*")),
            None => paths.push(format!("{prefix}*")),
        }
        return;
    }
    if id == kinds.use_as_clause {
        // Handle `foo as bar` - we want the original name (foo)
        if let Some(path_node) = node.child_by_field_name("path") {
            collect_use_paths(path_node, code, kinds, prefix, paths);
        }
        return;
    }
    if id == kinds.use_list {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.is_named() {
                collect_use_paths(child, code, kinds, prefix, paths);
            }
        }
        return;
    }
    if id == kinds.scoped_use_list {
        let path_text = node
            .child_by_field_name("path")
            .map(|p| get_text(p, code))
            .unwrap_or_default();
        let new_prefix = if path_text.is_empty() {
            prefix.to_string()
        } else {
            format!("{prefix}{path_text}::")
        };
        if let Some(list_node) = node.child_by_field_name("list") {
            collect_use_paths(list_node, code, kinds, &new_prefix, paths);
        }
    }
}

fn parser_loop<P: AsRef<Path>>(
    path: P,
    code: &str,
    root_node: tree_sitter::Node<'_>,
) -> Option<FileNode> {
    let loc = code.matches("\n").count() as u32 + 1; // count number of newlines bc code.lines() has failed me

    let mut imports = HashSet::new();
    let mut functions = HashSet::new();
    let mut containers = HashSet::new();
    let mut external_references = HashSet::new();

    // Traverse the syntax tree, one cursor for the whole file
    let kinds = &*KINDS;
    let mut cursor = root_node.walk();

    loop {
        let node = cursor.node();
        let mut skip_subtree = false;
        let mut descend_into_body = None;

        match node.kind_id() {
            id if id == kinds.use_declaration => {
                let import_paths = extract_use_paths(node, code);
                for import_path in import_paths {
                    if !import_path.is_empty() {
                        let is_local = is_local_import(&import_path, path.as_ref());
                        imports.insert(Import::new(import_path, is_local));
                    }
                }
                // don't descend further, otherwise the same scoped_identifier/identifier nodes get re-visited
                skip_subtree = true;
            }
            id if id == kinds.mod_item => {
                // Handle module declarations like "pub mod python;" or "mod utils;"
                let mut mod_name = String::new();
                let mut is_declaration = false;

                let mut child_cursor = node.walk();
                for child in node.children(&mut child_cursor) {
                    if child.kind_id() == kinds.identifier {
                        mod_name = get_text(child, code);
                    } else if child.kind_id() == kinds.semicolon {
                        // If we find a semicolon, this is a module declaration (not inline definition)
                        is_declaration = true;
                    }
                }

                // Only add as import if it's a declaration (has semicolon)
                if !mod_name.is_empty() && is_declaration {
                    imports.insert(Import::new(mod_name, true));
                }
            }
            id if id == kinds.function_item || id == kinds.function_signature_item => {
                // Get function name - look for the first identifier after any visibility modifiers
                let mut child_cursor = node.walk();
                for child in node.children(&mut child_cursor) {
                    if child.kind_id() == kinds.identifier {
                        let name = get_text(child, code);
                        functions.insert(name);
                    }
                }
            }
            id if id == kinds.struct_item || id == kinds.enum_item || id == kinds.trait_item => {
                let mut child_cursor = node.walk();
                for child in node.children(&mut child_cursor) {
                    if child.kind_id() == kinds.type_identifier {
                        let name = get_text(child, code);
                        containers.insert(name);
                    }
                }
            }
            id if id == kinds.impl_item => {
                // impl target type and implemented trait name are declaration
                // sites, not container definitions or external usages
                descend_into_body = node.child_by_field_name("body");
            }
            // For external references, look for scoped identifiers (e.g., foo::bar)
            id if id == kinds.scoped_identifier => {
                let text = get_text(node, code);
                external_references.insert(text);
            }
            _ => {}
        }

        let advanced = if let Some(body) = descend_into_body {
            descend_into(&mut cursor, body) || skip_children(&mut cursor)
        } else if skip_subtree {
            skip_children(&mut cursor)
        } else {
            advance(&mut cursor)
        };

        if !advanced {
            break;
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

/// Parse a Rust file and extract its structure. This is the main method
/// called by the parse loop in main.rs.
pub fn parse_rust_file<P: AsRef<Path>>(path: P) -> Option<FileNode> {
    let code = fs::read_to_string(&path).ok()?;

    let mut parser = Parser::new();
    parser.set_language(&ts_rust::LANGUAGE.into()).ok()?;
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
    fn every_dispatched_node_kind_resolves_to_an_id() {
        // an unknown kind would resolve to 0 and silently collide with the others
        for id in [
            KINDS.identifier,
            KINDS.type_identifier,
            KINDS.semicolon,
            KINDS.crate_kw,
            KINDS.self_kw,
            KINDS.super_kw,
            KINDS.metavariable,
            KINDS.use_wildcard,
            KINDS.use_as_clause,
            KINDS.use_list,
            KINDS.scoped_use_list,
            KINDS.use_declaration,
            KINDS.mod_item,
            KINDS.function_item,
            KINDS.function_signature_item,
            KINDS.struct_item,
            KINDS.enum_item,
            KINDS.trait_item,
            KINDS.impl_item,
            KINDS.scoped_identifier,
        ] {
            assert_ne!(id, 0);
        }
    }

    #[test]
    fn impl_bodies_are_walked_but_impl_headers_are_not() {
        let temp_dir = TempDir::new().unwrap();
        let content = r#"
impl std::fmt::Display for Thing {
    fn fmt(&self) { self::helper(); }
}
"#;
        let file_path = create_test_file(&temp_dir, "test.rs", content);

        let result = parse_rust_file(&file_path).unwrap();

        // the trait path is a declaration site, not a usage
        assert!(!result.external_references().contains("std::fmt::Display"));
        // functions and usages inside the body are still collected
        assert!(result.functions().contains("fmt"));
        assert!(result.external_references().contains("self::helper"));
    }

    #[test]
    fn use_declaration_subtrees_do_not_leak_external_references() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = create_test_file(
            &temp_dir,
            "test.rs",
            "use std::collections::HashMap;\n\nfn build() {}\n",
        );

        let result = parse_rust_file(&file_path).unwrap();

        assert!(
            result
                .imports()
                .iter()
                .any(|i| i.path() == "std::collections::HashMap")
        );
        assert!(!result.external_references().contains("std::collections"));
        assert!(result.functions().contains("build"));
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
    fn test_bare_identifier_import() {
        let temp_dir = TempDir::new().unwrap();
        let content = r#"use foo;"#;
        let file_path = create_test_file(&temp_dir, "test.rs", content);

        let result = parse_rust_file(&file_path).unwrap();
        let imports: Vec<_> = result.imports().iter().collect();

        assert!(imports.iter().any(|i| i.path() == "foo"));
    }

    #[test]
    fn test_wildcard_import() {
        let temp_dir = TempDir::new().unwrap();
        let content = r#"use std::collections::*;"#;
        let file_path = create_test_file(&temp_dir, "test.rs", content);

        let result = parse_rust_file(&file_path).unwrap();
        let imports: Vec<_> = result.imports().iter().collect();

        assert!(
            imports
                .iter()
                .any(|i| i.path() == "std::collections::*" && !i.is_local())
        );
    }

    #[test]
    fn test_use_list_with_self() {
        let temp_dir = TempDir::new().unwrap();
        let content = r#"use std::io::{self, Write};"#;
        let file_path = create_test_file(&temp_dir, "test.rs", content);

        let result = parse_rust_file(&file_path).unwrap();
        let imports: Vec<_> = result.imports().iter().collect();

        assert!(imports.iter().any(|i| i.path() == "std::io"));
        assert!(imports.iter().any(|i| i.path() == "std::io::Write"));
    }

    #[test]
    fn test_bare_self_and_super() {
        let temp_dir = TempDir::new().unwrap();
        let content = r#"
use self;
use super;
        "#;
        let file_path = create_test_file(&temp_dir, "test.rs", content);

        let result = parse_rust_file(&file_path).unwrap();
        let imports: Vec<_> = result.imports().iter().collect();

        assert!(imports.iter().any(|i| i.path() == "self" && i.is_local()));
        assert!(imports.iter().any(|i| i.path() == "super" && i.is_local()));
    }

    #[test]
    fn test_impl_trait_not_recorded_as_container() {
        let temp_dir = TempDir::new().unwrap();
        let content = r#"
use std::fmt::Display;

struct MyType;

impl Display for MyType {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "MyType")
    }
}
        "#;
        let file_path = create_test_file(&temp_dir, "test.rs", content);

        let result = parse_rust_file(&file_path).unwrap();
        let containers: Vec<_> = result.containers().iter().collect();

        assert!(containers.iter().any(|c| *c == "MyType"));
        assert!(!containers.iter().any(|c| *c == "Display"));
    }

    #[test]
    fn test_use_import_not_recorded_as_external_reference() {
        let temp_dir = TempDir::new().unwrap();
        let content = r#"
use crate::core::defs::FileNode;

fn foo() {}
        "#;
        let file_path = create_test_file(&temp_dir, "test.rs", content);

        let result = parse_rust_file(&file_path).unwrap();
        let refs: Vec<_> = result.external_references().iter().collect();

        assert!(!refs.iter().any(|r| *r == "crate::core::defs::FileNode"));
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
