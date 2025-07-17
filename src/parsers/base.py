#!/usr/bin/env python3
# -*- coding: utf-8 -*-

from abc import ABC, abstractmethod
from typing import Any


class BaseParser(ABC):
    """Base class for language parsers."""

    @abstractmethod
    def parse_file(self, filepath: str) -> dict[str, Any]:
        """Parse a file and return structured data."""
        pass

    @abstractmethod
    def get_file_extensions(self) -> list[str]:
        """Return list of file extensions this parser handles."""
        pass
