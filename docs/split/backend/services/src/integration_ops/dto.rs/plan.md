# `backend/services/src/integration_ops/dto.rs` 拆分分析

## 文件信息

| 项目 | 内容 |
|---|---|
| 源文件 | `backend/services/src/integration_ops/dto.rs` |
| 扫描行数 | 1122 |
| 分析状态 | 已完成深入分析 |
| 实施状态 | 已完成（2026-08-12） |
| 拆分结论 | split |
| 预估工作量 | M |
| 风险 | low |
| 生成来源 | workflow `analyze-large-files` |
| 生成日期 | 2026-08-11 |

## 实施结果

- `backend/services/src/integration_ops/dto.rs` 已改为薄门面，并按共享分页排序、入站消息、集成错误任务和对账差异拆分为四个私有子模块。
- facade 按拆分前形状公开重导出 `PageParams`、`PageView`、`SortDir`，`normalize_sort` 保持 `pub(crate)`；`integration_ops/mod.rs` 根模块未修改。
- facade 为 42 行，最大子文件 `error_task.rs` 为 379 行，所有生产子文件均低于 800 行。
- 已通过完整 workspace 格式、编译、Clippy（`-D warnings`）和测试门禁；integration_ops DTO 定向测试 5/5 通过。

## 拆分方案

