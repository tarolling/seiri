use crate::core::defs::{FileNode, Import, Language};
use crate::parsers::get_text;
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use tree_sitter::Parser;
use tree_sitter_cpp as ts_cpp;

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

/// Recursively resolve a function's declarator down to its name.
///
/// The C++ grammar wraps the name node in various declarator shapes
/// (pointer/reference return types, template functions, etc.) and the
/// terminal name itself may be a plain identifier, a qualified name
/// (`ns::f`), a destructor (`~Foo`), an operator overload (`operator==`),
/// or a conversion operator (`operator bool`).
fn extract_declarator_name(node: tree_sitter::Node, code: &str) -> Option<String> {
    match node.kind() {
        "identifier"
        | "field_identifier"
        | "qualified_identifier"
        | "destructor_name"
        | "operator_name" => Some(get_text(node, code)),
        "operator_cast" => {
            let type_node = node.child_by_field_name("type")?;
            Some(format!("operator {}", get_text(type_node, code)))
        }
        "function_declarator" => {
            extract_declarator_name(node.child_by_field_name("declarator")?, code)
        }
        "template_function" => extract_declarator_name(node.child_by_field_name("name")?, code),
        // these wrapping declarators don't expose
        // their inner declarator through a named field, so search their
        // named children for the first one that resolves to a name
        "pointer_declarator"
        | "reference_declarator"
        | "array_declarator"
        | "parenthesized_declarator"
        | "attributed_declarator"
        | "structured_binding_declarator" => {
            let mut cursor = node.walk();
            node.named_children(&mut cursor)
                .find_map(|child| extract_declarator_name(child, code))
        }
        _ => None,
    }
}

