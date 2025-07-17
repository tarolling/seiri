#!/usr/bin/env python3
# -*- coding: utf-8 -*-

import os
from pathlib import Path

from src.cli import detect_project_languages
from src.parsers.registry import ParserRegistry


def test_simple_file_1():
    test_file = Path(os.path.join(os.curdir, "tests", "fixtures", "simple_file_1.py"))
    assert test_file.exists(), f"Test file does not exist: {test_file}"

    languages = detect_project_languages(str(test_file))
    assert languages == ["python"], f"Expected ['python'], got {languages}"

    registry = ParserRegistry()
    parser_class = registry.get_parser("python")
    assert parser_class is not None, "Parser should not be None"

    parser = parser_class()
    ast = parser.parse_file(str(test_file))
    assert ast is not None, "AST should not be None"

    assert "NumberProcessor" in ast["classes"], "AST should contain 'NumberProcessor'"
    assert "generate_random_list" in ast["functions"], (
        "AST should contain 'generate_random_list'"
    )
    assert "sum_even_numbers" in ast["functions"], (
        "AST should contain 'sum_even_numbers'"
    )
