#!/usr/bin/env bash
# Service -> Repository 分层边界检查。现有债务按基线只减不增，清零后继续失败关闭。
set -euo pipefail

BACKEND_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BASELINE_FILE="${BACKEND_DIR}/scripts/service-boundary-baseline.tsv"

python3 - "${BACKEND_DIR}" "${BASELINE_FILE}" <<'PY'
from __future__ import annotations

import re
import sys
from collections import Counter
from pathlib import Path

backend = Path(sys.argv[1])
baseline_path = Path(sys.argv[2])
services = backend / "services" / "src"

rules = {
    "mongodb_bson": re.compile(r"mongodb::bson|bson::doc|bson::Document|bson::Bson"),
    "doc_macro": re.compile(r"\bdoc!\s*\{"),
    "raw_document": re.compile(r"\bDocument\b"),
    "raw_bson": re.compile(r"\bBson\b"),
    "find_one": re.compile(r"\.find_one\s*\("),
    "find_many": re.compile(r"\.find_many\s*\("),
    "find_many_sorted": re.compile(r"\.find_many_sorted\s*\("),
    "find_one_by_field": re.compile(r"\.find_one_by_field\s*\("),
    "exists": re.compile(r"\.exists\s*\("),
}

negative_samples = {
    "mongodb_bson": "use mongodb::bson::Document;",
    "doc_macro": 'let filter = doc! { "id": id };',
    "raw_document": "fn filter() -> Document { todo!() }",
    "raw_bson": "let value: Bson = todo!();",
    "find_one": "repo.find_one(filter, executor).await?;",
    "find_many": "repo.find_many(filter, executor).await?;",
    "find_many_sorted": "repo.find_many_sorted(filter, sort, executor).await?;",
    "find_one_by_field": 'repo.find_one_by_field("id", id, executor).await?;',
    "exists": "repo.exists(filter, executor).await?;",
}

errors: list[str] = []
for name, pattern in rules.items():
    sample = negative_samples[name]
    if pattern.search(sample) is None:
        errors.append(f"规则 {name} 未命中负向自检样例: {sample}")

if not baseline_path.is_file():
    errors.append(f"缺失边界基线文件: {baseline_path}")
    baseline: Counter[tuple[str, str]] = Counter()
else:
    baseline = Counter()
    for line_no, raw in enumerate(baseline_path.read_text().splitlines(), start=1):
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        parts = raw.split("\t")
        if len(parts) != 3:
            errors.append(f"基线第 {line_no} 行必须是 rule<TAB>path<TAB>count")
            continue
        rule, path, count_text = parts
        if rule not in rules:
            errors.append(f"基线第 {line_no} 行包含未知规则: {rule}")
            continue
        try:
            count = int(count_text)
        except ValueError:
            errors.append(f"基线第 {line_no} 行计数不是整数: {count_text}")
            continue
        if count <= 0:
            errors.append(f"基线第 {line_no} 行计数必须大于 0")
            continue
        key = (rule, path)
        if key in baseline:
            errors.append(f"基线重复项: {rule}\t{path}")
            continue
        baseline[key] = count

actual: Counter[tuple[str, str]] = Counter()
for source in sorted(services.rglob("*.rs")):
    relative = source.relative_to(backend).as_posix()
    text = source.read_text()
    for name, pattern in rules.items():
        count = len(pattern.findall(text))
        if count:
            actual[(name, relative)] = count

for key in sorted(set(actual) | set(baseline)):
    current = actual.get(key, 0)
    allowed = baseline.get(key, 0)
    rule, path = key
    if current > allowed:
        errors.append(f"新增或增加 Service 边界违规: {rule}\t{path}\t{allowed} -> {current}")
    elif current < allowed:
        errors.append(
            f"Service 边界违规已减少但基线未同步: {rule}\t{path}\t{allowed} -> {current}"
        )

if errors:
    print("Service 边界检查失败：", file=sys.stderr)
    for error in errors:
        print(f"- {error}", file=sys.stderr)
    sys.exit(1)

print(f"Service 边界检查通过；当前基线共 {sum(baseline.values())} 个受控命中。")
PY
