use crate::core::defs::{FileNode, Import, Language};
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use tree_sitter::Parser;
use tree_sitter_cpp as ts_cpp;

/// Helper function to get node text
fn get_text(n: tree_sitter::Node, code: &str) -> String {
    n.utf8_text(code.as_bytes()).unwrap_or("").to_string()
}

/// Determine if an include is local (quoted) vs system (angle brackets)
#[allow(dead_code)]
fn is_local_include(include_path: &str) -> bool {
    // This is determined when parsing #include directives
    // For now, we'll use a simple heuristic: if it starts with a dot or doesn't look like a stdlib header
    !is_system_include(include_path)
}

/// Check if an include is a standard library header
#[allow(dead_code)]
fn is_system_include(include_path: &str) -> bool {
    // Common C/C++ standard library headers
    const STDLIB_HEADERS: &[&str] = &[
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

    let header_name = include_path.trim_end_matches(".h").trim_end_matches(".hpp");
    STDLIB_HEADERS.contains(&header_name)
}

/// Extract include path from #include directive
fn extract_include_path(node: tree_sitter::Node, code: &str) -> Option<(String, bool)> {
    // For #include directives, the structure is:
    // preproc_include -> string_literal or system_lib_string
    let mut cursor = node.walk();

    for child in node.children(&mut cursor) {
        match child.kind() {
            "string_literal" => {
                // Quoted include: "file.h" -> local
                let text = get_text(child, code);
                let path = text.trim_matches('"').trim_matches('\'').to_string();
                return Some((path, true));
            }
            "system_lib_string" => {
                // System include: <vector> -> not local
                let text = get_text(child, code);
                let path = text.trim_matches('<').trim_matches('>').to_string();
                return Some((path, false));
            }
            _ => {}
        }
    }

    None
}

/// Check if a node is inside a conditional compilation block
/// Returns true if the node is within #ifdef, #ifndef, or #if directives
#[allow(dead_code)]
fn is_in_conditional_block(node: tree_sitter::Node) -> bool {
    let mut current = Some(node);
    while let Some(n) = current {
        if matches!(n.kind(), "preproc_ifdef" | "preproc_ifndef" | "preproc_if") {
            return true;
        }
        current = n.parent();
    }
    false
}

/// Extract conditional directive condition (e.g., "DEBUG" from "#ifdef DEBUG")
#[allow(dead_code)]
fn extract_conditional_condition(node: tree_sitter::Node, code: &str) -> Option<String> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "identifier" {
            return Some(get_text(child, code));
        }
    }
    None
}

/// Common patterns for macro-wrapped includes
/// Examples: BOOST_INCLUDE("file.h"), Q_INCLUDE("widget.h")
const MACRO_INCLUDE_PATTERNS: &[&str] = &[
    "BOOST_INCLUDE",
    "Q_INCLUDE",
    "QT_INCLUDE",
    "GL_INCLUDE",
    "SDL_INCLUDE",
    "INCLUDE",
    "SYSTEM_INCLUDE",
    "OPTIONAL_INCLUDE",
];

/// Extract includes from common macro patterns
#[allow(dead_code)]
fn extract_macro_includes(code: &str) -> HashSet<Import> {
    let mut includes = HashSet::new();

    // Look for patterns like PATTERN("file.h") or PATTERN(<file.h>)
    for line in code.lines() {
        for pattern in MACRO_INCLUDE_PATTERNS {
            if line.contains(pattern) && line.contains('(') {
                // Extract quoted path
                if let Some(first_quote) = line.find('"')
                    && let Some(second_quote) = line[first_quote + 1..].find('"')
                {
                    let path = &line[first_quote + 1..first_quote + 1 + second_quote];
                    includes.insert(Import::new(path.to_string(), true));
                }
                // Extract angle bracket path
                if let Some(open_bracket) = line.find('<')
                    && let Some(close_bracket) = line[open_bracket + 1..].find('>')
                {
                    let path = &line[open_bracket + 1..open_bracket + 1 + close_bracket];
                    includes.insert(Import::new(path.to_string(), false));
                }
            }
        }
    }

    includes
}

