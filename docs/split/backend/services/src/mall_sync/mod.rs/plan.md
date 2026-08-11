# `backend/services/src/mall_sync/mod.rs` 拆分分析

## 文件信息

| 项目 | 内容 |
|---|---|
| 源文件 | `backend/services/src/mall_sync/mod.rs` |
| 扫描行数 | 1160 |
| 分析状态 | 已完成深入分析 |
| 拆分结论 | split |
| 预估工作量 | M |
| 风险 | medium |
| 生成来源 | workflow `analyze-large-files` |
| 生成日期 | 2026-08-11 |

## 拆分方案

- 结论：**split**（工作量 M，风险 medium）
- 摘要：建议按“服务根、同步作业与游标、销售单快照、核对作业与差异、主数据映射任务”五个内聚簇拆分。mod.rs 仅保留领域文档、子模块声明、DTO re-export，并通过 pub use self::service::MallSyncService 保持现有公共 API 不变。拆分后各文件预计约 50 至 420 行，均低于约 800 行目标；同步作业、快照、核对和映射任务分别拥有独立的仓储依赖与私有 helper，边界清晰。
- 拆分建议：
  - **backend/services/src/mall_sync/service.rs**：放置公开类型 MallSyncService、impl MallSyncService 中的构造方法 new，以及被多个业务子模块复用的来源商城校验方法 ensure_source_system。
    - 依赖/注意：MallSyncService 仍声明为 pub，并由 mod.rs 使用 pub use self::service::MallSyncService 重新导出。由于 sync_job.rs、snapshot.rs、reconciliation.rs 和 mapping_task.rs 都需要访问 self.db，db 字段应改为 pub(super) db: Database，或提供等价的 pub(super) 访问器；不要将数据库字段公开到 mall_sync 模块之外。ensure_source_system 同时被 sync_job.rs 和 reconciliation.rs 调用，应声明为 pub(super) async fn。该文件需要 Database、SourceRegistryExt、NoTransaction、SourceSystemId、Error、Result 和 SOURCE_SYSTEM_NOT_FOUND_MESSAGE；不要反向依赖任何业务叶子模块，以避免模块耦合。
  - **backend/services/src/mall_sync/sync_job.rs**：放置同步作业和水位游标编排：MallSalesSyncJobFilter 类型别名；MallSyncService 的 create_sync_job、sync_job_list、sync_job_detail、complete_sync_job、sync_cursor_detail、cursor_after_success、sync_job_filter_of 方法；私有函数 outcome_of；私有枚举 CursorAction。
    - 依赖/注意：通过 super::MallSyncService 扩展 impl，并调用 service.rs 中 pub(super) 的 ensure_source_system。CursorAction、cursor_after_success 和 complete_sync_job 应留在同一文件，维持私有可见性。需要显式导入 AccessControlExt、MallSyncExt、NoTransaction、Transactional，以及 MallSalesSyncJob、MallSalesSyncCursor、相关 ID、状态类型、AuditActor、Error/Result 和 DTO 类型。outcome_of 继续保持文件私有。迁移事务闭包时必须保持作业更新、游标创建或推进、审计写入的原有顺序。
  - **backend/services/src/mall_sync/snapshot.rs**：放置商城销售单快照编排：MallSalesOrderSnapshotFilter 类型别名，以及 MallSyncService 的 ingest_snapshots、snapshot_list、snapshot_is_stale 方法。
    - 依赖/注意：snapshot_is_stale 仅由 ingest_snapshots 使用，可保持普通私有方法，无需 pub(super)。需要显式导入 AccessControlExt、MallSyncExt、NoTransaction、Transactional、ExternalOrderKey、MallSalesOrderSnapshot、MallSalesOrderSnapshotData、MallSalesOrderSnapshotId、Instant、next_id、Validate、AuditActor 和相关 DTO/View。必须保留事实键查重、迟到快照判定、accepted/skipped 计数以及快照批量写入与作业进度、审计日志同事务更新的原始语义。
  - **backend/services/src/mall_sync/reconciliation.rs**：放置核对作业与差异明细编排：MallSalesReconciliationJobFilter、MallSalesReconciliationItemFilter 类型别名；MallSyncService 的 create_reconciliation_job、reconciliation_job_list、reconciliation_item_list、resolve_reconciliation_item、ensure_erp_sides_exist 方法。
    - 依赖/注意：create_reconciliation_job 通过 pub(super) 的 ensure_source_system 复用来源商城校验；ensure_erp_sides_exist 只在本文件使用，应继续保持私有。需要显式导入 AccessControlExt、CustomerExt、MallSyncExt、SalesOrderExt、NoTransaction、Transactional，以及核对作业和明细实体、ID、状态类型、AuditActor、Validate 和 DTO 常量 SALES_ORDER_NOT_FOUND_MESSAGE、SALES_ORDER_CUSTOMER_MISSING_MESSAGE。不要让该模块依赖其他 Service，只继续通过 DatabaseExt/Repository 跨域读取 D13 和 D08。
  - **backend/services/src/mall_sync/mapping_task.rs**：放置主数据映射任务编排：MasterMappingTaskFilter 类型别名，以及 MallSyncService 的 mapping_task_list、create_mapping_task、resolve_mapping_task 方法。
    - 依赖/注意：需要显式导入 AccessControlExt、MallSyncExt、NoTransaction、Transactional、MasterMappingTask、MasterMappingTaskData、MasterMappingTaskId、MappingTaskStatus、Instant、next_id、Validate、AuditActor 和对应 DTO/View。终态状态匹配可直接导入 MappingTaskStatus，避免继续使用冗长的 entities::mall_sync::MappingTaskStatus 路径。拆分本身不得改变 create_mapping_task 当前任务写入与审计写入方式，也不得改变 resolve_mapping_task 的终态幂等判断和事务边界。该模块不应引用 sync_job、snapshot 或 reconciliation 模块，因此不存在必要的循环依赖。

## 实施约束

- 拆分应保持现有公开 API 和 re-export 路径，除非实施前另行确认契约变更。
- Service 继续负责事务边界；Repository 只负责数据访问；纯业务规则优先下沉到 Entity 或 Value Object。
- 私有 helper 应跟随唯一调用方迁移；仅在同一领域多个子模块复用时使用 `pub(super)`。
- 拆分完成后执行 `cargo fmt --all -- --check`、`cargo check --workspace`、`cargo clippy --workspace --all-targets --all-features` 和 `cargo test --workspace`。
