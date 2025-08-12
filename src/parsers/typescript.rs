use crate::core::defs::{FileNode, Import, Language};
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use tree_sitter::Parser;
use tree_sitter_typescript as ts_typescript;

/// Get node text
fn get_text(n: tree_sitter::Node, code: &str) -> String {
    n.utf8_text(code.as_bytes()).unwrap_or("").to_string()
}

/// Local imports are typically relative paths starting with '.'
fn is_local_import(import_path: &str) -> bool {
    import_path.starts_with('.')
}

/// Extracts the import path string from an import or export statement
fn extract_import_path(node: tree_sitter::Node, code: &str) -> Option<String> {
    node.child_by_field_name("source")
        .map(|source_node| get_text(source_node, code))
        .map(|path_text| path_text.trim_matches('"').trim_matches('\'').to_string())
}

pub fn parse_typescript_file<P: AsRef<Path>>(path: P) -> Option<FileNode> {
    let code = fs::read_to_string(&path).ok()?;
    let loc = code.matches('\n').count() as u32 + 1;

    let mut parser = Parser::new();
    parser
        .set_language(&ts_typescript::LANGUAGE_TYPESCRIPT.into())
        .expect("Error loading TypeScript grammar");
    let tree = parser.parse(&code, None)?;
    let root_node = tree.root_node();

    let mut imports = HashSet::new();
    let mut functions = HashSet::new();
    let mut containers = HashSet::new();
    let external_references = HashSet::new();

    let mut stack = vec![root_node];

    while let Some(node) = stack.pop() {
        match node.kind() {
            // `import ... from '...';`, `export ... from '...';`
            "import_statement" | "export_statement" => {
                if let Some(import_path) = extract_import_path(node, &code) {
                    let is_local = is_local_import(&import_path);
                    imports.insert(Import::new(import_path, is_local));
                }
            }

            // `function hello() {}`
            "function_declaration" => {
                if let Some(name_node) = node.child_by_field_name("name") {
                    functions.insert(get_text(name_node, &code));
                }
            }

            // `class MyClass { method() {} }`
            "method_definition" => {
                if let Some(name_node) = node.child_by_field_name("name") {
                    functions.insert(get_text(name_node, &code));
                }
            }

            // `const myFunc = () => {}`, `let myVar = () => {}`
            "lexical_declaration" => {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() != "variable_declarator" {
                        continue;
                    }

                    // Check if the declarator's value is an arrow function
                    if let Some(value_node) = child.child_by_field_name("value")
                        && value_node.kind() == "arrow_function"
                        && let Some(name_node) = child.child_by_field_name("name")
                    {
                        functions.insert(get_text(name_node, &code));
                    }
                }
            }

            // `class C {}`, `interface I {}`, `enum E {}`, `type T = ...`
            "class_declaration"
            | "interface_declaration"
            | "enum_declaration"
            | "type_alias_declaration" => {
                if let Some(name_node) = node.child_by_field_name("name") {
                    containers.insert(get_text(name_node, &code));
                }
            }

            _ => {}
        }

        let mut child_cursor = node.walk();
        for child in node.children(&mut child_cursor) {
            stack.push(child);
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
