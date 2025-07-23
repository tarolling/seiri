#!/usr/bin/env python3
# -*- coding: utf-8 -*-

import math
import tkinter as tk
from pathlib import Path
from typing import Any

import matplotlib.patches as patches
import matplotlib.pyplot as plt
import networkx as nx


class GraphVisualizer:
    """Visualizes graph using matplotlib and networkx."""

    def __init__(self):
        self.node_colors = {
            "python": "#3776ab",  # Python blue
            "javascript": "#f7df1e",  # JavaScript yellow
            "typescript": "#3178c6",  # TypeScript blue
            "rust": "#ce422b",  # Rust orange
            "go": "#00add8",  # Go cyan
            "java": "#ed8b00",  # Java orange
            "cpp": "#00599c",  # C++ blue
            "file": "#87ceeb",  # Sky blue
            "external": "#d3d3d3",  # Light gray
        }

        self.edge_colors = {
            "import": "#4169e1",  # Royal blue
            "call": "#dc143c",  # Crimson
            "default": "#808080",  # Gray
        }

    def visualize(self, graph_data: dict[str, Any]):
        """Create and display the graph visualization."""
        try:
            self._networkx_visualize(graph_data)
        except Exception as e:
            print(f"GUI visualization failed: {e}")
            print("Falling back to text-based visualization...")
            self._text_visualize(graph_data)

    def _networkx_visualize(self, graph_data: dict[str, Any]):
        """Use NetworkX for advanced graph layout."""
        G = nx.DiGraph()

        for node in graph_data["nodes"]:
            G.add_node(node["id"], **node)

        for edge in graph_data["edges"]:
            src: str = edge["source"]
            target: str = edge["target"]
            if src in G.nodes and target in G.nodes:
                G.add_edge(edge["source"], edge["target"], **edge)

        plt.figure(figsize=(16, 12))

        # Choose layout based on graph size
        if len(G.nodes) < 50:
            pos = nx.spring_layout(G, k=3, iterations=50)
        else:
            pos = nx.kamada_kawai_layout(G)

        # Prepare node colors and sizes
        node_colors = []
        node_sizes = []

        for node_id in G.nodes():
            node = G.nodes[node_id]
            language = node.get("language", node.get("type", "file"))
            node_colors.append(self.node_colors.get(language, self.node_colors["file"]))

            # Size based on metadata
            metadata = node.get("metadata", {})
            base_size = 500
            if "function_count" in metadata:
                base_size += metadata["function_count"] * 50
            if "container_count" in metadata:
                base_size += metadata["container_count"] * 100
            node_sizes.append(min(base_size, 2000))

        # Draw edges by type
        edge_types = set(edge.get("type", "default") for edge in graph_data["edges"])

        for edge_type in edge_types:
            edges_of_type = [
                (u, v) for u, v, d in G.edges(data=True) if d.get("type") == edge_type
            ]

            if edges_of_type:
                nx.draw_networkx_edges(
                    G,
                    pos,
                    edgelist=edges_of_type,
                    edge_color=self.edge_colors.get(
                        edge_type, self.edge_colors["default"]
                    ),
                    arrows=True,
                    arrowsize=20,
                    arrowstyle="-|>",
                    alpha=0.7,
                    width=2 if edge_type == "import" else 1,
                )

        # Draw nodes
        nx.draw_networkx_nodes(
            G, pos, node_color=node_colors, node_size=node_sizes, alpha=0.8
        )

        # Draw labels
        labels = {}
        for node_id in G.nodes():
            node = G.nodes[node_id]
            name = node.get("name", Path(node_id).name)
            if len(name) > 15:
                name = name[:12] + "..."
            labels[node_id] = name

        nx.draw_networkx_labels(G, pos, labels, font_size=8, font_weight="bold")

        # Add title and metadata
        metadata = graph_data.get("metadata", {})
        title = f"Project Structure - {metadata.get('total_files', 0)} files"
        if metadata.get("languages"):
            title += f" ({', '.join(metadata['languages'])})"

        plt.title(title, fontsize=16, fontweight="bold")

        # Add legend
        legend_elements = []
        for lang in set(
            node.get("language", node.get("type", "file"))
            for node in graph_data["nodes"]
        ):
            if lang in self.node_colors:
                legend_elements.append(
                    patches.Patch(color=self.node_colors[lang], label=lang.capitalize())
                )

        if legend_elements:
            plt.legend(
                handles=legend_elements, loc="upper right", bbox_to_anchor=(1.15, 1)
            )

        plt.axis("off")
        plt.tight_layout()
        plt.show()

    def _matplotlib_visualize(self, graph_data: dict[str, Any]):
        """Simple matplotlib visualization without NetworkX."""
        _, ax = plt.subplots(figsize=(14, 10))

        nodes = graph_data["nodes"]
        edges = graph_data["edges"]

        # Simple circular layout
        n_nodes = len(nodes)
        positions = {}

        for i, node in enumerate(nodes):
            angle = 2 * math.pi * i / n_nodes
            x = 5 * math.cos(angle)
            y = 5 * math.sin(angle)
            positions[node["id"]] = (x, y)

        # Draw edges
        for edge in edges:
            source_pos = positions.get(edge["source"])
            target_pos = positions.get(edge["target"])

            if source_pos and target_pos:
                color = self.edge_colors.get(edge["type"], self.edge_colors["default"])
                ax.annotate(
                    "",
                    xy=target_pos,
                    xytext=source_pos,
                    arrowprops=dict(arrowstyle="->", color=color, lw=1.5),
                )

        # Draw nodes
        for node in nodes:
            pos = positions[node["id"]]
            language = node.get("language", node.get("type", "file"))
            color = self.node_colors.get(language, self.node_colors["file"])

            circle = plt.Circle(pos, 0.3, color=color, alpha=0.8)
            ax.add_patch(circle)

            # Add label
            name = node.get("name", Path(node["id"]).name)
            if len(name) > 10:
                name = name[:8] + "..."
            ax.text(pos[0], pos[1] - 0.5, name, ha="center", va="top", fontsize=8)

        ax.set_xlim(-6, 6)
        ax.set_ylim(-6, 6)
        ax.set_aspect("equal")
        ax.axis("off")

        # Add title
        metadata = graph_data.get("metadata", {})
        title = f"Project Structure - {metadata.get('total_files', 0)} files"
        plt.title(title, fontsize=14, fontweight="bold")

        plt.tight_layout()
        plt.show()

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

    def _layout_nodes(self, nodes: list[dict]) -> dict[str, tuple]:
        """Calculate node positions using a simple grid layout."""
        positions = {}
        cols = math.ceil(math.sqrt(len(nodes)))

        for i, node in enumerate(nodes):
            x = 100 + (i % cols) * 200
            y = 100 + (i // cols) * 150
            positions[node["id"]] = (x, y)

        return positions

    def _draw_node(self, canvas, node: dict, positions: dict):
        """Draw a single node."""
        x, y = positions[node["id"]]

        # Choose color based on node type and language
        color = self.node_colors.get(node.get("language", node["type"]), "lightblue")

        # Draw node circle
        radius = 25
        canvas.create_oval(
            x - radius,
            y - radius,
            x + radius,
            y + radius,
            fill=color,
            outline="black",
            width=2,
        )

        # Draw node label
        label = node.get("name", node["id"])
        if len(label) > 15:
            label = label[:12] + "..."

        canvas.create_text(x, y + 35, text=label, font=("Arial", 9))

        # Add metadata as tooltip (simplified)
        metadata = node.get("metadata", {})
        if metadata:
            tooltip_text = []
            for key, value in metadata.items():
                tooltip_text.append(f"{key}: {value}")

            if tooltip_text:
                canvas.create_text(
                    x,
                    y - 35,
                    text="\n".join(tooltip_text[:3]),  # Show first 3 items
                    font=("Arial", 7),
                    fill="gray",
                )

    def _draw_edge(self, canvas, edge: dict, positions: dict):
        """Draw a single edge."""
        source_pos = positions.get(edge["source"])
        target_pos = positions.get(edge["target"])

        if not source_pos or not target_pos:
            return  # Skip if either node position is not found

        color = self.edge_colors.get(edge["type"], "gray")
        width = edge.get("weight", 1)

        canvas.create_line(
            source_pos[0],
            source_pos[1],
            target_pos[0],
            target_pos[1],
            fill=color,
            width=width,
            arrow=tk.LAST,
        )
