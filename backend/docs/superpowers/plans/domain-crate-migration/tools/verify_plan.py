#!/usr/bin/env python3
"""只读核验迁移计划的章节、链接、清单和可选源快照；不编译或迁移代码。"""

from __future__ import annotations

import argparse
import csv
import hashlib
import json
import re
import sys
import subprocess
from pathlib import Path
from urllib.parse import unquote


def read_tsv(path: Path) -> list[dict[str, str]]:
    """读取 UTF-8 TSV，保留字段用于交叉验证。"""
    with path.open(encoding="utf-8", newline="") as stream:
        return list(csv.DictReader(stream, delimiter="\t"))


EVIDENCE_FILES = (
    "input.json", "files.tsv", "metadata.json", "boundary.log", "unit-tests.log",
    "quality-gates.log", "contract-comparison.json", "transaction-contract.json",
)
# These are historical scope-review rows, never ownership assignments.
COMMERCE_SCOPE_ROWS = {
    "entities/src/ids.rs": "crates/erp-core/src/ids.rs",
    "services/src/errors.rs": "apps/web-api/src/core/errors.rs",
    "entities/src/sales_order/entity/order.rs": "crates/erp-sales/src/entity/sales_order/entity/order.rs",
}
GATE_COMMANDS = {
    "fmt": "cargo fmt --all -- --check",
    "check": "cargo check --workspace --locked",
    "clippy": "cargo clippy --workspace --all-targets --all-features --locked -- -D warnings",
    "lib_tests": "cargo test --workspace --lib --locked",
    "bpm": "./scripts/check-bpm-boundaries.sh",
    "service": "./scripts/check-service-boundaries.sh",
    "domain": "./scripts/check-domain-boundaries.sh",
    "permissions": "./scripts/check-permissions-drift.sh",
    "diff": "git diff --check",
}


def execution_structure(content: str) -> list[str]:
    """执行文档必须保留 1–15 节，可追加第 16 节；复选框仅表示任务记录。"""
    sections = re.findall(r"^## (\d+)\.", content, re.M)
    required = [str(number) for number in range(1, 16)]
    failures = []
    if sections not in (required, required + ["16"]):
        failures.append("执行阶段必须连续包含 1–15 节，且至多追加第 16 节")
    if not re.search(r"^\s*(?:\d+\.|-)\s+\[[ xX]\]", content, re.M):
        failures.append("缺少实际执行任务复选框")
    return failures


def scope_review_rows(phase: dict, content: str, sources: list[dict], repositories: list[dict]) -> set[str]:
    """仅识别阶段 15 三条显式非 owned 合同；普通迁移条目不能借范围复核豁免。"""
    if phase["id"] != "15" or phase["crates"]:
        return set()
    if any(row["phase"] == "15" for row in sources) or any(row["owner_phase"] == "15" for row in repositories):
        return set()
    reviewed = set()
    for source, target in COMMERCE_SCOPE_ROWS.items():
        row = f"| 非 owned 范围复核 | `{source}` | `{target}` |"
        if row in content and any(entry["source"] == source and entry["target"] == target for entry in phase["mappings"]):
            reviewed.add(source)
    return reviewed


def recorded_gate_results(content: str) -> dict[str, bool]:
    """解析已有命令区块的退出记录，不重跑命令；contract scanner 的 exit 1/2 不冒充质量失败。"""
    starts = list(re.finditer(r"^(?:COMMAND |command: |cmd: )(.+)$", content, re.M))
    results = {name: False for name in GATE_COMMANDS}
    for index, start in enumerate(starts):
        command = start.group(1)
        block = content[start.end():starts[index + 1].start() if index + 1 < len(starts) else len(content)]
        exits = re.findall(r"^(?:EXIT\s+|exit:\s*|[\w-]+_exit=)(-?\d+)\s*$", block, re.M)
        for name, required in GATE_COMMANDS.items():
            if required in command and exits:
                # A later rerun replaces the previous recorded result of this exact gate.
                results[name] = all(int(value) == 0 for value in exits)
    return results


