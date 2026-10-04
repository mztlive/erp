#!/usr/bin/env python3
"""Reject business reads from audit and require explicit execution registration.

This lexical gate enforces exact source registrations. It does not replace
command behavior tests or establish runtime transaction/concurrency evidence.
"""

from __future__ import annotations

import argparse
from collections import Counter
from dataclasses import dataclass
import json
from pathlib import Path
import re
import sys
import unittest

from cutover_workspace import _code_and_literals
from rust_size_gate import FN_NAME, block_end, find_block_or_semi, find_test_ranges, is_excluded_path, mask_ranges, submodule_paths


WRITE_OPERATIONS = {"create", "create_many_ordered"}
ACCESSOR = re.compile(r"\.\s*(audit_logs|audit_events|audit_attempts)\s*\(\s*\)")
OPERATION = re.compile(r"\s*\.\s*([A-Za-z_][A-Za-z0-9_]*)\s*(?:\(|::)")
FUNCTION = re.compile(r"\bfn\s+(?:r#)?([A-Za-z_][A-Za-z0-9_]*)\b")
RECEIPT_IMPORT = re.compile(r"\buse\s+erp_audit\s*::[^;]*\bCommandReceiptServiceExt\b[^;]*;", re.S)
RETIRED_READERS = re.compile(
    r"\b(?:CommandReceiptServiceExt|CommandReceiptFact|CommandReceiptMatch|SeparationAuditFact|"
    r"find_command_receipts_by_ids|list_separation_facts_by_resources|list_work_item_creation_audits|"
    r"list_successful_work_item_fact_audits|list_master_mapping_task_histories|list_master_mapping_task_history)\b"
)


ENTRY_CALL = re.compile(r"""(?x)
(?P<context>\b(?:BusinessEventContext|AuditLog|PreparedWorkflowAudit|AuditEvent|AuditLogService)\s*::\s*(?:new|success_resource_data|resource(?:_with_(?:message|id))?))\s*\(
|(?P<function>\b(?:prepare_business_log|run_audited|run_audited_event|execute_audited|execute_prepared|persist_log|persist_logs|record_login_audit|finish_attempt|attempt_context|record_command_attempt))\s*(?:::<[^;{}]+>)?\s*\(
|(?P<factory>\.\s*(?:resource_log(?:_with_(?:message|id))?|prepare_resource_log))\s*\(
|(?P<port>\b(?:audit|audit_port|sink)\s*\.\s*(?:persist(?:_attempt)?|validate))\s*\(
|(?P<owned>\.\s*(?:audit_logs|audit_events|audit_attempts)\s*\(\s*\)\s*\.\s*(?:create|create_many_ordered))\s*\(
|(?P<identity>\.\s*(?:with_audited_write|build_audit_event|prepare_audit_event|refresh_policy|attempt))\s*\(
|(?P<lifecycle>\.\s*(?:run_audited|run_audited_event|run_system_policy_transaction|run_authorized_policy_transaction|run_authorized_audited_policy_transaction|run_policy_transaction_at_revision|finish_policy_transaction))\s*(?:::<[^;{}]+>)?\s*\(
""")

@dataclass(frozen=True, order=True)
class Dependency:
    """One narrow source boundary requiring an explicit exit condition."""

    path: str
    function: str
    operation: str


def literal_value(literal: str) -> str | None:
    """Decode Rust collection-name literals while preserving raw-string semantics."""
    raw = re.fullmatch(r'(?:br|r)(?P<hashes>#{0,255})"(?P<value>.*)"(?P=hashes)', literal, re.S)
    if raw:
        return raw.group("value")
    quoted = re.fullmatch(r'b?"(.*)"', literal, re.S)
    if not quoted:
        return None
    return re.sub(
        r'\\(?:x([0-9a-fA-F]{2})|u\{([0-9a-fA-F_]{1,8})\})',
        lambda match: chr(int((match.group(1) or match.group(2)).replace("_", ""), 16)),
        quoted.group(1),
    )


