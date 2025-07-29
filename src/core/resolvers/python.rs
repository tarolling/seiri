use super::LanguageResolver;
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

pub struct PythonResolver {}

impl LanguageResolver for PythonResolver {
    fn build_module_map(&mut self, _files: &[PathBuf], _project_root: &Path) {
        todo!("Python module mapping not implemented yet");
    }

    fn resolve_import(&self, _import_path: &str, _from_file: &Path) -> Option<PathBuf> {
        todo!("Python import resolution not implemented yet");
    }

    fn resolve_external_references(
        &self,
        _references: &HashSet<String>,
        _from_file: &Path,
    ) -> Vec<PathBuf> {
        todo!("Python external reference resolution not implemented yet");
    }
}
