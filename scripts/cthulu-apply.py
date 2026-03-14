#!/usr/bin/env python3
"""
cthulu-apply — Create agent hierarchies from YAML files.

Usage:
    python3 scripts/cthulu-apply.py -f examples/hierarchy-org.yaml
    python3 scripts/cthulu-apply.py -f my-org.yaml --port 8081 --dry-run
    python3 scripts/cthulu-apply.py -f my-org.yaml --delete  # tear down

Requires: pyyaml (pip3 install pyyaml)
"""

import argparse
import json
import os
import sys
import urllib.request
import urllib.error

try:
    import yaml
except ImportError:
    print("ERROR: pyyaml is required. Install with: pip3 install pyyaml")
    sys.exit(1)


# ---------------------------------------------------------------------------
# Colors
# ---------------------------------------------------------------------------
class C:
    RED = "\033[0;31m"
    GREEN = "\033[0;32m"
    YELLOW = "\033[1;33m"
    BLUE = "\033[0;34m"
    CYAN = "\033[0;36m"
    BOLD = "\033[1m"
    NC = "\033[0m"


def info(msg):
    print(f"{C.BLUE}[INFO]{C.NC}  {msg}")


def ok(msg):
    print(f"{C.GREEN}[OK]{C.NC}    {msg}")


def warn(msg):
    print(f"{C.YELLOW}[WARN]{C.NC}  {msg}")


def err(msg):
    print(f"{C.RED}[ERR]{C.NC}   {msg}")


def step(msg):
    print(f"\n{C.BOLD}{C.CYAN}=== {msg} ==={C.NC}\n")


# ---------------------------------------------------------------------------
# HTTP helpers
# ---------------------------------------------------------------------------
def http_request(url, method="GET", data=None):
    """Make an HTTP request. Returns (status_code, response_body_dict_or_str)."""
    headers = {"Content-Type": "application/json"} if data else {}
    body = json.dumps(data).encode("utf-8") if data else None

    req = urllib.request.Request(url, data=body, headers=headers, method=method)
    try:
        with urllib.request.urlopen(req) as resp:
            raw = resp.read().decode("utf-8")
            try:
                return resp.status, json.loads(raw)
            except json.JSONDecodeError:
                return resp.status, raw
    except urllib.error.HTTPError as e:
        raw = e.read().decode("utf-8")
        try:
            return e.code, json.loads(raw)
        except json.JSONDecodeError:
            return e.code, raw


def wait_for_backend(base_url, timeout=30):
    """Wait for the backend to be healthy."""
    import time

    for _ in range(timeout):
        try:
            code, _ = http_request(f"{base_url}/health")
            if code == 200:
                return True
        except Exception:
            pass
        time.sleep(1)
    return False


