#!/usr/bin/env python3
# -*- coding: utf-8 -*-

import os

from src.cli import detect_project_languages
from src.parsers.registry import ParserRegistry


def test_simple_file_1():
    test_file = os.path.join(os.pardir, "examples", "simple_file_1.py")

    languages = detect_project_languages(test_file)
    assert languages == ["python"], f"Expected ['python'], got {languages}"

    registry = ParserRegistry()
    parser = registry.get_parser_for_file(test_file)

    assert parser is not None, "Parser should not be None"
    assert parser.language == "python", f"Expected 'python', got {parser.language}"

    ast = parser.parse_file(test_file)
    assert ast is not None, "AST should not be None"

    ast_json = ast.to_json()
    assert "NumberProcessor" in ast_json, "AST JSON should contain 'NumberProcessor'"
    assert "generate_random_list" in ast_json, (
        "AST JSON should contain 'generate_random_list'"
    )
    assert "sum_even_numbers" in ast_json, "AST JSON should contain 'sum_even_numbers'"