def git_read(repo: Path, *arguments: str) -> subprocess.CompletedProcess:
    """只读实际 Git 对象；参数按 argv 传递，不运行 shell/Cargo。"""
    return subprocess.run(["git", "-C", str(repo), *arguments], capture_output=True, check=False)


def resolve_commit(repo: Path, value: object) -> str | None:
    """必须是实际 commit 对象，拒绝缺值、伪 SHA 和 ref/option 注入。"""
    if not isinstance(value, str) or not re.fullmatch(r"[0-9a-f]{7,40}", value):
        return None
    result = git_read(repo, "rev-parse", "--verify", f"{value}^{{commit}}")
    return result.stdout.decode().strip() if result.returncode == 0 else None


def execution_evidence(repo: Path, phase: dict, *, scope_only: bool = False) -> tuple[dict, list[str]]:
    """绑定已记录的输入、代码、证据提交；执行中不假造尚未完成的质量证据。"""
    number, status = phase["id"], phase["status"]
    result = {"phase": number, "status": status, "local_gate_evidence_complete": False}
    failures = []
    if status == "未开始":
        return result, failures
    if status == "已验收":
        failures.append("执行模式最高只确认本地门禁通过，不能将状态伪装为已验收")
    directory = repo / ".domain-migration-evidence" / number
    input_file = directory / "input.json"
    try:
        record = json.loads(input_file.read_text())
    except (OSError, ValueError) as error:
        return result, failures + [f"输入证据不可读：{input_file.relative_to(repo)}：{error}"]
    if record.get("phase") != number or record.get("status") != status:
        failures.append("input.json 与阶段编号/状态不一致")
    if record.get("real_database_verified") is not False or record.get("disclaimer") != "真实数据库运行未验证":
        failures.append("缺少真实数据库运行未验证的固定边界")
    result["input_commit"] = resolve_commit(repo, record.get("input_head"))
    if not result["input_commit"]:
        failures.append("input_head 不是实际 Git commit")
    if status != "本地门禁通过":
        result["completion_pending"] = "当前阶段未登记本地门禁通过；未执行/缺失的最终证据不计完成"
        return result, failures
    output = record.get("output_code_commit") or record.get("head") or record.get("implement_commit")
    result["code_commit"] = resolve_commit(repo, output)
    if not result["code_commit"]:
        failures.append("缺少实际 output_code_commit/head/implement_commit")
    elif result["input_commit"] and git_read(repo, "merge-base", "--is-ancestor", result["input_commit"], result["code_commit"]).returncode:
        failures.append("代码提交不包含登记的输入提交")
    evidence_path = directory.relative_to(repo).as_posix()
    commit = git_read(repo, "log", "-1", "--format=%H", "--", evidence_path).stdout.decode().strip()
    result["evidence_commit"] = resolve_commit(repo, commit)
    if not result["evidence_commit"]:
        failures.append("缺少已落库的阶段证据提交")
    elif result["code_commit"] and git_read(repo, "merge-base", "--is-ancestor", result["code_commit"], result["evidence_commit"]).returncode:
        failures.append("证据提交不包含登记的代码提交")
    result["evidence_files"] = []
    for name in EVIDENCE_FILES:
        path = directory / name
        explicit_empty_scope_list = scope_only and number == "15" and name == "files.tsv"
        if not path.is_file() or (not path.stat().st_size and not explicit_empty_scope_list):
            failures.append(f"必填证据缺失或为空：{evidence_path}/{name}")
            continue
        current = path.read_bytes()
        if not current:
            result["empty_files_list_reason"] = "精确非 owned 范围复核且 source-map/owned 仓储归属均为零"
        original = git_read(repo, "show", f"{commit}:{evidence_path}/{name}") if result["evidence_commit"] else None
        matches = original is not None and original.returncode == 0 and original.stdout == current
        result["evidence_files"].append({"path": f"{evidence_path}/{name}", "sha256": hashlib.sha256(current).hexdigest(), "matches_evidence_commit": matches})
        if not matches:
            failures.append(f"证据未与实际证据 commit 绑定：{evidence_path}/{name}")
    gates = directory / "quality-gates.log"
    result["recorded_quality_gates"] = recorded_gate_results(gates.read_text()) if gates.is_file() else {}
    for gate, passed in result["recorded_quality_gates"].items():
        if not passed:
            failures.append(f"缺少公共门禁成功退出记录：{gate}")
    result["local_gate_evidence_complete"] = not failures
    return result, failures


