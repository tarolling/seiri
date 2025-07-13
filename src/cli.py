#!/usr/bin/env python3
# -*- coding: utf-8 -*-

import argparse

from python.ast_parser import parse_python_file


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--path", help="Path to Python project")
    args = parser.parse_args()
    results = parse_python_file(args.path)
    print(results)  # TODO: convert to graph JSON


if __name__ == "__main__":
    main()