pub fn parse_cpp_file<P: AsRef<Path>>(path: P) -> Option<FileNode> {
    let code = fs::read_to_string(&path).ok()?;
    let loc = code.matches('\n').count() as u32 + 1;

    let mut parser = Parser::new();
    parser
        .set_language(&ts_cpp::LANGUAGE.into())
        .expect("Error loading C++ grammar");
    let tree = parser.parse(&code, None)?;
    let root_node = tree.root_node();

    let mut imports = HashSet::new();
    let mut functions = HashSet::new();
    let mut containers = HashSet::new();
    let external_references = HashSet::new();

    // Traverse the syntax tree
    let mut cursor = root_node.walk();
    let mut stack = vec![root_node];

    while let Some(node) = stack.pop() {
        // Push children onto stack for DFS
        let mut node_cursor = node.walk();
        for child in node.children(&mut node_cursor) {
            stack.push(child);
        }

        match node.kind() {
            "preproc_include" => {
                // Extract include path
                if let Some((include_path, is_local)) = extract_include_path(node, &code) {
                    imports.insert(Import::new(include_path, is_local));
                }
            }
            "function_definition" => {
                // Extract function name
                if let Some(declarator_node) = node
                    .children(&mut cursor)
                    .find(|n| n.kind() == "function_declarator")
                {
                    let mut decl_cursor = declarator_node.walk();
                    for child in declarator_node.children(&mut decl_cursor) {
                        if child.kind() == "identifier" {
                            functions.insert(get_text(child, &code));
                            break;
                        }
                    }
                }
            }
            "class_specifier" | "struct_specifier" | "union_specifier" => {
                // Extract class/struct/union name
                let mut spec_cursor = node.walk();
                for child in node.children(&mut spec_cursor) {
                    if child.kind() == "identifier" {
                        containers.insert(get_text(child, &code));
                        break;
                    }
                }
            }
            "enum_specifier" => {
                // Extract enum name
                let mut enum_cursor = node.walk();
                for child in node.children(&mut enum_cursor) {
                    if child.kind() == "identifier" {
                        containers.insert(get_text(child, &code));
                        break;
                    }
                }
            }
            _ => {}
        }
    }

    Some(FileNode::new(
        path.as_ref().to_path_buf(),
        loc,
        Language::Cpp,
        imports,
        functions,
        containers,
        external_references,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn create_test_file(content: &str) -> NamedTempFile {
        let mut file = NamedTempFile::new().expect("Failed to create temp file");
        file.write_all(content.as_bytes())
            .expect("Failed to write to temp file");
        file
    }

    #[test]
    fn test_parse_simple_cpp_file() {
        let content = r#"
#include <iostream>
#include "myheader.h"

void hello_world() {
    std::cout << "Hello, World!" << std::endl;
}
"#;
        let temp_file = create_test_file(content);
        let result = parse_cpp_file(temp_file.path());
        assert!(result.is_some());
    }

    #[test]
    fn test_extract_system_include() {
        let content = r#"#include <vector>"#;
        let temp_file = create_test_file(content);
        let result = parse_cpp_file(temp_file.path()).expect("Failed to parse");
        assert_eq!(result.imports().len(), 1);
    }

    #[test]
    fn test_extract_local_include() {
        let content = r#"#include "myheader.h""#;
        let temp_file = create_test_file(content);
        let result = parse_cpp_file(temp_file.path()).expect("Failed to parse");
        assert_eq!(result.imports().len(), 1);
    }

    #[test]
    fn test_extract_includes_in_ifdef() {
        let content = r#"
#ifdef DEBUG
#include "debug.h"
#endif

#include "normal.h"
"#;
        let temp_file = create_test_file(content);
        let result = parse_cpp_file(temp_file.path()).expect("Failed to parse");
        // Should extract both includes regardless of conditional
        assert_eq!(result.imports().len(), 2);
    }

    #[test]
    fn test_extract_includes_in_ifndef() {
        let content = r#"
#ifndef NDEBUG
#include "debug_helper.h"
#endif

#include "main.h"
"#;
        let temp_file = create_test_file(content);
        let result = parse_cpp_file(temp_file.path()).expect("Failed to parse");
        // Should extract both includes
        assert_eq!(result.imports().len(), 2);
    }

    #[test]
    fn test_extract_includes_in_if_defined() {
        let content = r#"
#if defined(FEATURE_X)
#include "feature_x.h"
#endif

#if defined(FEATURE_Y)
#include "feature_y.h"
#else
#include "feature_y_fallback.h"
#endif
"#;
        let temp_file = create_test_file(content);
        let result = parse_cpp_file(temp_file.path()).expect("Failed to parse");
        // Should extract all includes from all branches
        assert!(result.imports().len() >= 3);
    }

    #[test]
    fn test_nested_conditional_includes() {
        let content = r#"
#ifdef WINDOWS
#ifdef UNICODE
#include "wide_string.h"
#endif
#include "windows.h"
#endif
"#;
        let temp_file = create_test_file(content);
        let result = parse_cpp_file(temp_file.path()).expect("Failed to parse");
        // Should extract nested includes
        assert_eq!(result.imports().len(), 2);
    }

    #[test]
    fn test_extract_macro_includes_basic() {
        let code = r#"
BOOST_INCLUDE("utility.hpp")
Q_INCLUDE("widget.h")
SYSTEM_INCLUDE(<vector>)
"#;
        let includes = extract_macro_includes(code);
        assert!(includes.len() >= 2); // At least the documented patterns
    }

    #[test]
    fn test_extract_macro_includes_quoted() {
        let code = r#"
BOOST_INCLUDE("filesystem.hpp")
"#;
        let includes = extract_macro_includes(code);
        assert!(!includes.is_empty());
    }

    #[test]
    fn test_extract_macro_includes_angle() {
        let code = r#"
SYSTEM_INCLUDE(<iostream>)
GL_INCLUDE(<gl.h>)
"#;
        let includes = extract_macro_includes(code);
        assert!(!includes.is_empty());
    }

    #[test]
    fn test_macro_includes_empty() {
        let code = "no macros here";
        let includes = extract_macro_includes(code);
        // Should not panic and return empty set for non-matching patterns
        assert!(includes.is_empty() || includes.len() == 0);
    }
}
