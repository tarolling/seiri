use once_cell::sync::Lazy;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    Rust,
}

impl Language {
    /// Returns all file extensions that indicate this language
    pub fn extensions(&self) -> &'static [&'static str] {
        match self {
            Language::Rust => &["rs"],
        }
    }

    /// Try to detect the language from a file extension or config file name
    pub fn from_file(filename: &str) -> Option<Self> {
        static EXTENSION_MAP: Lazy<HashMap<&'static str, Language>> = Lazy::new(|| {
            let mut map = HashMap::new();
            // Populate with all languages and their indicators
            for lang in &[Language::Rust] {
                for extension in lang.extensions() {
                    map.insert(*extension, *lang);
                }
            }
            map
        });

        let ext = filename.split('.').last().unwrap_or(filename);
        EXTENSION_MAP.get(ext).copied()
    }

    pub fn to_string(&self) -> &'static str {
        match self {
            Language::Rust => "Rust",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Import {
    pub path: String,
    pub is_local: bool,
}

#[derive(Debug, Clone)]
pub struct FileNode {
    pub file: PathBuf,
    pub language: Language,
    /// List of imports with local/external classification
    pub imports: Vec<Import>,
    /// List of function names defined in this file
    pub functions: Vec<String>,
    /// List of container names (classes, structs, etc.) defined in this file
    pub containers: Vec<String>,
    /// List of references to external functions/containers (as strings)
    pub external_references: HashSet<String>,
}

/// A node in the project graph, with edges to other nodes it references
#[derive(Debug, Clone)]
pub struct GraphNode {
    pub data: FileNode,
    /// Edges to other files (by file path)
    pub edges: Vec<PathBuf>,
}
