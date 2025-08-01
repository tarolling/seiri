use once_cell::sync::Lazy;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    Rust,
    Python,
}

impl Language {
    /// Returns all file extensions that indicate this language
    pub fn extensions(&self) -> &'static [&'static str] {
        match self {
            Language::Rust => &["rs"],
            Language::Python => &["py"],
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

        let ext = filename.split('.').next_back().unwrap_or(filename);
        EXTENSION_MAP.get(ext).copied()
    }

    #[allow(clippy::wrong_self_convention, dead_code)]
    pub fn to_string(&self) -> &'static str {
        match self {
            Language::Rust => "Rust",
            Language::Python => "Python",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Import {
    path: String,
    is_local: bool,
}

impl Import {
    pub fn new(path: String, is_local: bool) -> Self {
        Import { path, is_local }
    }

    /// Get the import path
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Check if this import is local (within the same project)
    /// or external (from another project or library)
    pub fn is_local(&self) -> bool {
        self.is_local
    }
}

#[derive(Debug, Clone)]
pub struct FileNode {
    file: PathBuf,
    loc: u32,
    language: Language,
    /// List of imports with local/external classification
    imports: Vec<Import>,
    /// List of function names defined in this file
    functions: Vec<String>,
    /// List of container names (classes, structs, etc.) defined in this file
    containers: Vec<String>,
    /// List of references to external functions/containers (as strings)
    external_references: HashSet<String>,
}

impl FileNode {
    pub fn new(
        file: PathBuf,
        loc: u32,
        language: Language,
        imports: Vec<Import>,
        functions: Vec<String>,
        containers: Vec<String>,
        external_references: HashSet<String>,
    ) -> Self {
        FileNode {
            file,
            loc,
            language,
            imports,
            functions,
            containers,
            external_references,
        }
    }

    pub fn file(&self) -> &PathBuf {
        &self.file
    }

    pub fn loc(&self) -> u32 {
        self.loc
    }

    pub fn language(&self) -> &Language {
        &self.language
    }

    pub fn imports(&self) -> &Vec<Import> {
        &self.imports
    }

    pub fn functions(&self) -> &Vec<String> {
        &self.functions
    }

    pub fn containers(&self) -> &Vec<String> {
        &self.containers
    }

    pub fn external_references(&self) -> &HashSet<String> {
        &self.external_references
    }
}

/// A node in the project graph, with edges to other nodes it references
#[derive(Debug, Clone)]
pub struct GraphNode {
    data: FileNode,
    /// Edges to other files (by file path)
    edges: Vec<PathBuf>,
}

impl GraphNode {
    pub fn new(data: FileNode, edges: Vec<PathBuf>) -> Self {
        GraphNode { data, edges }
    }

    pub fn data(&self) -> &FileNode {
        &self.data
    }

    pub fn edges(&self) -> &Vec<PathBuf> {
        self.edges.as_ref()
    }

    /// Calculate the normalized size for this node based on min/max LOC in the graph
    /// Returns a value between min_size and max_size
    pub fn calculate_size(&self, min_loc: u32, max_loc: u32, min_size: f32, max_size: f32) -> f32 {
        if max_loc == min_loc {
            return (min_size + max_size) / 2.0;
        }
        let loc = self.data.loc();
        let normalized = (loc - min_loc) as f32 / (max_loc - min_loc) as f32;
        min_size + normalized * (max_size - min_size)
    }
}