# ---------------------------------------------------------------------------
# Agent creation
# ---------------------------------------------------------------------------
class AgentApplier:
    def __init__(self, base_url, dry_run=False):
        self.api_url = f"{base_url}/api"
        self.dry_run = dry_run
        self.created_agents = []  # list of (id, name, role, parent_name)

    def create_agent(self, agent_def, defaults, parent_id=None, parent_name=None, depth=0):
        """
        Create an agent from a YAML definition.
        Returns the agent's ID.
        """
        name = agent_def["name"]
        role = agent_def.get("role")
        description = agent_def.get("description", "")
        prompt = agent_def.get("prompt", "")

        # Merge defaults for permissions/working_dir
        permissions = agent_def.get("permissions", defaults.get("permissions", []))
        working_dir = agent_def.get("working_dir", defaults.get("working_dir", ""))

        # Expand ~ in working_dir
        if working_dir and working_dir.startswith("~"):
            working_dir = os.path.expanduser(working_dir)

        # Heartbeat config (merge agent-level over defaults)
        hb_defaults = defaults.get("heartbeat", {})
        hb_agent = agent_def.get("heartbeat", {})
        heartbeat = {**hb_defaults, **hb_agent}

        indent = "  " * depth
        prefix = f"{indent}{'└── ' if depth > 0 else ''}"

        if self.dry_run:
            reports_str = f" -> {parent_name}" if parent_name else " (root)"
            info(f"{prefix}{name} (role={role}){reports_str}")
            # Process subordinates
            for sub in agent_def.get("subordinates", []):
                self.create_agent(sub, defaults, parent_id="DRY-RUN", parent_name=name, depth=depth + 1)
            return "DRY-RUN"

        # --- Create the agent ---
        create_body = {
            "name": name,
            "description": description.strip(),
            "prompt": prompt.strip(),
            "permissions": permissions,
        }
        if working_dir:
            create_body["working_dir"] = working_dir
        if role:
            create_body["role"] = role
        if parent_id:
            create_body["reports_to"] = parent_id

        code, resp = http_request(f"{self.api_url}/agents", method="POST", data=create_body)

        if code != 201:
            err(f"Failed to create '{name}' (HTTP {code}): {resp}")
            sys.exit(1)

        agent_id = resp["id"]
        self.created_agents.append((agent_id, name, role, parent_name))

        ok(f"{prefix}{name} (id={agent_id[:12]}..., role={role})")

        # --- Enable heartbeat via update ---
        if heartbeat.get("enabled"):
            update_body = {
                "heartbeat_enabled": True,
                "heartbeat_interval_secs": heartbeat.get("interval_secs", 600),
                "max_turns_per_heartbeat": heartbeat.get("max_turns", 5),
                "auto_permissions": heartbeat.get("auto_permissions", False),
            }
            hb_code, hb_resp = http_request(
                f"{self.api_url}/agents/{agent_id}", method="PUT", data=update_body
            )
            if hb_code != 200:
                warn(f"  Failed to enable heartbeat for '{name}': {hb_resp}")

        # --- Process subordinates ---
        for sub in agent_def.get("subordinates", []):
            self.create_agent(sub, defaults, parent_id=agent_id, parent_name=name, depth=depth + 1)

        return agent_id

    def delete_created(self):
        """Delete all agents created in this run (reverse order for clean hierarchy teardown)."""
        step("Deleting created agents")
        for agent_id, name, role, _ in reversed(self.created_agents):
            code, _ = http_request(f"{self.api_url}/agents/{agent_id}", method="DELETE")
            if code == 200:
                ok(f"Deleted {name} ({agent_id[:12]}...)")
            else:
                warn(f"Could not delete {name} ({agent_id[:12]}...)")

    def print_summary(self):
        """Print a summary of created agents as a tree."""
        step("Summary")
        print(f"{C.BOLD}Created {len(self.created_agents)} agents:{C.NC}\n")

        # Build tree
        roots = []
        children_map = {}  # parent_name -> [agents]
        for agent_id, name, role, parent_name in self.created_agents:
            if parent_name is None:
                roots.append((agent_id, name, role))
            else:
                children_map.setdefault(parent_name, []).append((agent_id, name, role))

        def print_tree(name, agent_id, role, prefix="", is_last=True):
            connector = "└── " if prefix else ""
            role_str = f" ({role})" if role else ""
            print(f"{prefix}{connector}{name}{role_str}  [{agent_id[:8]}...]")
            children = children_map.get(name, [])
            for i, (cid, cname, crole) in enumerate(children):
                new_prefix = prefix + ("    " if is_last or not prefix else "│   ")
                print_tree(cname, cid, crole, new_prefix, i == len(children) - 1)

        for aid, aname, arole in roots:
            print_tree(aname, aid, arole)

        print()


# ---------------------------------------------------------------------------
# Delete mode — find and delete agents matching a YAML file
# ---------------------------------------------------------------------------
def collect_names(agent_def):
    """Recursively collect all agent names from a YAML definition."""
    names = [agent_def["name"]]
    for sub in agent_def.get("subordinates", []):
        names.extend(collect_names(sub))
    return names