def dependencies(source: str, path: str) -> Counter[Dependency]:
    """Find direct reads, physical writes and retired receipt adapters."""
    code, literals = _code_and_literals(source)
    ranges, _ = find_test_ranges(code)
    code = mask_ranges(code, ranges)
    findings: Counter[Dependency] = Counter()
    functions = list(FUNCTION.finditer(code))
    for accessor in ACCESSOR.finditer(code):
        operation = OPERATION.match(code, accessor.end())
        name = operation.group(1) if operation else "escaped_repository"
        owner = next((fn.group(1) for fn in reversed(functions) if fn.start() < accessor.start()), "module")
        findings[Dependency(path, owner, f"{accessor.group(1)}.{name}")] += 1
    if RECEIPT_IMPORT.search(code):
        findings[Dependency(path, "module", "legacy_command_receipt_adapter")] += 1
    if RETIRED_READERS.search(code):
        findings[Dependency(path, "module", "retired_audit_business_reader")] += 1
    owns_audit = path.startswith("crates/erp-audit/") or path.startswith("crates/erp-identity/")
    if not owns_audit:
        if re.search(r"\b(?:AUDIT_LOGS|AUDIT_EVENTS|AUDIT_ATTEMPTS|AuditLogRepository|AuditAttemptRepository)\b", code):
            findings[Dependency(path, "module", "raw_audit_repository")] += 1
        for position, value in literals:
            if literal_value(value) in {"audit_logs", "audit_events", "audit_attempts"} and not any(start <= position < end for start, end in ranges):
                findings[Dependency(path, "module", "raw_audit_collection")] += 1
    return findings


def load_registry(path: Path) -> dict[Dependency, dict]:
    """Read explicit per-boundary scope, contract and removal requirements."""
    document = json.loads(path.read_text(encoding="utf-8"))
    if document.get("schema_version") != 1:
        raise ValueError("审计依赖登记 schema_version 必须为 1")
    registered = {}
    for row in document["boundaries"]:
        boundary = Dependency(row["path"], row["function"], row["operation"])
        if boundary in registered or row["max_occurrences"] < 1:
            raise ValueError(f"重复或无效边界: {boundary}")
        if row["policy"] not in {"Display", "Audited", "Exempt"}:
            raise ValueError(f"未声明边界政策: {boundary}")
        if not row["contract_items"] or not row["removal_condition"].strip():
            raise ValueError(f"缺少合同范围或退出条件: {boundary}")
        if "*" in row["path"] or row["path"].startswith("/") or ".." in Path(row["path"]).parts:
            raise ValueError(f"禁止通配或越界路径: {boundary}")
        is_write = boundary.operation.split(".")[-1] in WRITE_OPERATIONS
        if is_write == (row["policy"] == "Display"):
            raise ValueError(f"展示和持久化策略错配: {boundary}")
        registered[boundary] = row
    return registered


def production_sources(backend: Path) -> dict[str, str]:
    """Exclude inline and externally declared test modules from enforcement."""
    sources = {}
    test_files = set()
    for root in (backend / "apps", backend / "crates"):
        for path in sorted(root.rglob("*.rs")):
            relative = path.relative_to(backend).as_posix()
            if "src" not in Path(relative).parts or "archive" in Path(relative).parts or is_excluded_path(relative):
                continue
            source = path.read_text(encoding="utf-8")
            code, _ = _code_and_literals(source)
            _, submods = find_test_ranges(code)
            for name in submods:
                test_files.update(submodule_paths(relative, name))
            sources[relative] = source
    return {path: source for path, source in sources.items() if path not in test_files}


def scan(sources: dict[str, str]) -> Counter[Dependency]:
    """Inspect active production boundaries, preserving test exclusions."""
    findings: Counter[Dependency] = Counter()
    for path, source in sources.items():
        findings.update(dependencies(source, path))
    return findings


