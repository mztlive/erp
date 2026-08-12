# `backend/services/src/mall_sync/dto.rs` 拆分分析

## 文件信息

| 项目 | 内容 |
|---|---|
| 源文件 | `backend/services/src/mall_sync/dto.rs` |
| 扫描行数 | 1003 |
| 分析状态 | 已完成深入分析 |
| 拆分结论 | split |
| 预估工作量 | M |
| 风险 | medium |
| 生成来源 | workflow `analyze-large-files` |
| 生成日期 | 2026-08-11 |
| 实施状态 | 已完成（2026-08-12） |

## 实施结果

- `backend/services/src/mall_sync/dto.rs` 已改为薄门面，五个子模块均为私有模块。
- DTO 已按共享基础设施、同步作业、快照、核对、映射任务拆分到 `dto/`；公开与 crate 可见名称集合与拆分前一致。
- 所有生产子文件均低于 800 行；`mall_sync/mod.rs` 根 re-export 与下游 Handler 文件未修改。
- 已通过：`cargo test -p services mall_sync::dto::`、`cargo test -p services --all-features`、`cargo check -p services --all-targets --all-features`、`cargo clippy -p services --all-targets --all-features -- -D warnings`、`cargo check -p web-api --all-targets --all-features`、`cargo clippy -p web-api --all-targets --all-features -- -D warnings`。

## 拆分方案

