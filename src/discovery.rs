//! Source file discovery: walking a project while honoring ignore files and
//! skipping vendored dependencies, and bucketing the files it finds by
//! [`Language`].

use crate::core::defs::Language;
use ignore::WalkBuilder;
use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

/// Directories that hold third-party dependencies or sources vendored into the
/// repository rather than the project's own code. Parsing them is by far the
/// most expensive part of scanning a repository like OpenCV or Playwright, and
/// everything they contribute to the graph is noise, so they are skipped.
const VENDORED_DIRS: &[&str] = &[
    "3rdparty",
    "bower_components",
    "extern",
    "external",
    "node_modules",
    "site-packages",
    "third_party",
    "thirdparty",
    "vendor",
    "vendored",
    "venv",
];

/// Whether a directory named `name` holds vendored dependencies.
fn is_vendored_dir(name: &OsStr) -> bool {
    VENDORED_DIRS.contains(&name.to_str().unwrap_or_default())
}

/// How [`walk_directory`] treats a project's ignore files and vendored code.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WalkOptions {
    /// Walk every file, ignoring `.gitignore` and the other ignore files.
    pub no_gitignore: bool,
    /// Walk vendored dependency trees, which are skipped by default.
    pub include_vendored: bool,
}

impl WalkOptions {
    /// Honors ignore files and skips vendored dependency trees.
    pub fn new() -> Self {
        Self::default()
    }

    /// Honors ignore files but walks vendored dependency trees too.
    pub fn including_vendored() -> Self {
        Self {
            include_vendored: true,
            ..Self::default()
        }
    }
}

/// Classify `target_file` by language.
pub fn detect_file_language(
    target_file: PathBuf,
    language_files: &mut HashMap<PathBuf, Language>,
    detected_langs: &mut HashSet<Language>,
) {
    if let Some(file_language) = Language::from_file(&target_file.to_string_lossy()) {
        language_files.insert(target_file.clone(), file_language);
        detected_langs.insert(file_language);
    }
}

/// Walks `files_to_process`, returning the set of languages present.
/// Returns `None` when the project contains no supported files.
pub fn detect_project_languages(
    files_to_process: &[PathBuf],
    language_files: &mut HashMap<PathBuf, Language>,
) -> Option<HashSet<Language>> {
    let mut detected: HashSet<Language> = HashSet::new();
    files_to_process
        .iter()
        .for_each(|entry| detect_file_language(entry.to_path_buf(), language_files, &mut detected));

    if detected.is_empty() {
        None
    } else {
        Some(detected)
    }
}

