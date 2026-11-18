#!/usr/bin/env python3
"""Check domain-crate Cargo graphs and production-source import/query boundaries.

The checker loads the real `cargo metadata` graph (normal/build/dev, renamed
dependencies, workspace inheritance) and combines it with a lexical scan of
Rust `use` / `pub use` trees. Each rule has on-disk positive and negative
fixtures that must pass before the workspace is judged. Absence of domain
crates is reported explicitly and is not treated as proven isolation.
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from collections import defaultdict
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Iterable, Mapping, Sequence

from cutover_workspace import (
    active_source_paths, legacy_graph_errors, legacy_source_errors,
    legacy_tree_errors, workspace_errors,
)
from crate_layers import dependency_errors as layer_dependency_errors, load_registry, member_directories


FOUNDATION = {"erp-core", "erp-permissions", "application-core", "persistence-core"}
ORCHESTRATION = {name for name, entry in load_registry().items() if entry["layer"] == "orchestration"}
COMPOSITION = ORCHESTRATION | {"erp-read-models"}
CONTRACT_RECOGNITION = "erp-contract-recognition"
BUSINESS_DOMAINS = {
    "erp-identity",
    "erp-audit",
    "erp-notification",
    "erp-workflow",
    "erp-support",
    "erp-party",
    "erp-customer",
    "erp-supplier",
    "erp-catalog",
    "erp-warehouse",
    "erp-contract",
    "erp-sales",
    "erp-procurement",
    "erp-fulfillment",
    "erp-inventory",
    "erp-finance",
    "erp-returns",
    "erp-import",
    "erp-integration",
    "erp-supply",
    "erp-commerce",
}
OLD_BUSINESS = {"entities", "database", "services"}
BPM_FORBIDDEN_PACKAGES = {
    "rig-core",
    "rig-agent",
    CONTRACT_RECOGNITION,
    "aliyun-ocr",
    "entities",
    "database",
    "services",
    "erp-server",
    "config",
    "mongodb",
    "axum",
    "id-generator",
}
PLANNED_DOMAIN_PACKAGES = FOUNDATION | COMPOSITION | BUSINESS_DOMAINS
KINDS = ("normal", "build", "dev")
CUTOVER_REQUIRED_PACKAGES = (
    FOUNDATION | {"erp-read-models"} | (BUSINESS_DOMAINS - {"erp-commerce"})
    | {"bpm", "cli", "erp-server", "config", "entity-core", "entity-macros",
       "storage", "aliyun-ocr", CONTRACT_RECOGNITION, "id-generator", "test-support",
       "background-runtime", "erp-connector-jd-vop", "erp-connector-dangaoshushu",
       "erp-master-data", "erp-data-import", "erp-supplier-catalog", "erp-supplier-orders",
       "erp-order-to-cash", "erp-procure-to-pay", "erp-fulfillment-execution",
       "erp-finance-posting", "erp-reverse-flow", "erp-supply-settlement", "erp-demo-data",
       "erp-approval-runtime", "erp-file-assets"}
)


SERVICE_MONGO_RULES = {
    "mongodb_bson": re.compile(r"mongodb\s*::\s*bson|\bbson\s*::\s*(?:doc|Document|Bson|\{)"),
    "mongodb_options": re.compile(r"mongodb\s*::\s*options|\boptions\s*::\s*(?:Find|Update|Replace|Delete|Aggregate)"),
    "doc_macro": re.compile(r"\bdoc!\s*[({]"),
    "raw_document": re.compile(r"\bDocument\b"),
    "raw_bson": re.compile(r"\bBson\b"),
    "find_one": re.compile(r"\.find_one\s*\("),
    "find_many": re.compile(r"\.find_many\s*\("),
    "find_many_sorted": re.compile(r"\.find_many_sorted\s*\("),
    "find_one_by_field": re.compile(r"\.find_one_by_field\s*\("),
    "exists": re.compile(r"\.exists\s*\("),
    "raw_collection": re.compile(r"\.collection(?:_with_type)?\s*(?:::<[^>]+>)?\s*\("),
    "aggregate": re.compile(r"\.aggregate\s*\("),
    "count_documents": re.compile(r"\.count_documents\s*\("),
    "distinct": re.compile(r"\.distinct\s*\("),
    "raw_write": re.compile(
        r"\.(?:insert_one|insert_many|update_one|update_many|replace_one|delete_one|delete_many|bulk_write)\s*\("
    ),
    "run_command": re.compile(r"\.run_command\s*\("),
    "mongo_ops": re.compile(r"\b(?:database::)?mongo_ops\b"),
}

SERVICE_MONGO_RULE_HINTS: dict[str, tuple[str, ...]] = {
    "mongodb_bson": ("mongodb", "bson"),
    "mongodb_options": ("mongodb", "options"),
    "doc_macro": ("doc!",),
    "raw_document": ("Document",),
    "raw_bson": ("Bson",),
    "find_one": ("find_one",),
    "find_many": ("find_many",),
    "find_many_sorted": ("find_many_sorted",),
    "find_one_by_field": ("find_one_by_field",),
    "exists": ("exists",),
    "raw_collection": ("collection",),
    "aggregate": ("aggregate",),
    "count_documents": ("count_documents",),
    "distinct": ("distinct",),
    "raw_write": (
        "insert_one",
        "insert_many",
        "update_one",
        "update_many",
        "replace_one",
        "delete_one",
        "delete_many",
        "bulk_write",
    ),
    "run_command": ("run_command",),
    "mongo_ops": ("mongo_ops",),
}

ENTITY_FORBIDDEN = re.compile(
    r"\b(?:axum|mongodb)\s*::|\bRepository\s*<|\btrait\s+Executor\b|\bNoTransaction\b"
)
ENTITY_BSON = re.compile(r"\bbson\s*::")
BPM_FORBIDDEN_RE = re.compile(
    r"DocumentType|WorkItem|DataScope|Permission|Executor|mongodb|axum|id_generator|"
    r"next_id\(|Local::now|Utc::now|SystemTime::now|Instant::now|BaseModel::new|"
    r"Uuid::new|uuid::Uuid::new|nanoid|IdGenerator"
)
COLLECTION_LITERAL = re.compile(
    r"const\s+([A-Z][A-Z0-9_]*)\s*:\s*&'static\s+str\s*=\s*\"([^\"]+)\""
)
INDEX_CALL = re.compile(
    r"""(?P<fn>unique_index|named_index)\s*\(\s*"(?P<name>[^"]+)"\s*,\s*doc!\s*\{(?P<keys>.*?)\}""",
    re.S,
)
USE_START = re.compile(r"use\s+")


