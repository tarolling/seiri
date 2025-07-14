#!/usr/bin/env python3
# -*- coding: utf-8 -*-

from abc import ABC, abstractmethod
from typing import Any, Dict, List


class BaseParser(ABC):
    """Base class for language parsers."""

    @abstractmethod
    def parse_file(self, filepath: str) -> Dict[str, Any]:
        """Parse a file and return structured data."""
        pass

    @abstractmethod
    def get_file_extensions(self) -> List[str]:
        """Return list of file extensions this parser handles."""
        pass