def check(actual: Counter[Dependency], registered: dict[Dependency, dict]) -> list[str]:
    """Reject uncovered or expanded access and stale explicit registrations."""
    errors = []
    for dependency, count in sorted(actual.items()):
        row = registered.get(dependency)
        if row is None:
            errors.append(f"未登记审计读写: {dependency.path}::{dependency.function} {dependency.operation}")
        elif count > row["max_occurrences"]:
            errors.append(f"审计读写扩大: {dependency.path}::{dependency.function} {dependency.operation}")
    for dependency in registered:
        if dependency not in actual:
            errors.append(f"删除已退出边界登记: {dependency.path}::{dependency.function} {dependency.operation}")
    return errors


def execution_entries(source: str, path: str) -> dict[tuple[str, str], dict]:
    """Scan actual function bodies, including repeated names in distinct impls."""
    code, _ = _code_and_literals(source)
    ranges, _ = find_test_ranges(code)
    code = mask_ranges(code, ranges)
    actual: dict[tuple[str, str], Counter] = {}
    locations: Counter = Counter()
    for function in FN_NAME.finditer(code):
        try:
            start, has_body = find_block_or_semi(code, function.end())
            end = block_end(code, start) if has_body else None
        except ValueError:
            continue
        if not has_body:
            continue
        key = (path, function.group(1))
        locations[key] += 1
        body = code[start + 1 : end - 1]
        calls = Counter(
            re.sub(r"\s+", "", next(group for group in call.groups() if group))
            for call in ENTRY_CALL.finditer(body)
        )
        if key == ("crates/erp-processes/src/integration_resolution/transaction.rs", "run_audited"):
            calls[".with_transaction"] += len(re.findall(r"\.\s*with_transaction\s*\(", body))
        if calls:
            actual.setdefault(key, Counter()).update(calls)
    return {
        key: {"call_counts": dict(sorted(calls.items())), "location_count": locations[key]}
        for key, calls in actual.items()
    }


def check_execution_entries(backend: Path, sources: dict[str, str]) -> list[str]:
    """Fail closed on missing, changed or stale explicit boundary policies."""
    document = json.loads((backend / "scripts/audit-entrypoints.json").read_text(encoding="utf-8"))
    if document.get("schema_version") != 1:
        raise ValueError("审计执行入口登记 schema_version 必须为 1")
    registered = {}
    for row in document["entries"]:
        key = (row["path"], row["function"])
        if (key in registered or row["policy"] not in {"Audited", "Exempt"}
            or row["stage"] not in {"MetadataFactory", "Command", "Transaction", "PersistenceAdapter", "SpecialLifecycle"}
            or row["location_count"] < 1 or not row["reason"].strip()):
            raise ValueError(f"无效或重复审计入口: {key}")
        if "*" in row["path"] or row["path"].startswith("/") or ".." in Path(row["path"]).parts:
            raise ValueError(f"禁止通配或越界审计入口: {key}")
        if sorted(row["boundaries"]) != sorted(row["call_counts"]) or any(count < 1 for count in row["call_counts"].values()):
            raise ValueError(f"审计入口符号和次数不一致: {key}")
        registered[key] = row
    actual = {}
    for path, source in sources.items():
        actual.update(execution_entries(source, path))
    errors = []
    for key, observed in actual.items():
        row = registered.get(key)
        if row is None:
            errors.append(f"未登记审计执行入口: {key[0]}::{key[1]}")
        elif row["call_counts"] != observed["call_counts"] or row["location_count"] != observed["location_count"]:
            errors.append(f"审计执行入口已变化: {key[0]}::{key[1]}")
    for key in registered.keys() - actual.keys():
        errors.append(f"删除已退出审计入口登记: {key[0]}::{key[1]}")
    return errors


