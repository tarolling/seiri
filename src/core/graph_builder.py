#!/usr/bin/env python3
# -*- coding: utf-8 -*-


from pathlib import Path
from typing import Any


class GraphBuilder:
    """Builds graph representation from parse results."""

    def build_graph(self, parse_results: dict[str, dict]) -> dict[str, Any]:
        """Build graph from multi-language parse results."""
        nodes = []
        edges = []

        # Create file nodes
        for filepath, result in parse_results.items():
            language = result["language"]
            data = result["data"]

            node = {
                "id": filepath,
                "type": "file",
                "language": language,
                "name": Path(filepath).name,
                "metadata": self._extract_metadata(data),
            }
            nodes.append(node)

        # Create dependency edges
        for filepath, result in parse_results.items():
            data = result["data"]

            # Import dependencies
            for imp in data.get("imports", []):
                if imp:  # Skip None imports
                    edges.append(
                        {
                            "source": filepath,
                            "target": imp,
                            "type": "import",
                            "weight": 1,
                        }
                    )

            # Function call dependencies
            for call in data.get("calls", []):
                if call:
                    edges.append(
                        {
                            "source": filepath,
                            "target": call,
                            "type": "call",
                            "weight": 1,
                        }
                    )

        # Add module/package nodes for external dependencies
        external_deps = self._find_external_dependencies(parse_results, edges)
        for dep in external_deps:
            nodes.append({"id": dep, "type": "external", "name": dep, "metadata": {}})

        return {
            "nodes": nodes,
            "edges": edges,
            "metadata": {
                "total_files": len(parse_results),
                "languages": list(set(r["language"] for r in parse_results.values())),
            },
        }

    def _extract_metadata(self, data: dict) -> dict:
        """Extract metadata from parse data."""
        metadata = {}

        if "functions" in data:
            metadata["function_count"] = len(data["functions"])
        if "classes" in data:
            metadata["class_count"] = len(data["classes"])
        if "imports" in data:
            metadata["import_count"] = len(data["imports"])

        return metadata

    def _find_external_dependencies(
        self, parse_results: dict, edges: list[dict]
    ) -> list[str]:
        """Find external dependencies not in the project."""
        internal_files = set(parse_results.keys())
        external_deps = set()

        for edge in edges:
            target = edge["target"]
            if target not in internal_files and target:
                external_deps.add(target)

        return list(external_deps)

    def _text_visualize(self, graph_data: dict[str, Any]):
        """Text-based visualization as fallback."""
        print("\n" + "=" * 60)
        print("SEIRI - PROJECT STRUCTURE VISUALIZATION")
        print("=" * 60)

        metadata = graph_data.get("metadata", {})
        print(f"Total files: {metadata.get('total_files', 0)}")
        print(f"Languages: {', '.join(metadata.get('languages', []))}")
        print(f"Nodes: {len(graph_data['nodes'])}, Edges: {len(graph_data['edges'])}")

        # Group nodes by type
        file_nodes = [n for n in graph_data["nodes"] if n["type"] == "file"]
        external_nodes = [n for n in graph_data["nodes"] if n["type"] == "external"]

        print(f"\nFILE NODES ({len(file_nodes)}):")
        print("-" * 40)
        for node in file_nodes:
            lang = node.get("language", "unknown")
            metadata = node.get("metadata", {})
            meta_str = ", ".join([f"{k}: {v}" for k, v in metadata.items()])
            print(
                f"  📄 {node['name']} ({lang})" + (f" - {meta_str}" if meta_str else "")
            )

        if external_nodes:
            print(f"\nEXTERNAL DEPENDENCIES ({len(external_nodes)}):")
            print("-" * 40)
            for node in external_nodes:
                print(f"  📦 {node['name']}")

        # Show dependencies
        print(f"\nDEPENDENCIES ({len(graph_data['edges'])}):")
        print("-" * 40)
        for edge in graph_data["edges"]:
            edge_type = edge["type"]
            symbol = "→" if edge_type == "import" else "↗"
            source_name = (
                Path(edge["source"]).name
                if edge["source"] in [n["id"] for n in file_nodes]
                else edge["source"]
            )
            target_name = (
                Path(edge["target"]).name
                if edge["target"] in [n["id"] for n in file_nodes]
                else edge["target"]
            )
            print(f"  {source_name} {symbol} {target_name} ({edge_type})")

        print("\n" + "=" * 60)
