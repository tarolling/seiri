#!/usr/bin/env python3
# -*- coding: utf-8 -*-

import re
from typing import Any

from .base import BaseParser


class JavascriptParser(BaseParser):
    """Simple regex-based parser for JavaScript files."""

    def get_file_extensions(self) -> list[str]:
        return ["js", "ts", "jsx", "tsx"]

    def parse_file(self, filepath: str) -> dict[str, Any]:
        """Parse JavaScript file using regex patterns."""
        try:
            with open(filepath, "r", encoding="utf-8") as fp:
                content = fp.read()
        except UnicodeDecodeError:
            return {
                "error": "Encoding error",
                "imports": [],
                "functions": [],
                "classes": [],
            }

        # Find imports
        import_patterns = [
            r'import\s+.*?\s+from\s+[\'"]([^\'"]+)[\'"]',
            r'import\s+[\'"]([^\'"]+)[\'"]',
            r'require\s*\(\s*[\'"]([^\'"]+)[\'"]\s*\)',
        ]

        imports = []
        for pattern in import_patterns:
            imports.extend(re.findall(pattern, content))

        # Find function definitions
        function_patterns = [
            r"function\s+(\w+)\s*\(",
            r"const\s+(\w+)\s*=\s*\(",
            r"(\w+)\s*:\s*function\s*\(",
            r"(\w+)\s*=>\s*",
        ]

        functions = []
        for pattern in function_patterns:
            functions.extend(re.findall(pattern, content))

        # Find class definitions
        class_pattern = r"class\s+(\w+)"
        classes = re.findall(class_pattern, content)

        return {
            "imports": imports,
            "functions": [{"name": f} for f in functions],
            "classes": [{"name": c} for c in classes],
            "calls": [],  # Would need more sophisticated parsing
        }
