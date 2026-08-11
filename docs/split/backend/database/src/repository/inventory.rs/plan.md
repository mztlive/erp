# `backend/database/src/repository/inventory.rs` 拆分分析

## 文件信息

| 项目 | 内容 |
|---|---|
| 源文件 | `backend/database/src/repository/inventory.rs` |
| 扫描行数 | 1276 |
| 分析状态 | 已完成深入分析 |
| 拆分结论 | split |
| 预估工作量 | M |
| 风险 | medium |
| 生成来源 | workflow `analyze-large-files` |
| 生成日期 | 2026-08-11 |

## 拆分方案

- 结论：**split**（工作量 M，风险 medium）
- 摘要：建议按 purchase_order/ 的仓库目录模式，将单体 inventory.rs 改为 inventory/mod.rs 加 movement.rs、balance.rs、reservation.rs、adjustment.rs、common.rs。四个集合各自收拢 Row、Filter、trait impl、Repository impl 和投影 helper；InventoryRepository 的跨集合批量查询与事务写入保留在模块根；通用 BSON、排序和批量查询 helper 下沉到 common.rs。拆分后各文件预计约 150-400 行，均可控制在 800 行以内，同时保持 InventoryExt 的现有路径和 Service 侧关联类型用法不变。实施重点是正确处理 mod.rs re-export、pub(super) helper 可见性，以及删除原 inventory.rs 以避免 Rust 模块冲突。
- 拆分建议：
  - **backend/database/src/repository/inventory/mod.rs**：作为 inventory 仓储模块根：保留域级模块文档；声明 adjustment、balance、common、movement、reservation 子模块；通过 pub use 重新导出 StockAdjustmentFilter、StockBalanceFilter、StockMovementFilter、StockReservationFilter；保留集合常量 STOCK_MOVEMENTS、STOCK_RESERVATION_ENTRIES、STOCK_ADJUSTMENT_LINES；定义 InventoryRepository<'a> 及方法 new、movements_by_ids、reservation_entries_by_reservation_ids、adjustment_lines_by_adjustment_ids、create_stock_adjustment_with_lines。
    - 依赖/注意：结构与 purchase_order/mod.rs 保持一致。四个 Filter 必须在此 pub use，否则 repository/extensions/inventory.rs 中 super::super::inventory::{...} 的现有导入会失效；InventoryRepository 继续由 InventoryExt::inventory() 返回。应删除原 inventory.rs，不能与 inventory/mod.rs 同时存在。mod.rs 可从 common 导入 find_by_field_in 和 ids_to_strings；common 不应反向依赖任何领域子模块，避免循环依赖。
  - **backend/database/src/repository/inventory/movement.rs**：放置 StockMovementRow、StockMovementFilter、impl QueryFilter for StockMovementFilter、impl Pagination for StockMovementFilter、impl Repository<'a, StockMovement>，包括 search_stock_movements、find_by_source_document；放置私有 stock_movement_projection；内联测试 movement_filter_applies_dimensions_type_range_and_deleted_filter。
    - 依赖/注意：通过 super::common::sort_doc 使用共享排序 helper；使用 crate::repository::{PageResult, Pagination, QueryFilter}、crate::{mongo_ops, Repository, Result}。StockMovementFilter 保持 pub，并由 mod.rs re-export；StockMovementRow 保持 pub 以满足公开查询方法返回类型，但无需在 mod.rs 再导出，沿用 purchase_order/order.rs 的 Row 可见性模式。
  - **backend/database/src/repository/inventory/balance.rs**：放置 StockBalanceRow、StockBalanceFilter、impl QueryFilter for StockBalanceFilter、impl Pagination for StockBalanceFilter、impl Repository<'a, StockBalance>；具体方法为 search_stock_balances、find_by_dimensions、increase_on_hand、reserve_quantity、deduct_available、release_reserved；放置私有 stock_balance_projection。
    - 依赖/注意：余额原子写依赖 common 中的 to_bson、both_inc、both_dec、cross_inc 和 sort_doc，这些 helper 需使用 pub(super) 可见性。继续保留 filter 内的 available_quantity/reserved_quantity 条件写，不能在拆分时改成先读后写。该文件仅依赖实体 StockBalance，不应导入 InventoryRepository，以避免无必要的模块耦合。
  - **backend/database/src/repository/inventory/reservation.rs**：放置 StockReservationRow、StockReservationFilter、impl QueryFilter for StockReservationFilter、impl Pagination for StockReservationFilter、impl Repository<'a, StockReservation>；具体方法为 search_stock_reservations、consume_quantity、release_quantity；放置私有 stock_reservation_projection。
    - 依赖/注意：通过 super::common::{negate_bson, sort_doc, to_bson} 复用 BSON helper；std::str::FromStr 与 chrono::Local 应移动到本文件，因为 release_quantity 和状态更新时间仍需要它们。consume_quantity 的两步状态收敛语义及 executor 传递必须原样保留。StockReservationFilter 由 mod.rs re-export。
  - **backend/database/src/repository/inventory/adjustment.rs**：放置 StockAdjustmentRow、StockAdjustmentFilter、impl QueryFilter for StockAdjustmentFilter、impl Pagination for StockAdjustmentFilter、impl Repository<'a, StockAdjustment>；具体方法为 search_stock_adjustments、find_by_adjustment_no；放置私有 stock_adjustment_projection。
    - 依赖/注意：通过 super::common::sort_doc 使用统一排序白名单逻辑。跨集合方法 adjustment_lines_by_adjustment_ids 和 create_stock_adjustment_with_lines 留在 mod.rs 的 InventoryRepository 中，保持 purchase_order/mod.rs 将聚合事务写入放在模块根的仓库模式；StockAdjustmentFilter 必须由 mod.rs re-export。
  - **backend/database/src/repository/inventory/common.rs**：放置域内共享 helper：ids_to_strings、find_by_field_in、to_bson、negate_bson、both_inc、both_dec、cross_inc、sort_doc；放置对应测试 sort_doc_maps_whitelisted_fields_and_defaults_otherwise、negate_bson_flips_sign_without_touching_magnitude、ids_to_strings_converts_newtype_collection。
    - 依赖/注意：这些函数从原来的模块私有 fn 调整为 pub(super) fn，供 mod.rs 和各领域子文件共享，但不暴露到 inventory 模块之外。common 只能依赖 Database、Executor、mongo_ops、Quantity、BSON 和通用 Result，不得导入 movement、balance、reservation、adjustment 或 InventoryRepository，避免潜在循环依赖。to_bson 上的 #[allow(deprecated)] 及非 human-readable Decimal128 序列化配置必须保留。

## 实施约束

- 拆分应保持现有公开 API 和 re-export 路径，除非实施前另行确认契约变更。
- Service 继续负责事务边界；Repository 只负责数据访问；纯业务规则优先下沉到 Entity 或 Value Object。
- 私有 helper 应跟随唯一调用方迁移；仅在同一领域多个子模块复用时使用 `pub(super)`。
- 拆分完成后执行 `cargo fmt --all -- --check`、`cargo check --workspace`、`cargo clippy --workspace --all-targets --all-features` 和 `cargo test --workspace`。