def execution_completion(phases: list[dict], proofs: list[dict], failures: list[str]) -> bool:
    """全部 18 阶段最高本地门禁状态和绑定证据齐全才报告本地完成；不表示人工验收。"""
    return not failures and len(phases) == len(proofs) == 18 and all(
        phase["status"] == "本地门禁通过" and proof["local_gate_evidence_complete"]
        for phase, proof in zip(phases, proofs)
    )


def selftest() -> int:
    """纯 Python / 临时 Git 负例验证；不运行编译器、数据库或历史 tests。"""
    import tempfile
    import unittest

    class ExecutionTests(unittest.TestCase):
        def test_modes_remain_explicit_and_exclusive(self):
            import contextlib
            import io
            self.assertFalse(arguments([]).execution)
            self.assertFalse(arguments([]).sources)
            self.assertTrue(arguments(["--execution"]).execution)
            with contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit) as error:
                arguments(["--execution", "--sources"])
            self.assertEqual(error.exception.code, 2)

        def test_sections_and_executed_tasks(self):
            content = "\n".join(f"## {n}. 阶段" for n in range(1, 16)) + "\n1. [x] 已执行\n"
            self.assertEqual(execution_structure(content), [])
            self.assertEqual(execution_structure(content + "## 16. 固定合同\n"), [])
            for bad in (content.replace("## 7.", "## 8."), content + "## 17. 跳号\n", content.replace("[x]", "完成")):
                self.assertTrue(execution_structure(bad))

        def test_scope_rows_are_not_generic_exemptions(self):
            source, target = next(iter(COMMERCE_SCOPE_ROWS.items()))
            phase = {"id": "15", "crates": [], "mappings": [{"source": source, "target": target}]}
            content = f"| 非 owned 范围复核 | `{source}` | `{target}` |"
            self.assertEqual(scope_review_rows(phase, content, [], []), {source})
            for p, text, src, repos in [
                ({**phase, "id": "14"}, content, [], []), ({**phase, "crates": ["erp-commerce"]}, content, [], []),
                (phase, content.replace("非 owned", "owned"), [], []), (phase, content, [{"phase": "15"}], []),
                (phase, content, [], [{"owner_phase": "15"}]), (phase, content.replace(target, "crates/fake.rs"), [], []),
            ]:
                self.assertFalse(scope_review_rows(p, text, src, repos))

        def test_gate_log_requires_actual_exit(self):
            content = "\n".join(f"COMMAND {command}\nEXIT 0" for command in GATE_COMMANDS.values())
            self.assertTrue(all(recorded_gate_results(content).values()))
            self.assertTrue(all(recorded_gate_results(content + "\nCOMMAND compare-domain-contracts.py\nEXIT 2\n").values()))
            self.assertFalse(recorded_gate_results(content + "\nCOMMAND cargo check --workspace --locked\nEXIT 1\n")["check"])
            self.assertFalse(recorded_gate_results(content.replace("EXIT 0", "passed"))["fmt"])

        def test_completion_is_separate_from_structure(self):
            phases = [{"status": "本地门禁通过"} for _ in range(18)]
            proofs = [{"local_gate_evidence_complete": True} for _ in range(18)]
            self.assertTrue(execution_completion(phases, proofs, []))
            phases[-1]["status"] = "执行中"
            self.assertFalse(execution_completion(phases, proofs, []))
            phases[-1]["status"] = "本地门禁通过"
            proofs[-1]["local_gate_evidence_complete"] = False
            self.assertFalse(execution_completion(phases, proofs, []))
            self.assertFalse(execution_completion(phases, proofs, ["missing evidence"]))

        def test_real_git_evidence_and_negative_records(self):
            with tempfile.TemporaryDirectory(prefix="verify-plan-execution-") as directory:
                repo = Path(directory)
                def git(*args):
                    result = git_read(repo, "-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid", *args)
                    self.assertEqual(result.returncode, 0, result.stderr.decode())
                    return result.stdout.decode().strip()
                git("init", "-q")
                (repo / "code.txt").write_text("fixture\n")
                git("add", "code.txt"); git("commit", "-qm", "fixture input")
                code = git("rev-parse", "HEAD")
                evidence = repo / ".domain-migration-evidence/00"
                evidence.mkdir(parents=True)
                record = {"phase": "00", "status": "本地门禁通过", "input_head": code, "output_code_commit": code, "real_database_verified": False, "disclaimer": "真实数据库运行未验证"}
                for name in EVIDENCE_FILES:
                    (evidence / name).write_text("fixture evidence\n")
                (evidence / "input.json").write_text(json.dumps(record, ensure_ascii=False))
                (evidence / "quality-gates.log").write_text("\n".join(f"COMMAND {c}\nEXIT 0" for c in GATE_COMMANDS.values()))
                git("add", ".domain-migration-evidence/00"); git("commit", "-qm", "fixture evidence")
                phase = {"id": "00", "status": "本地门禁通过"}
                self.assertEqual(execution_evidence(repo, phase)[1], [])
                scope = repo / ".domain-migration-evidence/15"
                scope.mkdir()
                for name in EVIDENCE_FILES:
                    (scope / name).write_bytes((evidence / name).read_bytes())
                (scope / "input.json").write_text(json.dumps({**record, "phase": "15"}, ensure_ascii=False))
                (scope / "files.tsv").write_text("")
                scope_phase = {"id": "15", "status": "本地门禁通过"}
                self.assertTrue(execution_evidence(repo, scope_phase, scope_only=True)[1])  # uncommitted evidence
                git("add", ".domain-migration-evidence/15"); git("commit", "-qm", "fixture empty scope evidence")
                self.assertEqual(execution_evidence(repo, scope_phase, scope_only=True)[1], [])
                self.assertTrue(execution_evidence(repo, scope_phase)[1])  # no explicit zero-owned contract
                self.assertIsNone(resolve_commit(repo, "f" * 40))
                self.assertIsNone(resolve_commit(repo, "HEAD"))
                for key, value in [("output_code_commit", None), ("input_head", "f" * 40), ("status", "已验收"), ("real_database_verified", True)]:
                    (evidence / "input.json").write_text(json.dumps({**record, key: value}, ensure_ascii=False))
                    self.assertTrue(execution_evidence(repo, phase)[1], key)
                (evidence / "input.json").write_text(json.dumps(record, ensure_ascii=False))
                (evidence / "boundary.log").unlink()
                self.assertTrue(execution_evidence(repo, phase)[1])
                (evidence / "input.json").write_text(json.dumps({**record, "status": "执行中"}, ensure_ascii=False))
                proof, failures = execution_evidence(repo, {"id": "00", "status": "执行中"})
                self.assertFalse(failures)
                self.assertFalse(proof["local_gate_evidence_complete"])

    outcome = unittest.TextTestRunner(verbosity=2).run(unittest.defaultTestLoader.loadTestsFromTestCase(ExecutionTests))
    return int(not outcome.wasSuccessful())


