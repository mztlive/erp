#!/usr/bin/env bash
# 领域 crate 依赖图与源码边界检查。失败关闭，不允许扩大债务基线或忽略路径。
#
# 规则覆盖 Cargo metadata 的 normal/build/dev 边、重命名依赖、grouped/pub use、
# 实体纯度、Service/Process 原始 Mongo 操作、仓储集合隔离、单实现、已验收清零与 BPM 纯度。
# 每条规则带正/负夹具；尚无新领域 crate 时不得把领域隔离写成已通过。
set -euo pipefail

BACKEND_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
python3 "${BACKEND_DIR}/scripts/domain_boundaries.py" --backend "${BACKEND_DIR}" "$@"
