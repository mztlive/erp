#!/usr/bin/env bash
# 单个流程的完整 E2E 编排（每个 spec 文件即一个流程）：
#   1. 确保前后端服务已启动（已启动则复用）；
#   2. reset-db.sh 以 ERP_RESET_ONLY=1 清空业务数据（保留账号/主数据/已发布审批定义，
#      不停止 web-api）；若健康检查失败才重启；
#   3. 发布审批定义（已发布则跳过）；全量模式默认使用独立数据库并行；
#   4. 执行对应 playwright spec。
#
# 用法:
#   bash scripts/run-flow.sh e2e/tests/flow-01-sales-warehouse.spec.ts
#   bash scripts/run-flow.sh tests/flow-01-sales-warehouse.spec.ts
#   bash scripts/run-flow.sh flow-01-sales-warehouse.spec.ts
#   bash scripts/run-flow.sh all          # 6 个独立数据库并行运行全部 spec
#   E2E_WORKERS=4 bash scripts/run-flow.sh all  # 调整隔离并行数
#   E2E_ISOLATE=0 bash scripts/run-flow.sh all  # 原共享开发库串行模式
#   E2E_RESET=0 bash scripts/run-flow.sh e2e/tests/xxx.spec.ts   # 跳过 reset（调试用）
#   E2E_ALLOW_REMOTE_RESET=1 bash scripts/run-flow.sh e2e/tests/xxx.spec.ts  # 远程开发库需显式放行
#   E2E_HEADED=1 bash scripts/run-flow.sh e2e/tests/xxx.spec.ts  # 有界面观察浏览器操作
#   E2E_HEADED=1 E2E_SLOW_MO=500 bash scripts/run-flow.sh e2e/tests/xxx.spec.ts  # 有界面 + 慢动作
#   E2E_FRONTEND=dev bash scripts/run-flow.sh ...   # 改连 next dev（3000）；默认连生产构建（3100）
#   E2E_FRONT_BUILD=1 bash scripts/run-flow.sh ...  # 强制重建前端（默认按源码时间戳自动判断）
#   E2E_TRACE=1 bash scripts/run-flow.sh ...        # 录制 trace，结束后自动输出慢步骤分析
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
E2E_DIR="${REPO_ROOT}/e2e"
RESET="${E2E_RESET:-1}"
HEADED="${E2E_HEADED:-0}"
SLOW_MO="${E2E_SLOW_MO:-}"
TRACE="${E2E_TRACE:-0}"
ISOLATE="${E2E_ISOLATE:-1}"
TARGET="${1:-all}"
PREFLIGHT_DONE=0
RUN_STARTED=${SECONDS}

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

prepare_env() {
    bash "${SCRIPT_DIR}/ensure-services.sh"

    if [[ "${PREFLIGHT_DONE}" == "0" ]]; then
        echo "-- 校验 E2E 基础账号 --"
        if ! (cd "${REPO_ROOT}" && node --input-type=module -e '
            import { ADMIN, login } from "./scripts/dev-seed-lib.mjs";
            try { await login(ADMIN.account, ADMIN.password); }
            catch (error) { console.error(error.message); process.exitCode = error.status === 401 ? 2 : 1; }
        '); then
            echo "错误: E2E 管理员登录检查失败，尚未执行本流程清库。新库或账号未初始化时请先运行以下命令；网络或限流错误请按上方原因处理。" >&2
            echo "初始化命令: E2E_RESET=1 E2E_ALLOW_REMOTE_RESET=1 bash scripts/reset-db.sh" >&2
            exit 2
        fi
        PREFLIGHT_DONE=1
    fi

    if [[ "${RESET}" == "1" ]]; then
        echo "-- 数据库 reset（E2E 快路径：保留审批定义，不停 web-api） --"
        E2E_RESET=1 ERP_RESET_ONLY=1 bash "${SCRIPT_DIR}/reset-db.sh"
        if ! curl -sf --max-time 3 http://127.0.0.1:10001/health >/dev/null; then
            echo "-- web-api 未就绪，重启 --"
            bash "${SCRIPT_DIR}/restart-backend.sh"
        fi
        echo "-- 发布审批定义（已发布则跳过） --"
        node "${SCRIPT_DIR}/publish-approval-definitions.mjs"
        RESET=0
    fi
}

# E2E_TRACE=1 时分析本次 test-results 里的 trace（Playwright 每次运行前会清空 test-results）。
report_trace() {
    if [[ "${TRACE}" == "1" ]]; then
        echo ""
        echo "-- trace 慢步骤分析 --"
        (cd "${E2E_DIR}" && node scripts/trace-slow-steps.mjs) || true
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

    echo ""
    echo "############################################################"
    echo "# 流程: ${name}"
    echo "############################################################"

    prepare_env

    if [[ -n "${SLOW_MO}" ]]; then
        echo "-- 慢动作: E2E_SLOW_MO=${SLOW_MO}ms（playwright.config launchOptions.slowMo） --"
        export E2E_SLOW_MO
    fi
    if [[ "${TRACE}" == "1" ]]; then
        pw_args+=(--trace on)
    fi
    echo "-- 执行 playwright: ${pw_args[*]} --"
    local status=0
    (cd "${E2E_DIR}" && npx playwright test "${pw_args[@]}") || status=$?
    report_trace
    return "${status}"
}

if [[ "${TARGET}" == "all" ]]; then
    shopt -s nullglob
    specs=("${E2E_DIR}"/tests/*.spec.ts)
    if (( ${#specs[@]} == 0 )); then
        echo "未找到 ${E2E_DIR}/tests/*.spec.ts" >&2
        exit 1
    fi
    if [[ "${ISOLATE}" == "1" ]]; then
        echo "-- 全量流程：独立数据库/API 并行，开发库仅作只读基线 --"
        E2E_SKIP_BACKEND=1 bash "${SCRIPT_DIR}/ensure-services.sh"
        status=0
        python3 "${SCRIPT_DIR}/run-e2e-parallel.py" all || status=$?
        echo "-- 全量编排总耗时（含服务准备）：$((SECONDS - RUN_STARTED))s --"
        exit "${status}"
    fi
    [[ "${ISOLATE}" == "0" ]] || { echo "错误: E2E_ISOLATE 只能是 0 或 1" >&2; exit 2; }
    echo ""
    echo "############################################################"
    echo "# 全量流程：reset 一次后一次跑 Playwright"
    echo "############################################################"
    prepare_env
    pw_args=(--workers=1)
    if [[ "${HEADED}" == "1" ]]; then
        pw_args+=(--headed)
    fi
    if [[ -n "${SLOW_MO}" ]]; then
        echo "-- 慢动作: E2E_SLOW_MO=${SLOW_MO}ms（playwright.config launchOptions.slowMo） --"
        export E2E_SLOW_MO
    fi
    if [[ "${TRACE}" == "1" ]]; then
        pw_args+=(--trace on)
    fi
    echo "-- 执行 playwright: ${pw_args[*]} --"
    status=0
    (cd "${E2E_DIR}" && npx playwright test "${pw_args[@]}") || status=$?
    report_trace
    exit "${status}"
else
    run_one "${TARGET}"
fi
