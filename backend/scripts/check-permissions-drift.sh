#!/usr/bin/env bash
# P0-6.2 权限生成物漂移校验脚本。
#
# apps/web-api/build.rs 会把权限定义写入两处生成物：
#   - backend/fronts/admin/src/constants/permissions.generated.ts
#   - erp-client/lib/permissions.generated.ts
# 本脚本先重新构建 web-api 触发重新生成，再逐个校验生成物与提交版本一致，
# 存在漂移（缺失/未纳入版本控制/内容不一致）时以非零状态退出。
set -euo pipefail

BACKEND_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
REPO_DIR="$(cd "${BACKEND_DIR}/.." && pwd)"

GENERATED_FILES=(
    "${BACKEND_DIR}/fronts/admin/src/constants/permissions.generated.ts"
    "${REPO_DIR}/erp-client/lib/permissions.generated.ts"
)

echo "重新构建 web-api 以生成权限定义…"
cargo build -p web-api --manifest-path "${BACKEND_DIR}/Cargo.toml"

failed=0
for file in "${GENERATED_FILES[@]}"; do
    echo "检查生成物: ${file}"
    if [[ ! -f "${file}" ]]; then
        echo "错误: 生成物缺失，请重新生成并提交 ${file}" >&2
        failed=1
        continue
    fi
    if ! git -C "${REPO_DIR}" ls-files --error-unmatch -- "${file}" >/dev/null 2>&1; then
        echo "错误: 生成物未纳入版本控制，请 git add 并提交 ${file}" >&2
        failed=1
        continue
    fi
    if ! git -C "${REPO_DIR}" diff --exit-code -- "${file}" >/dev/null; then
        echo "错误: 生成物与提交版本不一致（漂移），请重新提交 ${file}" >&2
        failed=1
    fi
done

if [[ "${failed}" -ne 0 ]]; then
    echo "权限生成物存在漂移，请在 PR 中同步提交生成文件。" >&2
    exit 1
fi

echo "权限生成物无漂移。"
