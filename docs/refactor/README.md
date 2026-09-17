# 后端 crates 重构优化清单（不改变功能行为）

落盘位置：`docs/refactor/`。

- 每个 crate 一份 JSON：`docs/refactor/<crate>.json`，是状态的唯一真相源。
- `index.json`：31 个 crate 的汇总索引（文件数、条目数、高/中/低分布、待办数）。
- `schema.json`：JSON 字段约束说明（人类可读）。
- 本目录只做重构跟踪，不放业务基线；业务基线仍以 `docs/erp-phase-1.md`、`docs/erp-phase-2.md` 与各 `*-contract.md` 为准。

## 单条目字段

```json
{
  "id": "erp-sales-001",
  "file": "src/service/sales_order/mapper.rs",
  "lines": "333-357",
  "category": "重复",
  "severity": "高",
  "title": "行视图映射重复",
  "current": "现状描述",
  "proposal": "重构方向，不给完整代码",
  "status": "pending",
  "updated_at": "2026-09-17",
  "note": ""
}
```

- `category` 限用：`重复` / `复杂度` / `可读性` / `错误处理` / `类型建模` / `性能隐患` / `边界分层` / `测试性`。
- `severity` 限用：`高` / `中` / `低`。
- `status` 限用：
  - `pending`：待办（初始值，落盘时全部为此值）；
  - `in_progress`：进行中；
  - `done`：已修改并通过该 crate 定向门禁；
  - `wont_fix`：经确认不修，需在 `note` 写理由。
- 改完一条，只改该条的 `status` / `updated_at` / `note`，不要重排 `id`，不要删除历史条目。
- `id` 一旦分配永久不变，格式 `<crate>-NNN`（从 001 递增）。

## 更新流程

1. 认领时把 `status` 置为 `in_progress`。
2. 改完后跑对应 crate 的定向检查（`cargo fmt`、`cargo check -p <crate>`、`cargo test -p <crate> --lib`，涉及边界再跑对应 `check-*.sh`）。
3. 通过后置为 `done` 并填 `note`（PR 号 / 验证命令）。
4. 不修的置为 `wont_fix` 并在 `note` 写理由。

## 质量口径

- 生产文件 ≤800 行、生产方法 ≤50 有效行（`backend/scripts/check-rust-size.sh` 口径）。
- 重构均不改变功能行为；文档与代码冲突时先指出，不默默选边。
