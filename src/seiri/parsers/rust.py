#!/usr/bin/env python3
# -*- coding: utf-8 -*-

import re
from pathlib import Path

from seiri.parsers.base import BaseParser
from seiri.parsers.utils.datatypes import (
    ContainerNode,
    FunctionNode,
    ImportNode,
    ParsedFile,
)


class RustParser(BaseParser):
    """Simple regex-based parser for Rust files."""

    def get_file_extensions(self) -> list[str]:
        return ["rs"]

    def parse_file(self, filepath: str) -> ParsedFile:
        """Parse Rust file using regex patterns."""
        if not Path(filepath).exists():
            raise FileNotFoundError(f"File does not exist: {filepath}")

        try:
            with open(filepath, "r", encoding="utf-8") as fp:
                data = fp.read()
        except UnicodeDecodeError:
            return ParsedFile("error", Path(filepath))

        self._parsed_file = ParsedFile("rust", Path(filepath))

        self._extract_imports(data)
        self._extract_functions(data)
        # self._extract_function_refs(data)
        self._extract_containers(data)
        # self._extract_container_refs(data)

        return self._parsed_file

    def _extract_imports(self, data: str) -> None:
        use_pattern = r"use\s+([^;]+);"
        for stmt in re.findall(use_pattern, data):
            self._parsed_file.add_import(
                ImportNode(name=stmt, module=None, alias=None, level=0)
            )

    def _extract_functions(self, data: str) -> None:
        fn_pattern = r"fn\s+(\w+)\s*\("
        for stmt in re.findall(fn_pattern, data):
            self._parsed_file.add_function(
                FunctionNode(name=stmt, args=[], decorators=[], is_async=False)
            )

    def _extract_function_refs(self, data: str) -> None:
        raise NotImplementedError("_extract_function_refs")

    def _extract_containers(self, data: str) -> None:
        struct_pattern = r"struct\s+(\w+)"
        for stmt in re.findall(struct_pattern, data):
            self._parsed_file.add_container(
                ContainerNode(name=stmt, bases=[], container_vars=[])
            )

        impl_pattern = r"impl\s+(\w+)"
        for stmt in re.findall(impl_pattern, data):
            self._parsed_file.add_container(
                ContainerNode(name=stmt, bases=[], container_vars=[])
            )

    def _extract_container_refs(self, data: str) -> None:
        raise NotImplementedError("_extract_container_refs")
