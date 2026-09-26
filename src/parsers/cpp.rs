use crate::core::defs::{FileNode, Import, Language};
use crate::parsers::{advance, get_text};
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::sync::LazyLock;
use tree_sitter::{Node, Parser};
use tree_sitter_cpp as ts_cpp;

/// Node kinds this parser acts on, as numeric ids so the tree walk compares
/// `u16`s instead of node kind names.
struct CppKinds {
    preproc_include: u16,
    function_definition: u16,
    class_specifier: u16,
    struct_specifier: u16,
    union_specifier: u16,
    enum_specifier: u16,
    qualified_identifier: u16,
    call_expression: u16,
    type_identifier: u16,
    string_literal: u16,
    system_lib_string: u16,
    identifier: u16,
    preproc_ifdef: u16,
    preproc_if: u16,
    // declarator shapes extract_declarator_name unwraps
    field_identifier: u16,
    destructor_name: u16,
    operator_name: u16,
    operator_cast: u16,
    function_declarator: u16,
    template_function: u16,
    pointer_declarator: u16,
    reference_declarator: u16,
    array_declarator: u16,
    parenthesized_declarator: u16,
    attributed_declarator: u16,
    structured_binding_declarator: u16,
}

impl CppKinds {
    fn new() -> Self {
        let language: tree_sitter::Language = ts_cpp::LANGUAGE.into();
        let id = |kind: &str| language.id_for_node_kind(kind, true);
        CppKinds {
            preproc_include: id("preproc_include"),
            function_definition: id("function_definition"),
            class_specifier: id("class_specifier"),
            struct_specifier: id("struct_specifier"),
            union_specifier: id("union_specifier"),
            enum_specifier: id("enum_specifier"),
            qualified_identifier: id("qualified_identifier"),
            call_expression: id("call_expression"),
            type_identifier: id("type_identifier"),
            string_literal: id("string_literal"),
            system_lib_string: id("system_lib_string"),
            identifier: id("identifier"),
            preproc_ifdef: id("preproc_ifdef"),
            preproc_if: id("preproc_if"),
            field_identifier: id("field_identifier"),
            destructor_name: id("destructor_name"),
            operator_name: id("operator_name"),
            operator_cast: id("operator_cast"),
            function_declarator: id("function_declarator"),
            template_function: id("template_function"),
            pointer_declarator: id("pointer_declarator"),
            reference_declarator: id("reference_declarator"),
            array_declarator: id("array_declarator"),
            parenthesized_declarator: id("parenthesized_declarator"),
            attributed_declarator: id("attributed_declarator"),
            structured_binding_declarator: id("structured_binding_declarator"),
        }
    }

    /// Whether `id` declares a class, struct, union, or enum.
    fn is_container_declaration(&self, id: u16) -> bool {
        id == self.class_specifier
            || id == self.struct_specifier
            || id == self.union_specifier
            || id == self.enum_specifier
    }
}

/// Resolved once per process: looking ids up walks the grammar's symbol tables.
static KINDS: LazyLock<CppKinds> = LazyLock::new(CppKinds::new);

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
fn extract_declarator_name(
    node: tree_sitter::Node,
    code: &str,
    kinds: &CppKinds,
) -> Option<String> {
    match node.kind_id() {
        id if id == kinds.identifier
            || id == kinds.field_identifier
            || id == kinds.qualified_identifier
            || id == kinds.destructor_name
            || id == kinds.operator_name =>
        {
            Some(get_text(node, code))
        }
        id if id == kinds.operator_cast => {
            let type_node = node.child_by_field_name("type")?;
            Some(format!("operator {}", get_text(type_node, code)))
        }
        id if id == kinds.function_declarator => {
            extract_declarator_name(node.child_by_field_name("declarator")?, code, kinds)
        }
        id if id == kinds.template_function => {
            extract_declarator_name(node.child_by_field_name("name")?, code, kinds)
        }
        // these wrapping declarators don't expose
        // their inner declarator through a named field, so search their
        // named children for the first one that resolves to a name
        id if id == kinds.pointer_declarator
            || id == kinds.reference_declarator
            || id == kinds.array_declarator
            || id == kinds.parenthesized_declarator
            || id == kinds.attributed_declarator
            || id == kinds.structured_binding_declarator =>
        {
            let mut cursor = node.walk();
            node.named_children(&mut cursor)
                .find_map(|child| extract_declarator_name(child, code, kinds))
        }
        _ => None,
    }
}

