#!/usr/bin/env python3
# -*- coding: utf-8 -*-

import argparse
import json
from pathlib import Path

import pathspec

from seiri.core.graph_builder import GraphBuilder
from seiri.core.visualizer import GraphVisualizer
from seiri.parsers.utils.datatypes import ParsedFile
from seiri.parsers.utils.registry import ParserRegistry


def find_files_by_extensions(path: str, extensions: list[str]) -> list[str]:
    """Find all files with specified extensions in a directory."""
    patterns = [f"*.{ext}" for ext in extensions]
    files = []

    for pattern in patterns:
        files.extend(
            [
                str(p)
                for p in Path(path).rglob(pattern)
                if (
                    p.is_file()
                    and not any(
                        exclude in p.parts
                        for exclude in [
                            "__pycache__",
                            "site-packages",
                            "venv",
                            ".venv",
                            "env",
                            "node_modules",
                            ".git",
                            "target",
                            "build",
                        ]
                    )
                    and not p.name.startswith(".")
                )
            ]
        )

    return files


def detect_project_languages(path: str) -> list[str]:
    """Detect programming languages in the project."""

    language_indicators = {
        "python": ["*.py", "requirements.txt", "pyproject.toml", "setup.py"],
        "javascript": ["*.js", "package.json", "*.ts", "tsconfig.json"],
        "rust": ["*.rs", "Cargo.toml"],
        "go": ["*.go", "go.mod"],
        "java": ["*.java", "pom.xml", "build.gradle"],
        "cpp": ["*.cpp", "*.hpp", "*.c", "*.h", "CMakelists.txt"],
    }

    detected = []
    project_path = Path(path)

    # Load gitignore patterns if present
    gitignore_path = project_path / ".gitignore"
    if gitignore_path.exists():
        with open(gitignore_path) as f:
            spec = pathspec.PathSpec.from_lines("gitwildmatch", f)
    else:
        spec = None

    for lang, indicators in language_indicators.items():
        for indicator in indicators:
            if project_path.is_file():
                if project_path.match(indicator, case_sensitive=True):
                    print(f"Detected {lang} in {project_path}")
                    detected.append(lang)
                    break

            matches = list(
                project_path.rglob(
                    indicator, case_sensitive=True, recurse_symlinks=True
                )
            )
            if spec:
                matches = [
                    m
                    for m in matches
                    if not spec.match_file(str(m.relative_to(project_path)))
                ]

            if matches:
                print(f"Detected {lang} in {matches}")
                detected.append(lang)
                break

    return detected


def main():
    parser = argparse.ArgumentParser(
        description="Seiri - Language-agnostic project visualizer"
    )
    parser.add_argument("--path", required=True, help="Path to project")
    parser.add_argument(
        "--language", help="Specify language (auto-detect if not provided)"
    )
    parser.add_argument("--output", help="Output JSON file path")
    args = parser.parse_args()

    registry = ParserRegistry()

    # validate path as directory
    project_path = Path(args.path).resolve()

    # Detect or use specified language
    if args.language:
        languages = [args.language]
    else:
        languages = detect_project_languages(str(project_path))
        if not languages:
            print("No supported languages detected in project")
            return

    print(f"Detected languages: {', '.join(languages)}")

    # Parse files for each language
    all_parse_results: list[ParsedFile] = []
    for language in languages:
        parser_class = registry.get_parser(language)
        if not parser_class:
            print(f"No parser available for {language}")
            continue

        parser = parser_class()
        extensions = parser.get_file_extensions()
        files = find_files_by_extensions(args.path, extensions)

        print(f"Found {len(files)} {language} files")

        for file in files:
            try:
                result = parser.parse_file(file)
                all_parse_results.append(result)
            except Exception as e:
                print(f"Error parsing {file}: {e}")

    # Build graph
    graph_builder = GraphBuilder()
    graph_data = graph_builder.build_graph(all_parse_results)

    # Output JSON
    if args.output:
        with open(args.output, "w") as f:
            json.dump(graph_data, f, indent=2)
        print(f"Graph data saved to {args.output}")

    visualizer = GraphVisualizer()
    visualizer.visualize(graph_data)


if __name__ == "__main__":
    main()