@dataclass
class DepEdge:
    """One declared Cargo dependency edge."""

    from_package: str
    to_package: str
    kind: str
    rename: str | None = None


@dataclass
class Graph:
    """Workspace package graph used by forbidden-edge rules."""

    packages: set[str] = field(default_factory=set)
    edges: list[DepEdge] = field(default_factory=list)


@dataclass
class CheckResult:
    """Errors and informational notes for one rule or the full run."""

    errors: list[str] = field(default_factory=list)
    notes: list[str] = field(default_factory=list)

    def extend(self, other: CheckResult) -> None:
        """合并子检查的错误与说明，供领域边界门禁汇总各项结果而不丢失上下文。"""
        self.errors.extend(other.errors)
        self.notes.extend(other.notes)


# 将 Cargo 包名转换成 Rust crate 标识符，供源码门禁把 `use` 的根标识符对回依赖包名。
def crate_ident(name: str) -> str:
    return name.replace("-", "_")


# 通过已知映射将 Rust crate 标识符解析为 Cargo 包名；未知项返回 `None`，不猜测归属。
def ident_to_package(ident: str, known: Mapping[str, str]) -> str | None:
    return known.get(ident)


COMMENT_OR_STR_RE = re.compile(r'"(?:\\.|[^"\\])*"|//[^\n]*|/\*[\s\S]*?\*/')


def _mask_comment_match(match: re.Match[str]) -> str:
    token = match.group(0)
    if token[0] == '"':
        return token
    if "\n" not in token:
        return " " * len(token)
    return "\n".join(" " * len(line) for line in token.split("\n"))


# 以空格屏蔽 Rust 注释并保留行号与字符串，避免导入和实体边界扫描把注释误判为代码。
def strip_comments(source: str) -> str:
    if "//" not in source and "/*" not in source:
        return source
    return COMMENT_OR_STR_RE.sub(_mask_comment_match, source)


# 递归拆开分组 `use` 并去除别名，供导入扫描按原始路径判定 crate 依赖方向。
def _parse_use_tree(text: str, prefix: str, paths: list[str]) -> None:
    text = text.strip()
    if not text:
        return
    if text.startswith("{"):
        depth = 0
        start = 1
        i = 1
        while i < len(text):
            ch = text[i]
            if ch == "{":
                depth += 1
            elif ch == "}":
                if depth == 0:
                    _parse_use_tree(text[start:i], prefix, paths)
                    return
                depth -= 1
            elif ch == "," and depth == 0:
                _parse_use_tree(text[start:i], prefix, paths)
                start = i + 1
            i += 1
        return
    if "{" not in text and " as " in text:
        text = text.split(" as ", 1)[0].strip()
    if "::" in text:
        head, tail = text.split("::", 1)
        new_prefix = f"{prefix}::{head}" if prefix else head
        if tail.startswith("{"):
            _parse_use_tree(tail, new_prefix, paths)
        else:
            _parse_use_tree(tail, new_prefix, paths)
        return
    if text in ("self", "*"):
        if prefix:
            paths.append(prefix)
        return
    paths.append(f"{prefix}::{text}" if prefix else text)


# 提取 `use` 和 `pub use` 引用的路径，含分组与别名写法，供门禁识别源码中的跨域及旧三层依赖。
def extract_use_paths(source: str) -> list[str]:
    if "use" not in source:
        return []
    code = strip_comments(source)
    paths: list[str] = []
    i = 0
    length = len(code)
    while True:
        match = USE_START.search(code, i)
        if not match:
            break
        start = match.end()
        semi = code.find(";", start)
        end_bound = semi if semi >= 0 else length
        if code.find("{", start, end_bound) < 0 and code.find("}", start, end_bound) < 0:
            j = end_bound
        else:
            j = start
            depth = 0
            while j < length:
                ch = code[j]
                if ch == "{":
                    depth += 1
                elif ch == "}":
                    depth -= 1
                elif ch == ";" and depth <= 0:
                    break
                j += 1
        _parse_use_tree(code[start:j], "", paths)
        i = j + 1
    return paths


