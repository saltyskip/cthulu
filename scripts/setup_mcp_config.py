#!/usr/bin/env python3
"""Merge the cthulu MCP server entry into the Claude Desktop config file.

Usage:
    python3 scripts/setup_mcp_config.py <config_path> <launcher> <base_url> <searxng_url>

The <launcher> argument should point to scripts/mcp-launcher.sh, which
auto-starts the backend if it isn't running before exec-ing cthulu-mcp.

If the config file doesn't exist it is created from scratch.
If it already exists the script merges only the "cthulu" key inside
mcpServers, leaving all other registered servers untouched.
"""

import sys
import json
import os


def main() -> None:
    if len(sys.argv) != 5:
        print("Usage: setup_mcp_config.py <config_path> <launcher> <base_url> <searxng_url>")
        sys.exit(1)

    config_path = sys.argv[1]
    binary      = sys.argv[2]  # points to mcp-launcher.sh
    base_url    = sys.argv[3]
    searxng_url = sys.argv[4]

    # Load existing config or start fresh
    config: dict = {}
    if os.path.exists(config_path):
        with open(config_path, "r", encoding="utf-8") as fh:
            try:
                config = json.load(fh)
                print(f"Loaded existing config: {config_path}")
            except json.JSONDecodeError as exc:
                print(f"WARNING: {config_path} has invalid JSON ({exc}) — will overwrite")
                config = {}
    else:
        print(f"Creating new config: {config_path}")

    config.setdefault("mcpServers", {})

    config["mcpServers"]["cthulu"] = {
        "command": binary,
        "args": [
            "--base-url", base_url,
            "--searxng-url", searxng_url,
        ],
    }

    with open(config_path, "w", encoding="utf-8") as fh:
        json.dump(config, fh, indent=2)
        fh.write("\n")

    print(f"Registered 'cthulu' MCP server in {config_path}")

    # Print the final cthulu entry for confirmation
    entry = config["mcpServers"]["cthulu"]
    print(f"  command : {entry['command']}")
    print(f"  args    : {' '.join(entry['args'])}")


if __name__ == "__main__":
    main()
