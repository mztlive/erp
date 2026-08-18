#!/usr/bin/env bash
# reset-dev-business-data.sh 的离线合同测试；不得连接 MongoDB。
# 本文件只做语法、静态合同与 PATH 隔离的假 mongosh 断言，禁止真实 mongo/mongosh 连接。
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RESET_SCRIPT="${SCRIPT_DIR}/reset-dev-business-data.sh"
MONGOSH_SCRIPT="${SCRIPT_DIR}/reset-dev-business-data.mongosh.js"
TEST_DIR="$(mktemp -d)"
trap 'rm -rf "${TEST_DIR}"' EXIT

fail() {
    echo "测试失败: $*" >&2
    exit 1
}

bash -n "${RESET_SCRIPT}"
node --check "${MONGOSH_SCRIPT}" >/dev/null
if grep -Eq '\.dropDatabase[[:space:]]*\(' "${MONGOSH_SCRIPT}" "${RESET_SCRIPT}"; then
    fail "重置脚本禁止调用 dropDatabase()"
fi
for collection in \
    system_safety_pause_operations \
    low_margin_manager_confirmations \
    supplier_api_health_check_runs \
    supplier_api_connection_command_receipts \
    supplier_settlement_source_evidence \
    supplier_settlement_difference_evidence \
    product_publication_deliveries \
    product_publication_revision_media \
    product_publication_revisions \
    product_publications \
    approval_definitions \
    approval_step_definitions \
    approval_instances \
    approval_step_instances \
    approval_process_definitions \
    approval_node_definitions \
    approval_transition_definitions \
    approval_process_instances \
    approval_node_executions \
    approval_instance_assignees \
    approval_command_receipts \
    approval_subject_snapshots \
    approval_notification_outbox \
    work_items; do
    grep -q "\"${collection}\"" "${MONGOSH_SCRIPT}" ||
        fail "审批或业务重置集合未纳入合同: ${collection}"
done
for token in \
    CARD_SALES_MANAGER_APPROVAL \
    CARD_SALES_OPERATION_APPROVAL \
    DOCUMENT_APPROVAL \
    approval_step_instance_id \
    approval_node_execution_id \
    uk_work_items_open_approval_step \
    idx_work_items_team_pool; do
    grep -q "${token}" "${MONGOSH_SCRIPT}" ||
        fail "审批 WorkItem 或冲突索引 allowlist 缺失: ${token}"
done

party_delete_line="$(grep -n 'let deletedPartyTargets' "${MONGOSH_SCRIPT}" | cut -d: -f1)"
business_drop_line="$(
    grep -n 'for (const group of DROP_GROUPS)' "${MONGOSH_SCRIPT}" |
        cut -d: -f1 |
        awk -v party_line="${party_delete_line}" '$1 > party_line { print; exit }'
)"
[[ -n "${party_delete_line}" && -n "${business_drop_line}" ]] ||
    fail "未找到 Party 清理或业务集合 drop 步骤"
(( party_delete_line < business_drop_line )) ||
    fail "Party 专属链必须在业务来源集合 drop 前删除，保证中断后可重建候选集"

help_output="${TEST_DIR}/help.txt"
"${RESET_SCRIPT}" --config "${TEST_DIR}/missing.toml" --help >"${help_output}"
grep -q "默认行为" "${help_output}" || fail "--help 未输出合同"
grep -q -- "--verify" "${help_output}" || fail "--help 未声明 --verify"
grep -q -- "--expect-summary" "${help_output}" || fail "--help 未声明 --expect-summary"
grep -q "dropDatabase" "${help_output}" || fail "--help 未声明禁止 dropDatabase()"

mkdir -p "${TEST_DIR}/bin"
fixture_config="${TEST_DIR}/config.toml"
cat >"${fixture_config}" <<'EOF'
[database]
uri = "mongodb://sentinel-user:sentinel-password@127.0.0.1:27017/admin"
db_name = "reset_contract_test"
EOF

cat >"${TEST_DIR}/bin/mongosh" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
[[ "${ERP_RESET_MONGO_URI:-}" == "${EXPECTED_URI:-mongodb://sentinel-user:sentinel-password@127.0.0.1:27017/admin}" ]]
[[ "${ERP_RESET_DB_NAME:-}" == "reset_contract_test" ]]
[[ "${ERP_RESET_EXECUTE:-}" == "${EXPECTED_EXECUTE:-0}" ]]
[[ "${ERP_RESET_VERIFY:-}" == "${EXPECTED_VERIFY:-0}" ]]
[[ "${ERP_RESET_CONFIRMED_DB:-}" == "${EXPECTED_CONFIRMED_DB:-}" ]]
[[ "$*" == *"--nodb"* ]]
[[ "$*" == *"--norc"* ]]
[[ "$*" == *"--file"* ]]
echo "fake mongosh invoked"
EOF
chmod +x "${TEST_DIR}/bin/mongosh"

scope_digest() {
    local db_name="$1"
    python3 - "${db_name}" "${MONGOSH_SCRIPT}" <<'PY'
import hashlib
import pathlib
import sys

db_name = sys.argv[1]
script = pathlib.Path(sys.argv[2]).read_bytes()
print(hashlib.sha256(db_name.encode("utf-8") + b"\n" + script).hexdigest())
PY
}

