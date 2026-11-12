#!/usr/bin/env bash
# 独立执行与 server build.rs 相同的生成器，检查保存的目录，无需编译业务 crate。
set -euo pipefail

BACKEND_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
python3 "${BACKEND_DIR}/scripts/generate-permissions.py" --check
