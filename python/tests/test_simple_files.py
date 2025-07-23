#!/usr/bin/env python3
# -*- coding: utf-8 -*-

import os
from pathlib import Path

from seiri.cli import detect_project_languages
from seiri.parsers.utils.datatypes import ContainerNode, FunctionNode
from seiri.parsers.utils.registry import ParserRegistry


def test_simple_file_1():
    test_file = Path(os.path.join(os.curdir, "tests", "fixtures", "simple_file_1.py"))
    assert test_file.exists(), f"Test file does not exist: {test_file}"

    languages = detect_project_languages(str(test_file))
    assert languages == ["python"], f"Expected ['python'], got {languages}"

    registry = ParserRegistry()
    parser_class = registry.get_parser("python")
    assert parser_class is not None, "Parser should not be None"

    parser = parser_class()
    try:
        parsed_file = parser.parse_file(str(test_file))
    except FileNotFoundError:
        assert False, "Test file should exist"

    assert (
        ContainerNode(name="NumberProcessor", bases=[], container_vars=[])
        in parsed_file.containers
    ), "Parse result should contain 'NumberProcessor'"
    assert (
        FunctionNode(
            name="generate_random_list", args=["size"], decorators=[], is_async=False
        )
        in parsed_file.functions
    ), "Parse result should contain 'generate_random_list'"
    assert (
        FunctionNode(
            name="sum_even_numbers", args=["numbers"], decorators=[], is_async=False
        )
        in parsed_file.functions
    ), "Parse result should contain 'sum_even_numbers'"
