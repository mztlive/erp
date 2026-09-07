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


FOUNDATION = {"erp-core", "application-core", "persistence-core"}
COMPOSITION = {"erp-processes", "erp-read-models"}
BUSINESS_DOMAINS = {
    "erp-identity",
    "erp-audit",
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
    "entities",
    "database",
    "services",
    "web-api",
    "config",
    "mongodb",
    "axum",
    "id-generator",
    "permission-macros",
}
PLANNED_DOMAIN_PACKAGES = FOUNDATION | COMPOSITION | BUSINESS_DOMAINS
KINDS = ("normal", "build", "dev")
CUTOVER_REQUIRED_PACKAGES = (
    FOUNDATION | COMPOSITION | (BUSINESS_DOMAINS - {"erp-commerce"})
    | {"bpm", "cli", "web-api", "config", "entity-core", "entity-macros",
       "permission-macros", "storage", "id-generator", "test-support"}
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
USE_START = re.compile(r"(?:pub(?:\s*\([^)]+\))?\s+)?use\s+")


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
        """Merge another result into this one."""
        self.errors.extend(other.errors)
        self.notes.extend(other.notes)


def crate_ident(name: str) -> str:
    """Map a Cargo package name to the Rust crate identifier."""
    return name.replace("-", "_")


def ident_to_package(ident: str, known: Mapping[str, str]) -> str | None:
    """Map a Rust crate identifier back to a package name when known."""
    return known.get(ident)


def strip_comments(source: str) -> str:
    """Replace comments with spaces so lexical scans ignore them."""
    out: list[str] = []
    i = 0
    n = len(source)
    while i < n:
        ch = source[i]
        nxt = source[i + 1] if i + 1 < n else ""
        if ch == "/" and nxt == "/":
            while i < n and source[i] != "\n":
                out.append(" ")
                i += 1
            continue
        if ch == "/" and nxt == "*":
            out.append("  ")
            i += 2
            while i < n and not (source[i] == "*" and i + 1 < n and source[i + 1] == "/"):
                out.append("\n" if source[i] == "\n" else " ")
                i += 1
            if i < n:
                out.append("  ")
                i += 2
            continue
        if ch == '"':
            out.append(ch)
            i += 1
            while i < n:
                out.append(source[i])
                if source[i] == "\\" and i + 1 < n:
                    out.append(source[i + 1])
                    i += 2
                    continue
                if source[i] == '"':
                    i += 1
                    break
                i += 1
            continue
        out.append(ch)
        i += 1
    return "".join(out)


def _parse_use_tree(text: str, prefix: str, paths: list[str]) -> None:
    """Parse a `use` tree such as `foo::{bar, baz as Q}` into crate paths."""
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


def extract_use_paths(source: str) -> list[str]:
    """Return crate paths referenced by `use` / `pub use`, including grouped trees."""
    code = strip_comments(source)
    paths: list[str] = []
    i = 0
    while True:
        match = USE_START.search(code, i)
        if not match:
            break
        start = match.end()
        j = start
        depth = 0
        while j < len(code):
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


def root_crate(path: str) -> str | None:
    """Return the leading crate identifier of a use path, ignoring self/super/crate."""
    root = path.split("::", 1)[0]
    if root in {"self", "super", "crate"}:
        return None
    return root


def graph_from_packages(payload: Mapping[str, Any]) -> Graph:
    """Build a Graph from fixture or metadata-like package lists."""
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
    """Build a Graph from `cargo metadata` workspace members."""
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


def forbidden_graph_errors(graph: Graph) -> list[str]:
    """Return forbidden-edge errors for an instantiated package graph."""
    errors: list[str] = []
    present_domains = sorted(graph.packages & BUSINESS_DOMAINS)
    present_foundations = sorted(graph.packages & FOUNDATION)
    for edge in graph.edges:
        src, dst, kind = edge.from_package, edge.to_package, edge.kind
        label = f"{src} -[{kind}{f', rename={edge.rename}' if edge.rename else ''}]-> {dst}"
        if src in FOUNDATION | BUSINESS_DOMAINS and dst in OLD_BUSINESS:
            errors.append(f"新领域/基础 crate 不得依赖旧三层: {label}")
        if src in BUSINESS_DOMAINS and dst in BUSINESS_DOMAINS and src != dst:
            errors.append(f"普通领域 crate 之间禁止直接依赖: {label}")
        if src in FOUNDATION and dst in BUSINESS_DOMAINS | OLD_BUSINESS | COMPOSITION:
            errors.append(f"基础 crate 不得依赖业务/组合层: {label}")
        if src == "bpm" and dst in BPM_FORBIDDEN_PACKAGES:
            errors.append(f"BPM 不得依赖 ERP 业务或 I/O crate: {label}")
        if src in BUSINESS_DOMAINS and dst == "test-support":
            errors.append(f"新领域不得依赖共享 test-support 夹具: {label}")
        if src in OLD_BUSINESS and dst in COMPOSITION:
            errors.append(f"旧 services 不得依赖 erp-processes/erp-read-models: {label}")
    if present_domains or present_foundations:
        errors.extend(_cycle_notes(graph, present_domains + present_foundations))
    return errors


def _cycle_notes(graph: Graph, interesting: Sequence[str]) -> list[str]:
    """Detect directed cycles among foundation/domain packages."""
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


def scan_entity_source(path: str, source: str, *, money_exception: bool) -> list[str]:
    """Flag entity modules that reach HTTP/Mongo/Repository APIs."""
    errors: list[str] = []
    code = strip_comments(source)
    if ENTITY_FORBIDDEN.search(code):
        errors.append(f"entity 引用了 Repository/HTTP/Mongo/Executor: {path}")
    if ENTITY_BSON.search(code) and not money_exception:
        errors.append(f"entity 引用了 BSON（仅 erp-core 金额编码允许）: {path}")
    for used in extract_use_paths(source):
        root = root_crate(used)
        if root in {"axum", "mongodb", "database", "services", "web_api"}:
            errors.append(f"entity 通过 use 引入禁止 crate {used}: {path}")
    return errors


def scan_service_mongo(path: str, source: str) -> list[str]:
    """Flag Service/Process modules that assemble raw Mongo/BSON queries."""
    errors: list[str] = []
    for name, pattern in SERVICE_MONGO_RULES.items():
        if pattern.search(source):
            errors.append(f"Service/Process 原始 Mongo 操作 {name}: {path}")
    return errors


def scan_facade(path: str, source: str) -> list[str]:
    """Flag leftover re-export façades onto the old three-layer crates."""
    errors: list[str] = []
    for used in extract_use_paths(source):
        if re.match(r"^pub ", source) is None:
            break
    pub_uses = re.findall(
        r"pub\s+use\s+((?:entities|database|services)::[^;]+);", strip_comments(source)
    )
    for item in pub_uses:
        errors.append(f"禁止旧路径 façade `pub use {item}`: {path}")
    if re.search(r"#\[path\s*=\s*\"[^\"]*(?:entities|database|services)/", source):
        errors.append(f"禁止 #[path] 指向旧三层: {path}")
    return errors


def scan_bpm_source(path: str, source: str) -> list[str]:
    """Flag BPM production sources that mention ERP/I-O/clock/ID generation."""
    if BPM_FORBIDDEN_RE.search(source):
        return [f"BPM 源码包含禁止的 ERP/I-O/时钟/ID 生成符号: {path}"]
    return []


def scan_foreign_collections(path: str, source: str, own: set[str], foreign: set[str]) -> list[str]:
    """Flag a domain repository that names another domain's collection."""
    errors: list[str] = []
    for match in re.finditer(r'"([a-z][a-z0-9_]*)"', source):
        name = match.group(1)
        if name in foreign and name not in own:
            errors.append(f"领域仓储访问外域集合 {name}: {path}")
    return errors


def is_repository_source(path: str) -> bool:
    """Return whether a source path is a repository module (directory or `repository.rs`)."""
    normalized = "/" + path.replace("\\", "/").strip("/") + "/"
    return "/repository/" in normalized or Path(path).name == "repository.rs"


def crate_collection_literals(repo_root: Path) -> set[str]:
    """Collect collection string literals declared under a crate repository tree."""
    owned: set[str] = set()
    if not repo_root.is_dir():
        return owned
    for path in repo_root.rglob("*.rs"):
        for match in COLLECTION_LITERAL.finditer(path.read_text(encoding="utf-8")):
            owned.add(match.group(2))
    return owned


def load_fixtures(root: Path) -> Path:
    """Return the fixture directory next to this script."""
    return root / "domain-boundary-fixtures"


def read_text(path: Path) -> str:
    """Read a UTF-8 file."""
    return path.read_text(encoding="utf-8")


def run_fixture_suite(fixture_dir: Path) -> CheckResult:
    """Assert every rule has a passing positive fixture and a failing negative fixture."""
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
        "graph_negative_domain_to_domain.json",
        "graph_negative_foundation_to_business.json",
        "graph_negative_bpm_io.json",
        "graph_negative_services_to_processes.json",
    ):
        graph = graph_from_packages(json.loads(read_text(fixture_dir / name)))
        require_hit(name, forbidden_graph_errors(graph))

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
    """Load full cargo metadata including the dependency resolve graph."""
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


