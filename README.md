# seiri (整理)

[![dependency status](https://deps.rs/repo/github/tarolling/seiri/status.svg)](https://deps.rs/repo/github/tarolling/seiri)

A platform-agnostic project visualization tool.

![Sample output](docs/example.png)

## Overview

seiri breaks down project structures into a common format that can be used by developers and AI alike to better understand the design of large codebases. This tool can help all levels of developer, from showing new developers a clear look at code hierarchy, to aiding architects in a full system restructure. It uses tree-sitter to parse files and extract function and container dependency information.

## Usage

Provide a path to the project you want to analyze, and optionally specify to produce a visualization and/or a JSON file containing all extracted information.

```sh
seiri . gui
```

## Capabilities

Supported languages:

- Rust
- Python (in progress)
- C/C++ (in progress)
- More to come...

## Contributing

Because each file outputs data with a common JSON schema that is language-agnostic, plugins can easily be developed to visualize whatever kind of data is needed from a project. Any and all contributions are welcome!