- 结论：**split**（工作量 M，风险 low）
- 摘要：该文件可按“通用分页排序、入站消息、集成错误任务、对账差异”四个高内聚簇拆分。保留现有 dto.rs 作为内部 DTO 门面，在其中声明子模块并集中 re-export；integration_ops/mod.rs 继续作为领域模块根并维持现有公开 re-export，因此 Handler 和其他调用方的导入路径不变。实体到视图的 From 实现与对应视图放在同一文件，规范化实现与其查询参数放在同一文件，测试按领域就近迁移。拆分后各文件预计不超过约 420 行，显著低于 800 行目标。
- 拆分建议：
  - **backend/services/src/integration_ops/dto/common.rs**：放置共享分页、排序和校验基础设施：SortDir、PageParams、PageView<T>、normalize_sort、non_blank；同时迁移测试 sort_whitelist_rejects_unknown_fields_and_directions。
    - 依赖/注意：依赖 crate::errors::{Error, Result} 和 serde::Serialize。non_blank 当前是 dto.rs 私有函数，迁入后应调整为 pub(super)，使 inbox_message、error_task、reconciliation_difference 三个兄弟子模块可以通过 super::common::non_blank 使用；normalize_sort、SortDir 需保持 crate 内可见。dto.rs 应公开 re-export PageView，并以 pub(crate) 方式 re-export SortDir，保持 integration_ops/mod.rs 中 use self::dto::SortDir 有效。common 不应引用任何领域子模块，以形成单向依赖并避免循环。
  - **backend/services/src/integration_ops/dto/inbox_message.rs**：放置入站消息簇：INBOX_MESSAGE_SORT_FIELDS、RegisterInboxMessageRequest、WriteBackInboxResultRequest、WriteBackOutcome、InboxMessageListParams、InboxMessageListQuery、InboxMessageListParams::normalized、InboxMessageListView、InboxMessageView、From<InboxMessage> for InboxMessageView；同时迁移测试 inbox_list_params_normalize_paging_filters_and_sort_defaults。
    - 依赖/注意：依赖 entities::integration_ops::{InboxMessage, InboxMessageStatus, MessageType, SourceSystemId} 及 InboxMessageId 相关公开路径、serde、validator、crate::errors::Result、crate::query::{normalized_text, page_or_default, page_size_or_default}。通过 super::common::{non_blank, normalize_sort, PageParams} 使用共享逻辑。InboxMessageListQuery 仍需 pub(crate)，并由 dto.rs 以 pub(crate) re-export，避免服务编排层访问不可命名的私有返回类型；公开请求和视图由 dto.rs 再 re-export。INBOX_MESSAGE_SORT_FIELDS 保持本文件私有或 pub(crate)，不应移入 common，避免通用模块反向依赖具体领域规则。
  - **backend/services/src/integration_ops/dto/error_task.rs**：放置集成错误任务簇：ERROR_TASK_SORT_FIELDS、CreateErrorTaskRequest、ErrorTaskListParams、ErrorTaskListQuery、ErrorTaskListParams::normalized、QueryOriginalResultRequest、QueryOutcome 及 summary_marker、ReplayOriginalRequest、ReplayResultView、HoldErrorTaskRequest、HoldKind 及 summary_marker、TransferErrorTaskRequest、ResolveErrorTaskRequest、CloseErrorTaskRequest、CloseReason、ErrorTaskView、From<IntegrationErrorTask> for ErrorTaskView、ErrorTaskDetailView；同时迁移测试 error_task_list_params_normalize_flat_filters。
    - 依赖/注意：依赖 entities::integration_ops::{ErrorClass, ErrorTaskStatus, IntegrationErrorTask, ResolutionType} 以及 InboxMessageId、IntegrationErrorTaskId，依赖 serde、validator、crate::errors::Result 和 crate::query 的文本及分页规范化函数。通过 super::common::{non_blank, normalize_sort, PageParams} 使用共享 helper。ErrorTaskListQuery 应保持 pub(crate) 并由 dto.rs 以 pub(crate) re-export；QueryOutcome::summary_marker 和 HoldKind::summary_marker 保持 pub(crate)，供 integration_ops/mod.rs 的服务方法调用。ReplayOriginalRequest 上的 serde(deny_unknown_fields) 必须原样保留。该模块不应引用 inbox_message 子模块，消息 ID 直接依赖 entities 类型，以避免 DTO 子模块之间形成循环。
  - **backend/services/src/integration_ops/dto/reconciliation_difference.rs**：放置对账差异簇：DIFFERENCE_SORT_FIELDS、CreateDifferenceRequest、DifferenceListParams、DifferenceListQuery、DifferenceListParams::normalized、ProcessDifferenceRequest、DifferenceProcessAction、ResolveDifferenceRequest、DifferenceConclusion、DifferenceReasonCode 及 as_str、DifferenceView、ResolutionView、DifferenceDetailView、DifferenceActionView、From<ReconciliationDifferenceResolution> for ResolutionView、From<ReconciliationDifference> for DifferenceView；同时迁移测试 difference_list_params_normalize_and_reject_unbounded_page_size 和 reason_code_serializes_with_stable_codes。
    - 依赖/注意：依赖 entities::integration_ops::{ReconciliationDifference, ReconciliationDifferenceResolution, ResolutionAction, ResultingStatus}，以及 serde、validator、crate::errors::Result、crate::query::{normalized_text, page_or_default, page_size_or_default}。通过 super::common::{non_blank, normalize_sort, PageParams} 使用共享逻辑。DifferenceListQuery 应保持 pub(crate) 并由 dto.rs 以 pub(crate) re-export。DifferenceReasonCode 的 SCREAMING_SNAKE_CASE serde 契约及 as_str 返回值必须原样保留。From 实现应与对应 View 放在同一文件；不要让该模块依赖 error_task 子模块，replacement_task_id 继续使用 String 或 entities ID 转换，从而避免潜在循环依赖。

## 实施约束

- 拆分应保持现有公开 API 和 re-export 路径，除非实施前另行确认契约变更。
- Service 继续负责事务边界；Repository 只负责数据访问；纯业务规则优先下沉到 Entity 或 Value Object。
- 私有 helper 应跟随唯一调用方迁移；仅在同一领域多个子模块复用时使用 `pub(super)`。
- 拆分完成后执行 `cargo fmt --all -- --check`、`cargo check --workspace`、`cargo clippy --workspace --all-targets --all-features` 和 `cargo test --workspace`。