PREVIEW_DIGEST="$(scope_digest reset_contract_test)"

preview_output="${TEST_DIR}/preview.txt"
PATH="${TEST_DIR}/bin:${PATH}" EXPECTED_EXECUTE=0 EXPECTED_VERIFY=0 \
    "${RESET_SCRIPT}" --config "${fixture_config}" >"${preview_output}" 2>&1
grep -q "运行模式: PREVIEW" "${preview_output}" || fail "默认模式不是 PREVIEW"
grep -q "目标主机: 127.0.0.1" "${preview_output}" || fail "预览未输出脱敏目标主机"
grep -q "集合摘要: ${PREVIEW_DIGEST}" "${preview_output}" || fail "预览未输出集合摘要"
grep -q "fake mongosh invoked" "${preview_output}" || fail "预览未调用受控 mongosh"

if grep -Eq 'sentinel-user|sentinel-password|mongodb://' "${preview_output}"; then
    fail "标准输出泄露 MongoDB 连接信息"
fi

xtrace_output="${TEST_DIR}/xtrace.txt"
PATH="${TEST_DIR}/bin:${PATH}" EXPECTED_EXECUTE=0 \
    bash -x "${RESET_SCRIPT}" --config "${fixture_config}" >"${xtrace_output}" 2>&1
if grep -Eq 'sentinel-user|sentinel-password|mongodb://' "${xtrace_output}"; then
    fail "继承 xtrace 时泄露 MongoDB 连接信息"
fi

blocked_output="${TEST_DIR}/blocked.txt"
if PATH="${TEST_DIR}/bin:${PATH}" EXPECTED_EXECUTE=1 \
    "${RESET_SCRIPT}" --config "${fixture_config}" --execute >"${blocked_output}" 2>&1; then
    fail "缺少 --confirm-db 时仍允许执行"
fi
grep -q -- "--confirm-db" "${blocked_output}" || fail "缺少数据库确认时未给出受控错误"
if grep -q "fake mongosh invoked" "${blocked_output}"; then
    fail "执行门禁失败后仍调用 mongosh"
fi

missing_summary_output="${TEST_DIR}/missing-summary.txt"
if PATH="${TEST_DIR}/bin:${PATH}" EXPECTED_EXECUTE=1 EXPECTED_CONFIRMED_DB=reset_contract_test \
    "${RESET_SCRIPT}" \
    --config "${fixture_config}" \
    --execute \
    --confirm-db reset_contract_test >"${missing_summary_output}" 2>&1; then
    fail "缺少 --expect-summary 时仍允许执行"
fi
grep -q "expect-summary" "${missing_summary_output}" || fail "缺少集合摘要时未给出受控错误"
if grep -q "fake mongosh invoked" "${missing_summary_output}"; then
    fail "集合摘要门禁失败后仍调用 mongosh"
fi

mismatch_output="${TEST_DIR}/mismatch.txt"
if PATH="${TEST_DIR}/bin:${PATH}" EXPECTED_EXECUTE=1 EXPECTED_CONFIRMED_DB=reset_contract_test \
    "${RESET_SCRIPT}" \
    --config "${fixture_config}" \
    --execute \
    --confirm-db reset_contract_test \
    --expect-summary deadbeef >"${mismatch_output}" 2>&1; then
    fail "集合摘要不一致时仍允许执行"
fi
grep -q "集合摘要" "${mismatch_output}" || fail "集合摘要不一致时未给出受控错误"
if grep -q "fake mongosh invoked" "${mismatch_output}"; then
    fail "集合摘要不一致后仍调用 mongosh"
fi

execute_output="${TEST_DIR}/execute.txt"
PATH="${TEST_DIR}/bin:${PATH}" EXPECTED_EXECUTE=1 EXPECTED_VERIFY=0 EXPECTED_CONFIRMED_DB=reset_contract_test \
    "${RESET_SCRIPT}" \
    --config "${fixture_config}" \
    --execute \
    --confirm-db reset_contract_test \
    --expect-summary "${PREVIEW_DIGEST}" >"${execute_output}" 2>&1
grep -q "运行模式: EXECUTE" "${execute_output}" || fail "显式确认未进入 EXECUTE"
grep -q "集合摘要: ${PREVIEW_DIGEST}" "${execute_output}" || fail "执行未复用同一集合摘要"
if grep -Eq 'sentinel-user|sentinel-password|mongodb://' "${execute_output}"; then
    fail "执行摘要泄露 MongoDB 连接信息"
fi

verify_output="${TEST_DIR}/verify.txt"
PATH="${TEST_DIR}/bin:${PATH}" EXPECTED_EXECUTE=0 EXPECTED_VERIFY=1 \
    "${RESET_SCRIPT}" \
    --config "${fixture_config}" \
    --verify \
    --expect-summary "${PREVIEW_DIGEST}" >"${verify_output}" 2>&1
