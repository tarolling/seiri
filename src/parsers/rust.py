#!/usr/bin/env python3
# -*- coding: utf-8 -*-

import re
from typing import Any

from .base import BaseParser


class RustParser(BaseParser):
    """Simple regex-based parser for Rust files."""

    def get_file_extensions(self) -> list[str]:
        return ["rs"]

    def parse_file(self, filepath: str) -> dict[str, Any]:
        """Parse Rust file using regex patterns."""
        try:
            with open(filepath, "r", encoding="utf-8") as fp:
                content = fp.read()
        except UnicodeDecodeError:
            return {
                "error": "Encoding error",
                "imports": [],
                "functions": [],
                "structs": [],
            }

        # Find use statements (imports)
        use_pattern = r"use\s+([^;]+);"
        imports = re.findall(use_pattern, content)

        # Find function definitions
        fn_pattern = r"fn\s+(\w+)\s*\("
        functions = re.findall(fn_pattern, content)

        # Find struct definitions
        struct_pattern = r"struct\s+(\w+)"
        structs = re.findall(struct_pattern, content)

        # Find impl blocks
        impl_pattern = r"impl\s+(\w+)"
        impls = re.findall(impl_pattern, content)

        return {
            "imports": imports,
            "functions": [{"name": f} for f in functions],
            "structs": [{"name": s} for s in structs],
            "impls": [{"name": i} for i in impls],
            "calls": [],  # Would need more sophisticated parsing
        }
