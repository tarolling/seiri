#!/usr/bin/env python3
# -*- coding: utf-8 -*-

import ast


def parse_python_file(filepath: str) -> dict:
    with open(filepath, "r") as fp:
        tree = ast.parse(fp.read())
    imports = [n.module for n in ast.walk(tree) if isinstance(n, ast.Import)]
    calls = [f.id for f in ast.walk(tree) if isinstance(f, ast.Call)]
    return {"imports": imports, "calls": calls}
