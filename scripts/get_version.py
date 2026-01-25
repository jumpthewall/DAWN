#!/usr/bin/env python3
"""Extract the version from Cargo.toml."""

import re
import sys
from pathlib import Path


def get_version(cargo_toml_path: Path) -> str:
    """Parse Cargo.toml and return the workspace package version."""
    content = cargo_toml_path.read_text()

    # Look for version in [workspace.package] section
    in_workspace_package = False
    for line in content.splitlines():
        stripped = line.strip()
        if stripped == "[workspace.package]":
            in_workspace_package = True
            continue
        if in_workspace_package:
            if stripped.startswith("["):
                # New section, stop looking
                break
            match = re.match(r'^version\s*=\s*["\']([^"\']+)["\']', stripped)
            if match:
                return match.group(1)

    raise ValueError("Could not find version in [workspace.package]")


def main() -> None:
    # Find Cargo.toml relative to this script or use argument
    if len(sys.argv) > 1:
        cargo_toml = Path(sys.argv[1])
    else:
        # Default: look in parent of scripts directory
        cargo_toml = Path(__file__).parent.parent / "Cargo.toml"

    if not cargo_toml.exists():
        print(f"Error: {cargo_toml} not found", file=sys.stderr)
        sys.exit(1)

    try:
        version = get_version(cargo_toml)
        print(version)
    except ValueError as e:
        print(f"Error: {e}", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