- 结论：**split**（工作量 M，风险 medium）
- 摘要：建议将当前约 1003 行的 DTO 文件按共享基础设施、同步作业、快照、核对、映射任务五个内聚簇拆分。保留 backend/services/src/mall_sync/mod.rs 作为领域模块根，并保留 dto.rs 作为薄门面声明子模块和统一 re-export，从而维持 Handler 当前从 services::mall_sync 导入 DTO 的公开契约。拆分后各文件预计约 100 至 320 行，均可稳定控制在 800 行以内。
- 拆分建议：
  - **backend/services/src/mall_sync/dto/common.rs**：放置共享分页与排序定义 SortDir、PageParams、PageView<T>；共享函数 normalize_sort、non_blank；跨流程错误文案 SOURCE_SYSTEM_NOT_FOUND_MESSAGE、SALES_ORDER_NOT_FOUND_MESSAGE、SALES_ORDER_CUSTOMER_MISSING_MESSAGE；迁入测试 sort_whitelist_rejects_unknown_fields_and_directions。
    - 依赖/注意：依赖 crate::errors::{Error, Result} 和 validator::ValidationError。non_blank 原来是 dto.rs 私有函数，迁移后应设为 pub(super)，供 snapshot、reconciliation、mapping_task 子模块的 #[validate(custom(...))] 使用；normalize_sort 和错误文案维持 pub(crate)。dto.rs 应公开 re-export SortDir、PageParams、PageView，并以 crate 可见性 re-export normalize_sort 和错误文案。该文件不得依赖其他四个域 DTO 文件，以避免循环依赖。
  - **backend/services/src/mall_sync/dto/sync_job.rs**：放置 MALL_SALES_SYNC_JOB_SORT_FIELDS、CreateMallSalesSyncJobRequest、CompleteMallSalesSyncJobRequest、SyncJobOutcome、MallSalesSyncJobView 及其 From<MallSalesSyncJob> impl、MallSalesSyncJobListParams、MallSalesSyncJobListQuery、MallSalesSyncJobListParams::normalized、MallSalesSyncCursorView 及其 From<MallSalesSyncCursor> impl；迁入测试 job_list_params_normalize_paging_filters_and_sort_defaults 和 list_params_reject_unbounded_page_size。
    - 依赖/注意：通过 super::common 使用 normalize_sort、PageParams、SortDir；继续依赖 crate::query::{page_or_default, page_size_or_default}。MallSalesSyncJobListQuery 和排序白名单保持 pub(crate)。SyncJobOutcome 必须由 dto.rs 公开 re-export，因为它是 CompleteMallSalesSyncJobRequest 的公开字段类型且 mall_sync/mod.rs 会直接使用；MallSalesSyncCursorView 与作业成功推进水位的流程内聚，放在本文件不会形成反向依赖。
  - **backend/services/src/mall_sync/dto/snapshot.rs**：放置 MALL_SALES_ORDER_SNAPSHOT_SORT_FIELDS、SnapshotItemRequest、IngestMallSalesOrderSnapshotsRequest、IngestMallSalesOrderSnapshotsResult、MallSalesOrderSnapshotView 及其 From<MallSalesOrderSnapshot> impl、MallSalesOrderSnapshotListParams、MallSalesOrderSnapshotListQuery、MallSalesOrderSnapshotListParams::normalized；迁入测试 ingest_request_rejects_empty_items。
    - 依赖/注意：通过 super::common 导入 non_blank、normalize_sort、PageParams；non_blank 必须具有跨兄弟子模块可见性。继续依赖 crate::query::{page_or_default, page_size_or_default}。MallSalesOrderSnapshotListQuery 和排序白名单保持 pub(crate)，dto.rs 负责重导出以保留原有 crate 内路径；本文件不应依赖 sync_job.rs，sync_job_id 仅使用 entities 中的 ID 类型。
  - **backend/services/src/mall_sync/dto/reconciliation.rs**：放置 MALL_SALES_RECONCILIATION_JOB_SORT_FIELDS、MALL_SALES_RECONCILIATION_ITEM_SORT_FIELDS、ReconciliationItemRequest、CreateMallSalesReconciliationJobRequest、MallSalesReconciliationJobView 及其 From<MallSalesReconciliationJob> impl、MallSalesReconciliationJobListParams、MallSalesReconciliationJobListQuery 及 normalized；MallSalesReconciliationItemView 及其 From<MallSalesReconciliationItem> impl、MallSalesReconciliationItemListParams、MallSalesReconciliationItemListQuery 及 normalized；ResolveItemKind、ResolveMallSalesReconciliationItemRequest。
    - 依赖/注意：通过 super::common 使用 non_blank、normalize_sort、PageParams；继续依赖分页默认值 helper。两个 ListQuery 和两个排序白名单保持 pub(crate)。ResolveItemKind 必须由 dto.rs 公开 re-export，因为 mall_sync/mod.rs 当前通过 dto::ResolveItemKind 匹配处理方式。该文件只引用 entities 的销售单、销售版本和核对 ID，不应引用 sync_job.rs；single_order_sync_job_id 仅作为实体视图字段转换，因此不存在模块循环。
  - **backend/services/src/mall_sync/dto/mapping_task.rs**：放置 MASTER_MAPPING_TASK_SORT_FIELDS、CreateMasterMappingTaskRequest、MasterMappingTaskView 及其 From<MasterMappingTask> impl、MasterMappingTaskListParams、MasterMappingTaskListQuery、MasterMappingTaskListParams::normalized、ResolveTaskKind、ResolveMasterMappingTaskRequest。
    - 依赖/注意：通过 super::common 使用 non_blank、normalize_sort、PageParams；继续依赖 crate::query::{normalized_text, page_or_default, page_size_or_default}。MasterMappingTaskListQuery 和排序白名单保持 pub(crate)。ResolveTaskKind 必须由 dto.rs 公开 re-export，以保持 mall_sync/mod.rs 中 dto::ResolveTaskKind 的路径有效。source_snapshot_id 只依赖 entities::ids::MallSalesOrderSnapshotId，无需依赖 snapshot.rs，可避免映射与快照 DTO 之间形成循环依赖。

## 实施约束

- 拆分应保持现有公开 API 和 re-export 路径，除非实施前另行确认契约变更。
- Service 继续负责事务边界；Repository 只负责数据访问；纯业务规则优先下沉到 Entity 或 Value Object。
- 私有 helper 应跟随唯一调用方迁移；仅在同一领域多个子模块复用时使用 `pub(super)`。
- 拆分完成后执行 `cargo fmt --all -- --check`、`cargo check --workspace`、`cargo clippy --workspace --all-targets --all-features` 和 `cargo test --workspace`。