def arguments(argv: list[str] | None = None) -> argparse.Namespace:
    """交付、源快照、执行、自检模式互斥，默认仍为原交付验证。"""
    parser = argparse.ArgumentParser(description=__doc__)
    modes = parser.add_mutually_exclusive_group()
    modes.add_argument("--sources", action="store_true", help="核验编制时源快照；不适用于已开始代码迁移的工作区")
    modes.add_argument("--execution", action="store_true", help="核验执行文档与真实输入/代码/证据提交；不等于人工验收")
    modes.add_argument("--selftest", action="store_true", help="执行纯 Python 与临时 Git 正负例，不编译或运行数据库")
    return parser.parse_args(argv)


def main() -> int:
    """验证交付结构；传入 --sources 时额外核验编制时源文件和探针。"""
    args = arguments()
    if args.selftest:
        return selftest()
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

    execution_proofs: list[dict] = []
    non_owned_review_rows: list[dict] = []
    required_sections = [str(number) for number in range(1, 16)]
    allowed_statuses = {"未开始", "执行中", "本地门禁通过", "已验收", "阻塞"}
    for index, phase in enumerate(manifest["phases"]):
        path = plan / phase["file"]
        if not path.is_file():
            failures.append(f"阶段文件缺失：{path.name}")
            continue
        content = path.read_text()
        if args.execution:
            failures.extend(f"{path.name}: {failure}" for failure in execution_structure(content))
        elif re.findall(r"^## (\d+)\.", content, re.M) != required_sections:
            failures.append(f"阶段必须有完整且有序的 15 节：{path.name}")
        if phase["status"] not in allowed_statuses or f"| 状态 | {phase['status']} |" not in content:
            failures.append(f"文档与 manifest 阶段状态不一致：{path.name}")
        if args.execution:
            reviewed = scope_review_rows(phase, content, source_rows, repository_rows)
            scope_only = phase["id"] == "15" and bool(reviewed) and len(reviewed) == len(phase["mappings"])
            proof, evidence_failures = execution_evidence(backend.parent, phase, scope_only=scope_only)
            execution_proofs.append(proof)
            failures.extend(f"{path.name}: {failure}" for failure in evidence_failures)
            previous = next((item for item in execution_proofs if index and item["phase"] == manifest["phases"][index - 1]["id"]), None)
            if index and phase["status"] != "未开始" and (previous is None or not previous["local_gate_evidence_complete"]):
                failures.append(f"前序阶段没有真实本地门禁/输入/代码/证据提交：{path.name}")
            non_owned_review_rows.extend({"phase": phase["id"], "source": source, "target": COMMERCE_SCOPE_ROWS[source], "counted_as_owned_migration": False} for source in sorted(reviewed))
        elif index and phase["status"] != "未开始" and manifest["phases"][index - 1]["status"] != "已验收":
            failures.append(f"前序阶段未验收：{path.name}")
        if not args.execution and "[ ]" not in content:
            failures.append(f"缺少执行任务：{path.name}")
        for entry in phase["mappings"]:
            if args.execution and phase["id"] == "15":
                if entry["source"] not in reviewed:
                    failures.append(f"范围复核缺少精确非 owned 合同或错误计入迁移归属：{path.name}: {entry['source']}")
                continue
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

    migration_started = any(phase["id"] != "00" and phase["status"] != "未开始" for phase in manifest["phases"])
    if args.sources and migration_started:
        failures.append("--sources 只核验编制源快照；业务迁移开始后禁止使用，请改用 --execution")
    if args.sources and not migration_started:
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
    if args.execution:
        report.update({
            "scope": "execution structure + recorded input/code/evidence commits",
            "local_execution_complete": execution_completion(manifest["phases"], execution_proofs, failures),
            "human_acceptance_asserted": False,
            "real_database_verified": False,
            "quality_gates_rerun": False,
            "phase_execution_evidence": execution_proofs,
            "non_owned_scope_review_rows": non_owned_review_rows,
        })
    print(json.dumps(report, ensure_ascii=False, indent=2))
    return int(bool(failures))


if __name__ == "__main__":
    sys.exit(main())