# 取导入路径的首段供包名映射使用；忽略 `self`、`super`、`crate`，不把本地模块当成外部依赖。
def root_crate(path: str) -> str | None:
    root = path.split("::", 1)[0]
    if root in {"self", "super", "crate"}:
        return None
    return root


def graph_from_packages(payload: Mapping[str, Any]) -> Graph:
    """把简化包清单转换为保留依赖类型及别名的 `Graph`，让规则自检与工作区检查共用同一图模型。"""
    graph = Graph()
    for package in payload.get("packages") or []:
        name = package["name"]
        graph.packages.add(name)
        for dep in package.get("deps") or []:
            graph.edges.append(
                DepEdge(
                    from_package=name,
                    to_package=dep["name"],
                    kind=dep.get("kind") or "normal",
                    rename=dep.get("rename"),
                )
            )
    return graph


def graph_from_metadata(metadata: Mapping[str, Any]) -> Graph:
    """从 Cargo metadata 建立活动成员的依赖图，保留 normal/build/dev 声明，供分层与跨域检查使用。"""
    members = set(metadata.get("workspace_members") or [])
    graph = Graph()
    id_to_name = {}
    for package in metadata.get("packages") or []:
        id_to_name[package["id"]] = package["name"]
        if package["id"] not in members:
            continue
        graph.packages.add(package["name"])
        for dep in package.get("dependencies") or []:
            kind = dep.get("kind") or "normal"
            if kind not in KINDS and kind is not None:
                continue
            graph.edges.append(
                DepEdge(
                    from_package=package["name"],
                    to_package=dep["name"],
                    kind=kind or "normal",
                    rename=dep.get("rename"),
                )
            )
    return graph


# 统一依赖图和源码导入的跨域规则：普通领域间不得直接依赖，其他领域可依赖身份域。
def forbidden_domain_dependency(source: str, target: str | None) -> bool:
    return source in BUSINESS_DOMAINS and target in BUSINESS_DOMAINS and source != target and target != "erp-identity"


def forbidden_graph_errors(graph: Graph) -> list[str]:
    """核对实际依赖边是否违反 crate 分层、业务域隔离及 BPM 纯度，供门禁报告越界依赖和相关回边。"""
    errors: list[str] = layer_dependency_errors(graph.edges)
    present_domains = sorted(graph.packages & BUSINESS_DOMAINS)
    present_foundations = sorted(graph.packages & FOUNDATION)
    for edge in graph.edges:
        src, dst, kind = edge.from_package, edge.to_package, edge.kind
        label = f"{src} -[{kind}{f', rename={edge.rename}' if edge.rename else ''}]-> {dst}"
        if src == "erp-processes" or dst == "erp-processes":
            errors.append(f"退休 erp-processes 不得进入任何活动依赖图: {label}")
        if src in FOUNDATION | BUSINESS_DOMAINS and dst == "aliyun-ocr":
            errors.append(f"领域和基础 crate 必须通过 Port 使用 OCR，不能依赖供应商客户端: {label}")
        if src in FOUNDATION | BUSINESS_DOMAINS and dst in {"rig-core", "rig-agent"}:
            errors.append(f"领域和基础 crate 必须通过 Port 使用 AI，不能依赖供应商 SDK: {label}")
        if src in FOUNDATION | BUSINESS_DOMAINS | COMPOSITION and dst == CONTRACT_RECOGNITION:
            errors.append(f"合同识别适配器必须由应用装配，领域和流程仅持有 Port: {label}")
        if src == CONTRACT_RECOGNITION and dst in (
            FOUNDATION | (BUSINESS_DOMAINS - {"erp-contract"}) | COMPOSITION
            | {"config", "erp-server", "cli", "mongodb", "storage", "test-support"}
        ):
            errors.append(f"合同识别适配器只可依赖合同 Port 和技术客户端: {label}")
        if src == "aliyun-ocr" and dst in FOUNDATION | BUSINESS_DOMAINS | COMPOSITION | {CONTRACT_RECOGNITION, "config", "erp-server", "cli"}:
            errors.append(f"OCR 技术客户端不得依赖业务或应用组合层: {label}")
        if src in FOUNDATION | BUSINESS_DOMAINS and dst in OLD_BUSINESS:
            errors.append(f"新领域/基础 crate 不得依赖旧三层: {label}")
        if forbidden_domain_dependency(src, dst):
            errors.append(f"普通领域 crate 之间禁止直接依赖: {label}")
        if src in FOUNDATION and dst in BUSINESS_DOMAINS | OLD_BUSINESS | COMPOSITION:
            errors.append(f"基础 crate 不得依赖业务/组合层: {label}")
        if src == "bpm" and dst in BPM_FORBIDDEN_PACKAGES | BUSINESS_DOMAINS | COMPOSITION:
            errors.append(f"BPM 不得依赖 ERP 业务或 I/O crate: {label}")
        if src in BUSINESS_DOMAINS and dst == "test-support":
            errors.append(f"新领域不得依赖共享 test-support 夹具: {label}")
        if src in OLD_BUSINESS and dst in COMPOSITION:
            errors.append(f"旧 services 不得依赖 erp-processes/erp-read-models: {label}")
    if present_domains or present_foundations:
        errors.extend(_cycle_notes(graph, present_domains + present_foundations))
    return errors


