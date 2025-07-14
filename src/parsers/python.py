#!/usr/bin/env python3
# -*- coding: utf-8 -*-

import ast
from typing import Any

from .base import BaseParser


class PythonParser(BaseParser):
    """Parser for Python files."""

    def get_file_extensions(self) -> list[str]:
        return ["py"]

    def parse_file(self, filepath: str) -> dict[str, Any]:
        """Parse Python file using AST."""
        try:
            with open(filepath, "r", encoding="utf-8") as fp:
                tree = ast.parse(fp.read())
        except (SyntaxError, UnicodeDecodeError) as e:
            return {
                "error": str(e),
                "imports": [],
                "calls": [],
                "functions": [],
                "classes": [],
            }

        imports = []
        calls = []
        functions = []
        classes = []

        for node in ast.walk(tree):
            if isinstance(node, ast.Import):
                imports.extend(alias.name for alias in node.names)
            elif isinstance(node, ast.ImportFrom):
                if node.module:
                    imports.append(node.module)
            elif isinstance(node, ast.FunctionDef):
                functions.append(
                    {
                        "name": node.name,
                        "line": node.lineno,
                        "args": [arg.arg for arg in node.args.args],
                    }
                )
            elif isinstance(node, ast.ClassDef):
                classes.append(
                    {
                        "name": node.name,
                        "line": node.lineno,
                        "bases": [self._ast_to_string(base) for base in node.bases],
                    }
                )
            elif isinstance(node, ast.Call):
                call_name = self._ast_to_string(node.func)
                if call_name:
                    calls.append(call_name)

        return {
            "imports": imports,
            "calls": calls,
            "functions": functions,
            "classes": classes,
        }

    def _ast_to_string(self, node) -> str:
        """Convert AST node to string representation."""
        if isinstance(node, ast.Name):
            return node.id
        elif isinstance(node, ast.Attribute):
            return f"{self._ast_to_string(node.value)}.{node.attr}"
        elif isinstance(node, ast.Constant):
            return str(node.value)
        else:
            return ""
