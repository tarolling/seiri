#!/usr/bin/env python3
# -*- coding: utf-8 -*-

import os
from pathlib import Path

from seiri.cli import detect_project_languages
from seiri.parsers.utils.datatypes import (
    FunctionNode,
    FunctionRefNode,
    ImportNode,
    ParsedFile,
)
from seiri.parsers.utils.registry import ParserRegistry


def test_simple_project_a():
    test_dir = Path(os.path.join(os.curdir, "tests", "fixtures", "project_a"))
    assert test_dir.exists(), f"Test directory does not exist: {test_dir}"

    languages = detect_project_languages(str(test_dir))
    assert languages == ["python"], f"Expected ['python'], got {languages}"

    registry = ParserRegistry()
    parser_class = registry.get_parser("python")
    assert parser_class is not None, "Parser should not be None"

    all_parse_results: list[ParsedFile] = []

    parser = parser_class()
    for file in test_dir.iterdir():
        if file.is_dir():
            continue

        result = parser.parse_file(str(file))
        assert result is not None, "Parsed file should not be None"
        all_parse_results.append(result)
        assert result.language == "python", (
            "Parsed file should have a language of 'python'"
        )

        if file.name == "main.py":
            assert (
                ImportNode(name="foo", module="utils.helpers", alias=None, level=0)
                in result.imports
            )
            assert FunctionRefNode(name="foo", object=None) in result.function_refs
        else:
            assert (
                FunctionNode(name="foo", args=[], decorators=[], is_async=False)
                in result.functions
            )
            assert FunctionRefNode(name="print", object=None) in result.function_refs
