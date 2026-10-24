//! 客户回款撤回：调用统一 `prepare_cancel`，再执行业务 `cancel_action`。

use bpm::model::{IdempotencyKey, SubjectRef};
use erp_core::common::time::Instant;
use erp_finance::entity::receivable::CustomerReceipt;
use erp_finance::repository::ReceivableExt;
use erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding;
use erp_workflow::entity::work_item::WorkItem;
pub(crate) use erp_workflow::service::approval::execution::cancel_prepare::DocumentCancelRuntime as LoadedCancelRuntime;
use erp_workflow::service::approval::execution::cancel_prepare::document_cancel_runtime;
use erp_workflow::service::approval::execution::{
    CancelExecutionInput, PreparedExecution, claim_and_persist_document_cancel_runtime,
};
use mongodb::Database;
use persistence_core::{NoTransaction, Transactional};

use crate::Result;
use crate::adapters::persist_cancel_copies;
use crate::audit::persist_log;

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
pub(crate) async fn load_cancel_runtime(
    db: &Database,
    binding: &ApprovalDefinitionBinding,
    subject: &SubjectRef,
    subject_version: u32,
) -> Result<LoadedCancelRuntime> {
    Ok(document_cancel_runtime(
        db,
        binding,
        subject,
        subject_version,
        "客户回款单绑定的审批定义不存在",
        &mut NoTransaction,
    )
    .await?)
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
    Ok(runtime.cancel_input(reason, actor_id, idempotency_key, receipt, now)?)
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
pub(crate) struct CustomerReceiptCancelPersistInput {
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
    pub audit: erp_audit::AuditLog,
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
pub(crate) async fn persist_customer_receipt_cancel(
    db: &Database,
    input: CustomerReceiptCancelPersistInput,
) -> Result<()> {
    let CustomerReceiptCancelPersistInput { mut receipt, prepared, open_tasks, actor_id, reason, now, audit } =
        input;
    let PreparedExecution::Apply(writes) = prepared else {
        return Ok(());
    };
    let closed_tasks = WorkItem::close_all_for_approval_cancellation(open_tasks, &actor_id, &reason, now)?;
    let db = db.clone();
    let client = db.client().clone();
    client
        .with_transaction(move |executor| {
            Box::pin(async move {
                claim_and_persist_document_cancel_runtime(&db, &writes, &closed_tasks, executor).await?;
                persist_cancel_copies(&db, &writes, executor).await?;
                db.customer_receipts().update(&mut receipt, executor).await?;
                persist_log(&db, &audit, executor).await?;
                Ok::<(), crate::Error>(())
            })
        })
        .await
}
