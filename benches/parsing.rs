//! Compares sequential and parallel file parsing for every supported language,
//! so the impact of parallelising the parse stage can be measured on real code.
//!
//! ```sh
//! # Synthetic corpora, one per language (runs anywhere)
//! cargo bench
//!
//! # Real checkouts, one per language: the actual impact numbers
//! cargo bench -- --rust ~/src/serde --python ~/src/django \
//!                --typescript ~/src/vscode --cpp ~/src/llvm-project
//!
//! # Tune the synthetic corpus / sample count
//! cargo bench -- --files 2000 --iters 9
//! ```
//!
//! Both modes parse the same files twice — once through [`parse_all_sequential`]
//! and once through [`parse_all_parallel`] — and report the median wall-clock
//! time of each alongside the speedup. The two runs are also checked for
//! agreement, so a speedup is never reported for a parse that produced
//! different output.

use seiri_cli::core::defs::{FileNode, Language};
use seiri_cli::discovery::{detect_project_languages, walk_directory};
use seiri_cli::parsers::{parse_all_parallel, parse_all_sequential};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tempfile::TempDir;

/// The languages the benchmark sweeps, in the order they're reported.
const LANGUAGES: [Language; 4] = [
    Language::Rust,
    Language::Python,
    Language::TypeScript,
    Language::Cpp,
];

struct Config {
    /// How many files to generate per language in the synthetic corpora.
    files: usize,
    /// Timed samples per mode; the median is reported.
    iterations: usize,
    /// Real checkouts to measure instead of a synthetic corpus, per language.
    real_paths: HashMap<Language, PathBuf>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            files: 500,
            iterations: 5,
            real_paths: HashMap::new(),
        }
    }
}

/// The files a single language's benchmark run parses, plus a label describing
/// where they came from. `_temp_dir` keeps generated corpora alive on disk.
struct Corpus {
    label: String,
    files: HashMap<PathBuf, Language>,
    _temp_dir: Option<TempDir>,
}

fn main() {
    let config = match parse_args() {
        Ok(config) => config,
        Err(msg) => {
            eprintln!("error: {msg}\n\n{}", usage());
            std::process::exit(1);
        }
    };

    println!(
        "sequential vs parallel parsing ({} rayon threads)",
        rayon::current_num_threads()
    );
    println!("each mode timed {}x, median reported\n", config.iterations);
    println!(
        "{:<11} {:<44} {:>7} {:>12} {:>12} {:>9}",
        "language", "source", "files", "sequential", "parallel", "speedup"
    );

    for language in LANGUAGES {
        let corpus = match build_corpus(language, &config) {
            Ok(Some(corpus)) => corpus,
            Ok(None) => {
                println!(
                    "{:<11} {:<44}",
                    language.to_string(),
                    "no files found, skipping"
                );
                continue;
            }
            Err(msg) => {
                eprintln!("error: {msg}");
                std::process::exit(1);
            }
        };

        let file_count = corpus.files.len();
        let sequential = median_time(config.iterations, || parse_all_sequential(&corpus.files));
        let parallel = median_time(config.iterations, || {
            parse_all_parallel(&corpus.files, || {})
        });

        // Never report a speedup for a parse that changed the output.
        let (sequential_files, parallel_files) = (
            parse_all_sequential(&corpus.files),
            parse_all_parallel(&corpus.files, || {}),
        );
        if !same_files(&sequential_files, &parallel_files) {
            eprintln!(
                "error: parallel parsing disagreed with sequential parsing for {}",
                language.to_string()
            );
            std::process::exit(1);
        }

        println!(
            "{:<11} {:<44} {:>7} {:>12} {:>12} {:>8}",
            language.to_string(),
            corpus.label,
            file_count,
            format_duration(sequential),
            format_duration(parallel),
            format_speedup(sequential, parallel),
        );
    }
}

