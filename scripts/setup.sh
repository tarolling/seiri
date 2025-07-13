#!/bin/sh

# Install uv - https://github.com/astral-sh/uv
curl -LsSf https://astral.sh/uv/install.sh | sh

# Create virtual environment and install specified packages
uv venv
source .venv/bin/activate
uv sync
pre-commit install
