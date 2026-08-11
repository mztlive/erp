# `backend/services/src/legacy_import/mod.rs` 拆分分析

## 文件信息

| 项目 | 内容 |
|---|---|
| 源文件 | `backend/services/src/legacy_import/mod.rs` |
| 扫描行数 | 1030 |
| 分析状态 | 已完成深入分析 |
| 拆分结论 | split |
| 预估工作量 | M |
| 风险 | medium |
| 生成来源 | workflow `analyze-large-files` |
| 生成日期 | 2026-08-11 |

## 拆分方案

- 结论：**split**（工作量 M，风险 medium）
- 摘要：目标文件把服务类型、批次创建、三类只读查询、确认事实流程和批次应用流程集中在一个约 1030 行的 impl 中，已经形成五个清晰的内聚簇。建议保留 mod.rs 作为模块根和公共 re-export 入口，将 LegacyImportService 基础定义放入 service.rs，并按批次创建、查询、确认和应用拆为独立子文件。拆分后现有 dto.rs 约 765 行，其余文件预计均在 350 行以内，能够稳定满足单文件约 800 行以内的目标，同时保持 Handler 使用的公共导入路径不变。
- 拆分建议：
  - **backend/services/src/legacy_import/service.rs**：放置公开结构体 LegacyImportService、构造函数 LegacyImportService::new，以及多个流程共用的 LegacyImportService::batch_view_of。mod.rs 通过 pub use self::service::LegacyImportService 维持现有公共 API。
    - 依赖/注意：由于其他 impl 位于兄弟子模块，db 字段应声明为 pub(super) db: Database；batch_view_of 应为 pub(super)，供 batch.rs、query.rs 和 application.rs 调用。该文件需局部导入 database::{BulkJobExt, NoTransaction}、entities::legacy_import::LegacyImportBatch、mongodb::Database、LegacyImportBatchView 和 Result。service.rs 不应反向引用其他流程模块，以免形成共享层到业务流程层的循环依赖。
  - **backend/services/src/legacy_import/batch.rs**：放置批次创建编排及其专属 helper：LegacyImportService::create_batch、LegacyImportService::ensure_file_assets_exist、LegacyImportService::build_rows。
    - 依赖/注意：通过 super::LegacyImportService 扩展同一服务类型；从 super::dto 引入 build_background_job 和 CreateLegacyImportBatchRequest。需局部导入 AccessControlExt、BulkJobExt、FileAssetExt、LegacyImportExt、NoTransaction、Transactional 等扩展 trait。create_batch 依赖 service.rs 中 pub(super) 的 batch_view_of，但 service.rs 不依赖 batch.rs，因此不会形成循环。事务闭包、审计写入和后台任务创建的 clone/capture 必须原样保留。
  - **backend/services/src/legacy_import/query.rs**：集中所有只读查询和投影组装：LegacyImportBatchFilter、LegacyImportRowFilter、LegacyImportConfirmationFilter 三个私有类型别名；LegacyImportService::batch_list、batch_detail、row_list、confirmation_list、batch_filter_of、ensure_batch_exists。
    - 依赖/注意：通过 super::dto 引入 LegacyImportBatchListQuery、各列表参数、列表项和 PageView；这些 normalized 查询类型当前为 pub(crate)，子模块可直接使用。需局部导入 LegacyImportExt、NoTransaction 和 validator::Validate。batch_detail 调用 service.rs 中 pub(super) 的 batch_view_of；ensure_batch_exists 只被 row_list 使用，可继续保持 query.rs 私有。查询模块不应依赖 batch.rs、confirmation.rs 或 application.rs。
  - **backend/services/src/legacy_import/confirmation.rs**：放置确认事实完整生命周期：LegacyImportService::create_confirmation、decide_confirmation、confirm_matrix_complete、already_decided、advance_batch_to_pending_confirmation。
    - 依赖/注意：该簇内部依赖紧密，私有 helper 均可继续保持 fn/async fn 私有，不需要 pub(super)。需局部导入 AccessControlExt、LegacyImportExt、NoTransaction、Transactional、Instant、ConfirmationDecision、ConfirmationStatus 及相关批次和确认实体。创建与决策事务必须完整留在本文件。already_decided 和 advance_batch_to_pending_confirmation 是未来可下沉 entities 的候选，但不建议与本次机械拆分同时实施。
  - **backend/services/src/legacy_import/application.rs**：放置批次生产应用流程及行级处理 helper：LegacyImportService::apply_batch、advance_background_job、advance_row_to_applicable、party_validates_for、batch_terminal，以及文件私有函数 count_by_status、count_pending。
    - 依赖/注意：需从 super::dto 引入 ApplyLegacyImportBatchRequest、ApplyRowOutcome、ApplyRowResult 和 CUSTOMER_NOT_FOUND_* 常量；局部导入 AccessControlExt、BulkJobExt、LegacyImportExt、PartyExt、NoTransaction、Transactional。apply_batch 调用 service.rs 中 pub(super) 的 batch_view_of。count_by_status 和 count_pending 仅服务本流程，应保持文件私有。后台任务推进、客户主体校验和事务内批量更新必须留在同一文件，避免把跨域 I/O helper 错误下沉到 entities；纯状态 helper 后续可另行评估实体化。

## 实施约束

- 拆分应保持现有公开 API 和 re-export 路径，除非实施前另行确认契约变更。
- Service 继续负责事务边界；Repository 只负责数据访问；纯业务规则优先下沉到 Entity 或 Value Object。
- 私有 helper 应跟随唯一调用方迁移；仅在同一领域多个子模块复用时使用 `pub(super)`。
- 拆分完成后执行 `cargo fmt --all -- --check`、`cargo check --workspace`、`cargo clippy --workspace --all-targets --all-features` 和 `cargo test --workspace`。
