#!/usr/bin/env bash
# 单个流程的完整 E2E 编排（每个 spec 文件即一个流程）：
#   1. 确保前后端服务已启动（已启动则复用）；
#   2. reset-db.sh 以 ERP_RESET_ONLY=1 停止 web-api 并只清业务数据（保留账号/主数据）
#      → 重启 web-api 等待健康（重建索引/角色/RBAC）；
#   3. 发布审批定义（reset 会删除全部定义，按合同必须先发布才能开单）；
#   4. 执行对应 playwright spec。
#
# 用法:
#   bash scripts/run-flow.sh e2e/tests/flow-01-sales-warehouse.spec.ts
#   bash scripts/run-flow.sh tests/flow-01-sales-warehouse.spec.ts
#   bash scripts/run-flow.sh flow-01-sales-warehouse.spec.ts
#   bash scripts/run-flow.sh all          # 依序运行全部 spec
#   E2E_RESET=0 bash scripts/run-flow.sh e2e/tests/xxx.spec.ts   # 跳过 reset（调试用）
#   E2E_ALLOW_REMOTE_RESET=1 bash scripts/run-flow.sh e2e/tests/xxx.spec.ts  # 远程开发库需显式放行
#   E2E_HEADED=1 bash scripts/run-flow.sh e2e/tests/xxx.spec.ts  # 有界面观察浏览器操作
#   E2E_HEADED=1 E2E_SLOW_MO=500 bash scripts/run-flow.sh e2e/tests/xxx.spec.ts  # 有界面 + 慢动作
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
E2E_DIR="${REPO_ROOT}/e2e"
RESET="${E2E_RESET:-1}"
HEADED="${E2E_HEADED:-0}"
SLOW_MO="${E2E_SLOW_MO:-}"

if [[ ! -f "${E2E_DIR}/playwright.config.ts" ]]; then
    echo "找不到 ${E2E_DIR}/playwright.config.ts，Playwright 工程应在 e2e/ 目录。" >&2
    exit 1
fi

# 把调用方传入的 spec 解析成 e2e/ 下的绝对路径。
# 兼容仓库根、e2e/、tests/ 以及只给文件名。
resolve_spec() {
    local spec="$1"
    local base
    local candidate
    local abs
    base="$(basename "${spec}")"
    for candidate in \
        "${spec}" \
        "${REPO_ROOT}/${spec}" \
        "${E2E_DIR}/${spec}" \
        "${E2E_DIR}/tests/${spec}" \
        "${E2E_DIR}/tests/${base}"
    do
        if [[ -f "${candidate}" ]]; then
            abs="$(cd "$(dirname "${candidate}")" && pwd)/$(basename "${candidate}")"
            printf '%s\n' "${abs}"
            return 0
        fi
    done
    echo "找不到 spec: ${spec}" >&2
    echo "请传入 e2e/tests/*.spec.ts（或文件名 / tests/<文件名>）。" >&2
    return 1
}

# Playwright 在 e2e/ 目录执行；参数尽量用相对该目录的路径。
spec_for_playwright() {
    local abs="$1"
    if [[ "${abs}" == "${E2E_DIR}/"* ]]; then
        printf '%s\n' "${abs#"${E2E_DIR}/"}"
    else
        printf '%s\n' "${abs}"
    fi
}

run_one() {
    local spec_abs
    local spec_arg
    local name
    local -a pw_args
    spec_abs="$(resolve_spec "$1")"
    spec_arg="$(spec_for_playwright "${spec_abs}")"
    name="$(basename "${spec_abs}")"
    pw_args=("${spec_arg}" --workers=1)
    if [[ "${HEADED}" == "1" ]]; then
        pw_args+=(--headed)
    fi
    # E2E_SLOW_MO 由 e2e/playwright.config.ts 读取为 launchOptions.slowMo；
    # Playwright Test CLI 没有 --slow-mo，不能拼进 npx playwright test。

    echo ""
    echo "############################################################"
    echo "# 流程: ${name}"
    echo "############################################################"

    bash "${SCRIPT_DIR}/ensure-services.sh"

    if [[ "${RESET}" == "1" ]]; then
        echo "-- 数据库 reset（E2E 只清库） --"
        E2E_RESET=1 ERP_RESET_ONLY=1 bash "${SCRIPT_DIR}/reset-db.sh"
        echo "-- 重启 web-api --"
        bash "${SCRIPT_DIR}/restart-backend.sh"
        echo "-- 发布审批定义 --"
        node "${SCRIPT_DIR}/publish-approval-definitions.mjs"
    fi

    echo "-- 执行 playwright: ${pw_args[*]} --"
    if [[ -n "${SLOW_MO}" ]]; then
        echo "-- 慢动作: E2E_SLOW_MO=${SLOW_MO}ms（playwright.config launchOptions.slowMo） --"
        export E2E_SLOW_MO
    fi
    (cd "${E2E_DIR}" && npx playwright test "${pw_args[@]}")
}

if [[ "${1:-}" == "all" ]]; then
    shopt -s nullglob
    specs=("${E2E_DIR}"/tests/*.spec.ts)
    if (( ${#specs[@]} == 0 )); then
        echo "未找到 ${E2E_DIR}/tests/*.spec.ts" >&2
        exit 1
    fi
    for spec in "${specs[@]}"; do
        # 跳过 _ 前缀的基础设施测试（非业务流程）
        [[ "$(basename "${spec}")" == _* ]] && continue
        run_one "${spec}"
    done
else
    [[ -n "${1:-}" ]] || { echo "用法: bash scripts/run-flow.sh <spec 文件|all>" >&2; exit 2; }
    run_one "$1"
fi
