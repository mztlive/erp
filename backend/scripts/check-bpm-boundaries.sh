#!/usr/bin/env bash
# P0-A BPM 单向依赖与定义源边界检查。失败关闭，不允许跳过或禁用。
set -euo pipefail

BACKEND_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
FAILED=0

fail() {
    echo "错误: $1" >&2
    FAILED=1
}

search_rs() {
    grep -RIn --include='*.rs' -E "$1" "${BACKEND_DIR}" || true
}

require_file() {
    local path="$1"
    if [[ ! -f "${path}" ]]; then
        fail "缺失文件 ${path}"
        return 1
    fi
}

require_single_definition() {
    local type_name="$1"
    local expected_file="$2"
    local hits
    hits="$(search_rs "id_type!\\(${type_name}\\)")"
    local count
    if [[ -z "${hits}" ]]; then
        count=0
    else
        count="$(printf '%s\n' "${hits}" | wc -l | tr -d ' ')"
    fi
    if [[ "${count}" != "1" ]]; then
        fail "${type_name} 应只有 1 个定义源，实际 ${count}"
        printf '%s\n' "${hits}" >&2
        return
    fi
    if ! printf '%s\n' "${hits}" | grep -F -q "${expected_file}"; then
        fail "${type_name} 定义不在 ${expected_file}: ${hits}"
    fi
    local structs
    structs="$(search_rs "struct ${type_name}[[:space:](]")"
    if [[ -n "${structs}" ]]; then
        fail "${type_name} 存在手写 struct，禁止第二份 newtype"
        printf '%s\n' "${structs}" >&2
    fi
}

echo "检查 BPM crate 与成员 manifest…"
require_file "${BACKEND_DIR}/crates/bpm/Cargo.toml"
require_file "${BACKEND_DIR}/scripts/check-bpm-boundaries.sh"
require_file "${BACKEND_DIR}/services/src/approval/process_kind.rs"

if ! grep -E -q '^[[:space:]]*"crates/bpm"' "${BACKEND_DIR}/Cargo.toml"; then
    fail "workspace members 未登记 crates/bpm"
fi
if ! grep -E -q '^bpm = \{ path = "crates/bpm" \}' "${BACKEND_DIR}/Cargo.toml"; then
    fail "[workspace.dependencies] 未登记 bpm path"
fi

for crate in entities database services; do
    if ! grep -E -q '^bpm = \{ workspace = true \}' "${BACKEND_DIR}/${crate}/Cargo.toml"; then
        fail "${crate}/Cargo.toml 未直接声明 bpm = { workspace = true }"
    fi
    if grep -E -q 'bpm = \{ path' "${BACKEND_DIR}/${crate}/Cargo.toml"; then
        fail "${crate}/Cargo.toml 重复声明了 BPM path"
    fi
done

if grep -E -q 'bpm[[:space:]]*=' "${BACKEND_DIR}/apps/web-api/Cargo.toml"; then
    fail "apps/web-api 不得直接依赖 bpm"
fi

require_file "${BACKEND_DIR}/apps/cli/Cargo.toml"
if ! grep -E -q '^[[:space:]]*"apps/cli"' "${BACKEND_DIR}/Cargo.toml"; then
    fail "workspace members 未登记 apps/cli"
fi
if grep -E -q 'web-api[[:space:]]*=' "${BACKEND_DIR}/apps/cli/Cargo.toml"; then
    fail "apps/cli 不得依赖 web-api"
fi
if grep -E -q 'bpm[[:space:]]*=' "${BACKEND_DIR}/apps/cli/Cargo.toml"; then
    fail "apps/cli 不得直接依赖 bpm"
fi
CLI_TREE="$(cargo tree -p cli --edges normal --manifest-path "${BACKEND_DIR}/Cargo.toml")"
if printf '%s\n' "${CLI_TREE}" | grep -E -w 'web-api' >/dev/null; then
    fail "cli 依赖图包含 web-api"
    printf '%s\n' "${CLI_TREE}" | grep -E -w 'web-api' >&2 || true
fi

echo "检查 bpm 依赖图…"
if grep -E -q '^(entities|database|services|web-api|config|mongodb|axum|id-generator|permission-macros)[[:space:]]*=' \
    "${BACKEND_DIR}/crates/bpm/Cargo.toml"; then
    fail "bpm/Cargo.toml 声明了禁止依赖"
    grep -En '^(entities|database|services|web-api|config|mongodb|axum|id-generator|permission-macros)[[:space:]]*=' \
        "${BACKEND_DIR}/crates/bpm/Cargo.toml" >&2 || true
fi

TREE="$(cargo tree -p bpm --edges normal --manifest-path "${BACKEND_DIR}/Cargo.toml")"
if printf '%s\n' "${TREE}" | grep -E -w 'entities|database|services|web-api|mongodb|axum|id-generator|permission-macros' >/dev/null; then
    fail "bpm 依赖图包含禁止 crate"
    printf '%s\n' "${TREE}" | grep -E -w 'entities|database|services|web-api|mongodb|axum|id-generator|permission-macros' >&2 || true
fi

echo "检查 bpm 源码边界…"
# 禁止 ERP/I-O/时钟，以及调用方之外的 ID 生成入口。负向夹具见下方 self-check：
# 新增 Uuid::new / Uuid::new_v4 / uuid::Uuid::new / nanoid / IdGenerator 等必须非零退出。
BPM_FORBIDDEN_RE='DocumentType|WorkItem|DataScope|Permission|Executor|mongodb|axum|id_generator|next_id\(|Local::now|Utc::now|SystemTime::now|Instant::now|BaseModel::new|Uuid::new|uuid::Uuid::new|nanoid|IdGenerator'
NEGATIVE_ID_SAMPLE='fn forbidden_id_generation() { let _ = Uuid::new_v4(); let _ = Uuid::new(); let _ = uuid::Uuid::new(0); let _ = nanoid!(); let _ = IdGenerator::next(); }'
if ! printf '%s\n' "${NEGATIVE_ID_SAMPLE}" | grep -E -q "${BPM_FORBIDDEN_RE}"; then
    fail "边界正则未能匹配负向夹具中的 ID 生成入口"
fi
for symbol in 'Uuid::new' 'Uuid::new_v4' 'uuid::Uuid::new' 'nanoid' 'IdGenerator'; do
    if ! printf '%s\n' "${NEGATIVE_ID_SAMPLE}" | grep -F -q "${symbol}"; then
        fail "负向夹具缺少 ${symbol}"
    fi
    if ! printf '%s\n' "${symbol}" | grep -E -q "${BPM_FORBIDDEN_RE}"; then
        fail "边界正则未能匹配 ${symbol}"
    fi
done

BPM_HITS="$(grep -RIn --include='*.rs' -E "${BPM_FORBIDDEN_RE}" "${BACKEND_DIR}/crates/bpm/src" || true)"
if [[ -n "${BPM_HITS}" ]]; then
    fail "bpm 源码包含禁止的 ERP/I-O/时钟/ID 生成符号"
    printf '%s\n' "${BPM_HITS}" >&2
fi

echo "检查 ID 定义源…"
require_single_definition "ApprovalProcessDefinitionId" "crates/bpm/src/ids.rs"
require_single_definition "ApprovalNodeDefinitionId" "crates/bpm/src/ids.rs"
require_single_definition "ApprovalTransitionDefinitionId" "crates/bpm/src/ids.rs"
require_single_definition "ApprovalProcessInstanceId" "crates/bpm/src/ids.rs"
require_single_definition "ApprovalNodeExecutionId" "crates/bpm/src/ids.rs"
require_single_definition "ApprovalInstanceAssigneeId" "crates/bpm/src/ids.rs"
require_single_definition "ApprovalCommandReceiptId" "crates/bpm/src/ids.rs"
require_single_definition "ApprovalSubjectSnapshotId" "entities/src/ids.rs"
require_single_definition "ApprovalNotificationOutboxId" "entities/src/ids.rs"

echo "检查 id_type 过程宏定义源…"
MACRO_COUNT="$(python3 - "${BACKEND_DIR}/crates/entity-macros/src/lib.rs" <<'PY'
import re
import sys
from pathlib import Path
text = Path(sys.argv[1]).read_text()
print(len(re.findall(r"#\[proc_macro\]\s*pub fn id_type\b", text)))
PY
)"
if [[ "${MACRO_COUNT}" != "1" ]]; then
    fail "entity-macros 中 #[proc_macro] pub fn id_type 应为 1，实际 ${MACRO_COUNT}"
