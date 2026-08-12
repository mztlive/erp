//! 域 D34 `integration_ops` 服务编排。
//!
//! 事务边界只在 Service（conventions §6.1）：
//! - 单集合、无跨步骤原子性要求的查询一律 `&mut NoTransaction`；
//! - 所有业务写入均与审计日志跨集合（`audit_logs` 属 D06），统一
//!   `database::Transactional::with_transaction` 原子提交（D01 样板写法）；
//!   共享模板 `transaction::run_audited` 与乐观锁校验 `validation::ensure_version`
//!   仅限本域内部使用（`pub(super)`），不对外暴露；
//! - 跨域协作只调对方域 Repository（D01 `SourceRegistryExt::source_systems`），
//!   禁止 Service 依赖 Service（conventions §6.2）。
//!
//! 集成治理核心动作（W29 §7/§8.2，数据模型 §6.21、§7.7）：
//! - `inbox_message`：登记（消息层/业务事实层幂等由唯一索引保证，服务层不做
//!   「先查后插」重复性判断）、结果回写（processed / failed+错误任务）；
//! - `integration_error_task`：查询原结果（QUERY）→ 明确无结果才开放 REPLAY；
//!   REPLAY 永不接受客户端原幂等键，服务端锁定关联消息的业务事实键；暂挂/跳过
//!   保留在队列；RESOLVE/CLOSE/TRANSFER 终结当前处理；已解决/已关闭是终态；
//! - `reconciliation_difference`：查询/人工处理（只追加处理记录）/解决（固定
//!   原因枚举 + 受控证据，派生终态）。
//!
//! 文件组织（按业务边界拆分；public 方法与 DTO 导出保持不变）：
//! - `inbox_message.rs`：入站消息登记、列表、详情、结果回写；
//! - `error_task.rs`：错误任务登记、查询、REPLAY/DEFER/SKIP/TRANSFER/RESOLVE/CLOSE；
//! - `reconciliation_difference.rs`：差异创建、列表、详情、人工处理、解决；
//! - `transaction.rs`：共享 `run_audited` 事务模板（本域内部）；
//! - `validation.rs`：共享乐观锁版本校验（本域内部）。

use database::IntegrationOpsExt;
use mongodb::Database;

mod dto;
mod error_task;
mod inbox_message;
mod reconciliation_difference;
mod transaction;
mod validation;

pub use self::dto::{
    CloseErrorTaskRequest, CloseReason, CreateDifferenceRequest, CreateErrorTaskRequest,
    DifferenceActionView, DifferenceConclusion, DifferenceDetailView, DifferenceListParams,
    DifferenceProcessAction, DifferenceReasonCode, DifferenceView, ErrorTaskDetailView, ErrorTaskListParams,
    ErrorTaskView, HoldErrorTaskRequest, HoldKind, InboxMessageListParams, InboxMessageListView,
    InboxMessageView, PageView, ProcessDifferenceRequest, QueryOriginalResultRequest, QueryOutcome,
    RegisterInboxMessageRequest, ReplayOriginalRequest, ReplayResultView, ResolutionView,
    ResolveDifferenceRequest, ResolveErrorTaskRequest, TransferErrorTaskRequest, WriteBackInboxResultRequest,
    WriteBackOutcome,
};

/// 入站消息列表筛选条件类型（经 `IntegrationOpsExt` 关联类型跨 crate 可达）。
type InboxMessageFilter = <mongodb::Database as IntegrationOpsExt>::InboxMessageFilter;
/// 错误任务列表筛选条件类型。
type ErrorTaskFilter = <mongodb::Database as IntegrationOpsExt>::IntegrationErrorTaskFilter;
/// 对账差异列表筛选条件类型。
type DifferenceFilter = <mongodb::Database as IntegrationOpsExt>::ReconciliationDifferenceFilter;

/// 集成治理服务。
///
/// 提供 inbox_message 登记/回写、integration_error_task 人工处理与终态动作、
/// reconciliation_difference 查询/人工处理/解决的编排。
pub struct IntegrationOpsService {
    db: Database,
}

impl IntegrationOpsService {
    /// 创建集成治理服务实例。
    ///
    /// # 参数
    /// * `db` - 数据库实例
    ///
    /// # 返回
    /// 返回服务实例。
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}
