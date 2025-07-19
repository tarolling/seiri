#!/usr/bin/env python3
# -*- coding: utf-8 -*-

import ast
from pathlib import Path

from seiri.parsers.base import BaseParser
from seiri.parsers.utils.datatypes import (
    ContainerNode,
    ContainerRefNode,
    FunctionNode,
    FunctionRefNode,
    ImportNode,
    ParsedFile,
)


class PythonParser(BaseParser):
    """Parser for Python files."""

    def get_file_extensions(self) -> list[str]:
        return ["py"]

    def parse_file(self, filepath: str) -> ParsedFile:
        """Analyze a Python file and extract all requested information."""
        if not Path(filepath).exists():
            raise FileNotFoundError(f"File does not exist: {filepath}")

        try:
            with open(filepath, "r", encoding="utf-8") as f:
                source = f.read()
        except UnicodeDecodeError:
            return ParsedFile("error", Path(filepath))

        self._parsed_file = ParsedFile("python", Path(filepath))

        # Parse AST for structural analysis
        tree = ast.parse(source, filepath)

        self._extract_imports(tree)
        self._extract_functions(tree)
        self._extract_function_refs(tree)
        self._extract_containers(tree)
        self._extract_container_refs(tree)

        return self._parsed_file

    def _extract_imports(self, tree: ast.AST) -> None:
        """Extract import statements."""
        for node in ast.walk(tree):
            if isinstance(node, ast.Import):
                for alias in node.names:
                    self._parsed_file.add_import(
                        ImportNode(
                            module=None,
                            name=alias.name,
                            alias=alias.asname,
                            level=0,
                        )
                    )
            elif isinstance(node, ast.ImportFrom):
                for alias in node.names:
                    self._parsed_file.add_import(
                        ImportNode(
                            module=node.module,
                            name=alias.name,
                            alias=alias.asname,
                            level=node.level,
                        )
                    )

    def _extract_functions(self, tree: ast.AST):
        """Extract function definitions and their details."""
        for node in ast.walk(tree):
            if not isinstance(node, ast.FunctionDef):
                continue

            # Arguments
            args: list[str] = []
            for arg in node.args.args:
                args.append(arg.arg)

            # Decorators
            decorators: list[str] = []
            for decorator in node.decorator_list:
                if isinstance(decorator, ast.Name):
                    decorators.append(decorator.id)
                elif isinstance(decorator, ast.Attribute):
                    decorators.append(ast.unparse(decorator))

            self._parsed_file.add_function(
                FunctionNode(
                    name=node.name,
                    args=args,
                    decorators=decorators,
                    is_async=isinstance(node, ast.AsyncFunctionDef),
                )
            )

    def _extract_function_refs(self, tree: ast.AST):
        """Extract function and class calls."""
        for node in ast.walk(tree):
            if not isinstance(node, ast.Call):
                continue

            if isinstance(node.func, ast.Name):
                # Simple function call: func()
                call_name = node.func.id

                # Heuristic: if name starts with capital letter, likely a class
                if call_name[0].isupper():
                    self._parsed_file.add_container_ref(
                        ContainerRefNode(name=call_name, object=None, method=None)
                    )
                else:
                    self._parsed_file.add_function_ref(
                        FunctionRefNode(name=call_name, object=None)
                    )
            elif isinstance(node.func, ast.Attribute):
                # Method call: obj.method() or module.func()
                call_name = node.func.attr
                self._parsed_file.add_function_ref(
                    FunctionRefNode(name=call_name, object=ast.unparse(node.func.value))
                )

    def _extract_containers(self, tree: ast.AST):
        """Extract class definitions with methods and instance variables."""
        for node in ast.walk(tree):
            if not isinstance(node, ast.ClassDef):
                continue

            # Get base classes
            bases: list[str] = []
            for base in node.bases:
                if isinstance(base, ast.Name):
                    bases.append(base.id)
                elif isinstance(base, ast.Attribute):
                    bases.append(ast.unparse(base))

            # Get class-level variables
            container_vars: list[str] = []
            for item in node.body:
                if isinstance(item, ast.Assign):
                    for target in item.targets:
                        if isinstance(target, ast.Name):
                            container_vars.append(target.id)

            self._parsed_file.add_container(
                ContainerNode(
                    name=node.name,
                    bases=bases,
                    container_vars=container_vars,
                )
            )

    def _extract_container_refs(self, tree: ast.AST) -> None:
        pass
