#!/usr/bin/env python3
# -*- coding: utf-8 -*-

import re
from pathlib import Path

from seiri.parsers.base import BaseParser
from seiri.parsers.utils.datatypes import ParsedFile


class JavaScriptParser(BaseParser):
    """Simple regex-based parser for JavaScript files."""

    def get_file_extensions(self) -> list[str]:
        return ["js", "ts", "jsx", "tsx"]

    def parse_file(self, filepath: str) -> ParsedFile:
        """Parse JavaScript file using regex patterns."""
        if not Path(filepath).exists():
            raise FileNotFoundError(f"File does not exist: {filepath}")

        try:
            with open(filepath, "r", encoding="utf-8") as fp:
                content = fp.read()
        except UnicodeDecodeError:
            return ParsedFile("error", Path(filepath))

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
