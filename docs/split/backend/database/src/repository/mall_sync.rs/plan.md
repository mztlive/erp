# `backend/database/src/repository/mall_sync.rs` 拆分分析

## 文件信息

| 项目 | 内容 |
|---|---|
| 源文件 | `backend/database/src/repository/mall_sync.rs` |
| 扫描行数 | 1212 |
| 分析状态 | 已完成深入分析 |
| 拆分结论 | split |
| 预估工作量 | M |
| 风险 | medium |
| 生成来源 | workflow `analyze-large-files` |
| 生成日期 | 2026-08-11 |

## 拆分方案

- 结论：**split**（工作量 M，风险 medium）
- 摘要：建议参照 database/src/repository/purchase_order/ 的既有模式，将 1212 行单文件改为 mall_sync/ 目录：由 mod.rs 作为稳定模块根，保留跨集合事务仓储并 re-export 扩展 trait 所需的筛选类型；同步任务与游标、销售单快照、核对作业与差异明细、映射任务分别进入域内子文件，共享排序 helper 放入 common.rs。各文件预计均低于 450 行，职责边界对应实体聚合或集合，能够显著降低导入规模和修改冲突，同时不改变 Repository、MallSyncExt 和 Service 的外部使用方式。
- 拆分建议：
  - **backend/database/src/repository/mall_sync/mod.rs**：作为 D23 mall_sync 仓储模块根；声明 mod common、mod sync、mod snapshot、mod reconciliation、mod mapping_task；通过 pub use re-export MallSalesSyncJobFilter、MallSalesOrderSnapshotFilter、MallSalesReconciliationJobFilter、MallSalesReconciliationItemFilter、MasterMappingTaskFilter；保留公开类型 MallSyncRepository<'a>、MallSyncRepository::new、MallSyncRepository::create_reconciliation_job_with_items，以及 MALL_SALES_RECONCILIATION_JOBS、MALL_SALES_RECONCILIATION_ITEMS 集合常量。
    - 依赖/注意：必须删除原 mall_sync.rs，避免与 mall_sync/mod.rs 形成重复模块源；repository/mod.rs 中的 mod mall_sync 可保持不变。extensions/mall_sync.rs 当前通过 super::super::mall_sync 导入五个 Filter 和 MallSyncRepository，因此模块根必须 re-export 五个 Filter，并直接公开 MallSyncRepository。集合常量继续从 super::extensions::MallSyncExt 取得，维持唯一常量来源；该引用与 extensions/mall_sync.rs 对模块根的引用是当前已有的双向模块依赖，不应扩散到各子文件。
  - **backend/database/src/repository/mall_sync/common.rs**：放置域内共享私有 helper：pub(super) fn sort_doc(sort_by: Option<&str>, sort_ascending: bool) -> Document；保留排序白名单与 created_at 回退语义；内联测试 sort_doc_whitelists_known_fields_and_defaults_otherwise。
    - 依赖/注意：sort_doc 必须改为 pub(super)，供 sync、snapshot、reconciliation、mapping_task 四个兄弟模块调用；不要使用 pub，避免把 MongoDB 查询构造细节暴露到域外。首次拆分建议保留现有统一白名单行为，避免在纯文件迁移中附带行为变化；若后续要严格落实每个 Filter 注释中的独立白名单，可另行参照 purchase_order/common.rs 改为 whitelist 参数与分域常量。
  - **backend/database/src/repository/mall_sync/sync.rs**：放置同步执行状态聚合：公开类型 MallSalesSyncJobRow、MallSalesSyncJobFilter；MallSalesSyncJobFilter 的 QueryFilter、Pagination impl；Repository<MallSalesSyncJob> 的 search_mall_sales_sync_jobs、find_running_incremental_by_source；Repository<MallSalesSyncCursor> 的 find_by_source、advance；私有 helper mall_sales_sync_job_projection；内联测试 sync_job_filter_applies_optional_fields_and_deleted_filter。
    - 依赖/注意：通过 super::common::sort_doc 使用共享排序 helper；Repository、PageResult、Pagination、QueryFilter 建议从 crate::repository 导入，Executor、mongo_ops、Result 从 crate 导入，避免依赖 mod.rs 中偶然存在的 use。advance 继续依赖 MallSalesSyncCursor::move_forward，并保持实体错误映射为 OptimisticLockingError；不要把该规则移入 common.rs。
  - **backend/database/src/repository/mall_sync/snapshot.rs**：放置商城销售单快照聚合：公开类型 MallSalesOrderSnapshotRow、MallSalesOrderSnapshotFilter；对应 QueryFilter、Pagination impl；Repository<MallSalesOrderSnapshot> 的 find_by_fact_key、find_latest_by_order、search_mall_sales_order_snapshots、find_by_mapping_status_before；私有 helper mall_sales_order_snapshot_projection。
    - 依赖/注意：通过 super::common::sort_doc 调用共享排序；ExternalOrderKey、SnapshotMappingStatus、MallSalesOrderSnapshot 与 Instant 均在本文件直接导入。find_latest_by_order 直接使用 mongo_ops::find_many 和 FindOptions，相关导入不能依赖父模块。该模块不应引用 reconciliation 或 mapping_task，以免形成兄弟模块循环依赖。
  - **backend/database/src/repository/mall_sync/reconciliation.rs**：聚合核对批次及其差异明细：公开类型 MallSalesReconciliationJobRow、MallSalesReconciliationJobFilter、MallSalesReconciliationItemRow、MallSalesReconciliationItemFilter；四个 QueryFilter/Pagination impl；Repository<MallSalesReconciliationJob> 的 search_mall_sales_reconciliation_jobs、find_by_job_no；Repository<MallSalesReconciliationItem> 的 search_mall_sales_reconciliation_items、find_items_by_job、find_by_job_and_key；私有 helper mall_sales_reconciliation_job_projection、mall_sales_reconciliation_item_projection。
    - 依赖/注意：Job 与 Item 属于同一核对批次聚合且查询经 reconciliation_job_id 紧密关联，合并在一个文件可避免过度拆分，预计仍低于 450 行。跨集合写入 create_reconciliation_job_with_items 留在 mod.rs 的 MallSyncRepository 中，防止本文件同时承担查询和事务聚合根职责。通过 super::common::sort_doc 引用共享 helper，不反向引用 mod.rs 的 MallSyncRepository。
  - **backend/database/src/repository/mall_sync/mapping_task.rs**：放置主数据映射任务聚合：公开类型 MasterMappingTaskRow、MasterMappingTaskFilter；对应 QueryFilter、Pagination impl；Repository<MasterMappingTask> 的 search_master_mapping_tasks、find_pending_by_snapshot_and_type、find_by_snapshot；私有 helper master_mapping_task_projection。
    - 依赖/注意：通过 super::common::sort_doc 使用共享排序；MappingTaskType、MappingTaskStatus、MasterMappingTask 及快照 ID 类型在本文件直接导入。不要从 snapshot.rs 导入类型或 helper，实体 ID 应继续从 entities::ids 获取，从而避免 snapshot 与 mapping_task 之间产生循环依赖。

## 实施约束

- 拆分应保持现有公开 API 和 re-export 路径，除非实施前另行确认契约变更。
- Service 继续负责事务边界；Repository 只负责数据访问；纯业务规则优先下沉到 Entity 或 Value Object。
- 私有 helper 应跟随唯一调用方迁移；仅在同一领域多个子模块复用时使用 `pub(super)`。
- 拆分完成后执行 `cargo fmt --all -- --check`、`cargo check --workspace`、`cargo clippy --workspace --all-targets --all-features` 和 `cargo test --workspace`。
