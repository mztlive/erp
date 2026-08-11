# `backend/services/src/integration_ops/mod.rs` 拆分分析

## 文件信息

| 项目 | 内容 |
|---|---|
| 源文件 | `backend/services/src/integration_ops/mod.rs` |
| 扫描行数 | 1360 |
| 分析状态 | 已完成深入分析 |
| 拆分结论 | split |
| 预估工作量 | M |
| 风险 | medium |
| 生成来源 | workflow `analyze-large-files` |
| 生成日期 | 2026-08-11 |

## 拆分方案

- 结论：**split**（工作量 M，风险 medium）
- 摘要：建议按入站消息、集成错误任务、对账差异三个业务聚合拆分，并将服务类型和跨业务共享的事务、乐观锁、时间辅助集中到 service.rs。mod.rs 继续作为模块根，仅保留域文档、子模块声明、DTO re-export，以及 IntegrationOpsService 的公开 re-export。原文件只有一个公开服务类型，没有公开 enum 或 trait；主要体量来自同一 impl 中混合了三个独立业务簇。拆分后 service.rs 约 100 行、inbox_message.rs 约 280 行、error_task.rs 约 550 行、reconciliation_difference.rs 约 430 行，均低于约 800 行，且符合 services 域目录的多文件组织方式。
- 拆分建议：
  - **backend/services/src/integration_ops/service.rs**：放置公开类型 IntegrationOpsService、构造方法 IntegrationOpsService::new，以及跨业务簇共享的 IntegrationOpsService::run_audited、ensure_version、now_secs。IntegrationOpsService 的 db 字段建议声明为 pub(super)，使同级业务子模块中的 impl 块能够访问数据库实例。
    - 依赖/注意：mod.rs 增加 mod service; 并使用 pub use self::service::IntegrationOpsService; 对外保持原 API。run_audited 需要标记 pub(super)，供 inbox_message、error_task、reconciliation_difference 的同级 impl 调用；ensure_version 和 now_secs 同样需要 pub(super)。保留 Future、Pin、Transactional、Database 等基础设施依赖在本文件。db 字段若保持普通私有字段，同级子模块无法直接访问，因此必须使用 pub(super) 或提供 pub(super) 访问器。该文件不得反向依赖三个业务子模块，以避免循环依赖。
  - **backend/services/src/integration_ops/inbox_message.rs**：放置私有类型别名 InboxMessageFilter；放置 IntegrationOpsService 的入站消息方法 register_inbox_message、inbox_message_list、inbox_message_detail、write_back_inbox_result；放置仅服务于登记流程的私有 helper build_inbox_message。
    - 依赖/注意：通过 super::IntegrationOpsService 扩展同一服务类型；从 super::dto 引入 RegisterInboxMessageRequest、WriteBackInboxResultRequest、WriteBackOutcome、InboxMessageListParams、InboxMessageListView、InboxMessageView、PageView、SortDir。需要 IntegrationOpsExt、SourceRegistryExt、AccessControlExt、NoTransaction，以及 service.rs 中 pub(super) 的 ensure_version、now_secs 和 run_audited。build_inbox_message 只在本文件使用，应保持私有，不应提升可见性。该模块会创建 IntegrationErrorTask，但不应调用 error_task 子模块中的方法，避免业务子模块相互依赖。
  - **backend/services/src/integration_ops/error_task.rs**：放置私有类型别名 ErrorTaskFilter；放置 IntegrationOpsService 的错误任务方法 create_error_task、error_task_list、error_task_detail、query_error_task_result、replay_error_task、hold_error_task、transfer_error_task、resolve_error_task、close_error_task；放置私有 helper load_active_task、ensure_replay_allowed、mask_key；保留 mask_key_keeps_short_keys_and_masks_long_ones 单元测试。
    - 依赖/注意：通过 super::IntegrationOpsService 扩展服务；从 super::dto 引入错误任务相关 Request/View、CloseReason、HoldKind、PageView、ReplayResultView、SortDir。需要 IntegrationOpsExt、AccessControlExt、NoTransaction，以及 service.rs 中的 ensure_version 和 run_audited。load_active_task、ensure_replay_allowed、mask_key 均只服务本业务簇，应保持文件私有；测试改为 use super::mask_key 即可。该模块读取关联 InboxMessage Repository 以锁定原业务事实键，但不应依赖 inbox_message 子模块，跨聚合协作仍直接经 DatabaseExt/Repository 完成。
  - **backend/services/src/integration_ops/reconciliation_difference.rs**：放置私有类型别名 DifferenceFilter；放置 IntegrationOpsService 的对账差异方法 create_difference、difference_list、difference_detail、process_difference、resolve_difference；放置私有 helper load_active_difference、derived_difference_status、append_difference_resolution。
    - 依赖/注意：通过 super::IntegrationOpsService 扩展服务；从 super::dto 引入 CreateDifferenceRequest、DifferenceListParams、DifferenceView、DifferenceDetailView、DifferenceActionView、DifferenceProcessAction、DifferenceConclusion、ProcessDifferenceRequest、ResolveDifferenceRequest、ResolutionView、PageView、SortDir。需要 IntegrationOpsExt、AccessControlExt、NoTransaction，以及 service.rs 中的 run_audited。三个 helper 共享对账差异和处理记录实体，应留在同一文件并保持私有。不要让 service.rs 或其他业务模块反向引用本模块；差异状态查询和追加处理记录都在本模块内部闭合。

## 实施约束

- 拆分应保持现有公开 API 和 re-export 路径，除非实施前另行确认契约变更。
- Service 继续负责事务边界；Repository 只负责数据访问；纯业务规则优先下沉到 Entity 或 Value Object。
- 私有 helper 应跟随唯一调用方迁移；仅在同一领域多个子模块复用时使用 `pub(super)`。
- 拆分完成后执行 `cargo fmt --all -- --check`、`cargo check --workspace`、`cargo clippy --workspace --all-targets --all-features` 和 `cargo test --workspace`。
