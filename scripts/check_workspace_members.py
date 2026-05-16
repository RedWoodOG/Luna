#!/usr/bin/env python3
"""Fail when a crate directory is not registered as a Cargo workspace member."""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path


def main() -> int:
    repo = Path(__file__).resolve().parents[1]
    crate_manifests = sorted((repo / "crates").glob("*/Cargo.toml"))
    metadata = subprocess.run(
        ["cargo", "metadata", "--no-deps", "--format-version", "1"],
        cwd=repo,
        check=True,
        capture_output=True,
        text=True,
    )
    metadata_json = json.loads(metadata.stdout)
    workspace_member_ids = set(metadata_json["workspace_members"])
    workspace_members = {
        Path(package["manifest_path"]).resolve()
        for package in metadata_json["packages"]
        if package["id"] in workspace_member_ids
    }
    missing = [
        str(manifest.relative_to(repo)).replace("\\", "/")
        for manifest in crate_manifests
        if manifest.resolve() not in workspace_members
    ]
    if missing:
        print(
            "Workspace member check failed; crate manifests are not in workspace.members:",
            file=sys.stderr,
        )
        for manifest in missing:
            print(f"- {manifest}", file=sys.stderr)
        return 1
    print(f"Workspace member check: OK ({len(crate_manifests)} crate(s))")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
