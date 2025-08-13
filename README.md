<h1 align="center">
  seiri (整理)
</h1>

<p align="center">
  <a href="https://deps.rs/repo/github/tarolling/seiri">
    <img alt="Dependency status" src="https://deps.rs/repo/github/tarolling/seiri/status.svg" referrerpolicy="noreferrer">
  </a>
  <a href="https://github.com/tarolling/seiri/releases/latest">
    <img src="https://img.shields.io/github/v/release/tarolling/seiri?logo=semantic-release" referrerpolicy="noreferrer">
  </a>
  <a href="https://github.com/tarolling/seiri/blob/main/LICENSE">
    <img alt="GitHub License" src="https://img.shields.io/github/license/tarolling/seiri" referrerpolicy="noreferrer">
  </a>
</p>
<p align="center">
  <a href="https://github.com/tarolling/seiri/actions/workflows/ci.yml">
    <img alt="CI" src="https://github.com/tarolling/seiri/actions/workflows/ci.yml/badge.svg" referrerpolicy="noreferrer">
  </a>
  <a href="https://github.com/tarolling/seiri/actions/workflows/build.yml">
    <img alt="Build" src="https://github.com/tarolling/seiri/actions/workflows/build.yml/badge.svg" referrerpolicy="noreferrer">
  </a>
</p>

<p align="center">
  A platform-agnostic project visualization tool.
</p>
<br>
<p align="center">
  <img alt="Sample output" src="./docs/example.png" style="width: 85%;">
</p>
<br>
<h2 align="center">
  Overview
</h2>

seiri breaks down project structures into a common format that can be used by developers and AI alike to better understand the design of large codebases.  seiri breaks down project structures into a common format that can be used by developers and AI alike to better understand the design of large codebases.

* Explore system structure and dependencies visually
* Extract modules, imports, functions, and containers
* Uses [tree-sitter](https://github.com/tree-sitter/tree-sitter) for fast, incremental parsing

<h2 align="center">
  Installation
</h2>

You can find our pre-built binaries under the Releases tab to download.

If you want to build from source, clone the repository and make sure to install the Rust toolchain. Then you can run the following commands:

```sh
cargo build --release
./target/release/seiri <options>
```

<h2 align="center">
  Usage
</h2>

Provide a path to the project you want to analyze, and optionally specify to produce a visualization and/or a JSON file containing all extracted information.

```sh
seiri <path> [gui|<export_path>] [-v|--verbose]
```

* `<path>` - File or directory to analyze
* `gui` - Launch visualization
* `<export_path>` - Export graph to specified path; currently support `SVG` and `PNG` file exports
* `-v`/`--verbose` - Show detailed logging about file detection and parsing
* `--no-gitignore` - Do not respect `.gitignore` file if present

<h2 align="center">
  Supported Languages
</h2>

The following programming languages are supported
* Rust
* Python
* TypeScript
* C/C++ (planned for v0.2.4)

<h2 align="center">
  Contributions
</h2>

Contributions are greatly appreciated! :)

Follow the guidelines laid out in [CONTRIBUTING.md](/.github/CONTRIBUTING.md).
<br>
<h2 align="center">
  License
</h2>

[MIT License](/LICENSE)
