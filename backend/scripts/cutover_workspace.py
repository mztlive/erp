"""Fail-closed workspace/source validation shared by the final cutover tools.

Only metadata supplied by the real caller is inspected. This module never runs
Cargo, performs database I/O, or treats source scans as runtime proof.
"""
from __future__ import annotations

from pathlib import Path
import re
import tomllib
from typing import Any, Mapping

LEGACY = {"entities", "database", "services"}


def workspace_errors(backend: Path, metadata: Mapping[str, Any], required: set[str]) -> list[str]:
    """Require a real workspace and every declared/required member manifest."""
    errors: list[str] = []
    manifest = backend / "Cargo.toml"
    if not manifest.is_file():
        return [f"缺失 workspace manifest: {manifest}"]
    try:
        data = tomllib.loads(manifest.read_text())
    except (OSError, tomllib.TOMLDecodeError) as error:
        return [f"workspace manifest 无法解析: {error}"]
    workspace = data.get("workspace")
    if not isinstance(workspace, dict):
        return ["Cargo.toml 缺少 [workspace]"]
    members = metadata.get("workspace_members")
    packages = metadata.get("packages")
    if not isinstance(members, list) or not members:
        return ["Cargo metadata 缺少非空 workspace_members"]
    if not isinstance(packages, list) or not packages:
        return ["Cargo metadata 缺少非空 packages"]
    root = metadata.get("workspace_root")
    if not root or Path(root).resolve() != backend.resolve():
        errors.append("Cargo metadata workspace_root 与指定 backend 不一致")
    by_id = {package.get("id"): package for package in packages}
    member_packages = []
    for member in members:
        package = by_id.get(member)
        if not package:
            errors.append(f"workspace member 缺少 package 记录: {member}")
            continue
        member_packages.append(package)
        path = package.get("manifest_path")
        if not path or not Path(path).is_file():
            errors.append(f"workspace member manifest 不存在: {package.get('name')}: {path}")
        targets = package.get("targets")
        if not isinstance(targets, list) or not targets:
            errors.append(f"workspace member 缺少活动 target: {package.get('name')}")
        else:
            for target in targets:
                target_source = target.get("src_path")
                if not target_source or not Path(target_source).is_file():
                    errors.append(f"活动 target 源不存在: {package.get('name')}::{target.get('name')}: {target_source}")
    names = {package.get("name") for package in member_packages}
    for name in sorted(required - names):
        errors.append(f"workspace 缺少必需活动成员: {name}")
    actual_manifests = {Path(p["manifest_path"]).resolve() for p in member_packages if p.get("manifest_path")}
    excluded = {p.resolve() for pattern in workspace.get("exclude", []) for p in backend.glob(pattern)}
    patterns = workspace.get("members", [])
    if "package" in data:
        patterns = [*patterns, "."]
    if not patterns:
        errors.append("workspace 没有声明任何成员")
    for pattern in patterns:
        directories = [p for p in backend.glob(pattern) if p.resolve() not in excluded]
        if not directories:
            errors.append(f"workspace member 路径无匹配: {pattern}")
        for directory in directories:
            candidate = (directory / "Cargo.toml").resolve()
            if not candidate.is_file():
                errors.append(f"声明成员缺少 Cargo.toml: {candidate}")
            elif candidate not in actual_manifests:
                errors.append(f"声明成员未进入 Cargo metadata: {candidate}")
    return errors


def legacy_graph_errors(metadata: Mapping[str, Any]) -> list[str]:
    """Reject legacy packages and declared normal/build/dev edges at every depth."""
    errors = []
    for package in metadata.get("packages", []):
        name = package.get("name")
        if name in LEGACY:
            errors.append(f"最终依赖闭包仍含旧 package: {name}")
        for dep in package.get("dependencies", []):
            if dep.get("name") in LEGACY:
                kind = dep.get("kind") or "normal"
                errors.append(f"最终旧依赖: {name} -[{kind}, rename={dep.get('rename')}, target={dep.get('target')}]-> {dep['name']}")
    return errors


