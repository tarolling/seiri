//! Source file discovery: walking a project while honouring ignore files, and
//! bucketing the files it finds by [`Language`].

use crate::core::defs::Language;
use ignore::WalkBuilder;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Classifies `target_file` by language, recording both the file and the
/// detected language when it is supported.
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

/// Walks `files_to_process`, filling `language_files` with every supported file
/// and returning the set of languages present. Returns `None` when the project
/// contains no supported files at all.
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

/// Collects every regular file under `path`, respecting `.gitignore` unless
/// `no_gitignore` is set.
pub fn walk_directory(path: &Path, no_gitignore: bool) -> Vec<PathBuf> {
    let mut paths = Vec::new();

    let mut builder = WalkBuilder::new(path);
    if no_gitignore {
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
        let files_to_process = walk_directory(&current_dir, false);
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

        let files = walk_directory(dir.path(), false);
        assert!(!files.iter().any(|p| p.ends_with("ignored.txt")));
    }

    #[test]
    fn ignores_no_gitignore_flag() {
        let dir = tempfile::tempdir().unwrap();
        let ignored_file = dir.path().join("ignored.txt");
        File::create(&ignored_file).unwrap();

        fs::write(dir.path().join(".gitignore"), "ignored.txt\n").unwrap();

        let files = walk_directory(dir.path(), true);
        assert!(files.iter().any(|p| p.ends_with("ignored.txt")));
    }

    #[test]
    fn test_detect_empty_dir_returns_none() {
        let dir = TempDir::new().unwrap();
        let mut language_files: HashMap<PathBuf, Language> = HashMap::new();
        let files_to_process = walk_directory(dir.path(), true);

        assert!(detect_project_languages(&files_to_process, &mut language_files).is_none());
    }
}
