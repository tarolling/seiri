#!/usr/bin/env python3
# -*- coding: utf-8 -*-


from pathlib import Path
from typing import Any

from seiri.parsers.utils.datatypes import ImportNode, ParsedFile


class GraphBuilder:
    """Builds graph representation from parse results."""

    def build_graph(self, parse_results: list[ParsedFile]) -> dict[str, Any]:
        """Build graph from multi-language parse results."""
        nodes: list[dict[str, Any]] = []
        edges: list[dict[str, Any]] = []

        # Create file nodes
        for parsed_file in parse_results:
            language = parsed_file.language
            filepath = parsed_file.path

            nodes.append(
                {
                    "id": str(filepath),
                    "type": "file",
                    "language": language,
                    "name": filepath.name,
                    "metadata": self._extract_metadata(parsed_file),
                }
            )

        # Create dependency edges
        for parsed_file in parse_results:
            # Import dependencies
            for imp in parsed_file.imports:
                if imp:  # Skip None imports
                    edges.append(
                        {
                            "source": str(parsed_file.path),
                            "target": imp.module,
                            "type": "import",
                            "weight": 1,
                        }
                    )

            # Function call dependencies
            for call in parsed_file.function_refs:
                if call:
                    edges.append(
                        {
                            "source": str(parsed_file.path),
                            "target": call.object,
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
                "languages": list(set(r.language for r in parse_results)),
            },
        }

    def _extract_metadata(self, data: ParsedFile) -> dict[str, int]:
        """Extract metadata from parse data."""
        return {
            "import_count": len(data.imports),
            "function_count": len(data.functions),
            "function_ref_count": len(data.function_refs),
            "container_count": len(data.containers),
            "container_ref_count": len(data.container_refs),
        }

    def _find_external_dependencies(
        self, parse_results: list[ParsedFile], edges: list[dict[str, Any]]
    ) -> list[str]:
        """Find external dependencies not in the project."""
        internal_files: set[str] = set([str(file.path) for file in parse_results])
        external_deps: set[str] = set()

        for edge in edges:
            target = edge["target"]
            if not isinstance(target, ImportNode):
                continue
            if target.module is not None and target.module not in internal_files:
                external_deps.add(target.module)

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