/// Collects every regular file under `path`, respecting `.gitignore` and skipping
/// vendored dependency trees unless `options` says otherwise.
pub fn walk_directory(path: &Path, options: WalkOptions) -> Vec<PathBuf> {
    let mut paths = Vec::new();

    let mut builder = WalkBuilder::new(path);
    if options.no_gitignore {
        builder
            .git_ignore(false)
            .git_exclude(false)
            .git_global(false)
            .ignore(false);
    } else {
        // for most/all projects, gitignore and other ignore files will be automatically detected by ignore crate
        // but they don't when using tempfile and/or when running tests
        let gitignore_path = path.join(".gitignore");
        if gitignore_path.exists() {
            builder.add_ignore(gitignore_path);
        }
    }

    // Pruning here keeps the whole vendored subtree out of the walk, so its
    // files are never even stat'ed, let alone parsed.
    if !options.include_vendored {
        builder.filter_entry(|entry| {
            !entry
                .file_type()
                .is_some_and(|file_type| file_type.is_dir() && is_vendored_dir(entry.file_name()))
        });
    }

    for result in builder.build() {
        match result {
            Ok(entry) => {
                if let Some(file_type) = entry.file_type()
                    && file_type.is_file()
                {
                    paths.push(entry.path().to_path_buf());
                }
            }
            Err(msg) => eprintln!("Error reading entry: {msg}"),
        }
    }

    paths
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use tempfile::TempDir;

    #[test]
    fn test_detect_file() {
        let current_file = Path::new(file!());
        assert!(current_file.try_exists().is_ok());

        let mut language_files: HashMap<PathBuf, Language> = HashMap::new();
        let mut detected_languages = HashSet::new();
        detect_file_language(
            current_file.to_path_buf(),
            &mut language_files,
            &mut detected_languages,
        );

        assert!(!detected_languages.is_empty());
        assert!(detected_languages.contains(&Language::Rust));
    }

    #[test]
    fn test_detect_invalid_file() {
        let current_file = Path::new("Cargo.lock");
        assert!(current_file.try_exists().is_ok());

        let mut language_files: HashMap<PathBuf, Language> = HashMap::new();
        let mut detected_languages = HashSet::new();
        detect_file_language(
            current_file.to_path_buf(),
            &mut language_files,
            &mut detected_languages,
        );

        assert!(detected_languages.is_empty());
    }

    #[test]
    fn test_detect_dir() {
        let current_dir = Path::new(file!()).parent().unwrap().canonicalize().unwrap();
        assert!(current_dir.try_exists().is_ok());

        let mut language_files: HashMap<PathBuf, Language> = HashMap::new();
        let files_to_process = walk_directory(&current_dir, WalkOptions::new());
        let result = detect_project_languages(&files_to_process, &mut language_files);

        assert!(&result.is_some());

        let langs = result.unwrap();
        assert_eq!(langs.len(), 1);
        assert!(langs.contains(&Language::Rust));
    }

    #[test]
    fn respects_gitignore() {
        let dir = tempfile::tempdir().unwrap();
        let ignored_file = dir.path().join("ignored.txt");
        File::create(&ignored_file).unwrap();

        fs::write(dir.path().join(".gitignore"), "ignored.txt\n").unwrap();

        let files = walk_directory(dir.path(), WalkOptions::new());
        assert!(!files.iter().any(|p| p.ends_with("ignored.txt")));
    }

    #[test]
    fn ignores_no_gitignore_flag() {
        let dir = tempfile::tempdir().unwrap();
        let ignored_file = dir.path().join("ignored.txt");
        File::create(&ignored_file).unwrap();

        fs::write(dir.path().join(".gitignore"), "ignored.txt\n").unwrap();

        let files = walk_directory(
            dir.path(),
            WalkOptions {
                no_gitignore: true,
                ..WalkOptions::new()
            },
        );
        assert!(files.iter().any(|p| p.ends_with("ignored.txt")));
    }

    #[test]
    fn test_detect_empty_dir_returns_none() {
        let dir = TempDir::new().unwrap();
        let mut language_files: HashMap<PathBuf, Language> = HashMap::new();
        let files_to_process = walk_directory(dir.path(), WalkOptions::new());

        assert!(detect_project_languages(&files_to_process, &mut language_files).is_none());
    }

    /// Builds a project holding one file per vendored directory name, all
    /// committed rather than ignored, so only the vendored rule can skip them.
    fn vendored_project(dir: &Path) -> Vec<PathBuf> {
        let mut files = Vec::new();
        for name in VENDORED_DIRS {
            let nested = dir.join(name).join("package");
            fs::create_dir_all(&nested).unwrap();
            let file = nested.join("module.rs");
            fs::write(&file, "pub fn vendored() {}\n").unwrap();
            files.push(file);
        }

        let own = dir.join("src");
        fs::create_dir_all(&own).unwrap();
        fs::write(own.join("main.rs"), "pub fn own() {}\n").unwrap();

        files
    }

    #[test]
    fn skips_vendored_dependency_trees() {
        let dir = tempfile::tempdir().unwrap();
        let vendored = vendored_project(dir.path());

        let files = walk_directory(
            dir.path(),
            WalkOptions {
                no_gitignore: true,
                ..WalkOptions::new()
            },
        );

        for path in &vendored {
            assert!(
                !files.contains(path),
                "vendored file walked: {}",
                path.display()
            );
        }
        assert!(files.iter().any(|p| p.ends_with("src/main.rs")));
    }

    #[test]
    fn include_vendored_walks_dependency_trees() {
        let dir = tempfile::tempdir().unwrap();
        let vendored = vendored_project(dir.path());

        let files = walk_directory(
            dir.path(),
            WalkOptions {
                no_gitignore: true,
                ..WalkOptions::including_vendored()
            },
        );

        for path in &vendored {
            assert!(
                files.contains(path),
                "vendored file skipped: {}",
                path.display()
            );
        }
    }

    #[test]
    fn vendored_directory_names_only_match_whole_names() {
        // a first-party directory that merely contains a vendor-ish name stays
        assert!(!is_vendored_dir(OsStr::new("my_vendor_utils")));
        assert!(!is_vendored_dir(OsStr::new("vendor.rs")));
        assert!(is_vendored_dir(OsStr::new("node_modules")));
    }
}
