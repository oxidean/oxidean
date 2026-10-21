"""CLI entrypoint."""

from __future__ import annotations

import argparse


def greet(name: str) -> str:
    return f"Hello, {name}!"


def main() -> None:
    parser = argparse.ArgumentParser(description="Python starter")
    parser.add_argument("name", nargs="?", default="world")
    args = parser.parse_args()
    print(greet(args.name))


if __name__ == "__main__":
    main()
