//! An on-disk cache of parsed files, so re-scanning a project that has not
//! changed does not run every parser again.
//!
//! Parsing is nearly the whole cost of a scan, and it is pure: the same bytes
//! always produce the same [`FileNode`]. An entry is therefore only good for as
//! long as the file's size and modification time are unchanged, which costs one
//! `stat` per file instead of a parse. A cache written by another version of
//! seiri, or one that has been damaged, is discarded rather than trusted: a
//! slower run is always better than a wrong graph.

use crate::core::defs::{FileNode, Import, Language};
use std::collections::{HashMap, HashSet};
use std::env;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

/// Starts every cache file, so an unrelated file is never mistaken for one.
const MAGIC: &[u8; 8] = b"SEIRICCH";

/// Identifies the layout of a cache file. Bump it whenever the encoding below
/// changes shape.
const FORMAT_VERSION: u32 = 1;

/// The seiri version that wrote a cache. Releases bump the crate version, so a
/// release that parses differently never reuses an older run's results.
const CACHE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Largest cache seiri will write, so scanning a large monorepo cannot fill up
/// a disk. A project over this size is simply rescanned on every run.
const MAX_CACHE_BYTES: usize = 512 * 1024 * 1024;

/// The directory seiri keeps parse caches in, or `None` when the environment
/// names no such directory.
///
/// [`ParseCache::load`] reads it; tests write their own directory instead.
pub fn cache_dir() -> Option<PathBuf> {
    cache_dir_from(
        env::var_os("SEIRI_CACHE_DIR"),
        env::var_os("XDG_CACHE_HOME"),
        env::var_os("HOME"),
    )
}

/// Picks the cache directory from the environment variables that name one,
/// most explicit first.
///
/// A directory only counts when the current user owns it, so nothing another
/// process owns (the system temp directory, say) can be read or overwritten
/// through a planted symlink. An empty variable is treated as unset, so an
/// exported-but-blank one cannot turn the cache into a relative path. Returns
/// `None` when the environment names none.
fn cache_dir_from(
    override_dir: Option<OsString>,
    xdg_cache_home: Option<OsString>,
    home: Option<OsString>,
) -> Option<PathBuf> {
    let named = |value: Option<OsString>| value.filter(|value| !value.is_empty());

    if let Some(dir) = named(override_dir) {
        return Some(PathBuf::from(dir));
    }
    if let Some(dir) = named(xdg_cache_home) {
        return Some(PathBuf::from(dir).join("seiri"));
    }
    named(home).map(|home| PathBuf::from(home).join(".cache").join("seiri"))
}

/// The cache file for `project_root` inside `dir`.
///
/// The name folds in a hash of the root so that two projects scanned from the
/// same directory do not share a file.
fn cache_file_for(dir: &Path, project_root: &Path) -> PathBuf {
    // FNV-1a, so the name is stable across seiri releases; a hash whose value
    // changed with the toolchain would silently throw every cache away.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in project_root.to_string_lossy().as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    dir.join(format!("{hash:016x}.cache"))
}

/// A file's size and modification time, which together stand in for its
/// contents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FileStamp {
    modified_secs: u64,
    modified_nanos: u32,
    size: u64,
}

impl FileStamp {
    /// Reads the stamp of `path`, or `None` when it cannot be read at all.
    fn read(path: &Path) -> Option<Self> {
        let metadata = fs::metadata(path).ok()?;
        let since_epoch = metadata.modified().ok()?.duration_since(UNIX_EPOCH).ok()?;

        Some(FileStamp {
            modified_secs: since_epoch.as_secs(),
            modified_nanos: since_epoch.subsec_nanos(),
            size: metadata.len(),
        })
    }
}

/// A parsed file, remembered from a previous run.
#[derive(Debug, Clone)]
struct CacheEntry {
    stamp: FileStamp,
    node: FileNode,
}

/// Reads the bytes that make up a cache file, refusing anything malformed.
struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Reader { bytes, at: 0 }
    }

    /// Takes the next `count` bytes, or `None` when the file is shorter.
    fn take(&mut self, count: usize) -> Option<&'a [u8]> {
        let end = self.at.checked_add(count)?;
        let slice = self.bytes.get(self.at..end)?;
        self.at = end;
        Some(slice)
    }

    fn u8(&mut self) -> Option<u8> {
        Some(self.take(1)?[0])
    }

    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }

    fn u64(&mut self) -> Option<u64> {
        Some(u64::from_le_bytes(self.take(8)?.try_into().ok()?))
    }

    /// Reads a length-prefixed string. The length is checked against what is
    /// left of the file, so a corrupt one cannot ask for a huge allocation.
    fn string(&mut self) -> Option<String> {
        let len = self.u32()? as usize;
        String::from_utf8(self.take(len)?.to_vec()).ok()
    }

    fn is_empty(&self) -> bool {
        self.at == self.bytes.len()
    }
}