# 检查指定包集合内部的有向依赖环，供分层门禁核对领域和基础层，并在最终切换时覆盖全部活动成员。
def _cycle_notes(graph: Graph, interesting: Sequence[str]) -> list[str]:
    adj: dict[str, list[str]] = defaultdict(list)
    interesting_set = set(interesting)
    for edge in graph.edges:
        if edge.from_package in interesting_set and edge.to_package in interesting_set:
            adj[edge.from_package].append(edge.to_package)
    visiting: set[str] = set()
    seen: set[str] = set()
    errors: list[str] = []

    def walk(node: str, stack: list[str]) -> None:
        if node in visiting:
            cycle = stack[stack.index(node) :] + [node]
            errors.append("领域/基础依赖回边: " + " -> ".join(cycle))
            return
        if node in seen:
            return
        visiting.add(node)
        stack.append(node)
        for nxt in adj.get(node, []):
            walk(nxt, stack)
        stack.pop()
        visiting.remove(node)
        seen.add(node)

    for name in interesting:
        walk(name, [])
    return errors


# 检查实体是否引入 HTTP、MongoDB 或仓储执行接口，避免领域实体承担协议适配或持久化访问。
def scan_entity_source(
    path: str,
    source: str,
    *,
    money_exception: bool,
    use_paths: Sequence[str] | None = None,
) -> list[str]:
    errors: list[str] = []
    if any(k in source for k in ("axum", "mongodb", "Repository", "Executor", "NoTransaction", "bson")):
        code = strip_comments(source)
        if ENTITY_FORBIDDEN.search(code):
            errors.append(f"entity 引用了 Repository/HTTP/Mongo/Executor: {path}")
        if ENTITY_BSON.search(code) and not money_exception:
            errors.append(f"entity 引用了 BSON（仅 erp-core 金额编码允许）: {path}")
    for used in (use_paths if use_paths is not None else extract_use_paths(source)):
        root = root_crate(used)
        if root in {"axum", "mongodb", "database", "services", "web_api"}:
            errors.append(f"entity 通过 use 引入禁止 crate {used}: {path}")
    return errors


def scan_service_mongo(path: str, source: str) -> list[str]:
    """检查 Service 或 Process 中的原始 MongoDB/BSON 操作，避免应用用例和编排绕过领域仓储。"""
    errors: list[str] = []
    # An inner cfg(test) applies to the entire module; its BSON is unit-test data.
    if re.match(r"\s*#!\s*\[\s*cfg\s*\(\s*test\s*\)\s*\]", strip_comments(source)):
        return errors
    has_doc = "Document" in source
    # XML documents are technical input, not Mongo BSON. Only discard the
    # explicitly aliased XML import; Mongo imports and bare Document stay checked.
    if has_doc and "roxmltree" in source:
        document_source = re.sub(
            r"\buse\s+roxmltree::Document\s+as\s+[A-Za-z_][A-Za-z0-9_]*\s*;",
            "",
            source,
        )
        document_source = re.sub(
            r"\buse\s+roxmltree::\{[^;]*\};",
            lambda xml_import: re.sub(
                r"\bDocument\s+as\s+[A-Za-z_][A-Za-z0-9_]*", "", xml_import.group()
            ),
            document_source,
        )
    else:
        document_source = source
    for name, pattern in SERVICE_MONGO_RULES.items():
        target = document_source if name == "raw_document" else source
        hints = SERVICE_MONGO_RULE_HINTS[name]
        if any(h in target for h in hints) and pattern.search(target):
            errors.append(f"Service/Process 原始 Mongo 操作 {name}: {path}")
    return errors


# 检查指向旧三层的重导出和 `#[path]`，避免迁移后仍能通过兼容门面引用已退役实现。
def scan_facade(path: str, source: str) -> list[str]:
    if "entities" not in source and "database" not in source and "services" not in source:
        return []
    errors: list[str] = []
    pub_uses = re.findall(
        r"pub\s+use\s+((?:entities|database|services)::[^;]+);", strip_comments(source)
    )
    for item in pub_uses:
        errors.append(f"禁止旧路径 façade `pub use {item}`: {path}")
    if re.search(r"#\[path\s*=\s*\"[^\"]*(?:entities|database|services)/", source):
        errors.append(f"禁止 #[path] 指向旧三层: {path}")
    return errors


# 检查 BPM 源码是否引用 ERP、I/O、时钟或 ID 生成符号，防止通用流程计算依赖业务或外部状态。
def scan_bpm_source(path: str, source: str) -> list[str]:
    if BPM_FORBIDDEN_RE.search(source):
        return [f"BPM 源码包含禁止的 ERP/I-O/时钟/ID 生成符号: {path}"]
    return []


