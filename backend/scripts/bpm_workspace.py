#!/usr/bin/env python3
"""Validate BPM/entry dependency rules from the actual Cargo workspace.

Normal execution loads one all-features, locked Cargo metadata snapshot. Pure
self-tests use synthetic temporary workspaces and never invoke Cargo.
"""
from __future__ import annotations

import argparse
from pathlib import Path
import sys

from cutover_workspace import active_source_paths, workspace_errors
from domain_boundaries import BUSINESS_DOMAINS, BPM_FORBIDDEN_PACKAGES, COMPOSITION, cargo_metadata

REQUIRED = {"bpm", "erp-workflow", "erp-core", "entity-macros", "cli", "web-api"}


def dependency_errors(metadata) -> list[str]:
    """Include renamed, optional, target-specific and all dependency-kind edges."""
    errors = []
    packages = metadata.get("packages", [])
    graph = {}
    for package in packages:
        graph.setdefault(package["name"], set()).update(dep["name"] for dep in package.get("dependencies", []))
    forbidden = BPM_FORBIDDEN_PACKAGES | BUSINESS_DOMAINS | COMPOSITION
    workflow_deps = [dep for package in packages if package.get("name") == "erp-workflow"
                     for dep in package.get("dependencies", [])]
    if not any(dep.get("name") == "bpm" and (dep.get("kind") or "normal") == "normal" for dep in workflow_deps):
        errors.append("erp-workflow 必须直接 normal 依赖 bpm")
    for entry in ("web-api", "cli"):
        if "bpm" in graph.get(entry, set()):
            errors.append(f"{entry} 不得直接依赖 bpm（含 normal/build/dev）")
    for start, targets in (("bpm", forbidden), ("cli", {"web-api"})):
        stack = [(start, [start])]
        seen = set()
        while stack:
            node, chain = stack.pop()
            if node in seen:
                continue
            seen.add(node)
            for dep in sorted(graph.get(node, set())):
                path = [*chain, dep]
                if dep in targets:
                    errors.append("禁止依赖路径（含 normal/build/dev）: " + " -> ".join(path))
                if dep not in seen:
                    stack.append((dep, path))
    return errors


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--backend", type=Path, required=True)
    parser.add_argument("--self-test-only", action="store_true")
    args = parser.parse_args(argv)
    if args.self_test_only:
        from cutover_tool_selftests import run_pure_self_tests
        ok = run_pure_self_tests()
        print("仅工具自检；未检查实际 workspace，未调用 Cargo", file=sys.stderr)
        return 0 if ok else 1
    backend = args.backend.resolve()
    try:
        metadata = cargo_metadata(backend)
        errors = workspace_errors(backend, metadata, REQUIRED)
        if not errors:
            errors.extend(dependency_errors(metadata))
        if errors:
            for error in errors:
                print(error, file=sys.stderr)
            return 1
        for path in active_source_paths(metadata):
            print(path)
        return 0
    except (OSError, ValueError, RuntimeError) as error:
        print(f"BPM workspace 检查失败: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
