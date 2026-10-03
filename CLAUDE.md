# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working
with code in this repository.

## Project overview

seiri (整理) is a platform-agnostic project visualization tool written in
Rust. It parses a codebase in one or more supported languages. It then
resolves the import/reference graph between files. Finally, it renders that
graph either interactively, in a graphical user interface (GUI), or as a
static SVG/PNG export.

Supported languages: Rust, Python, TypeScript, C++.

## Glossary

- AST: abstract syntax tree, the parse of one source file.
- CI: continuous integration.
- CLI: command-line interface.
- GL: the OpenGL graphics library the GUI renders with.
- LOC: lines of code.
- PR: pull request.
- SVG, PNG: the image formats used for static exports.
- TDD: test-driven development.
- X11: the Linux windowing system the GUI opens windows on.

## Engineering practices

This is production software, not a prototype. Practice TDD whenever a change
has observable behavior. Write a failing test that captures the expected
behavior before writing the implementation, then implement to make it pass.
For a bug fix, first reproduce the failure as a test. A small mechanical
change may land without a new test, when the commit message says why.

Hold these four lines in review:

- Keep each pipeline stage in its own module: parser, resolver, analysis,
  layout, or render. Do not move logic across those boundaries.
- Cover each new branch of logic with at least one test.
- Keep a commit to one change.
- Fix `cargo fmt` and `cargo clippy -- -D warnings` failures before pushing.
  If a failure is worth accepting, say so in the PR description.

### Doc comment guidelines

- Prefer American English spelling, such as "color" not "colour".
- Prefer commas or parentheses over em-dashes.
- Describe what an item does, not why or how. Put implementation details in
  inline comments instead.
- Keep file paths, examples, and algorithm explanations out of doc comments.
  They belong in `CONTRIBUTING.md` or in this file.
- Name a test for what it checks. A prefix such as "Test T0XX:" or
  "Regression test for issue #XXX:" adds nothing.

## Common commands

```sh
# Build
cargo build
cargo build --release

# Run
cargo run -- <path> [gui|<export_path>] [-v|--verbose] [--no-gitignore]

# Lint / format (CI enforces both, with -D warnings on clippy)
cargo fmt --all -- --check
cargo clippy -- -D warnings

# Type/compile check only
cargo check --all-features

# Tests
cargo test
cargo test <test_name>        # one test by name (substring match)
cargo test --package seiri-cli \
    test_cpp_layout_sugiyama_and_circular   # one exact test

# Benchmarks: parallel parsing throughput, one row per supported language
cargo bench
cargo bench -- --rust ~/src/serde --python ~/src/django \
    --typescript ~/src/vscode --cpp ~/src/llvm-project

# Coverage (matches CI; excludes src/main.rs and benches, 70% threshold)
cargo install cargo-tarpaulin
cargo tarpaulin --verbose --all-features --workspace --timeout 120 \
    --exclude-files src/main.rs --exclude-files 'benches/*' \
    --fail-under 70
```

On Linux, building the GUI (`eframe`/`egui`) requires X11 and GL dev
packages: `pkg-config libx11-dev libxcursor-dev libxrandr-dev
libxinerama-dev libxi-dev libgl1-mesa-dev libfontconfig-dev`.

There is no separate Docker-only workflow required.
`.github/DEVELOPMENT.md` documents an optional `docker-compose up -d` dev
container if system deps are inconvenient to install locally.

## Architecture

The pipeline, end to end (see `docs/interfaces.md` for the canonical
diagram):

```text
File --> Parser --> Resolver --> Graph Nodes + Edges --> GUI / PNG / SVG
```

Everything except the CLI entry point lives in the `seiri-cli` library
(`src/lib.rs`). The binary and `benches/` therefore share one
implementation, and `src/main.rs` is a thin CLI over it.

1. **Discovery** (`src/discovery.rs`). `walk_directory` uses the `ignore`
   crate to walk the project. It respects `.gitignore` unless
   `--no-gitignore`. It prunes vendored dependency directories
   (`node_modules`, `3rdparty`, `vendor`, ...). Pass `--include-vendored` to
   keep them. `Language::from_file` (in `src/core/defs.rs`) then buckets the
   files by extension. Both knobs live on `WalkOptions`.

2. **Parsing** (`src/parsers/{rust,python,typescript,cpp}.rs`). Each
   language has its own tree-sitter grammar and a `parse_<lang>_file`
   function. That function walks the abstract syntax tree (AST) and produces
   a `FileNode` (`src/core/defs.rs`).

   A `FileNode` holds:

   - File path and lines of code (LOC).
   - Imports, each marked local or external.
   - Defined functions.
   - Defined containers (classes, structs, and so on).
   - External references.

   `src/parsers.rs` drives them. `parse_file` dispatches by language, and
   `parse_all_parallel` runs every file across rayon's thread pool. The CLI
   uses it, and `benches/parsing.rs` measures it, so it keeps doing the real
   parse. A faster path belongs in a wrapper such as
   `parse_all_parallel_cached`, not inside this function.

   The wrapper reuses a previous run's `FileNode` for any file whose size
   and mtime are unchanged. It keeps one `ParseCache` per project
   (`src/parsers/cache.rs`). Entries are stamped with the crate version and
   written atomically, so a cache from another version or a damaged one is
   discarded rather than trusted. Without a cache directory the current user
   owns, it parses everything instead. Pass `--no-cache` to bypass it.

   The wrapper returns a `ParseOutcome`. It pairs the parsed files with the
   paths that could not be read or parsed, so callers can report them.

   Parsers walk the AST with one `TreeCursor` per file
   (`advance`/`skip_children`/`descend_into` in `src/parsers.rs`). They
   dispatch on resolved numeric node kind ids instead of kind names. Every
   thread keeps one tree-sitter `Parser` per language (`with_parser`), since
   a parser carries a parse stack and subtree pool worth reusing across a
   project's files. Names reached by the walk go into their sets through
   `insert_text`, which only copies text the set does not already hold.

   Parsers are otherwise independent of each other. A new language needs a
   module here, a matching resolver (see below), and a `Language` variant.

