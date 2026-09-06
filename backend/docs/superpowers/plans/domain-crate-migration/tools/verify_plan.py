#!/usr/bin/env python3
"""只读核验迁移计划的章节、链接、清单和可选源快照；不编译或迁移代码。"""

from __future__ import annotations

import argparse
import csv
import hashlib
import json
import re
import sys
from pathlib import Path
from urllib.parse import unquote


def read_tsv(path: Path) -> list[dict[str, str]]:
    """读取 UTF-8 TSV，保留字段用于交叉验证。"""
    with path.open(encoding="utf-8", newline="") as stream:
        return list(csv.DictReader(stream, delimiter="\t"))


def main() -> int:
    """验证交付结构；传入 --sources 时额外核验编制时源文件和探针。"""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sources", action="store_true", help="核验编制时源快照；不适用于已开始代码迁移的工作区")
    args = parser.parse_args()
    plan = Path(__file__).resolve().parent.parent
    backend = plan.parents[3]
    failures: list[str] = []
    manifest = json.loads((plan / "plan-manifest.json").read_text())
    source_rows = read_tsv(plan / "source-map.tsv")
    symbol_rows = read_tsv(plan / "source-symbols.tsv")
    repository_rows = read_tsv(plan / "repository-types.tsv")
    probes = json.loads((plan / "compile-probes.json").read_text())["probes"]

    expected_ids = [f"{number:02d}" for number in range(18)]
    phase_ids = [phase["id"] for phase in manifest["phases"]]
    if phase_ids != expected_ids:
        failures.append("阶段必须连续且唯一：00–17")
    if len(manifest["implemented_domain_crates"]) != 19:
        failures.append("当前有实现领域清单必须为 19 个；变更范围时同步设计与验证规则")
    if len(source_rows) != manifest["production_source_count"]:
        failures.append("源文件总数与 manifest 不符")
    if len(repository_rows) != manifest["repository_owned_type_count"]:
        failures.append("仓储类型数量与 manifest 不符")

    source_paths = [row["source"] for row in source_rows]
    if len(source_paths) != len(set(source_paths)):
        failures.append("source-map 中有重复默认源文件")
    if len({row["owned_type"] for row in repository_rows}) != len(repository_rows):
        failures.append("拥有仓储类型存在重复定义计划")
    for row in source_rows:
        if row["phase"] not in expected_ids:
            failures.append(f"未知阶段：{row['source']}")
        if row["target"] and not row["target"].startswith("crates/"):
            failures.append(f"非法生产目标：{row['target']}")
        for column in ["source", "target"]:
            if Path(row[column]).is_absolute() or ".." in Path(row[column]).parts:
                failures.append(f"越界路径：{row[column]}")
    for row in symbol_rows:
        if row["source"] not in source_paths:
            failures.append(f"符号索引没有源映射：{row['source']}")
            break
    for row in repository_rows:
        if row["preparation_phase"] != "01" or row["owner_phase"] not in expected_ids:
            failures.append(f"仓储阶段不合法：{row['owned_type']}")
        if not set(row["impl_sources"].split(",")) <= set(source_paths):
            failures.append(f"仓储实现源缺失：{row['owned_type']}")

    required_sections = [str(number) for number in range(1, 16)]
    allowed_statuses = {"未开始", "执行中", "本地门禁通过", "已验收", "阻塞"}
    for index, phase in enumerate(manifest["phases"]):
        path = plan / phase["file"]
        if not path.is_file():
            failures.append(f"阶段文件缺失：{path.name}")
            continue
        content = path.read_text()
        if re.findall(r"^## (\d+)\.", content, re.M) != required_sections:
            failures.append(f"阶段必须有完整且有序的 15 节：{path.name}")
        if phase["status"] not in allowed_statuses or f"| 状态 | {phase['status']} |" not in content:
            failures.append(f"文档与 manifest 阶段状态不一致：{path.name}")
        if index and phase["status"] != "未开始" and manifest["phases"][index - 1]["status"] != "已验收":
            failures.append(f"前序阶段未验收：{path.name}")
        if "[ ]" not in content:
            failures.append(f"缺少执行任务：{path.name}")
        for entry in phase["mappings"]:
            if entry["source"] not in content or (entry["target"] and entry["target"] not in content):
                failures.append(f"阶段映射与 manifest 不符：{path.name}: {entry['source']}")

    all_docs = sorted(plan.glob("*.md"))
    for doc in all_docs:
        content = doc.read_text()
        if len(re.findall(r"^```", content, re.M)) % 2:
            failures.append(f"代码围栏未闭合：{doc.name}")
        for label, target in re.findall(r"\[([^\]\n]+)\]\(([^)\n]+)\)", content):
            if target.startswith(("https://", "http://", "#")):
                continue
            destination = (doc.parent / unquote(target.split("#", 1)[0])).resolve()
            if not destination.exists():
                failures.append(f"本地链接失效：{doc.name}: {label} -> {target}")
        for block in re.findall(r"```(?:bash|sh)\n(.*?)```", content, re.S):
            if "--include-ignored" in block or re.search(r"cargo test --workspace(?![^\n]*--lib)", block):
                failures.append(f"含禁止的可执行集成测试命令：{doc.name}")
            if re.search(r"\bgit\s+(?:reset\s+--hard|clean\b|add\s+-A)", block):
                failures.append(f"含不适用的破坏性/全量暂存命令：{doc.name}")

    if args.sources:
        current_sources = set()
        for root in manifest["source_roots"]:
            current_sources.update(file.relative_to(backend).as_posix() for file in (backend / root).rglob("*.rs"))
        if current_sources != set(source_paths):
            failures.append(f"源清单漂移；新增={sorted(current_sources-set(source_paths))}，移除={sorted(set(source_paths)-current_sources)}")
        for row in source_rows:
            file = backend / row["source"]
            if not file.is_file():
                failures.append(f"源文件缺失：{row['source']}")
                continue
            if hashlib.sha256(file.read_bytes()).hexdigest() != row["source_sha256"]:
                failures.append(f"源内容已变化，需核验符号/归属：{row['source']}")
        for phase in manifest["phases"]:
            for entry in phase["mappings"]:
                file = backend / entry["source"]
                if not file.is_file():
                    failures.append(f"主映射源文件不存在：{phase['id']}: {entry['source']}")
                    continue
                for symbol in entry["symbols"].split("；"):
                    if symbol not in file.read_text():
                        failures.append(f"主映射选择符不存在：{phase['id']}: {entry['source']}: {symbol}")
        for probe in probes:
            file = backend / probe["baseline_path"]
            if not file.is_file():
                failures.append(f"探针文件缺失：{probe['id']}")
                continue
            source = file.read_text()
            if source.count(probe["before"]) != 1 or probe["after"] in source:
                failures.append(f"探针不满足唯一替换前置条件：{probe['id']}")

    report = {
        "status": "failed" if failures else "passed",
        "scope": "document handoff + current source snapshot" if args.sources else "document handoff",
        "phase_documents": len(phase_ids),
        "source_files": len(source_rows),
        "repository_types": len(repository_rows),
        "symbol_index_rows": len(symbol_rows),
        "markdown_documents": len(all_docs),
        "checks_do_not_compile_or_migrate_code": True,
        "failures": failures,
    }
    print(json.dumps(report, ensure_ascii=False, indent=2))
    return int(bool(failures))


if __name__ == "__main__":
    sys.exit(main())
