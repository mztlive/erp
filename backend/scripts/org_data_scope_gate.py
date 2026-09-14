#!/usr/bin/env python3
"""Enforce mechanically checkable organization/data-scope chapter 9 boundaries.

This is a lexical architecture gate, not a proof of query equivalence or runtime
authorization. Existing domain checks own Cargo graphs and persistence isolation.
No baseline, ignore list, or report-only success mode is supported.
"""

from __future__ import annotations

import argparse
from dataclasses import asdict, dataclass
import json
from pathlib import Path
import re
import subprocess
import sys
import unittest

from cutover_workspace import _code_and_literals, active_source_paths
from domain_boundaries import BUSINESS_DOMAINS, FOUNDATION, cargo_metadata


RESOLVER = "crates/erp-identity/src/service/access_control/resolve.rs"
CUSTOMER_PORT = "crates/erp-customer/src/ports/data_scope.rs"
CUSTOMER_ADAPTER = "crates/erp-processes/src/adapters/customer_data_scope.rs"
CUSTOMER_ACCESS = "crates/erp-customer/src/service/customer/access.rs"
REQUIRED = (RESOLVER, CUSTOMER_PORT, CUSTOMER_ADAPTER, CUSTOMER_ACCESS)
# These labels describe existing migration gaps, never exemptions from failure.
LEGACY = {
    "crates/erp-workflow/src/ports/authorization.rs",
    "crates/erp-workflow/src/service/work_item/access.rs",
    "crates/erp-workflow/src/service/approval/business_adapter.rs",
    "crates/erp-workflow/src/service/approval/scope.rs",
    "crates/erp-processes/src/adapters/workflow/authorization.rs",
}
RAW = r"\b(?:scope_type|scope_targets)\b"
INTERNAL = r"\b(?:AuthorizedDataScope|SharedRbacService|OrganizationState|OrgTree|ScopeResolution|DataScopeService)\b"
SCAN_HINT = re.compile(
    RAW + "|" + INTERNAL
    + r"|\b(?:DataScope|ResolvedScope|ScopeClause|\w*DataScopePort|data_scopes|role_permission_snapshot|granting_role_ids_for_all)\b"
)
NOT_PROVEN = [
    "9.1.3 / A15: 同快照过滤、计数、分组、指标、稳定排序及分页",
    "9.1.4-8 / A16,A36: 各入口独立鉴权、导出下载撤权、跨页版本、缓存期限和事务重验",
    "9.1.9-11 / A19: 批量展开、超限拒绝、索引执行计划、并发审计和幂等",
    "9.3.1-5 / A34: 全维度保留、角色并集/个人上限/参与边界、公共判定与数据库条件等价",
    "9.5 / A35: 逐资源动作完整登记、配置与初始化共用准入、全部消费入口和旧读取器清零",
]


@dataclass(frozen=True)
class Finding:
    """One blocking diagnostic; line offsets refer to the original source."""

    rule: str
    path: str
    line: int
    message: str
    category: str = "整改项"


def block_end(code: str, start: int) -> int:
    """Return the end of a balanced Rust block, rejecting malformed input."""
    depth = 0
    for pos in range(start, len(code)):
        depth += (code[pos] == "{") - (code[pos] == "}")
        if depth == 0:
            return pos + 1
    raise ValueError("Rust 块未闭合，无法完成源码门禁")


def production_code(source: str) -> str:
    """Mask literals, comments and inline cfg(test) modules without moving lines.

Other cfg branches are checked conservatively. This intentionally does not
expand macros or resolve arbitrary Rust call graphs.
"""
    code, _ = _code_and_literals(source)
    pattern = r"#\s*\[\s*cfg\s*\(\s*test\s*\)\s*\]\s*(?:#\s*\[[^\]]*\]\s*)*(?:pub\s+)?mod\s+\w+\s*\{"
    for match in reversed(list(re.finditer(pattern, code))):
        end = block_end(code, match.end() - 1)
        masked = re.sub(r"[^\n]", " ", code[match.start():end])
        code = code[:match.start()] + masked + code[end:]
    return code


