//! Measures parallel file parsing for every supported language, so the speed of
//! the parse stage can be tracked on real code.
//!
//! ```sh
//! # Synthetic corpora, one per language (runs anywhere)
//! cargo bench
//!
//! # Real checkouts, one per language: the numbers that matter
//! cargo bench -- --rust ~/src/serde --python ~/src/django \
//!                --typescript ~/src/vscode --cpp ~/src/llvm-project
//!
//! # Tune the synthetic corpus / sample count
//! cargo bench -- --files 2000 --iters 9
//! ```
//!
//! Each row reports the files it found, the files it parsed, their total lines
//! of code, and the median wall-clock time of a [`parse_all_parallel`] pass. Any
//! file that could not be read or parsed is listed on stderr.
//!
//! With `--budget-ms-per-kloc`, a language whose median exceeds that many
//! milliseconds per 1000 lines of code prints a GitHub warning annotation; a
//! slow run is never reported as a failure.

use seiri_cli::core::defs::Language;
use seiri_cli::discovery::{detect_project_languages, walk_directory};
use seiri_cli::parsers::parse_all_parallel;
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

/// How many unparsed files each row lists before summarizing the rest.
const REPORTED_FAILURES: usize = 5;

struct Config {
    /// How many files to generate per language in the synthetic corpora.
    files: usize,
    /// Timed samples per language; the median is reported.
    iterations: usize,
    /// Real checkouts to measure instead of a synthetic corpus, per language.
    real_paths: HashMap<Language, PathBuf>,
    /// Per-language time budget in milliseconds per 1000 lines of code;
    /// exceeding it warns but never fails.
    budget_ms_per_kloc: Option<f64>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            files: 500,
            iterations: 5,
            real_paths: HashMap::new(),
            budget_ms_per_kloc: None,
        }
    }
}

/// The files a single language's benchmark run parses, plus a label describing their source.
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
        "parallel parsing ({} rayon threads)",
        rayon::current_num_threads()
    );
    println!(
        "each language timed {}x, median reported\n",
        config.iterations
    );
    println!(
        "{:<11} {:<44} {:>7} {:>7} {:>10} {:>12}",
        "language", "source", "files", "parsed", "sloc", "median"
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
        let outcome = parse_all_parallel(&corpus.files, || {});
        let median = median_time(config.iterations, || {
            parse_all_parallel(&corpus.files, || {})
        });
        let total_loc = outcome.nodes().values().map(|node| node.loc() as u64).sum();

        println!(
            "{:<11} {:<44} {:>7} {:>7} {:>10} {:>12}",
            language.to_string(),
            corpus.label,
            file_count,
            outcome.nodes().len(),
            total_loc,
            format_duration(median),
        );

        report_failures(language, outcome.failed(), file_count);
        warn_if_over_budget(
            language,
            &corpus.label,
            median,
            total_loc,
            config.budget_ms_per_kloc,
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
  --iters <n>          timed samples per language (default 5)
  --budget-ms-per-kloc <n>
                       warn above n milliseconds per 1000 lines of code
  -h, --help           print this help"
        .to_string()
}

fn parse_args() -> Result<Config, String> {
    let mut config = Config::default();
    let args: Vec<String> = std::env::args()
        .skip(1)
        .filter(|a| a != "--bench")
        .collect();

    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                println!("{}", usage());
                std::process::exit(0);
            }
            "--rust" | "--python" | "--typescript" | "--cpp" => {
                let value = iter.next().ok_or_else(|| format!("`{arg}` needs a path"))?;
                let language = match arg.as_str() {
                    "--rust" => Language::Rust,
                    "--python" => Language::Python,
                    "--typescript" => Language::TypeScript,
                    _ => Language::Cpp,
                };
                config.real_paths.insert(language, real_path(value)?);
            }
            "--files" => {
                let value = iter
                    .next()
                    .ok_or_else(|| "`--files` needs a value".to_string())?;
                config.files = positive(value, "--files")?;
            }
            "--iters" => {
                let value = iter
                    .next()
                    .ok_or_else(|| "`--iters` needs a value".to_string())?;
                config.iterations = positive(value, "--iters")?;
            }
            "--budget-ms-per-kloc" => {
                let value = iter
                    .next()
                    .ok_or_else(|| "`--budget-ms-per-kloc` needs a value".to_string())?;
                config.budget_ms_per_kloc = Some(positive_f64(value, "--budget-ms-per-kloc")?);
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

fn positive_f64(value: String, flag: &str) -> Result<f64, String> {
    match value.parse::<f64>() {
        Ok(n) if n.is_finite() && n > 0.0 => Ok(n),
        _ => Err(format!("`{flag}` needs a positive number, got `{value}`")),
    }
}

/// Builds the corpus to measure for `language`.
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

/// Lists the files that could not be parsed, so an incomplete parse is never
/// reported as a clean run.
fn report_failures(language: Language, failed: &[PathBuf], discovered: usize) {
    if failed.is_empty() {
        return;
    }

    eprintln!(
        "{:<11} {} of {discovered} file(s) could not be read or parsed:",
        language.to_string(),
        failed.len()
    );
    for path in failed.iter().take(REPORTED_FAILURES) {
        eprintln!("            {}", path.display());
    }
    if failed.len() > REPORTED_FAILURES {
        eprintln!(
            "            ... and {} more",
            failed.len() - REPORTED_FAILURES
        );
    }
}

/// Emits a GitHub warning annotation when a language parses slower than its
/// per-SLOC budget. Scaling by lines of code keeps the budget meaningful across
/// corpora that differ wildly in size. A budget is advisory: the run still exits
/// successfully.
fn warn_if_over_budget(
    language: Language,
    source: &str,
    median: Duration,
    total_loc: u64,
    budget_ms_per_kloc: Option<f64>,
) {
    let Some(budget) = budget_ms_per_kloc else {
        return;
    };
    if total_loc == 0 {
        return;
    }

    let ms_per_kloc = median.as_secs_f64() * 1_000.0 / (total_loc as f64 / 1_000.0);
    if ms_per_kloc > budget {
        println!(
            "::warning title=Parsing over budget::{} parsed {source} at {ms_per_kloc:.2} ms/kloc, over the {budget} ms/kloc budget ({total_loc} SLOC)",
            language.to_string(),
        );
    }
}

fn format_duration(duration: Duration) -> String {
    format!("{:.2} ms", duration.as_secs_f64() * 1_000.0)
}
