pub mod cpp;
pub mod python;
pub mod rust;
pub mod typescript;

/// Helper function to extract text from a node.
#[inline]
pub fn get_text(n: tree_sitter::Node, code: &str) -> String {
    n.utf8_text(code.as_bytes()).unwrap_or("").to_string()
}