/// Collects the bytes of a cache file, stopping once it has grown too large.
struct Writer {
    bytes: Vec<u8>,
    too_large: bool,
}

impl Writer {
    fn with_capacity(capacity: usize) -> Self {
        Writer {
            bytes: Vec::with_capacity(capacity),
            too_large: false,
        }
    }

    fn u8(&mut self, value: u8) {
        self.bytes.push(value);
    }

    fn u32(&mut self, value: u32) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn u64(&mut self, value: u64) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn string(&mut self, value: &str) {
        self.u32(value.len() as u32);
        self.bytes.extend_from_slice(value.as_bytes());
    }

    /// Writes a set of names in sorted order, so the same inputs always
    /// produce the same bytes and a cache file can be compared run to run.
    fn string_set(&mut self, values: &HashSet<String>) {
        let mut values: Vec<&str> = values.iter().map(String::as_str).collect();
        values.sort_unstable();
        self.u32(values.len() as u32);
        for value in values {
            self.string(value);
        }
    }

    /// Stops recording once the cache has passed [`MAX_CACHE_BYTES`], so one
    /// enormous project cannot grow the buffer without bound.
    fn check_size(&mut self) {
        if self.bytes.len() > MAX_CACHE_BYTES {
            self.too_large = true;
        }
    }
}

/// Parse results from an earlier run of the same project.
#[derive(Debug, Default)]
pub struct ParseCache {
    file: PathBuf,
    project_root: PathBuf,
    entries: HashMap<PathBuf, CacheEntry>,
    dirty: bool,
    hits: usize,
}

impl ParseCache {
    /// Reads the cache for `project_root` from [`cache_dir`], or `None` when
    /// the environment names no cache directory.
    ///
    /// Without a cache every run parses again, which is slower but always
    /// correct.
    pub fn load(project_root: &Path) -> Option<Self> {
        Some(Self::load_from(project_root, &cache_dir()?))
    }

    /// Reads the cache for `project_root` from `dir`, or starts an empty one.
    ///
    /// A cache that is missing, unreadable, written by another version, or
    /// damaged is ignored rather than reported: the caller can always fall back
    /// to parsing.
    pub fn load_from(project_root: &Path, dir: &Path) -> Self {
        let file = cache_file_for(dir, project_root);
        let entries = fs::read(&file)
            .ok()
            .and_then(|bytes| decode(&bytes, project_root))
            .unwrap_or_default();

        ParseCache {
            file,
            project_root: project_root.to_path_buf(),
            entries,
            dirty: false,
            hits: 0,
        }
    }

    /// The parsed file remembered for `path`, or `None` when the cache holds
    /// none that is still current.
    ///
    /// A hit costs a single `stat`; a miss means the caller parses the file and
    /// hands the result to [`ParseCache::record`].
    pub fn get(&mut self, path: &Path) -> Option<FileNode> {
        let stamp = FileStamp::read(path);
        let (recorded, node) = self.entries.get(path).map(|e| (e.stamp, &e.node))?;

        if Some(recorded) != stamp {
            // The file has changed, or has gone away entirely, so the entry is
            // history either way.
            self.entries.remove(path);
            self.dirty = true;
            return None;
        }

        self.hits += 1;
        Some(node.clone())
    }

    /// Remembers a freshly parsed file, so the next run can skip it.
    ///
    /// A file that has gone away between the parse and here is not recorded.
    pub fn record(&mut self, path: &Path, node: &FileNode) {
        let Some(stamp) = FileStamp::read(path) else {
            return;
        };

        self.entries.insert(
            path.to_path_buf(),
            CacheEntry {
                stamp,
                node: node.clone(),
            },
        );
        self.dirty = true;
    }

    /// How many files the last scan took from the cache.
    pub fn hits(&self) -> usize {
        self.hits
    }

    /// How many files the cache holds.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the cache holds no files.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Whether anything read or written since loading differs from disk.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// The file this cache is read from and written to.
    pub fn path(&self) -> &Path {
        &self.file
    }

