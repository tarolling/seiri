# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project overview

seiri (整理) is a platform-agnostic project visualization tool written in Rust. It parses a codebase in one or more supported languages, resolves the import/reference graph between files, and renders it either interactively (GUI) or as a static SVG/PNG export.

Supported languages: Rust, Python, TypeScript, C++.

## Engineering practices

This is production software, not a prototype. Practice TDD whenever possible: write a failing test that captures the expected behavior before writing the implementation, then implement to make it pass. Apply this to bug fixes too — reproduce with a failing test first, then fix. Beyond TDD, hold the line on good software engineering fundamentals: clear separation of concerns (don't blur the parser/resolver/analysis/layout/render boundaries described below), meaningful test coverage for new logic, small focused commits, and treating `cargo fmt`/`cargo clippy -- -D warnings` failures as blocking, not advisory.

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
cargo test <test_name>          # run a single test by name (substring match)
cargo test --package seiri-cli test_cpp_layout_sugiyama_and_circular  # run one exact test

# Coverage (matches CI; excludes src/main.rs, 40% threshold)
cargo install cargo-tarpaulin
cargo tarpaulin --verbose --all-features --workspace --timeout 120 --exclude-files src/main.rs
```

On Linux, building the GUI (`eframe`/`egui`) requires X11/GL dev packages: `pkg-config libx11-dev libxcursor-dev libxrandr-dev libxinerama-dev libxi-dev libgl1-mesa-dev libfontconfig-dev`.

There is no separate Docker-only workflow required, but `.github/DEVELOPMENT.md` documents an optional `docker-compose up -d` dev container if system deps are inconvenient to install locally.

## Architecture

The pipeline, end to end (see `docs/interfaces.md` for the canonical diagram):

```
File --> Parser --> Resolver --> Graph Nodes + Edges --> GUI / PNG / SVG
```

1. **Discovery** (`src/main.rs`): `walk_directory` uses the `ignore` crate to walk the project (respecting `.gitignore` unless `--no-gitignore`), then `Language::from_file` (in `src/core/defs.rs`) buckets files by extension.

2. **Parsing** (`src/parsers/{rust,python,typescript,cpp}.rs`): each language has its own tree-sitter grammar and a `parse_<lang>_file` function that walks the AST and produces a `FileNode` (`src/core/defs.rs`) — file path, LOC, imports (with local/external classification), defined functions, defined containers (classes/structs/etc.), and external references. Parsers are otherwise independent of each other; add a new language by adding a new module here plus a matching resolver (see below) and a `Language` variant.

3. **Resolution** (`src/core/resolvers.rs` + `src/core/resolvers/{rust,python,typescript,cpp}.rs`): each language implements the `LanguageResolver` trait (`build_module_map`, `resolve_import`, `resolve_external_references`) to turn raw import strings into actual file paths within the project (e.g. Rust's `crate::foo::bar` -> `src/foo/bar.rs`). `GraphBuilder` (in `resolvers.rs`) owns one resolver per `Language`, builds each resolver's module map first, then walks every `FileNode`'s imports/external references to produce `GraphNode`s (`FileNode` + resolved edges as `Vec<PathBuf>`). Only local imports become edges; external/library imports are currently skipped.

4. **Analysis** (`src/analysis.rs`): `GraphAnalysis` computes graph-theoretic metrics on a `petgraph::Graph<(), ()>` built from the resolved nodes/edges — strongly connected components and Brandes' betweenness centrality. This feeds node sizing (larger nodes sit on more shortest paths) in both the GUI and exports.

5. **Layout** (`src/layout.rs` + `src/layout/{circular,sugiyama}.rs`): the `Layout` trait maps a `petgraph` graph to 2D node positions. Two implementations exist — `CircularLayout` (default) and `SugiyamaLayout` (layered/hierarchical) — selected via `LayoutType`.

6. **Output**: either
   - `src/gui.rs` (+ `src/gui/camera.rs`) — an interactive `eframe`/`egui` app (`SeiriGraph`) with pan/zoom camera, node selection/hover, and toggling between layouts, or
   - `src/export.rs` — renders the same graph data to a static SVG (via the `svg` crate) or PNG (via `tiny-skia`, with `fontdue`/`font-kit` for text rendering) without needing a windowing system.

`main.rs`'s `run()` wires all of the above together based on CLI args (via `clap`): default with no output arg opens the GUI; `gui` explicitly opens it; a `.svg`/`.png` filename triggers the corresponding export instead.

### Key data types (`src/core/defs.rs`)

- `Language` — enum with per-language extensions, display name, and color (used consistently across GUI/SVG/PNG rendering).
- `Import` — an import path plus whether it's local to the project.
- `FileNode` — everything extracted from parsing one file.
- `GraphNode` — a `FileNode` plus resolved edges; also owns `calculate_size` (LOC + betweenness centrality -> render radius).

### Adding a new supported language

Touch all of: `Language` enum + `extensions()`/`from_file`/`color()` in `core/defs.rs`, a new `parsers/<lang>.rs` (tree-sitter grammar + `FileNode` extraction), a new `core/resolvers/<lang>.rs` (`LanguageResolver` impl), registration in `GraphBuilder::new()`, and the match arm in `main.rs`'s parse loop.

## CI expectations

`.github/workflows/ci.yml` runs on every PR to `main`: `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo check --all-features`, then `cargo tarpaulin` with a 40% coverage threshold (excluding `src/main.rs`). Match these locally before pushing.

## Releasing

Release process (`.github/DEVELOPMENT.md`): bump `version` in `Cargo.toml`, commit as "Bump version to X.Y.Z", tag `vX.Y.Z`, push the tag, then run the release GitHub Actions workflow with that tag (builds Linux/macOS/Windows binaries and publishes them). Pass `dry-run` as the tag to test the workflow without publishing.

## Terminology (from CONTRIBUTING.md)

- **Defect** — incorrect code. **Infection** — incorrect program state caused by a defect. **Failure** — the observable incorrect behavior (also called an "issue"/"problem"). When bug-hunting, contributors are asked to follow TRAFFIC: Track, Reproduce, Automate, Find origins, Focus, Isolate, Correct.
