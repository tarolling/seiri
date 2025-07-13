#!/usr/bin/env python3
# -*- coding: utf-8 -*-

import argparse
from pathlib import Path

from python import ast_parser, graph_builder


def find_python_files(path: str) -> list:
    """Recursively find all .py files in a directory."""
    return [str(p) for p in Path(path).rglob("*.py") if p.is_file()]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--path", help="Path to Python project")
    args = parser.parse_args()

    python_files = find_python_files(args.path)
    parse_results = {}
    for file in python_files:
        parse_results[file] = ast_parser.parse_python_file(file)

    graph_data = graph_builder.build_graph(parse_results)
    print(graph_data)  # TODO: convert to graph JSON


if __name__ == "__main__":
    main()
