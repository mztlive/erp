#!/usr/bin/env bash
# 单个流程的完整 E2E 编排（每个 spec 文件即一个流程）：
#   1. 确保前后端服务已启动（已启动则复用）；
#   2. 停止 web-api（停写）→ 数据库 reset（清业务数据，保留账号/主数据，不填充种子）
#      → 重启 web-api 等待健康（重建索引/角色/RBAC）；
#   3. 发布审批定义（reset 会删除全部定义，按合同必须先发布才能开单）；
#   4. 执行对应 playwright spec。
#
# 用法:
#   bash scripts/run-flow.sh tests/flow-01-sales-warehouse.spec.ts
#   bash scripts/run-flow.sh all          # 依序运行全部 spec
#   E2E_RESET=0 bash scripts/run-flow.sh tests/xxx.spec.ts   # 跳过 reset（调试用）
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
E2E_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
RESET="${E2E_RESET:-1}"

run_one() {
    local spec="$1"
    local name
    name="$(basename "${spec}")"
    echo ""
    echo "############################################################"
    echo "# 流程: ${name}"
    echo "############################################################"

    bash "${SCRIPT_DIR}/ensure-services.sh"

    if [[ "${RESET}" == "1" ]]; then
        echo "-- 停止 web-api（停写） --"
        bash "${SCRIPT_DIR}/stop-backend.sh"
        echo "-- 数据库 reset --"
        bash "${SCRIPT_DIR}/reset-db.sh"
        echo "-- 重启 web-api --"
        bash "${SCRIPT_DIR}/restart-backend.sh"
        echo "-- 发布审批定义 --"
        node "${SCRIPT_DIR}/publish-approval-definitions.mjs"
    fi

    echo "-- 执行 playwright: ${spec} --"
    (cd "${E2E_DIR}" && npx playwright test "${spec}" --workers=1)
}

if [[ "${1:-}" == "all" ]]; then
    shopt -s nullglob
    for spec in "${E2E_DIR}"/tests/*.spec.ts; do
        # 跳过 _ 前缀的基础设施测试（非业务流程）
        [[ "$(basename "${spec}")" == _* ]] && continue
        run_one "${spec}"
    done
else
    [[ -n "${1:-}" ]] || { echo "用法: bash scripts/run-flow.sh <spec 文件|all>" >&2; exit 2; }
    run_one "$1"
fi