/// Extract include path from #include directive.
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

    let mut imports = extract_macro_includes(&code);
    let mut functions = HashSet::new();
    let mut containers = HashSet::new();
    let mut external_references = HashSet::new();

    // Traverse the syntax tree
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
                if let Some(declarator_node) = node.child_by_field_name("declarator")
                    && let Some(name) = extract_declarator_name(declarator_node, &code)
                {
                    functions.insert(name);
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
            // Qualified identifiers, e.g. `ns::helper`, `Foo::method`
            "qualified_identifier" => {
                external_references.insert(get_text(node, &code));
            }
            // `foo()`, `ns::helper()` - record the callee, not the whole call
            "call_expression" => {
                if let Some(function_node) = node.child_by_field_name("function") {
                    external_references.insert(get_text(function_node, &code));
                }
            }
            // Type references (parameter/variable/return types, etc.), but not
            // the declaration's own name
            "type_identifier" => {
                let is_declaration_name = node.parent().is_some_and(|parent| {
                    matches!(
                        parent.kind(),
                        "class_specifier"
                            | "struct_specifier"
                            | "union_specifier"
                            | "enum_specifier"
                    ) && parent.child_by_field_name("name") == Some(node)
                });
                if !is_declaration_name {
                    external_references.insert(get_text(node, &code));
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
    fn test_extract_macro_includes_through_parser() {
        let content = r#"
BOOST_INCLUDE("utility.hpp")
SYSTEM_INCLUDE(<vector>)
"#;
        let temp_file = create_test_file(content);
        let result = parse_cpp_file(temp_file.path()).expect("Failed to parse");

        assert_eq!(result.imports().len(), 2);
        assert!(
            result
                .imports()
                .contains(&Import::new("utility.hpp".to_string(), true))
        );
        assert!(
            result
                .imports()
                .contains(&Import::new("vector".to_string(), false))
        );
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
    fn test_extract_qualified_function_name() {
        let content = r#"
namespace ns {
    void f();
}
void ns::f() {}
"#;
        let temp_file = create_test_file(content);
        let result = parse_cpp_file(temp_file.path()).expect("Failed to parse");
        assert!(
            result.functions().contains("ns::f"),
            "functions: {:?}",
            result.functions()
        );
    }

    #[test]
    fn test_extract_destructor_name() {
        let content = r#"
struct Foo {
    ~Foo();
};
Foo::~Foo() {}
"#;
        let temp_file = create_test_file(content);
        let result = parse_cpp_file(temp_file.path()).expect("Failed to parse");
        assert!(
            result.functions().contains("Foo::~Foo"),
            "functions: {:?}",
            result.functions()
        );
    }

    #[test]
    fn test_extract_inline_destructor_name() {
        let content = r#"
struct Foo {
    ~Foo() {}
};
"#;
        let temp_file = create_test_file(content);
        let result = parse_cpp_file(temp_file.path()).expect("Failed to parse");
        assert!(
            result.functions().contains("~Foo"),
            "functions: {:?}",
            result.functions()
        );
    }

    #[test]
    fn test_extract_operator_overload_name() {
        let content = r#"
struct Foo {
    bool operator==(const Foo& other) const { return true; }
};
"#;
        let temp_file = create_test_file(content);
        let result = parse_cpp_file(temp_file.path()).expect("Failed to parse");
        assert!(
            result.functions().contains("operator=="),
            "functions: {:?}",
            result.functions()
        );
    }

    #[test]
    fn test_extract_qualified_operator_overload_name() {
        let content = r#"
struct Foo {
    bool operator==(const Foo& other) const;
};
bool Foo::operator==(const Foo& other) const { return true; }
"#;
        let temp_file = create_test_file(content);
        let result = parse_cpp_file(temp_file.path()).expect("Failed to parse");
        assert!(
            result.functions().contains("Foo::operator=="),
            "functions: {:?}",
            result.functions()
        );
    }

    #[test]
    fn test_extract_conversion_operator_name() {
        let content = r#"
struct Foo {
    operator bool() const { return true; }
};
"#;
        let temp_file = create_test_file(content);
        let result = parse_cpp_file(temp_file.path()).expect("Failed to parse");
        assert!(
            result.functions().contains("operator bool"),
            "functions: {:?}",
            result.functions()
        );
    }

    #[test]
    fn test_extract_reference_wrapped_function_name() {
        let content = r#"
struct Foo {
    Foo& operator=(const Foo& other) { return *this; }
};
"#;
        let temp_file = create_test_file(content);
        let result = parse_cpp_file(temp_file.path()).expect("Failed to parse");
        assert!(
            result.functions().contains("operator="),
            "functions: {:?}",
            result.functions()
        );
    }

    #[test]
    fn test_extract_pointer_return_function_name() {
        let content = r#"
int* make_int() { return nullptr; }
"#;
        let temp_file = create_test_file(content);
        let result = parse_cpp_file(temp_file.path()).expect("Failed to parse");
        assert!(
            result.functions().contains("make_int"),
            "functions: {:?}",
            result.functions()
        );
    }

    #[test]
    fn test_extract_qualified_identifier_reference() {
        let content = r#"
namespace ns {
    void helper();
}
void caller() {
    ns::helper();
}
"#;
        let temp_file = create_test_file(content);
        let result = parse_cpp_file(temp_file.path()).expect("Failed to parse");
        assert!(
            result.external_references().contains("ns::helper"),
            "external_references: {:?}",
            result.external_references()
        );
    }

    #[test]
    fn test_extract_call_target_reference() {
        let content = r#"
void doWork();
void caller() {
    doWork();
}
"#;
        let temp_file = create_test_file(content);
        let result = parse_cpp_file(temp_file.path()).expect("Failed to parse");
        assert!(
            result.external_references().contains("doWork"),
            "external_references: {:?}",
            result.external_references()
        );
    }

    #[test]
    fn test_extract_type_reference() {
        let content = r#"
struct Foo {};
void useFoo(Foo f) {}
"#;
        let temp_file = create_test_file(content);
        let result = parse_cpp_file(temp_file.path()).expect("Failed to parse");
        assert!(
            result.external_references().contains("Foo"),
            "external_references: {:?}",
            result.external_references()
        );
    }

    #[test]
    fn test_declaration_name_not_treated_as_external_reference() {
        let content = r#"
struct OnlyDeclared {};
"#;
        let temp_file = create_test_file(content);
        let result = parse_cpp_file(temp_file.path()).expect("Failed to parse");
        assert!(!result.external_references().contains("OnlyDeclared"));
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
        assert!(includes.is_empty());
    }
}
