#!/usr/bin/env python3
# -*- coding: utf-8 -*-

from typing import Optional, Type

from seiri.parsers.base import BaseParser
from seiri.parsers.javascript import JavaScriptParser
from seiri.parsers.python import PythonParser
from seiri.parsers.rust import RustParser


class ParserRegistry:
    """Registry for language parsers."""

    def __init__(self):
        self._parsers: dict[str, Type[BaseParser]] = {}
        self._load_builtin_parsers()

    def register_parser(self, language: str, parser_class: Type[BaseParser]):
        """Register a parser for a language."""
        self._parsers[language] = parser_class

    def get_parser(self, language: str) -> Optional[Type[BaseParser]]:
        """Get parser class for a language."""
        return self._parsers.get(language)

    def _load_builtin_parsers(self):
        """Load built-in parsers."""
        self.register_parser("python", PythonParser)
        self.register_parser("javascript", JavaScriptParser)
        self.register_parser("rust", RustParser)