def scan_source(path: str, source: str) -> list[Finding]:
    """Check ownership, raw-rule leakage, adapter execution and admission traps."""
    # Every applicable source rule contains one of these identifiers. Avoid
    # lexing unrelated large files, but keep the fixed resolver checks enabled.
    if path != RESOLVER and not SCAN_HINT.search(source):
        return []
    code = production_code(source)
    parts = path.split("/")
    package = parts[1] if len(parts) > 2 and parts[0] == "crates" else ""
    consumer = package in BUSINESS_DOMAINS - {"erp-identity"}
    findings: list[Finding] = []

    def reject(rule: str, pattern: str, message: str, category: str = "整改项") -> None:
        """Emit one actionable diagnostic per rule/file, retaining the first line."""
        match = re.search(pattern, code, re.S)
        if match:
            findings.append(Finding(rule, path, code.count("\n", 0, match.start()) + 1, message, category))

    if consumer and "/ports/" in path:
        reject("ODS-PORT", INTERNAL, "Port 不得暴露身份域内部类型或具体解析实现（9.2/9.3/9.4）")
    if consumer or package in {"erp-processes", "erp-read-models"}:
        reject("ODS-RAW", RAW, "消费路径携带或解释原始 scope_type/scope_targets；必须接入公共解析（9.2.3/9.5.5）",
               "待接入项" if path in LEGACY else "整改项")
    if package in FOUNDATION:
        reject("ODS-OWNER", INTERNAL + r"|\b(?:DataScope|ResolvedScope|ScopeClause)\b",
               "范围实体与算法须归身份域，不得迁入基础 crate（9.2.1）")
    if package != "erp-identity":
        reject("ODS-ENGINE", r"\b(?:struct|enum|trait)\s+(?:DataScopeService|ScopeResolution)\b",
               "禁止定义第二份公共授权解析器（9.2）")
    if consumer and "/repository/" in path:
        reject("ODS-REPOSITORY", r"\b(?:DataScopeService|ScopeResolution|SharedRbacService|AuthorizedDataScope)\b|\.(?:data_scopes|role_permission_snapshot|granting_role_ids_for_all)\s*\(",
               "业务 Repository 只能编译明确对象条件，不得加载/解释登录授权（9.1.2/9.2）")
    adapter = package == "erp-processes" and re.search(r"\bimpl\s+\w*DataScopePort\s+for\b", code)
    if adapter:
        reject("ODS-EXECUTOR", r"\bNoTransaction\b|\.(?:with_transaction|start_session|start_transaction)\s*\(",
               "授权 adapter 不得替换执行器或另开事务（9.3 执行上下文）")
        reject("ODS-FALLBACK", r"\bunwrap_or(?:_else|_default)?\s*\(|\b(?:company|authorized_customer_ids)\s*:\s*(?:true|None)\b",
               "授权 adapter 不得用默认成功/全量结果吞掉解析失败（9.3 错误合同）")
        resolve = re.search(r"\basync\s+fn\s+resolve\s*\([^{}]*\)[^{]*\{", code)
        if resolve is None:
            findings.append(Finding("ODS-ADAPTER", path, 1, "生产范围 adapter 缺少可检查的 resolve 方法（9.2.3）"))
        else:
            body = code[resolve.start():block_end(code, resolve.end() - 1)]
            aliases = ["DataScopeService"] + re.findall(r"\bDataScopeService\s+as\s+(\w+)", code)
            constructor = r"\b(?:" + "|".join(aliases) + r")\s*::\s*new\s*\("
            construction = re.search(constructor, body)
            # Inspect the same statement: an unrelated local resolver or a call
            # in another method must not satisfy the production wiring check.
            statement = body[construction.start():].split(";", 1)[0] if construction else ""
            call = re.search(r"\.(?:resolve|resolve_permissions)\s*\(", statement)
            if call is None:
                findings.append(Finding("ODS-ADAPTER", path, code.count("\n", 0, resolve.start()) + 1,
                                        "resolve 必须实际调用 DataScopeService 公共解析；类型名/注释不构成接入（9.2.3）"))
            elif not re.search(r",\s*executor\s*,?\s*\)", statement[call.end():]):
                findings.append(Finding("ODS-EXECUTOR", path, 1,
                                        "公共解析调用必须直接传递收到的 executor（9.3）"))
            if not re.search(r"executor\s*:\s*&mut\s+dyn\s+Executor\b", body):
                findings.append(Finding("ODS-EXECUTOR", path, 1, "resolve 必须接收调用方 Executor（9.3）"))
    if path == RESOLVER:
        reject("ODS-REGISTRY", r"\bpredefined_data_scopes\s*::\s*RESOURCE_ACTIONS\b",
               "初始化资源清单不能作为消费者接入准入依据（9.5.1-2 / A35）")
        reject("ODS-DIMENSIONS", r"\brequired_dimensions\s*:\s*&\s*\[\s*ScopeDimension\s*::\s*\w+\s*\]",
               "公共解析器不得对所有资源硬编码相同必需维度；须按资源动作登记（9.5.1）")
    return findings


