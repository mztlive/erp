# `backend/entities/src/supplier_fulfillment/fulfillment_order.rs` 拆分分析

## 文件信息

| 项目 | 内容 |
|---|---|
| 源文件 | `backend/entities/src/supplier_fulfillment/fulfillment_order.rs` |
| 扫描行数 | 1078 |
| 分析状态 | 已完成深入分析 |
| 拆分结论 | split |
| 预估工作量 | M |
| 风险 | low |
| 生成来源 | workflow `analyze-large-files` |
| 生成日期 | 2026-08-11 |

## 拆分方案

- 结论：**split**（工作量 M，风险 low）
- 摘要：建议按供应商履约域内的聚合边界拆分：原文件继续承载 SupplierFulfillmentOrder 订单聚合及其里程碑时间、状态推进和敏感字段脱敏规则；三条正交状态机迁入独立 status.rs；SupplierFulfillmentItem 明细聚合及金额快照校验迁入 fulfillment_item.rs。该结构符合仓库 entities 按域目录、按聚合或状态类型分文件并由 mod.rs 统一 re-export 的既有模式，拆分后每个文件预计均低于 800 行。
- 拆分建议：
  - **backend/entities/src/supplier_fulfillment/status.rs**：放置 FulfillmentStatus、CancelStatus、RefundStatus 三个公开枚举；对应的 inherent impl，包括 FulfillmentStatus::label、FulfillmentStatus::as_str、FulfillmentStatus::is_terminal、CancelStatus::label、CancelStatus::as_str、RefundStatus::label、RefundStatus::as_str；以及三个 DocumentState impl 的 allowed_next。状态机纯规则测试可一并迁入，包括 terminal_states_are_absorbing 中不依赖订单实体的断言、exception_branches_and_result_unknown_resolution，以及取消和退款测试中的直接 ensure_transition 断言。
    - 依赖/注意：仅依赖 serde::{Deserialize, Serialize} 与 crate::common::state::DocumentState。fulfillment_order.rs 应通过 super::status 导入三个状态类型；status_history.rs 的 super::fulfillment_order::FulfillmentStatus 应改为 super::status::FulfillmentStatus。为兼容旧的 fulfillment_order 深层导入路径，可在 fulfillment_order.rs 中 pub use super::status::{CancelStatus, FulfillmentStatus, RefundStatus}。status.rs 不得反向引用 SupplierFulfillmentOrder，避免形成循环依赖。
  - **backend/entities/src/supplier_fulfillment/fulfillment_item.rs**：放置 SUPPLIER_ITEM_CODE_MAX_LEN、SupplierFulfillmentItemData、SupplierFulfillmentItem、SupplierFulfillmentItem::new、私有 helper ensure_positive_quantity 与 ensure_snapshot_consistent；同时放置 sample_item_data 和 item_new_accepts_valid_item_and_keeps_snapshot、item_new_rejects_blank_supplier_sku_snapshot、item_new_rejects_non_positive_quantity、item_new_rejects_inconsistent_cost_snapshot、item_new_rejects_over_scale_money 五个单元测试。
    - 依赖/注意：ensure_positive_quantity 和 ensure_snapshot_consistent 必须随明细实体迁移并继续保持私有；所需依赖为 Decimal、BaseModel、Entity、Serde、MallOrderItemId、SupplierFulfillmentItemId、SupplierFulfillmentOrderId、SupplierOfferingRevisionId、Amount、Quantity、Rate、UnitPrice、round_to_cent 和文本规范化函数。该模块只持有 SupplierFulfillmentOrderId，不应引用 SupplierFulfillmentOrder 类型，因此不会与 fulfillment_order.rs 形成循环依赖。mod.rs 应从本模块 re-export 两个公开类型；如需保持旧路径兼容，可由 fulfillment_order.rs 再次 re-export 这两个类型。

## 实施约束

- 拆分应保持现有公开 API 和 re-export 路径，除非实施前另行确认契约变更。
- Service 继续负责事务边界；Repository 只负责数据访问；纯业务规则优先下沉到 Entity 或 Value Object。
- 私有 helper 应跟随唯一调用方迁移；仅在同一领域多个子模块复用时使用 `pub(super)`。
- 拆分完成后执行 `cargo fmt --all -- --check`、`cargo check --workspace`、`cargo clippy --workspace --all-targets --all-features` 和 `cargo test --workspace`。
