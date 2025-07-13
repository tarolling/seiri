#!/usr/bin/env python3
# -*- coding: utf-8 -*-

import argparse
import json
import tkinter as tk
from pathlib import Path

from python import ast_parser, graph_builder


def visualize_graph(graph_data: dict):
    root = tk.Tk()
    root.title("Seiri - Project Visualizer")
    canvas = tk.Canvas(root, width=800, height=600)
    canvas.pack()

    # Calculate and store node positions
    node_positions = {}
    for i, node in enumerate(graph_data["nodes"]):
        x, y = 100 + (i % 3) * 200, 100 + (i // 3) * 150
        node_positions[node["id"]] = (x, y)
        canvas.create_oval(x - 20, y - 20, x + 20, y + 20, fill="lightblue")
        canvas.create_text(x, y + 30, text=node["id"])

    for edge in graph_data["edges"]:
        src_x, src_y = node_positions[edge["source"]]
        dst_x, dst_y = node_positions[edge["target"]]
        canvas.create_line(src_x, src_y, dst_x, dst_y, width=2)

    root.mainloop()


def find_python_files(path: str) -> list:
    """Recursively find all .py files in a directory."""
    return [
        str(p)
        for p in Path(path).rglob("*.py")
        if (
            p.is_file()
            and not p.name.startswith("__")
            and "site-packages" not in p.parts
            and "venv" not in p.parts
            and ".venv" not in p.parts
            and "env" not in p.parts
        )
    ]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--path", help="Path to Python project")
    args = parser.parse_args()

    python_files = find_python_files(args.path)
    parse_results = {}
    for file in python_files:
        parse_results[file] = ast_parser.parse_python_file(file)

    graph_data = graph_builder.build_graph(parse_results)
    visualize_graph(graph_data)
    print(json.dumps(graph_data))  # TODO: convert to graph JSON


if __name__ == "__main__":
    main()
