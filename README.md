# seiri (整理)

A project visualization tool.

![Sample output](docs/example.png)

## Overview

seiri breaks down project structures into a common format that can be used by developers and AI alike to better understand the design of large codebases. This tool can help all levels of developer, from showing new developers a clear look at code hierarchy, to aiding architects in a full system restructure. It uses simple static analysis to determine function and class dependencies between files.

## Installation

### Linux/MacOS

```sh
bash ./scripts/setup.sh
```

## Usage

Provide a path to the project you want to analyze, and optionally specify to produce a visualization and/or a JSON file containing all extract information.

```sh
python src/seiri/cli.py --path ./tests/examples/project_a/ --visualize --output output.json
```

## Capabilities

Supported languages:

- Python
- JavaScript (in progress)
- Rust (in progress)
- C/C++ (in progress)

## Contributing

Because each file outputs data with a common JSON schema that is language-agnostic, plugins can easily be developed to visualize whatever kind of data is needed from a project. Any and all contributions are welcome!
