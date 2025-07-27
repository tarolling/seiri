use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    C,
    Cpp,
    JavaScript,
    Python,
    Rust,
    Go,
    Java,
}

impl Language {
    /// Returns the primary file extension for this language
    pub fn extension(&self) -> &'static str {
        match self {
            Language::C => "c",
            Language::Cpp => "cpp",
            Language::JavaScript => "js",
            Language::Python => "py",
            Language::Rust => "rs",
            Language::Go => "go",
            Language::Java => "java",
        }
    }

    /// Returns all file extensions that indicate this language
    pub fn indicators(&self) -> &'static [&'static str] {
        match self {
            Language::C => &["c", "h"],
            Language::Cpp => &["cpp", "hpp"],
            Language::JavaScript => &["js"],
            Language::Python => &["py"],
            Language::Rust => &["rs"],
            Language::Go => &["go"],
            Language::Java => &["java"],
        }
    }

    /// Try to detect the language from a file extension or config file name
    pub fn from_file(filename: &str) -> Option<Self> {
        static EXTENSION_MAP: Lazy<HashMap<&'static str, Language>> = Lazy::new(|| {
            let mut map = HashMap::new();
            // Populate with all languages and their indicators
            for lang in &[
                Language::C,
                Language::Cpp,
                Language::JavaScript,
                Language::Python,
                Language::Rust,
                Language::Go,
                Language::Java,
            ] {
                for indicator in lang.indicators() {
                    map.insert(*indicator, *lang);
                }
            }
            map
        });

        let ext = filename.split('.').last().unwrap_or(filename);
        EXTENSION_MAP.get(ext).copied()
    }

    pub fn to_string(&self) -> &'static str {
        match self {
            Language::C => "C",
            Language::Cpp => "Cpp",
            Language::JavaScript => "JavaScript",
            Language::Python => "Python",
            Language::Rust => "Rust",
            Language::Go => "Go",
            Language::Java => "Java",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Import {
    pub path: String,
    pub is_local: bool,
}

#[derive(Debug, Clone)]
pub struct Node {
    pub file: PathBuf,
    pub language: Language,
    /// List of imports with local/external classification
    pub imports: Vec<Import>,
    /// List of function names defined in this file
    pub functions: Vec<String>,
    /// List of container names (classes, structs, etc.) defined in this file
    pub containers: Vec<String>,
    /// List of references to external functions/containers (as strings)
    pub external_references: Vec<String>,
}

/// A node in the project graph, with edges to other nodes it references
#[derive(Debug, Clone)]
pub struct GraphNode {
    pub node: Node,
    /// Edges to other files (by file path)
    pub edges: Vec<PathBuf>,
}
