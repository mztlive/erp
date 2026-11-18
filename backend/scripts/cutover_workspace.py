"""Fail-closed workspace/source validation shared by the final cutover tools.

Only metadata supplied by the real caller is inspected. This module never runs
Cargo, performs database I/O, or treats source scans as runtime proof.
"""
from __future__ import annotations

import functools
from pathlib import Path
import re
import tomllib
from typing import Any, Mapping

LEGACY = {"entities", "database", "services"}


@functools.lru_cache(maxsize=8192)
def _cached_resolve(path: Path) -> Path:
    return path.resolve()


def workspace_errors(backend: Path, metadata: Mapping[str, Any], required: set[str]) -> list[str]:
    """核对指定后端的工作区清单、Cargo metadata 和必需成员；领域与 BPM 门禁据此拒绝不完整的源码输入。"""
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
    backend_resolved = _cached_resolve(backend)
    if not root or _cached_resolve(Path(root)) != backend_resolved:
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
    actual_manifests = {_cached_resolve(Path(p["manifest_path"])) for p in member_packages if p.get("manifest_path")}
    excluded = {_cached_resolve(p) for pattern in workspace.get("exclude", []) for p in backend_resolved.glob(pattern)}
    patterns = workspace.get("members", [])
    if "package" in data:
        patterns = [*patterns, "."]
    if not patterns:
        errors.append("workspace 没有声明任何成员")
    for pattern in patterns:
        if "*" not in pattern and "?" not in pattern and "[" not in pattern:
            candidate_dir = backend_resolved / pattern
            directories = (
                [candidate_dir]
                if candidate_dir.exists() and (not excluded or _cached_resolve(candidate_dir) not in excluded)
                else []
            )
        else:
            directories = [p for p in backend_resolved.glob(pattern) if _cached_resolve(p) not in excluded]
        if not directories:
            errors.append(f"workspace member 路径无匹配: {pattern}")
        for directory in directories:
            candidate = _cached_resolve(directory / "Cargo.toml")
            if not candidate.is_file():
                errors.append(f"声明成员缺少 Cargo.toml: {candidate}")
            elif candidate not in actual_manifests:
                errors.append(f"声明成员未进入 Cargo metadata: {candidate}")
    return errors


def legacy_graph_errors(metadata: Mapping[str, Any]) -> list[str]:
    """检查解析出的所有包及其 normal/build/dev 声明，供最终切换门禁拒绝仍在依赖闭包中的旧包或旧依赖。"""
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
    """检查已退役的三层目录是否仍在后端根目录；最终切换只允许历史文件留在 `docs/archive`。"""
    return [f"最终仍有旧目录或路径: {path}" for name in sorted(LEGACY)
            for path in (backend / name,) if path.exists() or path.is_symlink()]


def active_source_paths(metadata: Mapping[str, Any]) -> list[Path]:
    """列出活动成员的 Rust 源码，供边界门禁共用，包括测试支持 crate 和显式 target 入口。

不纳入成员根目录下的 `tests/` 归档；`src` 中的测试模块仍列出。
宏展开和 `cfg` 可达性仍须由 Rust 编译验证。
"""
    paths: set[Path] = set()
    members = set(metadata.get("workspace_members", []))
    for package in metadata.get("packages", []):
        if package.get("id") not in members or not package.get("manifest_path"):
            continue
        directory = _cached_resolve(Path(package["manifest_path"]).parent)
        historical = directory / "tests"
        roots = [directory / "src"]
        for target in package.get("targets", []):
            if not target.get("src_path"):
                continue
            path = _cached_resolve(Path(target["src_path"]))
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
            is_src_root = root == directory / "src"
            for path in root.rglob("*.rs"):
                if is_src_root:
                    paths.add(path if not path.is_symlink() else path.resolve())
                else:
                    resolved = _cached_resolve(path) if not path.is_symlink() else path.resolve()
                    if historical not in resolved.parents:
                        paths.add(resolved)
    return sorted(paths)