# 按本域与外域的集合清单检查仓储源码，阻断借集合名字面量绕过领域所有权的访问。
def scan_foreign_collections(path: str, source: str, own: set[str], foreign: set[str]) -> list[str]:
    errors: list[str] = []
    for match in re.finditer(r'"([a-z][a-z0-9_]*)"', source):
        name = match.group(1)
        if name in foreign and name not in own:
            errors.append(f"领域仓储访问外域集合 {name}: {path}")
    return errors


# 识别 `repository/` 与 `repository.rs`，供 Service/Process 门禁排除实际拥有持久化职责的仓储模块。
def is_repository_source(path: str) -> bool:
    normalized = "/" + path.replace("\\", "/").strip("/") + "/"
    return "/repository/" in normalized or Path(path).name == "repository.rs"


# 收集指定仓储目录声明的集合名字面量，作为核对 crate 集合所有权的源码依据。
def crate_collection_literals(repo_root: Path) -> set[str]:
    owned: set[str] = set()
    if not repo_root.is_dir():
        return owned
    for path in repo_root.rglob("*.rs"):
        for match in COLLECTION_LITERAL.finditer(path.read_text(encoding="utf-8")):
            owned.add(match.group(2))
    return owned


# 定位领域规则夹具目录，使命令行自检不依赖调用时的工作目录。
def load_fixtures(root: Path) -> Path:
    return root / "domain-boundary-fixtures"


# 按 UTF-8 读取 Rust 或 JSON 规则输入，确保自检不受宿主默认编码影响。
def read_text(path: Path) -> str:
    return path.read_text(encoding="utf-8")


# 核对规则的正例能通过、反例能触发阻断，避免门禁规则失效后仍将工作区判为通过。
def run_fixture_suite(fixture_dir: Path) -> CheckResult:
    result = CheckResult()
    if not fixture_dir.is_dir():
        result.errors.append(f"缺失夹具目录: {fixture_dir}")
        return result

    def require_empty(label: str, errors: Sequence[str]) -> None:
        if errors:
            result.errors.append(f"正向夹具不应失败 [{label}]: " + "; ".join(errors))

    def require_hit(label: str, errors: Sequence[str]) -> None:
        if not errors:
            result.errors.append(f"负向夹具必须非零退出 [{label}]")

    graph_pos = graph_from_packages(json.loads(read_text(fixture_dir / "graph_positive.json")))
    require_empty("graph_positive", forbidden_graph_errors(graph_pos))
    for name in (
        "graph_negative_domain_to_old.json",
        "graph_negative_ocr_to_domain.json",
        "graph_negative_domain_to_ocr.json",
        "graph_negative_domain_to_ai.json",
        "graph_negative_domain_to_domain.json",
        "graph_negative_identity_to_domain.json",
        "graph_negative_foundation_to_identity.json",
        "graph_negative_permissions_io.json",
        "graph_negative_platform_to_identity.json",
        "graph_negative_foundation_to_business.json",
        "graph_negative_bpm_io.json",
        "graph_negative_services_to_processes.json",
        "graph_negative_domain_to_recognition.json",
        "graph_negative_process_to_recognition.json",
        "graph_negative_recognition_to_process.json",
        "graph_negative_recognition_to_domain.json",
    ):
        graph = graph_from_packages(json.loads(read_text(fixture_dir / name)))
        require_hit(name, forbidden_graph_errors(graph))

    if forbidden_domain_dependency("erp-workflow", "erp-identity"):
        result.errors.append("领域到 identity 的源码依赖应允许")
    if not forbidden_domain_dependency("erp-identity", "erp-workflow"):
        result.errors.append("identity 反向源码依赖必须拒绝")
    if not forbidden_domain_dependency("erp-sales", "erp-procurement"):
        result.errors.append("其他领域之间源码依赖必须拒绝")

    entity_pos = read_text(fixture_dir / "entity_positive.rs")
    require_empty("entity_positive", scan_entity_source("entity_positive.rs", entity_pos, money_exception=False))
    money_pos = read_text(fixture_dir / "entity_money_positive.rs")
    require_empty(
        "entity_money_positive",
        scan_entity_source("entity_money_positive.rs", money_pos, money_exception=True),
    )
    require_hit(
        "entity_negative_bson.rs",
        scan_entity_source(
            "entity_negative_bson.rs",
            read_text(fixture_dir / "entity_negative_bson.rs"),
            money_exception=False,
        ),
    )
    require_hit(
        "entity_negative_http.rs",
        scan_entity_source(
            "entity_negative_http.rs",
            read_text(fixture_dir / "entity_negative_http.rs"),
            money_exception=False,
        ),
    )

    require_empty(
        "service_positive.rs",
        scan_service_mongo("service_positive.rs", read_text(fixture_dir / "service_positive.rs")),
    )
    xml_document = "use roxmltree::Document as XmlDocument; fn parse() -> XmlDocument { todo!() }"
    require_empty("service_xml_document", scan_service_mongo("service_xml_document.rs", xml_document))
    require_empty("service_test_module", scan_service_mongo("service/tests/behavior.rs", "#![cfg(test)]\nuse mongodb::bson::doc;"))
    grouped_xml = "use roxmltree::{Document as XmlDocument, ParsingOptions}; fn parse() -> XmlDocument { todo!() }"
    require_empty("service_grouped_xml", scan_service_mongo("service_xml_document.rs", grouped_xml))
    require_hit(
        "service_xml_document_with_bson",
        scan_service_mongo("service_xml_document.rs", xml_document + " fn filter() -> Document { todo!() }"),
    )
    require_hit(
        "service_negative_mongo.rs",
        scan_service_mongo(
            "service_negative_mongo.rs", read_text(fixture_dir / "service_negative_mongo.rs")
        ),
    )

    service_cases = json.loads(read_text(fixture_dir / "service_seven_rules.json"))
    for rule, sample in service_cases.items():
        if not SERVICE_MONGO_RULES[rule].search(sample):
            result.errors.append(f"新增 Service 规则负例未命中: {rule}")
        require_hit(rule, scan_service_mongo(f"service/{rule}.rs", sample))

    grouped = read_text(fixture_dir / "source_grouped_use_negative.rs")
    paths = extract_use_paths(grouped)
    required_paths = {
        "erp_sales::entity::SalesOrder",
        "erp_sales::dto::SubmitSalesOrderRequest",
        "erp_sales::dto::Line",
        "erp_finance::entity::Invoice",
        "erp_finance::service::ReceivableService",
    }
    if not required_paths <= set(paths):
        result.errors.append(f"grouped use 夹具未展开: {paths}")
    alias = extract_use_paths(read_text(fixture_dir / "source_alias_use_negative.rs"))
    if "erp_sales::service::SalesOrderService" not in alias:
        result.errors.append(f"别名 use 夹具未解析: {alias}")
    require_hit(
        "source_pub_use_facade_negative.rs",
        scan_facade(
            "source_pub_use_facade_negative.rs",
            read_text(fixture_dir / "source_pub_use_facade_negative.rs"),
        ),
    )

    require_empty("bpm_positive.rs", scan_bpm_source("bpm_positive.rs", read_text(fixture_dir / "bpm_positive.rs")))
    require_hit(
        "bpm_negative_io.rs",
        scan_bpm_source("bpm_negative_io.rs", read_text(fixture_dir / "bpm_negative_io.rs")),
    )

    require_empty(
        "repo_positive.rs",
        scan_foreign_collections(
            "repo_positive.rs",
            read_text(fixture_dir / "repo_positive.rs"),
            {"customer_accounts"},
            {"sales_orders"},
        ),
    )
    if not is_repository_source("crates/erp-read-models/src/fulfillment_queue/repository.rs"):
        result.errors.append("repository.rs 文件应视为仓储源")
    if is_repository_source("crates/erp-workflow/src/service/approval/scope.rs"):
        result.errors.append("service 源不应视为仓储源")
    require_hit(
        "repo_negative_foreign_collection.rs",
        scan_foreign_collections(
            "repo_negative_foreign_collection.rs",
            read_text(fixture_dir / "repo_negative_foreign_collection.rs"),
            {"customer_accounts"},
            {"sales_orders"},
        ),
    )

    import tempfile
    clearance = json.loads(read_text(fixture_dir / "clearance_negative.json"))
    with tempfile.TemporaryDirectory(prefix="domain-clearance-fixture-") as temporary:
        fixture_backend = Path(temporary)
        leftover = fixture_backend / clearance["old_source"]
        leftover.parent.mkdir(parents=True, exist_ok=True)
        leftover.write_text("pub struct ResidualEntity;\n")
        require_hit("clearance_negative", legacy_tree_errors(fixture_backend))
    return result