def load_plan_manifest(backend: Path) -> dict[str, Any]:
    """Read plan-manifest.json for accepted-phase clearance."""
    path = (
        backend
        / "docs/superpowers/plans/domain-crate-migration/plan-manifest.json"
    )
    return json.loads(path.read_text(encoding="utf-8"))


def load_source_map(backend: Path) -> list[dict[str, str]]:
    """Read source-map.tsv rows."""
    path = backend / "docs/superpowers/plans/domain-crate-migration/source-map.tsv"
    rows: list[dict[str, str]] = []
    lines = path.read_text(encoding="utf-8").splitlines()
    header = lines[0].split("\t")
    for line in lines[1:]:
        parts = line.split("\t")
        rows.append(dict(zip(header, parts)))
    return rows


def collection_ownership(backend: Path) -> dict[str, str]:
    """Map collection string literals to the extensions/<domain>.rs that owns them."""
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


def iter_rs(root: Path) -> Iterable[Path]:
    """Yield production .rs files under root, skipping tests archives."""
    if not root.is_dir():
        return
    for path in root.rglob("*.rs"):
        parts = set(path.parts)
        if "tests" in parts and f"{root.name}/tests" in path.as_posix():
            continue
        yield path


def check_workspace(backend: Path, metadata: Mapping[str, Any], *, cutover: bool = False) -> CheckResult:
    """Apply boundary rules to the current workspace."""
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
    if result.errors:
        return result
    if cutover:
        result.errors.extend(legacy_graph_errors(metadata))
        result.errors.extend(legacy_tree_errors(backend))
        for path in active_source_paths(metadata):
            result.errors.extend(legacy_source_errors(path, path.read_text(encoding="utf-8")))
        result.notes.append("最终切换模式：所有活动成员/依赖类别/旧源路径均执行零 legacy 检查")
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

    crates_root = backend / "crates"
    for name in present_planned:
        crate_dir = crates_root / name
        if not crate_dir.is_dir():
            # application-core / persistence-core / erp-core use those directory names.
            continue
        src = crate_dir / "src"
        if not src.is_dir():
            continue
        for path in src.rglob("*.rs"):
            rel = path.relative_to(backend).as_posix()
            text = path.read_text(encoding="utf-8")
            for used in extract_use_paths(text):
                root = root_crate(used)
                if not root:
                    continue
                package = ident_map.get(root)
                if name not in COMPOSITION and package and package in OLD_BUSINESS:
                    result.errors.append(f"新 crate 源码 use 旧三层 {used}: {rel}")
                if name in BUSINESS_DOMAINS and package in BUSINESS_DOMAINS and package != name:
                    result.errors.append(f"领域源码直接 use 其他领域 {used}: {rel}")
            if "/entity/" in f"/{rel}/" or path.parent.name == "entity" or rel.endswith("/entity.rs"):
                money_exception = name == "erp-core" and path.name == "money.rs"
                result.errors.extend(scan_entity_source(rel, text, money_exception=money_exception))
            if ("/service/" in f"/{rel}/" or name in COMPOSITION) and not is_repository_source(rel):
                result.errors.extend(scan_service_mongo(rel, text))
            result.errors.extend(scan_facade(rel, text))

    bpm_src = backend / "crates/bpm/src"
    if bpm_src.is_dir():
        for path in bpm_src.rglob("*.rs"):
            rel = path.relative_to(backend).as_posix()
            result.errors.extend(scan_bpm_source(rel, path.read_text(encoding="utf-8")))

    old_owned = collection_ownership(backend)
    crate_owned: dict[str, set[str]] = {}
    migrated = set()
    for name in present_planned:
        if name not in BUSINESS_DOMAINS:
            continue
        crate_owned[name] = crate_collection_literals(crates_root / name / "src" / "repository")
        migrated |= crate_owned[name]
    leftover_old = set(old_owned) - migrated
    for name in present_planned:
        if name not in BUSINESS_DOMAINS:
            continue
        repo_root = crates_root / name / "src" / "repository"
        if not repo_root.is_dir():
            continue
        own = crate_owned.get(name, set())
        foreign = (migrated | leftover_old) - own
        for path in repo_root.rglob("*.rs"):
            rel = path.relative_to(backend).as_posix()
            result.errors.extend(
                scan_foreign_collections(rel, path.read_text(encoding="utf-8"), own, foreign)
            )

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


def format_report(fixtures: CheckResult, workspace: CheckResult) -> str:
    """Render a human-readable report."""
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
    """Run fixture self-checks, then the workspace graph and source scan."""
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
