use once_cell::sync::Lazy;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use tiny_skia::Color;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    Python,
    Rust,
    TypeScript,
}

impl Language {
    /// Returns all file extensions that indicate this language
    pub fn extensions(&self) -> &'static [&'static str] {
        match self {
            Language::Python => &["py"],
            Language::Rust => &["rs"],
            Language::TypeScript => &["ts"],
        }
    }

    /// Try to detect the language from a file extension or config file name
    pub fn from_file(filename: &str) -> Option<Self> {
        static EXTENSION_MAP: Lazy<HashMap<&'static str, Language>> = Lazy::new(|| {
            let mut map = HashMap::new();
            for lang in &[Language::Python, Language::Rust, Language::TypeScript] {
                for extension in lang.extensions() {
                    map.insert(*extension, *lang);
                }
            }
            map
        });

        let ext = filename.split('.').next_back().unwrap_or(filename);
        EXTENSION_MAP.get(ext).copied()
    }

    #[allow(clippy::wrong_self_convention)]
    pub fn to_string(&self) -> &'static str {
        match self {
            Language::Python => "Python",
            Language::Rust => "Rust",
            Language::TypeScript => "TypeScript",
        }
    }

    /// Returns the color associated with this language (in hex format)
    pub fn color(&self) -> &'static str {
        match self {
            Language::Python => "#FFD43B",
            Language::Rust => "#DEA584",
            Language::TypeScript => "#007ACC",
        }
    }

    /// Returns the color associated with this language (in RGBA format)
    pub fn color_rgba(&self) -> Color {
        match self {
            Language::Python => Color::from_rgba(1.0, 212.0 / 255.0, 59.0 / 255.0, 1.0).unwrap(),
            Language::Rust => {
                Color::from_rgba(222.0 / 255.0, 165.0 / 255.0, 132.0 / 255.0, 1.0).unwrap()
            }
            Language::TypeScript => {
                Color::from_rgba(0.0, 122.0 / 255.0, 204.0 / 255.0, 1.0).unwrap()
            }
        }
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
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
    imports: HashSet<Import>,
    /// List of function names defined in this file
    functions: HashSet<String>,
    /// List of container names (classes, structs, etc.) defined in this file
    containers: HashSet<String>,
    /// List of references to external functions/containers (as strings)
    external_references: HashSet<String>,
}

impl FileNode {
    pub fn new(
        file: PathBuf,
        loc: u32,
        language: Language,
        imports: HashSet<Import>,
        functions: HashSet<String>,
        containers: HashSet<String>,
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

    pub fn imports(&self) -> &HashSet<Import> {
        &self.imports
    }

    pub fn functions(&self) -> &HashSet<String> {
        &self.functions
    }

    pub fn containers(&self) -> &HashSet<String> {
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

    /// Calculate the normalized size for this node based on min/max LOC and betweenness centrality
    /// Returns a value between min_size and max_size
    pub fn calculate_size(
        &self, 
        min_loc: u32, 
        max_loc: u32, 
        min_size: f32, 
        max_size: f32,
        betweenness: Option<f64>
    ) -> f32 {
        // Calculate base size from LOC
        let base_size = if max_loc == min_loc {
            (min_size + max_size) / 2.0
        } else {
            let loc = self.data.loc();
            let normalized = (loc - min_loc) as f32 / (max_loc - min_loc) as f32;
            min_size + normalized * (max_size - min_size)
        };

        // Adjust size based on betweenness centrality if available
        if let Some(betweenness_score) = betweenness {
            // Increase size by up to 40% based on betweenness centrality
            base_size * (1.0 + betweenness_score as f32 * 0.4)
        } else {
            base_size
        }
    }
}
