#!/usr/bin/env bash
# 生产 Rust 源码体积门禁。失败关闭，不允许忽略路径或扩大债务基线。
#
# 单文件生产代码不超过 800 物理行（扣除测试）；单方法不超过 50 有效行
# （空行和纯注释不计入方法体）。测试、`#[cfg(test)]`、`#[test]` 与 `build.rs`
# 方法体除外。Clippy `too_many_lines` 不能按「去掉测试」限制文件行数。
set -euo pipefail

BACKEND_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export PYTHONDONTWRITEBYTECODE=1
python3 "${BACKEND_DIR}/scripts/rust_size_gate.py" --backend "${BACKEND_DIR}" "$@"