/// Extract include path from #include directive.
fn extract_include_path(
    node: tree_sitter::Node,
    code: &str,
    kinds: &CppKinds,
) -> Option<(String, bool)> {
    // For #include directives, the structure is:
    // preproc_include -> string_literal or system_lib_string
    let mut cursor = node.walk();

    for child in node.children(&mut cursor) {
        let id = child.kind_id();
        if id == kinds.string_literal {
            // Quoted include: "file.h" -> local
            let text = get_text(child, code);
            let path = text.trim_matches('"').trim_matches('\'').to_string();
            return Some((path, true));
        }
        if id == kinds.system_lib_string {
            // System include: <vector> -> not local
            let text = get_text(child, code);
            let path = text.trim_matches('<').trim_matches('>').to_string();
            return Some((path, false));
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
        let id = n.kind_id();
        // tree-sitter-cpp has no `preproc_ifndef` node: `#ifndef` yields a
        // `preproc_ifdef` whose `#ifndef` is just an anonymous token
        if id == KINDS.preproc_ifdef || id == KINDS.preproc_if {
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
        if child.kind_id() == KINDS.identifier {
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

    // The line-by-line scan below only ever finds something when the file mentions
    // one of the patterns, and most files never do.
    if !MACRO_INCLUDE_PATTERNS
        .iter()
        .any(|pattern| code.contains(pattern))
    {
        return includes;
    }

    // Look for patterns like PATTERN("file.h") or PATTERN(<file.h>)
    for line in code.lines() {
        if !line.contains('(') {
            continue;
        }

        for pattern in MACRO_INCLUDE_PATTERNS {
            if line.contains(pattern) {
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

/// Records the name of a class, struct, union, or enum declaration, if it has one.
fn insert_container_name(node: Node, code: &str, containers: &mut HashSet<String>) {
    if let Some(name_node) = node.child_by_field_name("name") {
        containers.insert(get_text(name_node, code));
    }
}

pub fn parse_cpp_file<P: AsRef<Path>>(path: P) -> Option<FileNode> {
    let code = fs::read_to_string(&path).ok()?;
    let loc = code.matches('\n').count() as u32 + 1;

    let mut parser = Parser::new();
    parser.set_language(&ts_cpp::LANGUAGE.into()).ok()?;
    let tree = parser.parse(&code, None)?;
    let root_node = tree.root_node();

    let mut imports = HashSet::new();
    let mut functions = HashSet::new();
    let mut containers = HashSet::new();
    let mut external_references = HashSet::new();

    // Traverse the syntax tree, one cursor for the whole file
    let kinds = &*KINDS;
    let mut cursor = root_node.walk();

    loop {
        let node = cursor.node();

        match node.kind_id() {
            id if id == kinds.preproc_include => {
                // Extract include path
                if let Some((include_path, is_local)) = extract_include_path(node, &code, kinds) {
                    imports.insert(Import::new(include_path, is_local));
                }
            }
            id if id == kinds.function_definition => {
                // Extract function name
                if let Some(declarator_node) = node.child_by_field_name("declarator")
                    && let Some(name) = extract_declarator_name(declarator_node, &code, kinds)
                {
                    functions.insert(name);
                }
            }
            id if kinds.is_container_declaration(id) => {
                insert_container_name(node, &code, &mut containers);
            }
            // Qualified identifiers, e.g. `ns::helper`, `Foo::method`
            id if id == kinds.qualified_identifier => {
                external_references.insert(get_text(node, &code));
            }
            // `foo()`, `ns::helper()` - record the callee, not the whole call
            id if id == kinds.call_expression => {
                if let Some(function_node) = node.child_by_field_name("function") {
                    external_references.insert(get_text(function_node, &code));
                }
            }
            // Type references (parameter/variable/return types, etc.), but not
            // the declaration's own name
            id if id == kinds.type_identifier => {
                let is_declaration_name = node.parent().is_some_and(|parent| {
                    kinds.is_container_declaration(parent.kind_id())
                        && parent.child_by_field_name("name") == Some(node)
                });
                if !is_declaration_name {
                    external_references.insert(get_text(node, &code));
                }
            }
            _ => {}
        }

        if !advance(&mut cursor) {
            break;
        }
    }

    imports.extend(extract_macro_includes(&code));

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
    fn containers_are_recorded_for_named_declarations() {
        let content = "struct Foo {};\nclass Bar {};\nunion Baz {};\nenum Qux { A };\n";
        let temp_file = create_test_file(content);
        let result = parse_cpp_file(temp_file.path()).expect("Failed to parse");

        assert_eq!(
            result.containers().len(),
            4,
            "containers: {:?}",
            result.containers()
        );
        for name in ["Foo", "Bar", "Baz", "Qux"] {
            assert!(
                result.containers().contains(name),
                "missing {name} in {:?}",
                result.containers()
            );
        }
    }

    #[test]
    fn anonymous_declarations_record_no_container() {
        let content = "struct { int value; } holder;\nenum { A, B } letters;\n";
        let temp_file = create_test_file(content);
        let result = parse_cpp_file(temp_file.path()).expect("Failed to parse");

        assert!(result.containers().is_empty(), "{:?}", result.containers());
    }

    #[test]
    fn every_dispatched_node_kind_resolves_to_an_id() {
        // an unknown kind would resolve to 0 and silently collide with the others
        for id in [
            KINDS.preproc_include,
            KINDS.function_definition,
            KINDS.class_specifier,
            KINDS.struct_specifier,
            KINDS.union_specifier,
            KINDS.enum_specifier,
            KINDS.qualified_identifier,
            KINDS.call_expression,
            KINDS.type_identifier,
            KINDS.string_literal,
            KINDS.system_lib_string,
            KINDS.identifier,
            KINDS.preproc_ifdef,
            KINDS.preproc_if,
            KINDS.field_identifier,
            KINDS.destructor_name,
            KINDS.operator_name,
            KINDS.operator_cast,
            KINDS.function_declarator,
            KINDS.template_function,
            KINDS.pointer_declarator,
            KINDS.reference_declarator,
            KINDS.array_declarator,
            KINDS.parenthesized_declarator,
            KINDS.attributed_declarator,
            KINDS.structured_binding_declarator,
        ] {
            assert_ne!(id, 0);
        }
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

    #[test]
    fn test_macro_includes_merged_into_parsed_imports() {
        let content = r#"
BOOST_INCLUDE("utility.hpp")
SYSTEM_INCLUDE(<vector>)
"#;
        let temp_file = create_test_file(content);
        let result = parse_cpp_file(temp_file.path()).expect("Failed to parse");
        assert!(
            result
                .imports()
                .contains(&Import::new("utility.hpp".to_string(), true)),
            "imports: {:?}",
            result.imports()
        );
        assert!(
            result
                .imports()
                .contains(&Import::new("vector".to_string(), false)),
            "imports: {:?}",
            result.imports()
        );
    }
}
