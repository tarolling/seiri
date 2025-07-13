#!/usr/bin/env python3
# -*- coding: utf-8 -*-


def build_graph(parse_results: dict) -> dict:
    nodes = [{"id": file, "type": "file"} for file in parse_results.keys()]
    edges = []
    for file, data in parse_results.items():
        for imp in data["imports"]:
            edges.append({"source": file, "target": imp, "type": "import"})
    return {"nodes": nodes, "edges": edges}
