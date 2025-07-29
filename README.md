# seiri (整理)

[![dependency status](https://deps.rs/repo/github/tarolling/seiri/status.svg)](https://deps.rs/repo/github/tarolling/seiri)

A platform-agnostic project visualization tool.

![Sample output](docs/example.png)

---

## Overview

**seiri** breaks down project structures into a common format that can be used by developers and AI alike to better understand the design of large codebases.

- Explore system structure and dependencies visually
- Extract modules, imports, functions, and containers
- Uses [tree-sitter](https://tree-sitter.github.io/tree-sitter/) for fast, incremental parsing

---

## Usage

Basic command:

```bash
seiri <path> [output|gui] [--verbose]
```
* `<path>`: File or directory to analyze
* `output`: (optional) Name of output file, or use `gui` to launch visualization
* `--verbose`: (optional) Show detailed logging about detection and parsing

---
#### Examples
Visualize a Rust project:

```bash
seiri . gui
```
#### Analyze a file and save output:

```bash
seiri src/main.rs output.json
```

#### Verbose mode (shows selected options, detected languages, etc.):

```bash
seiri . --verbose
```
---
## Capabilities
### Supported Languages
* Rust
* Python (in progress)
* C/C++ (planned)

If no supported language is found, seiri will exit with:
```pgsql
⚠️ No supported languages were detected in the provided path.
```
---
## GUI Support
If GUI libraries are available (e.g., `eframe`, `egui`), the program will print:

```yaml
GUI support: Yes
```
> Otherwise, you can still use CLI mode.
--- 
## CLI Output Details
With --verbose, seiri shows more helpful context:

```yaml
Input Path: "./myproject"
Output Mode: gui
GUI support: Yes
Detected languages: {Rust}
Files detected: 12
Resolved 7 nodes with connections:
  main.rs (Rust) -> 2 dependencies
    -> core.rs
    -> utils.rs
```
---
## Contributing
Because each file outputs data in a **language-agnostic JSON schema**, plugins and visualizers can be built independently of the parser logic. Contributions of any kind are welcome!

* Add new language parsers under `parsers/`
* Improve visual UI in `gui.rs`
* Extend test coverage in `main.rs`
---
##  Installation
To install or build from source:

```bash
cargo build --release
./target/release/seiri <path> gui
```
---
## License  

#### MIT License [**© tarolling**](https://github.com/tarolling)
---
**HAPPY CODING**