def cargo_metadata(backend: Path) -> dict[str, Any]:
    """读取锁定且启用全部 feature 的 Cargo metadata（含解析图），供领域及其他源码门禁共用实际活动成员和依赖。"""
    proc = subprocess.run(
        [
            "cargo",
            "metadata",
            "--format-version",
            "1",
            "--all-features",
            "--locked",
            "--manifest-path",
            str(backend / "Cargo.toml"),
        ],
        cwd=str(backend),
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if proc.returncode != 0:
        raise RuntimeError(f"cargo metadata 失败: {proc.stderr[-4000:]}")
    return json.loads(proc.stdout)


# 读取迁移阶段计划，供过渡期门禁仅对已验收阶段核销旧源码。
def load_plan_manifest(backend: Path) -> dict[str, Any]:
    path = (
        backend
        / "docs/superpowers/plans/domain-crate-migration/plan-manifest.json"
    )
    return json.loads(path.read_text(encoding="utf-8"))


# 读取迁移源路径映射，供过渡期门禁定位已验收阶段移动后不应残留的旧文件。
def load_source_map(backend: Path) -> list[dict[str, str]]:
    path = backend / "docs/superpowers/plans/domain-crate-migration/source-map.tsv"
    rows: list[dict[str, str]] = []
    lines = path.read_text(encoding="utf-8").splitlines()
    header = lines[0].split("\t")
    for line in lines[1:]:
        parts = line.split("\t")
        rows.append(dict(zip(header, parts)))
    return rows


# 从旧仓储的 `extensions/<domain>.rs` 提取集合归属，供门禁识别尚未迁移的外域集合。
def collection_ownership(backend: Path) -> dict[str, str]:
    owned: dict[str, str] = {}
    ext_dir = backend / "database/src/repository/extensions"
    if not ext_dir.is_dir():
        return owned
    for path in sorted(ext_dir.glob("*.rs")):
        if path.name == "mod.rs":
            continue
        domain = path.stem
        for match in COLLECTION_LITERAL.finditer(path.read_text(encoding="utf-8")):
            owned[match.group(2)] = domain
    return owned


# 遍历指定根下的 Rust 源码并跳过其 `tests` 归档，供生产边界检查限定扫描范围。
def iter_rs(root: Path) -> Iterable[Path]:
    if not root.is_dir():
        return
    for path in root.rglob("*.rs"):
        parts = set(path.parts)
        if "tests" in parts and f"{root.name}/tests" in path.as_posix():
            continue
        yield path


def check_workspace(backend: Path, metadata: Mapping[str, Any], *, cutover: bool = False) -> CheckResult:
    """基于实际活动成员执行领域依赖与源码检查；最终切换模式额外拒绝残留旧包、目录和引用。"""
    backend = backend.resolve()
    result = CheckResult()
    # Once old members are absent, the normal CI entry must not silently retain
    # migration-era composition exemptions. --cutover also catches partial cuts.
    members = set(metadata.get("workspace_members") or [])
    old_members = {package.get("name") for package in metadata.get("packages") or []
                   if package.get("id") in members} & OLD_BUSINESS
    cutover = cutover or not old_members
    result.errors.extend(workspace_errors(
        backend, metadata, CUTOVER_REQUIRED_PACKAGES if cutover else set()
    ))
    directories, directory_errors = member_directories(backend, metadata)
    result.errors.extend(directory_errors)
    if result.errors:
        return result
    source_cache: dict[Path, str] = {}

    def read_cached(path: Path) -> str:
        text = source_cache.get(path)
        if text is None:
            text = path.read_text(encoding="utf-8")
            source_cache[path] = text
        return text

    if cutover:
        result.errors.extend(legacy_graph_errors(metadata))
        result.errors.extend(legacy_tree_errors(backend))
        active_paths = active_source_paths(metadata)
        for path in active_paths:
            result.errors.extend(legacy_source_errors(path, read_cached(path)))
        result.notes.append("最终切换模式：所有活动成员/依赖类别/旧源路径均执行零 legacy 检查")
    else:
        active_paths = []
    graph = graph_from_metadata(metadata)
    result.errors.extend(forbidden_graph_errors(graph))
    if cutover:
        result.errors.extend(_cycle_notes(graph, sorted(graph.packages)))
    present_planned = sorted(graph.packages & PLANNED_DOMAIN_PACKAGES)
    if not present_planned:
        result.notes.append(
            "planned domain crates not present; forbidden-edge contract loaded; "
            "current workspace has no instantiated domain isolation to certify"
        )
    else:
        result.notes.append("instantiated planned crates: " + ", ".join(present_planned))

    members = set(metadata.get("workspace_members") or [])
    test_targets: list[str] = []
    ident_map = {crate_ident(pkg["name"]): pkg["name"] for pkg in metadata.get("packages") or []}
    for package in metadata.get("packages") or []:
        if package["id"] not in members:
            continue
        for target in package.get("targets") or []:
            if "test" in (target.get("kind") or []):
                test_targets.append(f"{package['name']}::{target.get('name')}")
    if test_targets:
        result.errors.append(
            "workspace 仍注册 kind=test 集成目标（autotests=false 未生效）: "
            + ", ".join(test_targets[:20])
        )
    else:
        result.notes.append("cargo metadata 无 kind=test 集成目标")

    paths_by_src: dict[Path, list[Path]] = defaultdict(list)
    if active_paths:
        src_roots = {directories[name] / "src" for name in present_planned}
        if "bpm" in directories:
            src_roots.add(directories["bpm"] / "src")
        for path in active_paths:
            for parent in path.parents:
                if parent in src_roots:
                    paths_by_src[parent].append(path)
                    break

    repo_files_by_crate: dict[str, list[tuple[str, str]]] = {}
    for name in present_planned:
        crate_dir = directories[name]
        src = crate_dir / "src"
        repo_files: list[tuple[str, str]] = []
        crate_rs_paths = paths_by_src.get(src) if active_paths else list(src.rglob("*.rs"))
        for path in crate_rs_paths or ():
            rel = path.relative_to(backend).as_posix()
            text = read_cached(path)
            if "/repository/" in f"/{rel}/":
                repo_files.append((rel, text))
            is_entity = "/entity/" in f"/{rel}/" or path.parent.name == "entity" or rel.endswith("/entity.rs")
            need_uses = (
                is_entity
                or name in BUSINESS_DOMAINS
                or (
                    name not in COMPOSITION
                    and ("entities" in text or "database" in text or "services" in text)
                )
            )
            use_paths = extract_use_paths(text) if need_uses else []
            for used in use_paths:
                root = root_crate(used)
                if not root:
                    continue
                package = ident_map.get(root)
                if name not in COMPOSITION and package and package in OLD_BUSINESS:
                    result.errors.append(f"新 crate 源码 use 旧三层 {used}: {rel}")
                if forbidden_domain_dependency(name, package):
                    result.errors.append(f"领域源码直接 use 其他领域 {used}: {rel}")
            if is_entity:
                money_exception = name == "erp-core" and path.name == "money.rs"
                result.errors.extend(
                    scan_entity_source(rel, text, money_exception=money_exception, use_paths=use_paths)
                )
            if ("/service/" in f"/{rel}/" or name in COMPOSITION) and not is_repository_source(rel):
                result.errors.extend(scan_service_mongo(rel, text))
            result.errors.extend(scan_facade(rel, text))
        repo_files_by_crate[name] = repo_files

    bpm_src = directories.get("bpm", backend / "__missing_bpm__") / "src"
    if "bpm" in directories:
        bpm_rs_paths = paths_by_src.get(bpm_src) if active_paths else list(bpm_src.rglob("*.rs"))
        for path in bpm_rs_paths or ():
            rel = path.relative_to(backend).as_posix()
            result.errors.extend(scan_bpm_source(rel, read_cached(path)))

    old_owned = collection_ownership(backend)
    crate_owned: dict[str, set[str]] = {}
    migrated = set()
    for name in present_planned:
        if name not in BUSINESS_DOMAINS:
            continue
        owned_set: set[str] = set()
        for _rel, text in repo_files_by_crate.get(name, ()):
            for match in COLLECTION_LITERAL.finditer(text):
                owned_set.add(match.group(2))
        crate_owned[name] = owned_set
        migrated |= owned_set
    leftover_old = set(old_owned) - migrated
    for name in present_planned:
        if name not in BUSINESS_DOMAINS:
            continue
        own = crate_owned.get(name, set())
        foreign = (migrated | leftover_old) - own
        for rel, text in repo_files_by_crate.get(name, ()):
            result.errors.extend(
                scan_foreign_collections(rel, text, own, foreign)
            )

    if cutover:
        # Final mode already checks every active source and legacy source tree.
        # Phase-specific clearance belongs only to the transitional workspace.
        result.notes.append("最终切换以全量旧源清零为准，不依赖已归档的阶段迁移计划")
        return result

    manifest = load_plan_manifest(backend)
    accepted = {phase["id"] for phase in manifest.get("phases") or [] if phase.get("status") == "已验收"}
    if accepted:
        for row in load_source_map(backend):
            if row.get("phase") not in accepted:
                continue
            op = row.get("operation") or ""
            source = row.get("source") or ""
            if op.startswith("move") and source:
                leftover = backend / source
                if leftover.is_file():
                    result.errors.append(f"已验收阶段仍残留旧源: {source}")
    else:
        result.notes.append("尚无已验收阶段；旧源清零规则已加载，当前不核销业务源文件")

    return result


# 分别呈现规则自检和工作区结果，便于调用方区分门禁失效与业务源码违规。
def format_report(fixtures: CheckResult, workspace: CheckResult) -> str:
    lines = ["领域边界检查"]
    lines.append(f"夹具: errors={len(fixtures.errors)}")
    for note in fixtures.notes:
        lines.append(f"  夹具说明: {note}")
    for error in fixtures.errors:
        lines.append(f"  夹具失败: {error}")
    lines.append(f"工作区: errors={len(workspace.errors)}")
    for note in workspace.notes:
        lines.append(f"  说明: {note}")
    for error in workspace.errors:
        lines.append(f"  失败: {error}")
    if fixtures.errors or workspace.errors:
        lines.append("领域边界检查失败。")
    else:
        lines.append("领域边界检查通过（夹具自检通过；未伪造未实例化领域的隔离证据）。")
    return "\n".join(lines) + "\n"


def main(argv: Sequence[str] | None = None) -> int:
    """先验证领域边界规则；常规运行还核验实际依赖图和源码，并以退出码告知 CI 是否阻断。"""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--backend", type=Path, required=True, help="backend 目录")
    parser.add_argument("--cutover", action="store_true", help="阶段 17：完整 workspace 与零 legacy 强制验收")
    parser.add_argument("--self-test-only", action="store_true", help="只运行临时纯工具夹具，不调用 Cargo")
    args = parser.parse_args(argv)
    backend = args.backend.resolve()
    fixture_dir = load_fixtures(Path(__file__).resolve().parent)
    fixtures = run_fixture_suite(fixture_dir)
    from cutover_tool_selftests import run_pure_self_tests
    if not run_pure_self_tests():
        fixtures.errors.append("最终切换工具纯夹具失败")
    if args.self_test_only:
        for error in fixtures.errors:
            print(error, file=sys.stderr)
        print(f"仅工具自检：errors={len(fixtures.errors)}；未检查实际 workspace，未调用 Cargo")
        return 1 if fixtures.errors else 0
    try:
        metadata = cargo_metadata(backend)
        workspace = check_workspace(backend, metadata, cutover=args.cutover)
    except Exception as error:  # noqa: BLE001 — report as checker failure
        workspace = CheckResult(errors=[str(error)])
    sys.stdout.write(format_report(fixtures, workspace))
    return 1 if fixtures.errors or workspace.errors else 0


if __name__ == "__main__":
    sys.exit(main())
