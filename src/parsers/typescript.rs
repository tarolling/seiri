use crate::core::defs::{FileNode, Import, Language};
use crate::parsers::{advance, get_text};
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::sync::LazyLock;
use tree_sitter::Parser;
use tree_sitter_typescript as ts_typescript;

/// Node kinds this parser acts on, as numeric ids so the tree walk compares
/// `u16`s instead of node kind names.
struct TypeScriptKinds {
    import_statement: u16,
    export_statement: u16,
    function_declaration: u16,
    method_definition: u16,
    lexical_declaration: u16,
    variable_declaration: u16,
    class_declaration: u16,
    interface_declaration: u16,
    enum_declaration: u16,
    type_alias_declaration: u16,
    member_expression: u16,
    call_expression: u16,
    new_expression: u16,
    type_identifier: u16,
    nested_type_identifier: u16,
    import_require_clause: u16,
    variable_declarator: u16,
    arrow_function: u16,
    function_expression: u16,
}

impl TypeScriptKinds {
    fn new() -> Self {
        let language: tree_sitter::Language = ts_typescript::LANGUAGE_TYPESCRIPT.into();
        let id = |kind: &str| language.id_for_node_kind(kind, true);
        TypeScriptKinds {
            import_statement: id("import_statement"),
            export_statement: id("export_statement"),
            function_declaration: id("function_declaration"),
            method_definition: id("method_definition"),
            lexical_declaration: id("lexical_declaration"),
            variable_declaration: id("variable_declaration"),
            class_declaration: id("class_declaration"),
            interface_declaration: id("interface_declaration"),
            enum_declaration: id("enum_declaration"),
            type_alias_declaration: id("type_alias_declaration"),
            member_expression: id("member_expression"),
            call_expression: id("call_expression"),
            new_expression: id("new_expression"),
            type_identifier: id("type_identifier"),
            nested_type_identifier: id("nested_type_identifier"),
            import_require_clause: id("import_require_clause"),
            variable_declarator: id("variable_declarator"),
            arrow_function: id("arrow_function"),
            function_expression: id("function_expression"),
        }
    }

    /// Whether `id` declares a named container.
    fn is_declaration_kind(&self, id: u16) -> bool {
        id == self.class_declaration
            || id == self.interface_declaration
            || id == self.enum_declaration
            || id == self.type_alias_declaration
    }
}

/// Resolved once per process: looking ids up walks the grammar's symbol tables.
static KINDS: LazyLock<TypeScriptKinds> = LazyLock::new(TypeScriptKinds::new);

/// Local imports are typically relative paths starting with '.'
fn is_local_import(import_path: &str) -> bool {
    import_path.starts_with('.')
}

/// Extracts the import path string from an import or export statement
fn extract_import_path(
    node: tree_sitter::Node,
    code: &str,
    kinds: &TypeScriptKinds,
) -> Option<String> {
    let mut source_node = node.child_by_field_name("source");
    if source_node.is_none() {
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            if child.kind_id() == kinds.import_require_clause {
                source_node = child.child_by_field_name("source");
                break;
            }
        }
    }

    source_node
        .map(|source_node| get_text(source_node, code))
        .map(|path_text| path_text.trim_matches('"').trim_matches('\'').to_string())
}