def check_required(sources: dict[str, str]) -> list[Finding]:
    """Prevent deleted/empty sample files and comment-only wiring from passing."""
    requirements = {
        RESOLVER: [r"\bstruct\s+DataScopeService\b", r"\bScopeResolution\s*\{"],
        CUSTOMER_PORT: [r"\btrait\s+CustomerDataScopePort\b", r"\bexecutor\s*:\s*&mut\s+dyn\s+Executor\b"],
        CUSTOMER_ADAPTER: [r"\bimpl\s+CustomerDataScopePort\s+for\b"],
        CUSTOMER_ACCESS: [r"\bscope\s*:\s*Arc\s*<\s*dyn\s+CustomerDataScopePort\s*>"],
    }
    findings = []
    for path, patterns in requirements.items():
        if path not in sources:
            findings.append(Finding("ODS-SAMPLE", path, 1, "客户接入样例必需文件不在活动源码中（9.4）"))
            continue
        code = production_code(sources[path])
        for pattern in patterns:
            if not re.search(pattern, code):
                findings.append(Finding("ODS-SAMPLE", path, 1, f"缺少生产样例结构：{pattern}（9.4）"))
    return findings


def self_tests() -> bool:
    """Run mandatory positive/negative fixtures before judging any workspace."""
    import test_org_data_scope_gate

    suite = unittest.defaultTestLoader.loadTestsFromModule(test_org_data_scope_gate)
    if suite.countTestCases() == 0:
        raise RuntimeError("门禁自检未发现测试，禁止空跑成功")
    return unittest.TextTestRunner(stream=sys.stderr, verbosity=1).run(suite).wasSuccessful()


def workspace(backend: Path) -> tuple[list[Finding], int]:
    """Inspect actual Cargo member sources; fail if inventory or metadata fails."""
    metadata = cargo_metadata(backend)
    sources = {}
    for path in active_source_paths(metadata):
        relative = path.relative_to(backend).as_posix()
        sources[relative] = path.read_text(encoding="utf-8")
    if not sources:
        raise RuntimeError("活动源码清单为空")
    findings = check_required(sources)
    for path, source in sources.items():
        # Separate test fixture modules are not production consumers.
        if "tests" in Path(path).parts or Path(path).stem in {"tests", "test"} or Path(path).stem.endswith("_tests"):
            continue
        findings.extend(scan_source(path, source))
    return findings, len(sources)


def main(argv: list[str] | None = None) -> int:
    """Return 0 for static checks only, 1 for blockers, 2 for tool/input errors."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--backend", required=True, type=Path)
    parser.add_argument("--self-test-only", action="store_true", help="仅验证门禁夹具，不检查业务代码")
    parser.add_argument("--json", action="store_true", help="stdout 输出 JSON；子门禁日志输出 stderr")
    args = parser.parse_args(argv)
    try:
        if not self_tests():
            return 2
        if args.self_test_only:
            print(json.dumps({"status": "SELF_TEST_ONLY", "workspace_checked": False}) if args.json else
                  "门禁夹具通过；未检查实际工作区。")
            return 0
        backend = args.backend.resolve()
        domain = subprocess.run(["bash", str(backend / "scripts/check-domain-boundaries.sh"), "--cutover"],
                                cwd=backend, stdout=sys.stderr, stderr=sys.stderr, check=False)
        findings, count = workspace(backend)
        if domain.returncode:
            findings.insert(0, Finding("ODS-DOMAIN", "scripts/check-domain-boundaries.sh", 1,
                                      f"既有领域边界门禁失败，退出码 {domain.returncode}（9.1/9.2 / A33）"))
        status = "BLOCKED" if findings else "STATIC_CHECKS_PASSED"
        if args.json:
            print(json.dumps({"status": status, "files": count, "findings": [asdict(f) for f in findings],
                              "not_proven": NOT_PROVEN}, ensure_ascii=False, indent=2))
        else:
            for finding in findings:
                print(f"[{finding.rule}][{finding.category}] {finding.path}:{finding.line}: {finding.message}")
            print(f"{status}: 扫描 {count} 个活动源码文件，阻断 {len(findings)} 项。")
            print("本门禁不证明以下条款；按执行说明补齐行为与验收证据：")
            for item in NOT_PROVEN:
                print(f"  - {item}")
        return 1 if findings else 0
    except Exception as error:
        if args.json:
            print(json.dumps({"status": "TOOL_ERROR", "error": str(error)}, ensure_ascii=False))
        else:
            print(f"TOOL_ERROR: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
