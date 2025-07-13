#!/usr/bin/env python3
# -*- coding: utf-8 -*-

import ast


def parse_python_file(filepath: str) -> dict:
    with open(filepath, "r") as fp:
        tree = ast.parse(fp.read())

    imports = []
    calls = []

    for node in ast.walk(tree):
        if isinstance(node, ast.Import):
            imports.extend(alias.name for alias in node.names)
        elif isinstance(node, ast.ImportFrom):
            imports.append(node.module)
        elif isinstance(node, ast.FunctionDef) or isinstance(node, ast.ClassDef):
            continue
        if isinstance(node, ast.Call):
            calls.append(node.func)
    return {"imports": imports, "calls": calls}