pub fn parse_typescript_file<P: AsRef<Path>>(path: P) -> Option<FileNode> {
    let code = fs::read_to_string(&path).ok()?;
    let loc = code.matches('\n').count() as u32 + 1;

    let mut parser = Parser::new();
    parser
        .set_language(&ts_typescript::LANGUAGE_TYPESCRIPT.into())
        .ok()?;
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
            // `import ... from '...';`, `export ... from '...';`
            id if id == kinds.import_statement || id == kinds.export_statement => {
                if let Some(import_path) = extract_import_path(node, &code, kinds) {
                    let is_local = is_local_import(&import_path);
                    imports.insert(Import::new(import_path, is_local));
                }
            }

            // `function hello() {}` and `method_definition` both insert function names
            id if id == kinds.function_declaration || id == kinds.method_definition => {
                if let Some(name_node) = node.child_by_field_name("name") {
                    functions.insert(get_text(name_node, &code));
                }
            }

            // `const myFunc = () => {}`, `let myVar = function() {}`,
            // `var legacyFunc = () => {}`
            id if id == kinds.lexical_declaration || id == kinds.variable_declaration => {
                let mut declarator_cursor = node.walk();
                for child in node.children(&mut declarator_cursor) {
                    if child.kind_id() != kinds.variable_declarator {
                        continue;
                    }

                    if let Some(value_node) = child.child_by_field_name("value")
                        && {
                            let value_id = value_node.kind_id();
                            value_id == kinds.arrow_function
                                || value_id == kinds.function_expression
                        }
                        && let Some(name_node) = child.child_by_field_name("name")
                    {
                        functions.insert(get_text(name_node, &code));
                    }
                }
            }

            // `class C {}`, `interface I {}`, `enum E {}`, `type T = ...`
            id if kinds.is_declaration_kind(id) => {
                if let Some(name_node) = node.child_by_field_name("name") {
                    containers.insert(get_text(name_node, &code));
                }
            }

            // `obj.prop`, `this.field`, etc.
            id if id == kinds.member_expression => {
                external_references.insert(get_text(node, &code));
            }

            // `foo()`, `obj.method()` - record the callee, not the whole call
            id if id == kinds.call_expression => {
                if let Some(function_node) = node.child_by_field_name("function") {
                    external_references.insert(get_text(function_node, &code));
                }
            }

            // `new Foo()`, `new ns.Foo()`
            id if id == kinds.new_expression => {
                if let Some(constructor_node) = node.child_by_field_name("constructor") {
                    external_references.insert(get_text(constructor_node, &code));
                }
            }

            // `x: Foo`, `x: ns.Foo` - type references, but not the declaration's own name
            id if id == kinds.type_identifier || id == kinds.nested_type_identifier => {
                let is_declaration_name = node.parent().is_some_and(|parent| {
                    kinds.is_declaration_kind(parent.kind_id())
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

    Some(FileNode::new(
        path.as_ref().to_path_buf(),
        loc,
        Language::TypeScript,
        imports,
        functions,
        containers,
        external_references,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use std::io::Write;
    use tempfile::TempDir;

    fn create_test_file(dir: &TempDir, filename: &str, content: &str) -> std::path::PathBuf {
        let file_path = dir.path().join(filename);
        fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        let mut file = File::create(&file_path).unwrap();
        file.write_all(content.as_bytes()).unwrap();
        file_path
    }

    #[test]
    fn every_dispatched_node_kind_resolves_to_an_id() {
        // an unknown kind would resolve to 0 and silently collide with the others
        for id in [
            KINDS.import_statement,
            KINDS.export_statement,
            KINDS.function_declaration,
            KINDS.method_definition,
            KINDS.lexical_declaration,
            KINDS.variable_declaration,
            KINDS.class_declaration,
            KINDS.interface_declaration,
            KINDS.enum_declaration,
            KINDS.type_alias_declaration,
            KINDS.member_expression,
            KINDS.call_expression,
            KINDS.new_expression,
            KINDS.type_identifier,
            KINDS.nested_type_identifier,
            KINDS.import_require_clause,
            KINDS.variable_declarator,
            KINDS.arrow_function,
            KINDS.function_expression,
        ] {
            assert_ne!(id, 0);
        }
    }

    #[test]
    fn test_simple_imports() {
        let temp_dir = TempDir::new().unwrap();
        let content = r#"
import { A } from "lib-a"; // external
import { B } from "./local-b"; // local
import * as C from "../parent/local-c"; // local
import D from "lib-d"; // external
        "#;
        let file_path = create_test_file(&temp_dir, "test.ts", content);

        let result = parse_typescript_file(&file_path).unwrap();
        let imports = result.imports();

        assert!(imports.iter().any(|i| i.path() == "lib-a" && !i.is_local()));
        assert!(
            imports
                .iter()
                .any(|i| i.path() == "./local-b" && i.is_local())
        );
        assert!(
            imports
                .iter()
                .any(|i| i.path() == "../parent/local-c" && i.is_local())
        );
        assert!(imports.iter().any(|i| i.path() == "lib-d" && !i.is_local()));
        assert_eq!(imports.len(), 4);
    }

    #[test]
    fn test_import_require_clause() {
        let temp_dir = TempDir::new().unwrap();
        let content = r#"
import local = require("./local-module");
import external = require("external-module");
        "#;
        let file_path = create_test_file(&temp_dir, "test.ts", content);

        let result = parse_typescript_file(&file_path).unwrap();
        let imports = result.imports();

        assert!(
            imports
                .iter()
                .any(|i| i.path() == "./local-module" && i.is_local())
        );
        assert!(
            imports
                .iter()
                .any(|i| i.path() == "external-module" && !i.is_local())
        );
        assert_eq!(imports.len(), 2);
    }

    #[test]
    fn test_functions_and_containers() {
        let temp_dir = TempDir::new().unwrap();
        let content = r#"
export function a() {}
const b = () => {};
export class C {}
interface D {}
enum E { VAL }
type F = string;

class G {
    methodH() {}
}
        "#;
        let file_path = create_test_file(&temp_dir, "test.ts", content);

        let result = parse_typescript_file(&file_path).unwrap();
        let functions = result.functions();
        let containers = result.containers();

        assert!(functions.contains(&"a".to_string()));
        assert!(functions.contains(&"b".to_string()));
        assert!(functions.contains(&"methodH".to_string()));
        assert_eq!(functions.len(), 3);

        assert!(containers.contains(&"C".to_string()));
        assert!(containers.contains(&"D".to_string()));
        assert!(containers.contains(&"E".to_string()));
        assert!(containers.contains(&"F".to_string()));
        assert!(containers.contains(&"G".to_string()));
        assert_eq!(containers.len(), 5);
    }

    #[test]
    fn test_var_declared_functions() {
        let temp_dir = TempDir::new().unwrap();
        let content = r#"
var arrow = () => {};
var expression = function() {};
        "#;
        let file_path = create_test_file(&temp_dir, "test.ts", content);

        let result = parse_typescript_file(&file_path).unwrap();
        let functions = result.functions();

        assert!(functions.contains(&"arrow".to_string()));
        assert!(functions.contains(&"expression".to_string()));
        assert_eq!(functions.len(), 2);
    }

    #[test]
    fn test_re_exports() {
        let temp_dir = TempDir::new().unwrap();
        let content = r#"
export { A } from "./local-a";
export * from "lib-b";
        "#;
        let file_path = create_test_file(&temp_dir, "test.ts", content);

        let result = parse_typescript_file(&file_path).unwrap();
        let imports = result.imports();

        assert!(
            imports
                .iter()
                .any(|i| i.path() == "./local-a" && i.is_local())
        );
        assert!(imports.iter().any(|i| i.path() == "lib-b" && !i.is_local()));
        assert_eq!(imports.len(), 2);
    }

    #[test]
    fn test_lines_of_code() {
        let temp_dir = TempDir::new().unwrap();
        // 3 lines, including the empty line at the end
        let content = "const x = 1;\nconst y = 2;\n";
        let file_path = create_test_file(&temp_dir, "test.ts", content);
        let result = parse_typescript_file(&file_path).unwrap();
        assert_eq!(result.loc(), 3);
    }

    #[test]
    fn test_external_references_member_and_call() {
        let temp_dir = TempDir::new().unwrap();
        let content = r#"
function main() {
    myObject.myMethod();
    otherFunc();
}
        "#;
        let file_path = create_test_file(&temp_dir, "test.ts", content);

        let result = parse_typescript_file(&file_path).unwrap();
        let refs = result.external_references();

        assert!(refs.contains("myObject.myMethod"));
        assert!(refs.contains("otherFunc"));
    }

    #[test]
    fn test_external_references_type_annotations() {
        let temp_dir = TempDir::new().unwrap();
        let content = r#"
function f(x: Foo): Bar {
    return x as unknown as Bar;
}
class MyClass {}
        "#;
        let file_path = create_test_file(&temp_dir, "test.ts", content);

        let result = parse_typescript_file(&file_path).unwrap();
        let refs = result.external_references();

        assert!(refs.contains("Foo"));
        assert!(refs.contains("Bar"));
        // a container's own declaration name is not an external reference
        assert!(!refs.contains("MyClass"));
    }

    #[test]
    fn test_external_references_qualified_type() {
        let temp_dir = TempDir::new().unwrap();
        let content = r#"
let x: ns.Type;
        "#;
        let file_path = create_test_file(&temp_dir, "test.ts", content);

        let result = parse_typescript_file(&file_path).unwrap();
        let refs = result.external_references();

        assert!(refs.contains("ns.Type"));
    }

    #[test]
    fn test_external_references_new_expression() {
        let temp_dir = TempDir::new().unwrap();
        let content = r#"
const x = new Foo();
        "#;
        let file_path = create_test_file(&temp_dir, "test.ts", content);

        let result = parse_typescript_file(&file_path).unwrap();
        let refs = result.external_references();

        assert!(refs.contains("Foo"));
    }

    #[test]
    fn test_empty_file() {
        let temp_dir = TempDir::new().unwrap();
        let content = "";
        let file_path = create_test_file(&temp_dir, "test.ts", content);
        let result = parse_typescript_file(&file_path).unwrap();
        assert_eq!(result.loc(), 1);
        assert!(result.imports().is_empty());
        assert!(result.functions().is_empty());
        assert!(result.containers().is_empty());
    }
}
