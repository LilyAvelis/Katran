"""Inventory Cargo outputs and plan bounded cleanup; never deletes files."""
import argparse
import json
import os
import re
import stat
from collections import defaultdict
from pathlib import Path


def plan(root, budget_gib=32, incremental_gib=8):
    root = Path(root).absolute()
    if root.lstat().st_file_attributes & stat.FILE_ATTRIBUTE_REPARSE_POINT:
        raise ValueError("target must not be a link")
    sizes = defaultdict(int)
    mtimes = {}
    files = []

    def scan(directory):
        with os.scandir(directory) as entries:
            for entry in entries:
                path = Path(entry.path)
                info = entry.stat(follow_symlinks=False)
                if info.st_file_attributes & stat.FILE_ATTRIBUTE_REPARSE_POINT:
                    raise ValueError(f"Refusing linked build entry: {path}")
                relative = path.relative_to(root)
                mtimes[str(relative)] = info.st_mtime
                if entry.is_dir(follow_symlinks=False):
                    scan(path)
                elif entry.is_file(follow_symlinks=False):
                    files.append(relative)
                    sizes[str(relative)] += info.st_size
                    for parent in relative.parents:
                        sizes[str(parent)] += info.st_size

    scan(root)
    total = before = sizes["."]
    items = []

    def add(relative, reason):
        nonlocal total
        key = str(relative)
        items.append(dict(relative=key, bytes=sizes[key], reason=reason))
        total -= sizes[key]

    budget = budget_gib * 1024**3
    if total > budget and (root / "debug").is_dir():
        add(Path("debug"), "duplicate-default-target-debug")
    incremental = Path("build/debug/incremental")
    incremental_size = sizes[str(incremental)]
    if incremental_size > incremental_gib * 1024**3:
        children = sorted((root / incremental).iterdir(), key=lambda p: mtimes[str(p.relative_to(root))])
        for child in children:
            if incremental_size <= incremental_gib * 1024**3:
                break
            relative = child.relative_to(root)
            if child.is_dir():
                add(relative, "old-incremental-session")
                incremental_size -= sizes[str(relative)]
    groups = defaultdict(list)
    for relative in files:
        if relative.parent == Path("build/debug/deps") and re.fullmatch(r"[\w]+-[0-9a-f]{16}\.(exe|pdb)", relative.name):
            groups[relative.stem].append(relative)
    ordered = sorted(groups.values(), key=lambda paths: max(mtimes[str(p)] for p in paths))
    for paths in ordered:
        if total <= budget:
            break
        for relative in paths:
            add(relative, "old-debug-test-executable-symbols")
    return dict(root=str(root), before_bytes=before, planned_after_bytes=total,
                budget_bytes=budget, items=items,
                over_budget=total > budget,
                preserved="release artifacts, dependency libraries, logs, runtime and user data")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1] / "target")
    parser.add_argument("--budget-gib", type=int, default=32)
    parser.add_argument("--incremental-gib", type=int, default=8)
    args = parser.parse_args()
    if args.budget_gib < 1 or args.incremental_gib < 0:
        parser.error("Invalid cache budget")
    print(json.dumps(plan(args.root, args.budget_gib, args.incremental_gib)))
