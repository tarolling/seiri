#!/usr/bin/env python3
# -*- coding: utf-8 -*-

from dataclasses import dataclass, field
from pathlib import Path


@dataclass
class ImportNode:
    name: str
    module: str | None
    alias: str | None
    level: int


@dataclass
class FunctionNode:
    name: str
    args: list[str]
    decorators: list[str]
    is_async: bool


@dataclass
class FunctionRefNode:
    name: str
    object: str | None


@dataclass
class ContainerNode:
    name: str
    bases: list[str]
    container_vars: list[str]


@dataclass
class ContainerRefNode:
    name: str
    object: str | None
    method: str | None


@dataclass
class ParsedFile:
    language: str
    path: Path
    # mutable default fields: https://docs.python.org/3/library/dataclasses.html#default-factory-functions
    imports: list[ImportNode] = field(default_factory=list[ImportNode])
    functions: list[FunctionNode] = field(default_factory=list[FunctionNode])
    function_refs: list[FunctionRefNode] = field(default_factory=list[FunctionRefNode])
    containers: list[ContainerNode] = field(default_factory=list[ContainerNode])
    container_refs: list[ContainerRefNode] = field(
        default_factory=list[ContainerRefNode]
    )

    def add_import(self, import_node: ImportNode):
        self.imports.append(import_node)

    def add_function(self, function_node: FunctionNode):
        self.functions.append(function_node)

    def add_function_ref(self, function_ref_node: FunctionRefNode):
        self.function_refs.append(function_ref_node)

    def add_container(self, container_node: ContainerNode):
        self.containers.append(container_node)

    def add_container_ref(self, container_ref_node: ContainerRefNode):
        self.container_refs.append(container_ref_node)
