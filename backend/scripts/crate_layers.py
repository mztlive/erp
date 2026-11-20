"""Explicit crate ownership and dependency contracts for the layered workspace."""
from __future__ import annotations

import functools
import json
from pathlib import Path
from typing import Any, Mapping

REGISTRY_PATH = Path(__file__).with_name("crate-layers.json")
LAYERS = {"domains", "orchestration", "read-models", "adapters", "foundation", "platform", "testing", "apps", "configuration"}


@functools.lru_cache(maxsize=1)
def load_registry() -> dict[str, dict[str, Any]]:
    """读取并校验已审查的 crate 层次、路径和编排依赖登记，为工作区成员定位与依赖门禁提供统一依据。"""
    payload = json.loads(REGISTRY_PATH.read_text(encoding="utf-8"))
    if payload.get("schema_version") != 1 or not isinstance(payload.get("packages"), dict):
        raise ValueError(f"非法 crate 层次登记: {REGISTRY_PATH}")
    packages = payload["packages"]
    for name, entry in packages.items():
        if not isinstance(entry, dict) or entry.get("layer") not in LAYERS:
            raise ValueError(f"crate 层次登记缺少有效 layer: {name}")
        paths = entry.get("paths")
        if not isinstance(paths, list) or not paths or any(
            not isinstance(path, str) or Path(path).is_absolute() or ".." in Path(path).parts for path in paths
        ):
            raise ValueError(f"crate 层次登记缺少有效相对路径: {name}")
        if entry["layer"] == "orchestration":
            dependencies = entry.get("dependencies")
            if not isinstance(dependencies, list) or any(not isinstance(dep, str) for dep in dependencies):
                raise ValueError(f"编排 crate 缺少显式直接依赖清单: {name}")
    return packages


@functools.lru_cache(maxsize=4096)
def _resolve_dir(path: Path) -> Path:
    return path.resolve()


def member_directories(backend: Path, metadata: Mapping[str, Any]) -> tuple[dict[str, Path], list[str]]:
    """从 Cargo metadata 定位活动 crate 并报告未登记、路径不符或缺失源码的成员，供领域门禁确定可扫描的源码目录。"""
    registry = load_registry()
    backend_resolved = _resolve_dir(backend)
    members = set(metadata.get("workspace_members") or [])
    directories: dict[str, Path] = {}
    errors: list[str] = []
    for package in metadata.get("packages") or []:
        if package.get("id") not in members:
            continue
        name = package["name"]
        contract = registry.get(name)
        if contract is None:
            errors.append(f"活动 crate 未登记层次: {name}")
            continue
        manifest = package.get("manifest_path")
        if not manifest:
            errors.append(f"活动 crate 缺少 manifest_path: {name}")
            continue
        directory = _resolve_dir(Path(manifest)).parent
        allowed = {_resolve_dir(backend_resolved / path) for path in contract["paths"]}
        if directory not in allowed:
            errors.append(f"crate 路径与登记层次 {contract['layer']} 不符: {name}: {directory}")
        if not (directory / "src").is_dir():
            errors.append(f"活动 crate 缺失源码目录: {name}: {directory / 'src'}")
        directories[name] = directory
    return directories, errors


def dependency_errors(edges: Any) -> list[str]:
    """按 crate 层次和编排直接依赖登记检查 normal/build/dev 边，供领域门禁报告违反分层的依赖。"""
    registry = load_registry()
    errors: list[str] = []
    upper_layers = {"domains", "orchestration", "read-models", "adapters", "apps"}
    for edge in edges:
        src, dst, kind = edge.from_package, edge.to_package, edge.kind
        source = registry.get(src)
        target = registry.get(dst)
        if source is None:
            continue
        layer = source["layer"]
        target_layer = target["layer"] if target else None
        label = f"{src} -[{kind}{f', rename={edge.rename}' if edge.rename else ''}]-> {dst}"
        if src == "erp-permissions" and not (
            (dst in {"serde", "thiserror"} and kind == "normal")
            or (dst == "serde_json" and kind == "dev")
        ):
            errors.append(f"公共权限定义仅允许纯值及序列化依赖: {label}")
        if layer == "orchestration":
            if dst not in source.get("dependencies", []):
                errors.append(f"编排直接依赖未登记: {label}")
            if target_layer in {"adapters", "apps"}:
                errors.append(f"编排不得依赖具体适配器或应用: {label}")
            if src != "erp-processes" and dst == "erp-processes":
                errors.append(f"新编排不得依赖旧 erp-processes: {label}")
        if layer == "domains" and target_layer in {"orchestration", "read-models", "adapters", "apps"}:
            errors.append(f"领域不得依赖编排、读模型、适配器或应用: {label}")
        if layer == "read-models" and target_layer in {"orchestration", "adapters", "apps"}:
            errors.append(f"读模型不得依赖编排、适配器或应用: {label}")
        if layer == "adapters" and target_layer in {"orchestration", "read-models", "apps"}:
            errors.append(f"适配器不得依赖编排、读模型或应用: {label}")
        if layer in {"foundation", "platform"} and target_layer in upper_layers:
            errors.append(f"基础或技术 crate 不得依赖 ERP 上层: {label}")
        if target_layer == "testing" and kind != "dev":
            errors.append(f"测试辅助仅允许 dev-dependency: {label}")
        if dst == "erp-demo-data" and layer != "apps":
            errors.append(f"演示数据仅允许由应用入口按需调用: {label}")
        if src == "erp-server" and dst == "cli":
            errors.append(f"erp-server 不得依赖 cli: {label}")
    return errors