3. **Resolution** (`src/core/resolvers.rs` and
   `src/core/resolvers/{rust,python,typescript,cpp}.rs`). Each language
   implements the `LanguageResolver` trait (`build_module_map`,
   `resolve_import`, `resolve_external_references`). That turns raw import
   strings into actual file paths within the project, for example Rust's
   `crate::foo::bar` to `src/foo/bar.rs`.

   `GraphBuilder` (in `resolvers.rs`) owns one resolver per `Language`. It
   builds each resolver's module map first, then walks every `FileNode`'s
   imports and external references to produce `GraphNode`s (`FileNode` plus
   resolved edges as `Vec<PathBuf>`). Local imports become edges. External
   and library imports are skipped for now.

4. **Analysis** (`src/analysis.rs`). `GraphAnalysis` computes graph-theoretic
   metrics on a `petgraph::Graph<(), ()>` built from the resolved nodes and
   edges: strongly connected components and Brandes' betweenness
   centrality. The result feeds node sizing, where a node on more shortest
   paths renders larger, in both the GUI and exports.

   `src/analysis/community.rs` adds Louvain community detection and
   Newman-Girvan modularity over the undirected view of the graph. Import
   A->B couples A and B. Reciprocal or parallel imports sum, and
   self-imports are ignored. Louvain visits nodes in index order, so its
   partitions are deterministic.

   The GUI reports modularity for the Louvain partition and for the "one
   community per parent directory" partition (`partition_by_parent_dir`,
   where a Rust-style `foo.rs` joins its sibling `foo/` directory). The
   second one compares the declared folder layout against the real coupling.

5. **Layout** (`src/layout.rs` and `src/layout/{circular,sugiyama}.rs`). The
   `Layout` trait maps a `petgraph` graph to 2D node positions. Two
   implementations exist: `CircularLayout` (the default) and
   `SugiyamaLayout` (layered/hierarchical), selected via `LayoutType`.

6. **Output**. One of two renderers:

   - `src/gui.rs` (plus `src/gui/camera.rs`): an interactive
     `eframe`/`egui` app, `SeiriGraph`. It has a pan/zoom camera, node
     selection and hover, and toggling between layouts.
   - `src/export.rs`: renders the same graph data to a static file. SVG goes
     through the `svg` crate. PNG goes through `tiny-skia`, with
     `fontdue`/`font-kit` for text. Neither needs a windowing system.

`main.rs`'s `run()` wires all of the above together based on CLI args (via
`clap`). With no output arg it opens the GUI. `gui` explicitly opens it. A
`.svg` or `.png` filename triggers the corresponding export instead.

### Key data types (`src/core/defs.rs`)

- `Language`: enum with per-language extensions, display name, and color
  (used consistently across GUI/SVG/PNG rendering).
- `Import`: an import path plus whether it is local to the project.
- `FileNode`: everything extracted from parsing one file.
- `GraphNode`: a `FileNode` plus resolved edges. It also owns
  `calculate_size` (LOC + betweenness centrality to render radius).

### Adding a new supported language

A new language needs changes in all of these places:

- `Language` enum plus `extensions()`/`from_file`/`color()` in
  `core/defs.rs`.
- A new `parsers/<lang>.rs` (tree-sitter grammar + `FileNode` extraction).
- A new `core/resolvers/<lang>.rs` (`LanguageResolver` impl).
- Registration in `GraphBuilder::new()`.
- The match arm in `main.rs`'s parse loop.

## CI expectations

`.github/workflows/ci.yml` runs on every PR to `main`. It runs
`cargo fmt --check`, then `cargo clippy -- -D warnings`, then
`cargo check --all-features`. Finally it runs `cargo tarpaulin` with a 70%
coverage threshold, excluding `src/main.rs` and `benches/`, and uploads the
report to Codacy. Run the same four commands locally before pushing, since
CI repeats them.

## Releasing

Release process (`.github/DEVELOPMENT.md`): bump `version` in `Cargo.toml`,
commit as "Bump version to X.Y.Z", tag `vX.Y.Z`, push the tag. Pushing a
`v*` tag triggers the release GitHub Actions workflow automatically. That
workflow builds Linux/macOS/Windows binaries, publishes them, and publishes
the crate. To rehearse the workflow, push it manually with `dry-run` as the
tag, which publishes nothing.

## Terminology (from CONTRIBUTING.md)

- **Defect**: incorrect code.
- **Infection**: incorrect program state caused by a defect.
- **Failure**: the observable incorrect behavior, also called an
  "issue" or "problem".

When bug-hunting, contributors are asked to follow TRAFFIC: Track,
Reproduce, Automate, Find origins, Focus, Isolate, Correct.
