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

    /// Returns all file extensions and config files that indicate this language
    pub fn indicators(&self) -> &'static [&'static str] {
        match self {
            Language::C => &["c", "h"],
            Language::Cpp => &["cpp", "hpp", "h", "CMakeLists.txt"],
            Language::JavaScript => &["js", "ts", "package.json", "tsconfig.json"],
            Language::Python => &["py", "requirements.txt", "pyproject.toml", "setup.py"],
            Language::Rust => &["rs", "Cargo.toml"],
            Language::Go => &["go", "go.mod"],
            Language::Java => &["java", "pom.xml", "build.gradle"],
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

pub struct Node {
    pub file: PathBuf,
    pub language: Language,
}
