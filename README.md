# seiri (整理)

[![dependency status](https://deps.rs/repo/github/tarolling/seiri/status.svg)](https://deps.rs/repo/github/tarolling/seiri)
[![CI](https://github.com/tarolling/seiri/actions/workflows/ci.yml/badge.svg)](https://github.com/tarolling/seiri/actions/workflows/ci.yml)

A platform-agnostic project visualization tool.

![Sample output](/docs/example.png)

## Overview

seiri breaks down project structures into a common format that can be used by developers and AI alike to better understand the design of large codebases.

* Explore system structure and dependencies visually
* Extract modules, imports, functions, and containers
* Uses [tree-sitter](https://github.com/tree-sitter/tree-sitter) for fast, incremental parsing

## Installation

You can find our pre-built binaries under the Releases tab to download.

If you want to build from source, clone the repository and make sure to install the Rust toolchain. Then you can run the following commands:

```sh
cargo build --release
./target/release/seiri <options>
```

## Usage

Provide a path to the project you want to analyze, and optionally specify to produce a visualization and/or a JSON file containing all extracted information.

```sh
seiri <path> [gui] [-v|--verbose]
```

* `<path>` - File or directory to analyze
* `gui` - (optional) Launch visualization
* `-v`/`--verbose` - Show detailed logging about file detection and parsing

## Supported Languages

* Rust
* Python
* C/C++ (planned for v0.2.2)

## Contributing

Follow the guidelines laid out in [CONTRIBUTING.md](/.github/CONTRIBUTING.md). Contributions are greatly appreciated! :)

## License

[MIT License](/LICENSE)