fi
MACRO_RULES="$(search_rs 'macro_rules![[:space:]]+id_type')"
if [[ -n "${MACRO_RULES}" ]]; then
    fail "backend 中仍存在 macro_rules! id_type"
    printf '%s\n' "${MACRO_RULES}" >&2
fi

echo "检查 DocumentType -> ProcessKind 映射入口…"
PROCESS_KIND_FILE="${BACKEND_DIR}/services/src/approval/process_kind.rs"
if ! grep -E -q 'pub fn process_kind_of\(' "${PROCESS_KIND_FILE}"; then
    fail "缺少 process_kind_of 映射入口"
fi
if ! grep -E -q 'pub fn document_type_of\(' "${PROCESS_KIND_FILE}"; then
    fail "缺少 document_type_of 反向映射入口"
fi
if ! grep -E -q 'match document_type' "${PROCESS_KIND_FILE}"; then
    fail "process_kind_of 必须对 DocumentType 使用穷尽 match"
fi
if ! grep -E -q 'match process_kind' "${PROCESS_KIND_FILE}"; then
    fail "document_type_of 必须对 ProcessKind 使用穷尽 match"
fi
if grep -En 'HashMap' "${PROCESS_KIND_FILE}" >/dev/null; then
    fail "DocumentType <-> ProcessKind 映射不得使用 HashMap"
    grep -En 'HashMap' "${PROCESS_KIND_FILE}" >&2 || true
fi

if [[ "${FAILED}" -ne 0 ]]; then
    echo "BPM 边界检查失败。" >&2
    exit 1
fi

echo "BPM 边界检查通过。"