    /// Writes the cache back, unless nothing changed.
    ///
    /// Returns the file written, or `None` when there was nothing to do or the
    /// cache could not be written. A cache seiri cannot write is not an error:
    /// the scan already succeeded, and the next run simply parses again.
    pub fn save(&mut self) -> Option<PathBuf> {
        if !self.dirty {
            return None;
        }

        let Some(bytes) = encode(&self.project_root, &self.entries) else {
            self.dirty = false;
            return None;
        };
        if write_atomically(&self.file, &bytes).is_err() {
            return None;
        }

        self.dirty = false;
        Some(self.file.clone())
    }
}

/// Decodes a cache file, or `None` when it is not one seiri wrote for this
/// project and version.
fn decode(bytes: &[u8], project_root: &Path) -> Option<HashMap<PathBuf, CacheEntry>> {
    let mut reader = Reader::new(bytes);
    if reader.take(MAGIC.len())? != MAGIC {
        return None;
    }
    if reader.u32()? != FORMAT_VERSION || reader.string()? != CACHE_VERSION {
        return None;
    }
    if PathBuf::from(reader.string()?) != project_root {
        // A hash collision, so this cache belongs to some other project.
        return None;
    }

    let mut entries = HashMap::new();
    for _ in 0..reader.u32()? {
        let path = PathBuf::from(reader.string()?);
        let stamp = FileStamp {
            modified_secs: reader.u64()?,
            modified_nanos: reader.u32()?,
            size: reader.u64()?,
        };
        let loc = reader.u32()?;
        let language = language_from_name(&reader.string()?)?;
        let mut imports = HashSet::new();
        for _ in 0..reader.u32()? {
            let path = reader.string()?;
            let is_local = reader.u8()? != 0;
            imports.insert(Import::new(path, is_local));
        }
        let node = FileNode::new(
            path,
            loc,
            language,
            imports,
            read_string_set(&mut reader)?,
            read_string_set(&mut reader)?,
            read_string_set(&mut reader)?,
        );

        entries.insert(node.file().clone(), CacheEntry { stamp, node });
    }

    reader.is_empty().then_some(entries)
}

/// Encodes the cache, or `None` when it has grown past [`MAX_CACHE_BYTES`].
fn encode(project_root: &Path, entries: &HashMap<PathBuf, CacheEntry>) -> Option<Vec<u8>> {
    let mut writer = Writer::with_capacity(entries.len() * 512);
    writer.bytes.extend_from_slice(MAGIC);
    writer.u32(FORMAT_VERSION);
    writer.string(CACHE_VERSION);
    writer.string(&project_root.to_string_lossy());
    writer.u32(u32::try_from(entries.len()).ok()?);

    for (path, entry) in entries {
        let node = &entry.node;
        writer.string(&path.to_string_lossy());
        writer.u64(entry.stamp.modified_secs);
        writer.u32(entry.stamp.modified_nanos);
        writer.u64(entry.stamp.size);
        writer.u32(node.loc());
        writer.string(node.language().to_string());

        let mut imports: Vec<(&str, bool)> = node
            .imports()
            .iter()
            .map(|import| (import.path(), import.is_local()))
            .collect();
        imports.sort_unstable();
        writer.u32(u32::try_from(imports.len()).ok()?);
        for (import, is_local) in imports {
            writer.string(import);
            writer.u8(u8::from(is_local));
        }

        writer.string_set(node.functions());
        writer.string_set(node.containers());
        writer.string_set(node.external_references());
        writer.check_size();
        if writer.too_large {
            return None;
        }
    }

    Some(writer.bytes)
}

/// The language whose display name is `name`.
fn language_from_name(name: &str) -> Option<Language> {
    match name {
        "Python" => Some(Language::Python),
        "Rust" => Some(Language::Rust),
        "TypeScript" => Some(Language::TypeScript),
        "C++" => Some(Language::Cpp),
        _ => None,
    }
}

fn read_string_set(reader: &mut Reader) -> Option<HashSet<String>> {
    let mut values = HashSet::new();
    for _ in 0..reader.u32()? {
        values.insert(reader.string()?);
    }
    Some(values)
}