fn usage() -> String {
    "\
usage: cargo bench -- [options]

options:
  --rust <path>        measure a real Rust checkout instead of a synthetic corpus
  --python <path>      measure a real Python checkout
  --typescript <path>  measure a real TypeScript checkout
  --cpp <path>         measure a real C++ checkout
  --files <n>          files to generate per language (default 500)
  --iters <n>          timed samples per mode (default 5)
  -h, --help           print this help"
        .to_string()
}

fn parse_args() -> Result<Config, String> {
    let mut config = Config::default();
    let mut args = std::env::args().skip(1);

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                println!("{}", usage());
                std::process::exit(0);
            }
            "--rust" | "--python" | "--typescript" | "--cpp" => {
                let value = args.next().ok_or_else(|| format!("`{arg}` needs a path"))?;
                let language = match arg.as_str() {
                    "--rust" => Language::Rust,
                    "--python" => Language::Python,
                    "--typescript" => Language::TypeScript,
                    _ => Language::Cpp,
                };
                config.real_paths.insert(language, real_path(value)?);
            }
            "--files" => {
                let value = args
                    .next()
                    .ok_or_else(|| "`--files` needs a value".to_string())?;
                config.files = positive(value, "--files")?;
            }
            "--iters" => {
                let value = args
                    .next()
                    .ok_or_else(|| "`--iters` needs a value".to_string())?;
                config.iterations = positive(value, "--iters")?;
            }
            other => return Err(format!("unrecognized argument `{other}`")),
        }
    }

    Ok(config)
}

fn real_path(value: String) -> Result<PathBuf, String> {
    let path = PathBuf::from(&value);
    if !path.is_dir() {
        return Err(format!("`{value}` is not an existing directory"));
    }
    Ok(path)
}

fn positive(value: String, flag: &str) -> Result<usize, String> {
    match value.parse::<usize>() {
        Ok(n) if n > 0 => Ok(n),
        _ => Err(format!("`{flag}` needs a positive integer, got `{value}`")),
    }
}

/// Builds the corpus to measure for `language`: a real checkout when one was
/// supplied, otherwise a generated project. Returns `Ok(None)` when a real
/// checkout contains no files of that language.
fn build_corpus(language: Language, config: &Config) -> Result<Option<Corpus>, String> {
    let Some(root) = config.real_paths.get(&language) else {
        return Ok(Some(synthetic_corpus(language, config.files)));
    };

    let mut files = HashMap::new();
    let discovered = walk_directory(root, false);
    // `detect_project_languages` returns None when nothing is supported; this
    // benchmark is only ever run on supported projects.
    let _ = detect_project_languages(&discovered, &mut files);
    files.retain(|_, detected| *detected == language);

    if files.is_empty() {
        return Ok(None);
    }

    Ok(Some(Corpus {
        label: format!("{} (real checkout)", root.display()),
        files,
        _temp_dir: None,
    }))
}

/// Generates a project of `count` files for `language`, each importing a
/// sibling so the parsers do realistic import-extraction work.
fn synthetic_corpus(language: Language, count: usize) -> Corpus {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let (extension, source) = match language {
        Language::Rust => ("rs", rust_source as fn(usize, usize) -> String),
        Language::Python => ("py", python_source as fn(usize, usize) -> String),
        Language::TypeScript => ("ts", typescript_source as fn(usize, usize) -> String),
        Language::Cpp => ("cpp", cpp_source as fn(usize, usize) -> String),
    };

    let mut files = HashMap::with_capacity(count);
    for i in 0..count {
        let path = temp_dir.path().join(format!("module_{i}.{extension}"));
        std::fs::write(&path, source(i, count)).expect("failed to write synthetic file");
        files.insert(path, language);
    }

    Corpus {
        label: format!("synthetic ({count} files)"),
        files,
        _temp_dir: Some(temp_dir),
    }
}

