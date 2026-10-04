#!/usr/bin/env bash
# 拒绝业务审计反查、绕过统一写入与未登记的审计执行入口。
set -euo pipefail
BACKEND_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
python3 "${BACKEND_DIR}/scripts/audit_boundaries.py" --backend "${BACKEND_DIR}" "$@"
