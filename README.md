# seiri (整理)

A project visualization tool.

![Sample output](docs/example.png)

## Overview

seiri breaks down project structures into a common format that can be used by developers and AI alike to better understand the design of large codebases. This tool can help all levels of developer, from showing new developers a clear look at code hierarchy, to aiding architects in a full system restructure. It uses simple static analysis to determine function and class dependencies between files.

## Capabilities

Supported languages:

- Python
- JavaScript (in progress)
- Rust (in progress)
- C/C++ (in progress)

## Contributing

Because each file outputs data with a common JSON schema that is language-agnostic, plugins can easily be developed to visualize whatever kind of data is needed from a project. Any and all contributions are welcome!