fn rust_source(i: usize, count: usize) -> String {
    let next = (i + 1) % count;
    format!(
        "use crate::module_{next}::Thing{next};\n\
         use std::collections::HashMap;\n\n\
         pub struct Thing{i} {{\n    pub value: i32,\n}}\n\n\
         impl Thing{i} {{\n\
         \x20   /// Computes the current value.\n\
         \x20   pub fn compute(&self, other: &Thing{next}) -> i32 {{\n\
         \x20       let seen: HashMap<String, i32> = HashMap::new();\n\
         \x20       self.value + other.value + seen.len() as i32 + {i}\n\
         \x20   }}\n}}\n\n\
         pub fn build{i}() -> Thing{i} {{ Thing{i} {{ value: {i} }} }}\n"
    )
}

fn python_source(i: usize, count: usize) -> String {
    let next = (i + 1) % count;
    format!(
        "import os\nfrom module_{next} import helper_{next}\n\n\n\
         class Handler{i}:\n\
         \x20   \"\"\"Handles module {i}.\"\"\"\n\n\
         \x20   def __init__(self, value={i}):\n\
         \x20       self.value = value\n\n\
         \x20   def run(self):\n\
         \x20       return os.path.join(str(self.value), helper_{next}())\n\n\n\
         def helper_{i}():\n\
         \x20   return {i} + {next}\n"
    )
}

fn typescript_source(i: usize, count: usize) -> String {
    let next = (i + 1) % count;
    format!(
        "import {{ helper{next} }} from \"./module_{next}\";\n\
         import type {{ Options }} from \"./types\";\n\n\
         export interface Config{i} {{\n    value: number;\n}}\n\n\
         export class Service{i} {{\n\
         \x20   constructor(private readonly config: Config{i}) {{}}\n\n\
         \x20   handle(options: Options): number {{\n\
         \x20       return helper{next}() + this.config.value + {i};\n\
         \x20   }}\n}}\n\n\
         export function helper{i}(): number {{\n    return {i};\n}}\n"
    )
}

fn cpp_source(i: usize, count: usize) -> String {
    let next = (i + 1) % count;
    format!(
        "#include <vector>\n#include <string>\n#include \"module_{next}.h\"\n\n\
         namespace seiri {{\n\n\
         class Widget{i} {{\npublic:\n\
         \x20   explicit Widget{i}(int value) : value_(value) {{}}\n\n\
         \x20   int compute(const std::vector<int>& values) const {{\n\
         \x20       return value_ + static_cast<int>(values.size()) + {i};\n\
         \x20   }}\n\nprivate:\n    int value_;\n}};\n\n\
         int factory{i}() {{ return {i} + {next}; }}\n\n\
         }}  // namespace seiri\n"
    )
}

/// Runs `run` once to warm caches and the thread pool, then returns the median
/// of `iterations` timed samples.
fn median_time<T, F: FnMut() -> T>(iterations: usize, mut run: F) -> Duration {
    std::hint::black_box(run());

    let samples = iterations.max(1);
    let mut durations = Vec::with_capacity(samples);
    for _ in 0..samples {
        let start = Instant::now();
        std::hint::black_box(run());
        durations.push(start.elapsed());
    }

    durations.sort_unstable();
    durations[durations.len() / 2]
}

/// Checks both runs found the same files. Contents can't be compared directly
/// (`FileNode` isn't `PartialEq`), but a diverging parse shows up as a differing
/// file count long before it shows up in one of the fields.
fn same_files(
    sequential: &HashMap<PathBuf, FileNode>,
    parallel: &HashMap<PathBuf, FileNode>,
) -> bool {
    sequential.len() == parallel.len() && sequential.keys().all(|path| parallel.contains_key(path))
}

fn format_duration(duration: Duration) -> String {
    format!("{:.2} ms", duration.as_secs_f64() * 1_000.0)
}

fn format_speedup(sequential: Duration, parallel: Duration) -> String {
    let parallel_secs = parallel.as_secs_f64();
    if parallel_secs == 0.0 {
        return "n/a".to_string();
    }
    format!("{:.2}x", sequential.as_secs_f64() / parallel_secs)
}