def delete_by_yaml(api_url, yaml_data):
    """Delete agents whose names match those in the YAML file."""
    step("Deleting agents matching YAML")

    # Collect all names from the YAML
    target_names = set()
    for agent_def in yaml_data.get("agents", []):
        target_names.update(collect_names(agent_def))

    info(f"Looking for agents named: {', '.join(sorted(target_names))}")

    # List all agents
    code, resp = http_request(f"{api_url}/agents")
    if code != 200:
        err(f"Failed to list agents: {resp}")
        sys.exit(1)

    agents = resp.get("agents", [])
    to_delete = [(a["id"], a["name"]) for a in agents if a.get("name") in target_names]

    if not to_delete:
        info("No matching agents found")
        return

    info(f"Found {len(to_delete)} agents to delete")

    # Delete in reverse order (subordinates first)
    for agent_id, name in reversed(to_delete):
        code, _ = http_request(f"{api_url}/agents/{agent_id}", method="DELETE")
        if code == 200:
            ok(f"Deleted {name} ({agent_id[:12]}...)")
        else:
            warn(f"Could not delete {name} ({agent_id[:12]}...)")


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------
def main():
    parser = argparse.ArgumentParser(
        description="Create agent hierarchies from YAML files",
        prog="cthulu-apply",
    )
    parser.add_argument(
        "-f", "--file", required=True, help="Path to YAML hierarchy file"
    )
    parser.add_argument(
        "--port", type=int, default=8081, help="Backend port (default: 8081)"
    )
    parser.add_argument(
        "--host", default="localhost", help="Backend host (default: localhost)"
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Print what would be created without making API calls",
    )
    parser.add_argument(
        "--delete",
        action="store_true",
        help="Delete agents matching the YAML file instead of creating",
    )
    parser.add_argument(
        "--no-wait",
        action="store_true",
        help="Don't wait for backend to be healthy",
    )

    args = parser.parse_args()

    # --- Load YAML ---
    yaml_path = args.file
    if not os.path.exists(yaml_path):
        err(f"File not found: {yaml_path}")
        sys.exit(1)

    with open(yaml_path) as f:
        data = yaml.safe_load(f)

    if not data or "agents" not in data:
        err("YAML file must have an 'agents' key at the top level")
        sys.exit(1)

    base_url = f"http://{args.host}:{args.port}"
    api_url = f"{base_url}/api"

    # --- Header ---
    print(f"\n{C.BOLD}cthulu-apply{C.NC} — Agent Hierarchy Manager\n")
    info(f"File:     {yaml_path}")
    info(f"Backend:  {base_url}")
    info(f"Mode:     {'DELETE' if args.delete else 'DRY-RUN' if args.dry_run else 'CREATE'}")

    # --- Delete mode ---
    if args.delete:
        if not args.dry_run and not args.no_wait:
            step("Waiting for backend")
            if not wait_for_backend(base_url):
                err("Backend not reachable. Start with: cargo run -- serve")
                sys.exit(1)
            ok("Backend is healthy")

        if args.dry_run:
            step("Agents that would be deleted")
            names = set()
            for agent_def in data.get("agents", []):
                names.update(collect_names(agent_def))
            for name in sorted(names):
                info(f"  {name}")
            return

        delete_by_yaml(api_url, data)
        return

    # --- Create mode ---
    if not args.dry_run and not args.no_wait:
        step("Waiting for backend")
        if not wait_for_backend(base_url):
            err("Backend not reachable. Start with: cargo run -- serve")
            sys.exit(1)
        ok("Backend is healthy")

    defaults = data.get("defaults", {})
    applier = AgentApplier(base_url, dry_run=args.dry_run)

    step("Creating agents")
    for agent_def in data["agents"]:
        applier.create_agent(agent_def, defaults)

    if not args.dry_run:
        applier.print_summary()

        # Ensure working directory exists
        wd = defaults.get("working_dir", "")
        if wd:
            wd = os.path.expanduser(wd)
            os.makedirs(wd, exist_ok=True)
            info(f"Working directory ready: {wd}")

        print(f"\n{C.BOLD}Next steps:{C.NC}")
        print("  1. Open Cthulu Studio")
        print("  2. Navigate to Agents tab")
        print('  3. Click "Org Chart" to see the hierarchy')
        print("  4. Create tasks to trigger heartbeat runs")
        print()
        print(f"  To tear down:  python3 scripts/cthulu-apply.py -f {yaml_path} --delete")
        print()
    else:
        print(f"\n{C.BOLD}Dry run complete.{C.NC} No agents were created.")
        print(f"  Remove --dry-run to apply: python3 scripts/cthulu-apply.py -f {yaml_path}")
        print()


if __name__ == "__main__":
    main()