/// Writes `bytes` to `file` so a reader never sees a half-written cache.
fn write_atomically(file: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(parent) = file.parent() {
        fs::create_dir_all(parent)?;
    }

    let name = file.file_name().unwrap_or_default().to_string_lossy();
    let temporary = file.with_file_name(format!("{name}.{}.tmp", std::process::id()));
    fs::write(&temporary, bytes)?;

    match fs::rename(&temporary, file) {
        Ok(()) => Ok(()),
        Err(rename_error) => {
            // Windows will not rename over a file that already exists.
            if fs::remove_file(file).is_ok() {
                fs::rename(&temporary, file)
            } else {
                Err(rename_error)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// A parsed file with something in every field, so a round trip that drops
    /// or swaps any part of it shows up.
    fn sample_node(path: &Path) -> FileNode {
        FileNode::new(
            path.to_path_buf(),
            42,
            Language::Cpp,
            HashSet::from([
                Import::new("vector".to_string(), false),
                Import::new("helper".to_string(), true),
            ]),
            HashSet::from(["draw".to_string(), "measure".to_string()]),
            HashSet::from(["Mat".to_string()]),
            HashSet::from(["cv::Mat".to_string(), "std::move".to_string()]),
        )
    }

    /// The parts of a node a cache round trip has to preserve.
    fn same_contents(left: &FileNode, right: &FileNode) -> bool {
        left.file() == right.file()
            && left.loc() == right.loc()
            && left.language() == right.language()
            && left.imports() == right.imports()
            && left.functions() == right.functions()
            && left.containers() == right.containers()
            && left.external_references() == right.external_references()
    }

    fn write_source(dir: &Path, name: &str, contents: &str) -> PathBuf {
        let path = dir.join(name);
        fs::write(&path, contents).unwrap();
        path
    }

    #[test]
    fn picks_the_cache_dir_from_the_environment() {
        let override_dir = Some(OsString::from("/tmp/seiri-explicit"));
        let xdg = Some(OsString::from("/home/user/.xdg"));
        let home = Some(OsString::from("/home/user"));

        assert_eq!(
            cache_dir_from(override_dir.clone(), xdg.clone(), home.clone()),
            Some(PathBuf::from("/tmp/seiri-explicit"))
        );
        assert_eq!(
            cache_dir_from(None, xdg.clone(), home.clone()),
            Some(PathBuf::from("/home/user/.xdg/seiri"))
        );
        assert_eq!(
            cache_dir_from(None, None, home),
            Some(PathBuf::from("/home/user/.cache/seiri"))
        );
    }

    #[test]
    fn has_no_cache_dir_without_one_of_its_own() {
        assert_eq!(
            cache_dir_from(None, None, None),
            None,
            "an environment naming no user-owned directory must not fall back to a shared one"
        );
        assert_eq!(
            cache_dir_from(Some(OsString::new()), None, None),
            None,
            "an exported but blank variable names no directory"
        );
        assert_eq!(
            cache_dir_from(None, None, Some(OsString::new())),
            None,
            "an exported but blank HOME names no directory"
        );
    }

    #[test]
    fn round_trips_a_parsed_file() {
        let dir = TempDir::new().unwrap();
        let source = write_source(dir.path(), "alpha.cpp", "int alpha() { return 0; }\n");
        let node = sample_node(&source);

        let mut cache = ParseCache::load_from(dir.path(), dir.path());
        assert!(cache.get(&source).is_none(), "an empty cache holds nothing");
        cache.record(&source, &node);
        assert!(cache.save().is_some());

        let mut reloaded = ParseCache::load_from(dir.path(), dir.path());
        let cached = reloaded.get(&source).expect("the entry survives");
        assert!(same_contents(&node, &cached));
        assert_eq!(reloaded.hits(), 1);
        assert_eq!(reloaded.len(), 1);
    }

    #[test]
    fn round_trips_every_language() {
        let dir = TempDir::new().unwrap();
        let languages = [
            Language::Python,
            Language::Rust,
            Language::TypeScript,
            Language::Cpp,
        ];

        let mut cache = ParseCache::load_from(dir.path(), dir.path());
        for (i, language) in languages.iter().enumerate() {
            let path = write_source(dir.path(), &format!("file{i}"), "// content\n");
            let node = FileNode::new(
                path.clone(),
                1,
                *language,
                HashSet::new(),
                HashSet::new(),
                HashSet::new(),
                HashSet::new(),
            );
            cache.record(&path, &node);
        }
        cache.save();

        let mut reloaded = ParseCache::load_from(dir.path(), dir.path());
        for (i, language) in languages.iter().enumerate() {
            let path = dir.path().join(format!("file{i}"));
            assert_eq!(reloaded.get(&path).unwrap().language(), language);
        }
    }

    #[test]
    fn forgets_a_file_whose_contents_changed() {
        let dir = TempDir::new().unwrap();
        let source = write_source(dir.path(), "alpha.rs", "pub fn alpha() {}\n");

        let mut cache = ParseCache::load_from(dir.path(), dir.path());
        cache.record(&source, &sample_node(&source));
        cache.save();

        // A different length is enough to tell the file apart.
        fs::write(&source, "pub fn alpha() { } // edited\n").unwrap();

        let mut reloaded = ParseCache::load_from(dir.path(), dir.path());
        assert!(
            reloaded.get(&source).is_none(),
            "the edited file was reused"
        );
        assert!(reloaded.is_dirty(), "the stale entry should be dropped");
    }

    #[test]
    fn forgets_a_file_whose_timestamp_moved() {
        let dir = TempDir::new().unwrap();
        let source = write_source(dir.path(), "alpha.rs", "pub fn alpha() {}\n");

        let mut cache = ParseCache::load_from(dir.path(), dir.path());
        cache.record(&source, &sample_node(&source));
        let file = cache.path().to_path_buf();

        // Same bytes, newer timestamp. The stamp is what stands in for the
        // contents, so a checkout that preserves mtimes has to re-parse.
        let mut entries = cache.entries.clone();
        let entry = entries.get_mut(&source).unwrap();
        entry.stamp.modified_secs = entry.stamp.modified_secs.wrapping_add(60);
        fs::write(&file, encode(dir.path(), &entries).unwrap()).unwrap();

        let mut reloaded = ParseCache::load_from(dir.path(), dir.path());
        assert!(reloaded.get(&source).is_none(), "a moved mtime was reused");
    }

    #[test]
    fn ignores_a_cache_from_another_version() {
        let dir = TempDir::new().unwrap();
        let source = write_source(dir.path(), "alpha.rs", "pub fn alpha() {}\n");

        let mut cache = ParseCache::load_from(dir.path(), dir.path());
        cache.record(&source, &sample_node(&source));
        let file = cache.path().to_path_buf();
        cache.save();

        // The format version sits directly after the magic.
        let mut bytes = fs::read(&file).unwrap();
        let version_at = MAGIC.len();
        bytes[version_at..version_at + 4].copy_from_slice(&(FORMAT_VERSION + 1).to_le_bytes());
        fs::write(&file, &bytes).unwrap();

        let mut reloaded = ParseCache::load_from(dir.path(), dir.path());
        assert!(reloaded.get(&source).is_none());
        assert!(reloaded.is_empty(), "an unknown format is not adopted");
    }

    #[test]
    fn ignores_a_cache_from_another_project() {
        let dir = TempDir::new().unwrap();
        let source = write_source(dir.path(), "alpha.rs", "pub fn alpha() {}\n");

        let mut cache = ParseCache::load_from(dir.path(), dir.path());
        cache.record(&source, &sample_node(&source));
        let file = cache.path().to_path_buf();
        cache.save();

        // A hash collision would land the wrong project's entries here.
        let other = dir.path().join("other-project");
        fs::create_dir_all(&other).unwrap();
        assert!(decode(&fs::read(&file).unwrap(), &other).is_none());
    }

    #[test]
    fn ignores_a_damaged_cache() {
        let dir = TempDir::new().unwrap();
        let source = write_source(dir.path(), "alpha.rs", "pub fn alpha() {}\n");

        let damages: [fn(&mut Vec<u8>); 4] = [
            |bytes| bytes.truncate(bytes.len() / 2),
            |bytes| bytes[0] = b'X',
            |bytes| bytes.extend_from_slice(b"trailing garbage"),
            // A length prefix no sane file could back.
            |bytes| {
                bytes[MAGIC.len() + 4..MAGIC.len() + 8].copy_from_slice(&u32::MAX.to_le_bytes())
            },
        ];

        for damage in damages {
            let mut cache = ParseCache::load_from(dir.path(), dir.path());
            cache.record(&source, &sample_node(&source));
            let file = cache.path().to_path_buf();
            cache.save();

            let mut bytes = fs::read(&file).unwrap();
            damage(&mut bytes);
            fs::write(&file, &bytes).unwrap();

            let mut reloaded = ParseCache::load_from(dir.path(), dir.path());
            assert!(
                reloaded.get(&source).is_none(),
                "a damaged cache was trusted"
            );
        }
    }

    #[test]
    fn keeps_one_project_apart_from_another() {
        let dir = TempDir::new().unwrap();
        let first = dir.path().join("first");
        let second = dir.path().join("second");
        fs::create_dir_all(&first).unwrap();
        fs::create_dir_all(&second).unwrap();
        let source = write_source(&first, "alpha.rs", "pub fn alpha() {}\n");

        let mut cache = ParseCache::load_from(&first, dir.path());
        cache.record(&source, &sample_node(&source));
        cache.save();

        let other = ParseCache::load_from(&second, dir.path());
        assert!(
            other.is_empty(),
            "a second project read the first project's cache"
        );
        assert_ne!(cache.path(), other.path());
    }

    #[test]
    fn saves_nothing_when_nothing_changed() {
        let dir = TempDir::new().unwrap();
        let source = write_source(dir.path(), "alpha.rs", "pub fn alpha() {}\n");

        let mut cache = ParseCache::load_from(dir.path(), dir.path());
        assert!(!cache.is_dirty());
        assert!(cache.save().is_none(), "an empty cache writes nothing");

        cache.record(&source, &sample_node(&source));
        assert!(cache.save().is_some());
        let written_at = fs::metadata(cache.path()).unwrap().modified().unwrap();

        let mut reloaded = ParseCache::load_from(dir.path(), dir.path());
        assert!(reloaded.get(&source).is_some());
        assert!(!reloaded.is_dirty(), "a pure hit changes nothing");
        assert!(reloaded.save().is_none());
        assert_eq!(
            fs::metadata(reloaded.path()).unwrap().modified().unwrap(),
            written_at,
            "the cache file was left alone"
        );
    }

    #[test]
    fn drops_a_file_that_no_longer_exists() {
        let dir = TempDir::new().unwrap();
        let kept = write_source(dir.path(), "kept.rs", "pub fn kept() {}\n");
        let removed = write_source(dir.path(), "removed.rs", "pub fn removed() {}\n");

        let mut cache = ParseCache::load_from(dir.path(), dir.path());
        cache.record(&kept, &sample_node(&kept));
        cache.record(&removed, &sample_node(&removed));
        cache.save();

        fs::remove_file(&removed).unwrap();

        let mut reloaded = ParseCache::load_from(dir.path(), dir.path());
        assert!(reloaded.get(&kept).is_some());
        assert!(reloaded.get(&removed).is_none());
        reloaded.save();

        let mut after = ParseCache::load_from(dir.path(), dir.path());
        assert!(after.get(&kept).is_some());
        assert_eq!(after.len(), 1, "the deleted file lingers in the cache");
    }

    #[test]
    fn ignores_a_file_that_vanished_before_it_was_recorded() {
        let dir = TempDir::new().unwrap();
        let source = dir.path().join("alpha.rs");
        let mut cache = ParseCache::load_from(dir.path(), dir.path());

        cache.record(&source, &sample_node(&source));

        assert!(cache.is_empty());
        assert!(!cache.is_dirty());
    }

    #[test]
    fn writes_the_same_bytes_for_the_same_entries() {
        let dir = TempDir::new().unwrap();
        let source = write_source(dir.path(), "alpha.cpp", "int alpha() { return 0; }\n");
        let node = sample_node(&source);
        let mut cache = ParseCache::load_from(dir.path(), dir.path());
        cache.record(&source, &node);

        // Sets have no order of their own, so the encoding has to impose one.
        assert_eq!(
            encode(dir.path(), &cache.entries),
            encode(dir.path(), &cache.entries)
        );
    }

    #[test]
    fn leaves_no_temporary_file_behind() {
        let dir = TempDir::new().unwrap();
        let source = write_source(dir.path(), "alpha.rs", "pub fn alpha() {}\n");

        let mut cache = ParseCache::load_from(dir.path(), dir.path());
        cache.record(&source, &sample_node(&source));
        cache.save();
        cache.save();

        let leftovers: Vec<PathBuf> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| path.to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "left behind {leftovers:?}");
    }

    #[test]
    fn a_cache_file_for_another_directory_is_a_different_file() {
        let dir = TempDir::new().unwrap();
        let first = cache_file_for(dir.path(), Path::new("/projects/one"));
        let second = cache_file_for(dir.path(), Path::new("/projects/two"));

        assert_ne!(first, second);
        assert_eq!(
            first,
            cache_file_for(dir.path(), Path::new("/projects/one"))
        );
    }
}
