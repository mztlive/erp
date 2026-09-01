//! 客户回款撤回：调用统一 `prepare_cancel`，再执行业务 `cancel_action`。

use bpm::engine::DefinitionGraph;
use bpm::ids::{ApprovalCommandReceiptId, ApprovalNodeExecutionId, ApprovalProcessInstanceId};
use bpm::model::{
    ApprovalCancellationTaskPolicy, ApprovalNodeExecution, ApprovalProcessInstance, IdempotencyKey,
    ParticipantId, Timestamp,
};
use database::{AccessControlExt, BpmExt, NoTransaction, ReceivableExt, Transactional, WorkItemExt};
use entities::common::time::Instant;
use entities::document_registry::business_document::ApprovalDefinitionBinding;
use entities::receivable::CustomerReceipt;
use entities::work_item::WorkItem;
use id_generator::next_id;
use mongodb::Database;

use super::start_approval::load_bound_definition_graph;
use crate::approval::execution::authorization::converge_eligibility;
use crate::approval::execution::{
    claim_and_persist_document_cancel_runtime, normalize_document_cancel_reason, CancelExecutionInput,
    ExecutionCommandInput, PreparedExecution,
};
use crate::errors::{Error, Result};

/// 已加载的可撤回运行事实。
pub(super) struct LoadedCancelRuntime {
    /// 绑定定义图。
    pub graph: DefinitionGraph,
    /// 非终态实例。
    pub instance: ApprovalProcessInstance,
    /// 当前执行。
    pub current: ApprovalNodeExecution,
    /// 当前实例决定的任务关闭策略。
    pub task_policy: ApprovalCancellationTaskPolicy,
    /// 当前执行上的开放任务。
    pub open_tasks: Vec<WorkItem>,
}

/// 按主体加载 RUNNING/BLOCKED 实例、当前执行与开放任务。
///
/// `RUNNING` 必须恰有一个开放任务，`BLOCKED` 必须没有开放任务。
///
/// # 参数
/// * `db` - 数据库
/// * `binding` - 创建时冻结的定义绑定
/// * `subject` - 业务对象引用
/// * `subject_version` - 冻结提交版本
///
/// # 返回
/// 返回定义图、实例、当前执行、任务关闭策略与开放任务快照。
///
/// # 错误
/// 实例或当前执行缺失、状态与开放任务数量不一致或仓储失败时返回错误。
pub(super) async fn load_cancel_runtime(
    db: &Database,
    binding: &ApprovalDefinitionBinding,
    subject: &bpm::SubjectRef,
    subject_version: u32,
) -> Result<LoadedCancelRuntime> {
    let instance = db
        .bpm_workflow()
        .cancellation_instance_by_subject(subject, subject_version, &mut NoTransaction)
        .await?
        .ok_or_else(|| Error::ConflictError("没有可撤回的审批实例".to_string()))?;
    let task_policy = instance
        .cancellation_task_policy()
        .map_err(|error| Error::ConflictError(error.to_string()))?;
    let current = db
        .bpm_workflow()
        .current_execution_for_cancellation(
            &ApprovalProcessInstanceId::new(instance.base.id.clone()),
            &mut NoTransaction,
        )
        .await?
        .ok_or_else(|| Error::ConflictError("审批实例缺少当前执行".to_string()))?;
    let open_tasks = db
        .work_items()
        .open_approval_tasks_for_execution(
            &ApprovalNodeExecutionId::new(current.base.id.clone()),
            &mut NoTransaction,
        )
        .await?;
    task_policy
        .ensure_open_task_count(open_tasks.len())
        .map_err(|error| Error::ConflictError(error.to_string()))?;
    Ok(LoadedCancelRuntime {
        graph: load_bound_definition_graph(db, binding).await?,
        instance,
        current,
        task_policy,
        open_tasks,
    })
}

