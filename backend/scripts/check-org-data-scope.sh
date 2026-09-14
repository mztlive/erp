#!/usr/bin/env bash
# 合同第 9 节源码门禁。规则自检、既有领域门禁与范围检查任一失败即阻断。
# 只读取工作区与 Cargo metadata，不启动服务，不访问业务数据库。
set -euo pipefail

BACKEND_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export PYTHONDONTWRITEBYTECODE=1
python3 "${BACKEND_DIR}/scripts/org_data_scope_gate.py" --backend "${BACKEND_DIR}" "$@"
