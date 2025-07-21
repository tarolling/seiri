#!/usr/bin/env python3
# -*- coding: utf-8 -*-


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
                    "id": str(filepath)
                    .replace("/", ".")
                    .replace("\\", ".")
                    .removeprefix("src.")
                    .removesuffix(".py"),
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
                            "source": str(parsed_file.path)
                            .replace("/", ".")
                            .replace("\\", ".")
                            .removeprefix("src.")
                            .removesuffix(".py"),
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
                            "source": str(parsed_file.path)
                            .replace("/", ".")
                            .replace("\\", ".")
                            .removeprefix("src.")
                            .removesuffix(".py"),
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