TOKEN_RE = re.compile(
    r'//|/\*|(?:br|r)(#*)"|(?:b)?"(?:\\.|[^"\\])*"|(?:b)?\'(?:\\.|[^\'\\])\'',
    re.S,
)
LEGACY_CRATE_REF_RE = re.compile(
    r"\b(?:entities|database|services)\s*::|\bextern\s+crate\s+(?:entities|database|services)\b"
)
LEGACY_QUALIFIER_RE = re.compile(r"([A-Za-z_]\w*|>)\s*::\s*$")
LEGACY_LITERAL_PATH_RE = re.compile(r'(?:^|/)(?:entities|database|services)/(?:src(?:/|\b)|Cargo\.toml)')
LEGACY_INCLUDE_CONTEXT_RE = re.compile(
    r'(?:#\s*\[\s*(?:cfg_attr\s*\([^]]*)?path\s*=|include(?:_str|_bytes)?\s*!\s*\()\s*$'
)
LEGACY_DYNAMIC_CONTEXT_RE = re.compile(r'include(?:_str|_bytes)?\s*!\s*\(\s*concat!\s*\([^;]*$')


# 以空格覆盖注释或字面量片段中的非换行字符，使词法扫描仍能按原始偏移和行号定位。
def _mask_segment(segment: str) -> str:
    if "\n" not in segment:
        return " " * len(segment)
    return "\n".join(" " * len(line) for line in segment.split("\n"))


# 屏蔽 Rust 注释及字符串、字符字面量，并单独保留字面量位置，供源码门禁按原始偏移检查代码和集合名。
def _code_and_literals(source: str) -> tuple[str, list[tuple[int, str]]]:
    chunks: list[str] = []
    literals: list[tuple[int, str]] = []
    i = 0
    length = len(source)
    while i < length:
        match = TOKEN_RE.search(source, i)
        if not match:
            chunks.append(source[i:])
            break
        start = match.start()
        if start > i:
            chunks.append(source[i:start])
        token = match.group(0)
        if token == "//":
            end = source.find("\n", start + 2)
            i = length if end < 0 else end
        elif token == "/*":
            i, depth = start + 2, 1
            while i < length and depth:
                next_open = source.find("/*", i)
                next_close = source.find("*/", i)
                if next_close < 0:
                    i = length
                    break
                if 0 <= next_open < next_close:
                    depth += 1
                    i = next_open + 2
                else:
                    depth -= 1
                    i = next_close + 2
        elif match.group(1) is not None:
            close = '"' + match.group(1)
            end = source.find(close, match.end())
            i = length if end < 0 else end + len(close)
            literals.append((start, source[start:i]))
        else:
            i = match.end()
            literals.append((start, token))
        chunks.append(_mask_segment(source[start:i]))
    return "".join(chunks), literals


def legacy_source_errors(path: Path, source: str) -> list[str]:
    """区分实际旧 crate 引用与注释或普通字符串，供最终切换拒绝遗留依赖及指向旧源码的 `#[path]` 和 `include`。"""
    if "entities" not in source and "database" not in source and "services" not in source:
        return []
    code, literals = _code_and_literals(source)
    errors = []
    for match in LEGACY_CRATE_REF_RE.finditer(code):
        qualifier = LEGACY_QUALIFIER_RE.search(code, 0, match.start())
        if qualifier and qualifier.group(1) not in {"use", "return"}:
            continue
        errors.append(f"最终旧 crate 源引用: {path}:{code.count(chr(10), 0, match.start()) + 1}: {match.group()}")
    for start, literal in literals:
        if not LEGACY_LITERAL_PATH_RE.search(literal.strip('"#')):
            continue
        prefix = code[:start]
        include_context = LEGACY_INCLUDE_CONTEXT_RE.search(prefix)
        # concat!/env! include paths are kept reviewable rather than guessed safe.
        dynamic_context = LEGACY_DYNAMIC_CONTEXT_RE.search(prefix)
        if include_context or dynamic_context:
            errors.append(f"最终旧源 include/path: {path}: {literal}")
    return errors