class GateTests(unittest.TestCase):
    """Architecture fixtures exercise the actual lexical gate."""

    def test_attempts_are_subject_to_the_same_business_read_boundary(self):
        source = 'fn replay() { db.audit_attempts().find_by_id(id, ex); db.collection("audit_attempts"); }'
        self.assertEqual(dependencies(source, "sample.rs"), Counter({
            Dependency("sample.rs", "replay", "audit_attempts.find_by_id"): 1,
            Dependency("sample.rs", "module", "raw_audit_collection"): 1,
        }))

    def test_direct_collection_literals_cannot_escape_by_raw_or_encoded_strings(self):
        for literal in ['"audit_logs"', 'r#"audit_logs"#', 'b"audit_logs"', r'"\x61udit_logs"', r'"\u{61}udit_logs"']:
            source = f'fn replay() {{ db.collection({literal}); }}'
            self.assertEqual(dependencies(source, "sample.rs"), Counter({
                Dependency("sample.rs", "module", "raw_audit_collection"): 1,
            }))

    def test_ignores_literals_comments_tests_but_registers_write_boundary(self):
        source = '''
        fn write() {
            let text = "db.audit_logs().find_by_id(id, ex)";
            // db.audit_logs().find_by_id(id, ex);
            db.audit_logs().create(&event, ex);
        }
        #[cfg(test)] mod tests { fn read() { db.audit_logs().find_by_id(id, ex); } }
        '''
        self.assertEqual(dependencies(source, "sample.rs"), Counter({
            Dependency("sample.rs", "write", "audit_logs.create"): 1,
        }))

    def test_detects_multiline_reads_and_repository_escape(self):
        source = "fn replay() { db.audit_logs()\n.find_by_id(id, ex); let repo = db.audit_logs(); }"
        self.assertEqual(dependencies(source, "sample.rs"), Counter({
            Dependency("sample.rs", "replay", "audit_logs.find_by_id"): 1,
            Dependency("sample.rs", "replay", "audit_logs.escaped_repository"): 1,
        }))

    def test_new_boundary_increase_and_retired_registration_fail(self):
        dependency = Dependency("sample.rs", "replay", "audit_logs.find_by_id")
        registry = {dependency: {"max_occurrences": 1}}
        self.assertTrue(check(Counter({dependency: 2}), registry))
        self.assertTrue(check(Counter(), registry))
        self.assertTrue(check(Counter({dependency: 1}), {}))
        self.assertEqual(check(Counter({dependency: 1}), registry), [])

    def test_actual_entry_bodies_combine_impls_and_ignore_test_only_calls(self):
        source = """
        impl First { fn persist() { persist_log(db, log, ex); } }
        impl Second { fn persist() { sink.persist(log, ex); } }
        fn no_entry() { let literal = "persist_log(db, log, ex)"; }
        #[cfg(test)] mod tests { fn ignored() { run_audited(db, log, write); } }
        """
        observed = execution_entries(source, "sample.rs")
        self.assertEqual(observed, {("sample.rs", "persist"): {
            "call_counts": {"persist_log": 1, "sink.persist": 1}, "location_count": 2,
        }})

    def test_entry_factory_and_shared_executor_boundary_are_both_registered(self):
        observed = execution_entries(
            "fn commit() { actor.resource_log(action, kind, id); execute_audited(ctx, sink, ex, command); }",
            "sample.rs",
        )
        self.assertEqual(observed[("sample.rs", "commit")]["call_counts"], {
            ".resource_log": 1, "execute_audited": 1,
        })

    def test_legacy_adapter_import_and_identity_display_are_distinct(self):
        source = "use erp_audit::{AuditExt, CommandReceiptServiceExt as _}; fn list() { db.audit_events().search_audit_events(f, ex); }"
        self.assertEqual(len(dependencies(source, "sample.rs")), 3)


def main() -> int:
    """Run fixtures then enforce the checked-in migration register."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--backend", type=Path, default=Path(__file__).resolve().parent.parent)
    args = parser.parse_args()
    suite = unittest.defaultTestLoader.loadTestsFromTestCase(GateTests)
    if not unittest.TextTestRunner(stream=sys.stderr, verbosity=0).run(suite).wasSuccessful():
        return 1
    try:
        registered = load_registry(args.backend / "scripts/audit-boundaries.json")
        sources = production_sources(args.backend)
        errors = check(scan(sources), registered)
        errors.extend(check_execution_entries(args.backend, sources))
    except (ValueError, OSError, KeyError) as error:
        errors = [str(error)]
    if errors:
        print("\n".join(errors), file=sys.stderr)
        return 1
    print("审计业务反查已退出；仅保留授权展示，运行及业务覆盖验收不由本门禁核销。")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
