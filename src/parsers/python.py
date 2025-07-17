#!/usr/bin/env python3
# -*- coding: utf-8 -*-

import ast
import symtable
from typing import Any

from .base import BaseParser


class PythonParser(BaseParser):
    """Parser for Python files."""

    def __init__(self):
        self.imports = []
        self.functions = {}
        self.function_calls = []
        self.classes = {}
        self.class_calls = []

    def get_file_extensions(self) -> list[str]:
        return ["py"]

    def parse_file(self, filepath: str) -> dict[str, Any]:
        """Analyze a Python file and extract all requested information."""
        with open(filepath, "r", encoding="utf-8") as f:
            source = f.read()

        self.filepath = filepath

        # Parse AST for structural analysis
        tree = ast.parse(source, filepath)

        # Create symbol table for scope analysis
        st = symtable.symtable(source, filepath, "exec")

        # Extract information using both AST and symtable
        self._extract_imports(tree)
        self._extract_functions(tree, st)
        self._extract_classes(tree, st)
        self._extract_calls(tree)

        return {
            "imports": self.imports,
            "functions": self.functions,
            "function_calls": self.function_calls,
            "classes": self.classes,
            "class_calls": self.class_calls,
        }

    def _extract_imports(self, tree: ast.AST):
        """Extract import statements."""
        for node in ast.walk(tree):
            if isinstance(node, ast.Import):
                for alias in node.names:
                    self.imports.append(
                        {
                            "path": self.filepath,
                            "type": "import",
                            "module": alias.name,
                            "alias": alias.asname,
                            "line": node.lineno,
                        }
                    )
            elif isinstance(node, ast.ImportFrom):
                for alias in node.names:
                    self.imports.append(
                        {
                            "type": self.filepath,
                            "module": node.module,
                            "name": alias.name,
                            "alias": alias.asname,
                            "line": node.lineno,
                        }
                    )

    def _extract_functions(self, tree: ast.AST, st: symtable.SymbolTable):
        """Extract function definitions and their details."""
        for node in ast.walk(tree):
            if isinstance(node, ast.FunctionDef):
                # Get function arguments
                args = []
                for arg in node.args.args:
                    args.append(arg.arg)

                # Get decorators
                decorators = []
                for decorator in node.decorator_list:
                    if isinstance(decorator, ast.Name):
                        decorators.append(decorator.id)
                    elif isinstance(decorator, ast.Attribute):
                        decorators.append(ast.unparse(decorator))

                # Get local variables using symtable
                func_st = None
                for child in st.get_children():
                    if child.get_name() == node.name and child.get_type() == "function":
                        func_st = child
                        break

                local_vars = []
                if func_st:
                    for symbol in func_st.get_symbols():
                        if symbol.is_local():
                            local_vars.append(symbol.get_name())

                self.functions[node.name] = {
                    "line": node.lineno,
                    "args": args,
                    "decorators": decorators,
                    "local_variables": local_vars,
                    "is_async": isinstance(node, ast.AsyncFunctionDef),
                }

    def _extract_classes(self, tree: ast.AST, st: symtable.SymbolTable):
        """Extract class definitions with methods and instance variables."""
        for node in ast.walk(tree):
            if isinstance(node, ast.ClassDef):
                # Get base classes
                bases = []
                for base in node.bases:
                    if isinstance(base, ast.Name):
                        bases.append(base.id)
                    elif isinstance(base, ast.Attribute):
                        bases.append(ast.unparse(base))

                # Get methods
                methods = {}
                instance_vars = set()

                for item in node.body:
                    if isinstance(item, ast.FunctionDef):
                        # Extract method arguments
                        method_args = []
                        for arg in item.args.args:
                            method_args.append(arg.arg)

                        methods[item.name] = {
                            "line": item.lineno,
                            "args": method_args,
                            "is_property": any(
                                isinstance(d, ast.Name) and d.id == "property"
                                for d in item.decorator_list
                            ),
                        }

                        # Look for instance variable assignments (self.var = ...)
                        for subnode in ast.walk(item):
                            if isinstance(subnode, ast.Assign):
                                for target in subnode.targets:
                                    if (
                                        isinstance(target, ast.Attribute)
                                        and isinstance(target.value, ast.Name)
                                        and target.value.id == "self"
                                    ):
                                        instance_vars.add(target.attr)

                # Get class-level variables
                class_vars = []
                for item in node.body:
                    if isinstance(item, ast.Assign):
                        for target in item.targets:
                            if isinstance(target, ast.Name):
                                class_vars.append(target.id)

                self.classes[node.name] = {
                    "line": node.lineno,
                    "bases": bases,
                    "methods": methods,
                    "instance_variables": list(instance_vars),
                    "class_variables": class_vars,
                }

    def _extract_calls(self, tree: ast.AST):
        """Extract function and class calls."""
        for node in ast.walk(tree):
            if isinstance(node, ast.Call):
                call_info = {
                    "line": node.lineno,
                    "args_count": len(node.args),
                    "kwargs_count": len(node.keywords),
                }

                if isinstance(node.func, ast.Name):
                    # Simple function call: func()
                    call_name = node.func.id
                    call_info["name"] = call_name
                    call_info["type"] = "simple"

                    # Heuristic: if name starts with capital letter, likely a class
                    if call_name[0].isupper():
                        self.class_calls.append(call_info)
                    else:
                        self.function_calls.append(call_info)

                elif isinstance(node.func, ast.Attribute):
                    # Method call: obj.method() or module.func()
                    call_name = node.func.attr
                    call_info["name"] = call_name
                    call_info["type"] = "attribute"
                    call_info["object"] = ast.unparse(node.func.value)

                    self.function_calls.append(call_info)