grep -q "运行模式: VERIFY" "${verify_output}" || fail "校验模式未进入 VERIFY"
grep -q "集合摘要: ${PREVIEW_DIGEST}" "${verify_output}" || fail "校验未复用同一集合摘要"
if grep -Eq 'sentinel-user|sentinel-password|mongodb://' "${verify_output}"; then
    fail "校验摘要泄露 MongoDB 连接信息"
fi

mode_conflict_output="${TEST_DIR}/mode-conflict.txt"
if PATH="${TEST_DIR}/bin:${PATH}" EXPECTED_EXECUTE=1 EXPECTED_VERIFY=1 \
    "${RESET_SCRIPT}" \
    --config "${fixture_config}" \
    --execute \
    --verify \
    --confirm-db reset_contract_test \
    --expect-summary "${PREVIEW_DIGEST}" >"${mode_conflict_output}" 2>&1; then
    fail "--execute 与 --verify 同时使用仍允许运行"
fi
grep -q "不能同时使用" "${mode_conflict_output}" || fail "模式冲突未给出受控错误"

remote_config="${TEST_DIR}/remote.toml"
cat >"${remote_config}" <<'EOF'
[database]
uri = "mongodb://sentinel-user:sentinel-password@mongo.example.invalid:27017/admin"
db_name = "reset_contract_test"
EOF
remote_output="${TEST_DIR}/remote.txt"
if PATH="${TEST_DIR}/bin:${PATH}" EXPECTED_EXECUTE=1 \
    "${RESET_SCRIPT}" \
    --config "${remote_config}" \
    --execute \
    --confirm-db reset_contract_test >"${remote_output}" 2>&1; then
    fail "远程目标缺少 --allow-remote 时仍允许执行"
fi
grep -q -- "--allow-remote" "${remote_output}" || fail "远程门禁未给出受控错误"
if grep -Eq 'sentinel-user|sentinel-password|mongodb://' "${remote_output}"; then
    fail "远程门禁错误泄露 MongoDB 连接信息"
fi

remote_allowlist_output="${TEST_DIR}/remote-allowlist.txt"
if PATH="${TEST_DIR}/bin:${PATH}" EXPECTED_EXECUTE=1 \
    "${RESET_SCRIPT}" \
    --config "${remote_config}" \
    --execute \
    --confirm-db reset_contract_test \
    --allow-remote >"${remote_allowlist_output}" 2>&1; then
    fail "远程目标缺少精确主机白名单时仍允许执行"
fi
grep -q "ERP_RESET_ALLOWED_REMOTE_HOSTS" "${remote_allowlist_output}" ||
    fail "远程白名单门禁未给出受控错误"

remote_execute_output="${TEST_DIR}/remote-execute.txt"
PATH="${TEST_DIR}/bin:${PATH}" \
    EXPECTED_EXECUTE=1 \
    EXPECTED_VERIFY=0 \
    EXPECTED_CONFIRMED_DB=reset_contract_test \
    EXPECTED_URI="mongodb://sentinel-user:sentinel-password@mongo.example.invalid:27017/admin" \
    ERP_RESET_ALLOWED_REMOTE_HOSTS="mongo.example.invalid" \
    "${RESET_SCRIPT}" \
    --config "${remote_config}" \
    --execute \
    --confirm-db reset_contract_test \
    --expect-summary "${PREVIEW_DIGEST}" \
    --allow-remote >"${remote_execute_output}" 2>&1
grep -q "运行模式: EXECUTE" "${remote_execute_output}" || fail "远程精确白名单未通过"
grep -q "目标主机: mongo.example.invalid" "${remote_execute_output}" || fail "远程执行未输出脱敏主机"
if grep -Eq 'sentinel-user|sentinel-password|mongodb://' "${remote_execute_output}"; then
    fail "远程执行摘要泄露 MongoDB 连接信息"
fi

unsafe_remote_config="${TEST_DIR}/unsafe-remote.toml"
cat >"${unsafe_remote_config}" <<'EOF'
[database]
uri = "mongodb://sentinel-user:sentinel-password@mongo.example.invalid:27017/admin"
db_name = "erp"
EOF
unsafe_remote_output="${TEST_DIR}/unsafe-remote.txt"
if PATH="${TEST_DIR}/bin:${PATH}" \
    ERP_RESET_ALLOWED_REMOTE_HOSTS="mongo.example.invalid" \
    "${RESET_SCRIPT}" \
    --config "${unsafe_remote_config}" \
    --execute \
    --confirm-db erp \
    --allow-remote >"${unsafe_remote_output}" 2>&1; then
    fail "无开发命名标记的远程数据库仍允许执行"
fi
grep -q "开发环境标记" "${unsafe_remote_output}" || fail "远程库命名门禁未给出受控错误"

if grep -En '(echo|printf)[^#\n]*MONGO_URI' "${RESET_SCRIPT}" >/dev/null; then
    fail "入口脚本存在直接输出 MONGO_URI 的静态风险"
fi
if grep -En 'console\.(log|error)\((uri|process\.env)|\$\{uri\}' "${MONGOSH_SCRIPT}" >/dev/null; then
    fail "mongosh 脚本存在直接输出 URI 的静态风险"
fi

echo "reset-dev-business-data 离线合同测试通过。"