/// 构造统一 `cancel_approval` 输入。
///
/// 人员失效走业务取消；非人员一致性 blocker 必须走受阻取消。
///
/// # 参数
/// * `runtime` - 已加载运行事实
/// * `reason` - 已校验的非空原因
/// * `actor_id` - 撤回人
/// * `idempotency_key` - 幂等键
/// * `receipt` - 已存在收据
/// * `now` - 调用方时间
///
/// # 返回
/// 返回可交给统一 `prepare_cancel` 的取消编排输入。
///
/// # 错误
/// 原因/幂等键非法、审批人引用无效或端口与 blocker 不匹配时返回错误。
pub fn build_customer_receipt_cancel_input(
    runtime: &LoadedCancelRuntime,
    reason: &str,
    actor_id: &str,
    idempotency_key: &IdempotencyKey,
    receipt: Option<bpm::model::ApprovalCommandReceipt>,
    now: Instant,
) -> Result<CancelExecutionInput> {
    let reason = normalize_document_cancel_reason(reason)?;
    let actor =
        ParticipantId::new(actor_id).map_err(|_| Error::ValidationError("撤回人引用无效".to_string()))?;
    let eligibility = converge_eligibility(
        runtime.current.assignee_participant_id.as_str(),
        &runtime.current.assignee_name_snapshot,
        None,
    )?;
    Ok(CancelExecutionInput {
        command: ExecutionCommandInput {
            graph: runtime.graph.clone(),
            current_eligibility: eligibility.clone(),
            next_eligibility: eligibility,
            receipt,
            idempotency_key: idempotency_key.clone(),
            now: Timestamp::from_utc(now.as_utc()),
        },
        instance: runtime.instance.clone(),
        current: runtime.current.clone(),
        subject_version: runtime.instance.subject_version,
        expected_instance_version: runtime.instance.base.version,
        expected_execution_version: runtime.current.base.version,
        expected_task_version: runtime.open_tasks.first().map(|item| item.base.version),
        reason,
        actor,
        close_open_task: runtime.task_policy.closes_open_task(),
        blocked_port: false,
        receipt_id: ApprovalCommandReceiptId::new(next_id()),
    })
}

/// 客户回款撤回事务写入集合。
///
/// # 用途
/// 收拢取消计划、开放任务、撤回人与审计，供同一事务写入。
///
/// # 参数
/// 无。
///
/// # 返回
/// 无。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 运行事实、任务关闭与单据回写必须同事务；CAS 失败时回滚。
pub(super) struct CustomerReceiptCancelPersistInput {
    /// 已执行 `cancel_action` 的回款单。
    pub receipt: CustomerReceipt,
    /// `prepare_cancel` 结果。
    pub prepared: PreparedExecution,
    /// 待关闭的开放任务。
    pub open_tasks: Vec<WorkItem>,
    /// 撤回人。
    pub actor_id: String,
    /// 撤回原因。
    pub reason: String,
    /// 调用方时间。
    pub now: Instant,
    /// 已构造审计。
    pub audit: entities::AuditLog,
}

/// 在同一事务内应用取消计划、关闭任务并写回回款单。
///
/// # 用途
/// 撤回审批后原子写回运行事实与回款单。
///
/// # 参数
/// * `db` - 数据库
/// * `input` - 回款单、取消计划与开放任务
///
/// # 返回
/// 成功时无返回值。
///
/// # 错误
/// CAS 冲突或仓储失败时返回错误，事务回滚。
///
/// # 关键业务约束
/// Replay 不得重复关闭任务；Apply 必须关闭开放任务并写回草稿。
pub(super) async fn persist_customer_receipt_cancel(
    db: &Database,
    input: CustomerReceiptCancelPersistInput,
) -> Result<()> {
    let CustomerReceiptCancelPersistInput {
        mut receipt,
        prepared,
        open_tasks,
        actor_id,
        reason,
        now,
        audit,
    } = input;
    let PreparedExecution::Apply(writes) = prepared else {
        return Ok(());
    };
    let closed_tasks = WorkItem::close_all_for_approval_cancellation(open_tasks, &actor_id, &reason, now)?;
    let db = db.clone();
    let client = db.client().clone();
    client
        .with_transaction(move |session| {
            Box::pin(async move {
                claim_and_persist_document_cancel_runtime(&db, &writes, &closed_tasks, session).await?;
                db.customer_receipts().update(&mut receipt, session).await?;
                db.audit_logs().create(&audit, session).await?;
                Ok::<(), crate::errors::Error>(())
            })
        })
        .await
}