def legacy_tree_errors(backend: Path) -> list[str]:
    """Historical tests may remain; legacy production src/manifests may not."""
    return [f"最终仍有旧生产路径: {path}" for name in sorted(LEGACY)
            for path in (backend / name / "src", backend / name / "Cargo.toml") if path.exists()]


def active_source_paths(metadata: Mapping[str, Any]) -> list[Path]:
    """Inventory active members including test-support and explicit target roots.

Root tests/ archives are excluded. Inline src/tests modules stay included.
Rust compilation still supplies the final macro/cfg reachability evidence.
"""
    paths: set[Path] = set()
    members = set(metadata.get("workspace_members", []))
    for package in metadata.get("packages", []):
        if package.get("id") not in members or not package.get("manifest_path"):
            continue
        directory = Path(package["manifest_path"]).parent.resolve()
        historical = directory / "tests"
        roots = [directory / "src"]
        for target in package.get("targets", []):
            if not target.get("src_path"):
                continue
            path = Path(target["src_path"]).resolve()
            if path == historical or historical in path.parents:
                continue
            if path.is_file():
                paths.add(path)
            # Custom lib/bin roots can have sibling/submodule files outside src.
            if path.parent != directory and directory / "src" not in path.parents:
                roots.append(path.parent)
        build = directory / "build.rs"
        if build.is_file():
            paths.add(build)
        for root in roots:
            for path in root.rglob("*.rs"):
                path = path.resolve()
                if historical not in path.parents:
                    paths.add(path)
    return sorted(paths)


def _code_and_literals(source: str) -> tuple[str, list[tuple[int, str]]]:
    """Mask Rust comments and string/character literals; keep their offsets."""
    output = list(source)
    literals = []
    i = 0
    while i < len(source):
        start = i
        if source.startswith("//", i):
            end = source.find("\n", i)
            i = len(source) if end < 0 else end
        elif source.startswith("/*", i):
            i, depth = i + 2, 1
            while i < len(source) and depth:
                if source.startswith("/*", i):
                    i, depth = i + 2, depth + 1
                elif source.startswith("*/", i):
                    i, depth = i + 2, depth - 1
                else:
                    i += 1
        else:
            raw = re.match(r'(?:br|r)(#*)"', source[i:])
            quoted = re.match(r'''(?:b)?"(?:\\.|[^"\\])*"|(?:b)?'(?:\\.|[^'\\])' ''', source[i:], re.S | re.X)
            if raw:
                close = '"' + raw.group(1)
                end = source.find(close, i + raw.end())
                i = len(source) if end < 0 else end + len(close)
                literals.append((start, source[start:i]))
            elif quoted:
                i += quoted.end()
                literals.append((start, source[start:i]))
            else:
                i += 1
                continue
        for j in range(start, i):
            if output[j] != "\n":
                output[j] = " "
    return "".join(output), literals


def legacy_source_errors(path: Path, source: str) -> list[str]:
    """Reject real legacy tokens and legacy #[path]/include references."""
    code, literals = _code_and_literals(source)
    errors = []
    for match in re.finditer(r"\b(?:entities|database|services)\s*::|\bextern\s+crate\s+(?:entities|database|services)\b", code):
        qualifier = re.search(r"([A-Za-z_]\w*|>)\s*::\s*$", code[:match.start()])
        if qualifier and qualifier.group(1) not in {"use", "return"}:
            continue
        errors.append(f"最终旧 crate 源引用: {path}:{code.count(chr(10), 0, match.start()) + 1}: {match.group()}")
    for start, literal in literals:
        prefix = code[:start]
        include_context = re.search(r'(?:#\s*\[\s*(?:cfg_attr\s*\([^]]*)?path\s*=|include(?:_str|_bytes)?\s*!\s*\()\s*$', prefix)
        # concat!/env! include paths are kept reviewable rather than guessed safe.
        dynamic_context = re.search(r'include(?:_str|_bytes)?\s*!\s*\(\s*concat!\s*\([^;]*$', prefix)
        if (include_context or dynamic_context) and re.search(r'(?:^|/)(?:entities|database|services)/(?:src(?:/|\b)|Cargo\.toml)', literal.strip('"#')):
            errors.append(f"最终旧源 include/path: {path}: {literal}")
    return errors
