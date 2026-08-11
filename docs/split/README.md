# 后端大文件拆分报告与方案

## 扫描概览

- 扫描根目录：`backend`
- 完整大文件清单阈值：`>= 800` 行，共 **52** 个文件
- 深入分析阈值：`>= 1000` 行，共 **29** 个文件
- 成功生成详细方案：**28** 个文件
- 分析失败、待补充：**1** 个文件
- 仅列入清单、未深入分析：**23** 个文件

> 本目录由 workflow `analyze-large-files` 的完整输出落盘生成。每个源文件对应一个独立目录，目录中使用 `plan.md` 保存该文件的报告或分析状态。

## 目录约定

源文件路径会原样镜像到 `docs/split/` 下，并将源文件名作为目录名，以避免同名文件冲突。

```text
backend/services/src/sales_order/mod.rs
└── docs/split/backend/services/src/sales_order/mod.rs/plan.md
```

## 文件清单

| # | 文件 | 行数 | 状态 | 结论 | 工作量 | 风险 |
|---:|---|---:|---|---|---|---|
| 1 | [`backend/services/src/sales_order/mod.rs`](backend/services/src/sales_order/mod.rs/plan.md) | 2231 | 已完成深入分析 | split | L | medium |
| 2 | [`backend/services/src/catalog/dto.rs`](backend/services/src/catalog/dto.rs/plan.md) | 1961 | 已完成深入分析 | split | M | medium |
| 3 | [`backend/services/src/supplier/profile.rs`](backend/services/src/supplier/profile.rs/plan.md) | 1952 | 已完成深入分析 | split | L | medium |
| 4 | [`backend/services/src/mall_order/mod.rs`](backend/services/src/mall_order/mod.rs/plan.md) | 1891 | 已完成深入分析 | split | L | medium |
| 5 | [`backend/services/src/customer/profile.rs`](backend/services/src/customer/profile.rs/plan.md) | 1791 | 已完成深入分析 | split | L | medium |
| 6 | [`backend/services/src/supplier_fulfillment/mod.rs`](backend/services/src/supplier_fulfillment/mod.rs/plan.md) | 1714 | 已完成深入分析 | split | M | medium |
| 7 | [`backend/services/src/iam/rbac.rs`](backend/services/src/iam/rbac.rs/plan.md) | 1687 | 已完成深入分析 | split | L | medium |
| 8 | [`backend/services/src/returns/mod.rs`](backend/services/src/returns/mod.rs/plan.md) | 1625 | 已完成深入分析 | split | M | medium |
| 9 | [`backend/services/src/inventory/mod.rs`](backend/services/src/inventory/mod.rs/plan.md) | 1407 | 分析失败，待补充 | 分析失败 | - | - |
| 10 | [`backend/services/src/integration_ops/mod.rs`](backend/services/src/integration_ops/mod.rs/plan.md) | 1360 | 已完成深入分析 | split | M | medium |
| 11 | [`backend/database/src/repository/receivable.rs`](backend/database/src/repository/receivable.rs/plan.md) | 1354 | 已完成深入分析 | split | M | medium |
| 12 | [`backend/services/src/receivable/mod.rs`](backend/services/src/receivable/mod.rs/plan.md) | 1353 | 已完成深入分析 | split | M | medium |
| 13 | [`backend/database/src/repository/mall_order.rs`](backend/database/src/repository/mall_order.rs/plan.md) | 1328 | 已完成深入分析 | split | M | medium |
| 14 | [`backend/database/src/repository/inventory.rs`](backend/database/src/repository/inventory.rs/plan.md) | 1276 | 已完成深入分析 | split | M | medium |
| 15 | [`backend/database/src/repository/supplier.rs`](backend/database/src/repository/supplier.rs/plan.md) | 1214 | 已完成深入分析 | split | M | medium |
| 16 | [`backend/database/src/repository/mall_sync.rs`](backend/database/src/repository/mall_sync.rs/plan.md) | 1212 | 已完成深入分析 | split | M | medium |
| 17 | [`backend/services/src/mall_sync/mod.rs`](backend/services/src/mall_sync/mod.rs/plan.md) | 1160 | 已完成深入分析 | split | M | medium |
| 18 | [`backend/database/src/repository/party.rs`](backend/database/src/repository/party.rs/plan.md) | 1151 | 已完成深入分析 | split | M | medium |
| 19 | [`backend/database/src/repository/fulfillment.rs`](backend/database/src/repository/fulfillment.rs/plan.md) | 1145 | 已完成深入分析 | split | M | low |
| 20 | [`backend/entities/src/sales_order/working_copy.rs`](backend/entities/src/sales_order/working_copy.rs/plan.md) | 1139 | 已完成深入分析 | split | M | low |
| 21 | [`backend/services/src/integration_ops/dto.rs`](backend/services/src/integration_ops/dto.rs/plan.md) | 1122 | 已完成深入分析 | split | M | low |
| 22 | [`backend/database/src/repository/mall_after_sales.rs`](backend/database/src/repository/mall_after_sales.rs/plan.md) | 1103 | 已完成深入分析 | split | M | medium |
| 23 | [`backend/entities/src/supplier_fulfillment/fulfillment_order.rs`](backend/entities/src/supplier_fulfillment/fulfillment_order.rs/plan.md) | 1078 | 已完成深入分析 | split | M | low |
| 24 | [`backend/services/src/publication/mod.rs`](backend/services/src/publication/mod.rs/plan.md) | 1060 | 已完成深入分析 | split | M | medium |
| 25 | [`backend/database/src/repository/payable.rs`](backend/database/src/repository/payable.rs/plan.md) | 1046 | 已完成深入分析 | split | M | medium |
| 26 | [`backend/services/src/legacy_import/mod.rs`](backend/services/src/legacy_import/mod.rs/plan.md) | 1030 | 已完成深入分析 | split | M | medium |
| 27 | [`backend/services/src/mall_after_sales/mod.rs`](backend/services/src/mall_after_sales/mod.rs/plan.md) | 1017 | 已完成深入分析 | split | M | medium |
| 28 | [`backend/services/src/projection/mod.rs`](backend/services/src/projection/mod.rs/plan.md) | 1013 | 已完成深入分析 | split | M | medium |
| 29 | [`backend/services/src/mall_sync/dto.rs`](backend/services/src/mall_sync/dto.rs/plan.md) | 1003 | 已完成深入分析 | split | M | medium |
| 30 | [`backend/entities/src/source_registry/mod.rs`](backend/entities/src/source_registry/mod.rs/plan.md) | 995 | 仅纳入大文件清单，未深入分析 | 未深入分析 | - | - |
| 31 | [`backend/services/src/fulfillment/dto.rs`](backend/services/src/fulfillment/dto.rs/plan.md) | 987 | 仅纳入大文件清单，未深入分析 | 未深入分析 | - | - |
| 32 | [`backend/database/src/repository/integration_ops.rs`](backend/database/src/repository/integration_ops.rs/plan.md) | 977 | 仅纳入大文件清单，未深入分析 | 未深入分析 | - | - |
| 33 | [`backend/services/src/supplier_offering/mod.rs`](backend/services/src/supplier_offering/mod.rs/plan.md) | 931 | 仅纳入大文件清单，未深入分析 | 未深入分析 | - | - |
| 34 | [`backend/entities/src/sales_order/sales_order.rs`](backend/entities/src/sales_order/sales_order.rs/plan.md) | 929 | 仅纳入大文件清单，未深入分析 | 未深入分析 | - | - |
| 35 | [`backend/services/src/mall_order/dto.rs`](backend/services/src/mall_order/dto.rs/plan.md) | 909 | 仅纳入大文件清单，未深入分析 | 未深入分析 | - | - |
| 36 | [`backend/services/src/supplier_settlement/mod.rs`](backend/services/src/supplier_settlement/mod.rs/plan.md) | 893 | 仅纳入大文件清单，未深入分析 | 未深入分析 | - | - |
| 37 | [`backend/apps/web-api/build.rs`](backend/apps/web-api/build.rs/plan.md) | 876 | 仅纳入大文件清单，未深入分析 | 未深入分析 | - | - |
| 38 | [`backend/services/src/iam/predefined_roles.rs`](backend/services/src/iam/predefined_roles.rs/plan.md) | 875 | 仅纳入大文件清单，未深入分析 | 未深入分析 | - | - |
| 39 | [`backend/entities/src/integration_ops/integration_error_task.rs`](backend/entities/src/integration_ops/integration_error_task.rs/plan.md) | 873 | 仅纳入大文件清单，未深入分析 | 未深入分析 | - | - |
| 40 | [`backend/database/src/repository/returns.rs`](backend/database/src/repository/returns.rs/plan.md) | 873 | 仅纳入大文件清单，未深入分析 | 未深入分析 | - | - |
| 41 | [`backend/entities/src/purchase_order/change_order.rs`](backend/entities/src/purchase_order/change_order.rs/plan.md) | 870 | 仅纳入大文件清单，未深入分析 | 未深入分析 | - | - |
| 42 | [`backend/services/src/payable/mod.rs`](backend/services/src/payable/mod.rs/plan.md) | 865 | 仅纳入大文件清单，未深入分析 | 未深入分析 | - | - |
| 43 | [`backend/entities/src/mall_sync/mall_sales_reconciliation.rs`](backend/entities/src/mall_sync/mall_sales_reconciliation.rs/plan.md) | 829 | 仅纳入大文件清单，未深入分析 | 未深入分析 | - | - |
| 44 | [`backend/database/src/repository/legacy_import.rs`](backend/database/src/repository/legacy_import.rs/plan.md) | 817 | 仅纳入大文件清单，未深入分析 | 未深入分析 | - | - |
| 45 | [`backend/entities/src/fulfillment/delivery.rs`](backend/entities/src/fulfillment/delivery.rs/plan.md) | 811 | 仅纳入大文件清单，未深入分析 | 未深入分析 | - | - |
| 46 | [`backend/entities/src/sales_order/submission.rs`](backend/entities/src/sales_order/submission.rs/plan.md) | 809 | 仅纳入大文件清单，未深入分析 | 未深入分析 | - | - |
| 47 | [`backend/services/src/access_control/dto.rs`](backend/services/src/access_control/dto.rs/plan.md) | 807 | 仅纳入大文件清单，未深入分析 | 未深入分析 | - | - |
| 48 | [`backend/entities/src/sales_order/revision.rs`](backend/entities/src/sales_order/revision.rs/plan.md) | 807 | 仅纳入大文件清单，未深入分析 | 未深入分析 | - | - |
| 49 | [`backend/services/src/receivable/dto.rs`](backend/services/src/receivable/dto.rs/plan.md) | 806 | 仅纳入大文件清单，未深入分析 | 未深入分析 | - | - |
| 50 | [`backend/services/src/supplier/dto.rs`](backend/services/src/supplier/dto.rs/plan.md) | 804 | 仅纳入大文件清单，未深入分析 | 未深入分析 | - | - |
| 51 | [`backend/services/src/supplier_api/mod.rs`](backend/services/src/supplier_api/mod.rs/plan.md) | 802 | 仅纳入大文件清单，未深入分析 | 未深入分析 | - | - |
| 52 | [`backend/entities/src/sales_order/types.rs`](backend/entities/src/sales_order/types.rs/plan.md) | 802 | 仅纳入大文件清单，未深入分析 | 未深入分析 | - | - |

## 使用建议

1. 优先处理风险为 `low` 的 Entity/Repository 文件，验证拆分模式和质量门禁。
2. 再处理风险为 `medium`、工作量为 `M` 的 Service/Repository 文件。
3. 核心订单、供应商、客户与 RBAC 等工作量为 `L` 的文件应独立分批实施，避免一次改动覆盖多个关键领域。
4. `backend/services/src/inventory/mod.rs` 当前没有有效方案，实施前必须补充分析。
5. 对未深入分析的 23 个文件，不应仅因行数机械拆分；应在功能改动触及时按职责边界重新评估。

## 通用验收要求

- 不改变现有公开 API、DTO 契约和 re-export 路径，除非提前确认兼容策略。
- 不改变 Service 事务边界、幂等语义、审计写入顺序和外部调用位置。
- 避免 N+1、额外集合扫描和不必要的跨模块公开可见性。
- 私有 helper 跟随唯一调用方；跨领域纯规则优先下沉到 Entity 或 Value Object。
- 完成拆分后运行完整 Rust 质量门禁。
