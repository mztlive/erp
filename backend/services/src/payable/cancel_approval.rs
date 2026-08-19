//! 供应商付款撤回：调用统一 `prepare_cancel`，再执行业务 `cancel_action`。

use bpm::engine::{DefinitionGraph, Eligibility};
use bpm::ids::{ApprovalCommandReceiptId, ApprovalProcessInstanceId};
use bpm::model::types::ApprovalProcessInstanceStatus;
use bpm::model::{ApprovalNodeExecution, ApprovalProcessInstance, ParticipantId, Timestamp};
use database::repository::bpm::ApprovalInstanceListProjection;
use database::{AccessControlExt, BpmExt, NoTransaction, PayableExt, Transactional, WorkItemExt};
use entities::common::time::Instant;
use entities::document_registry::business_document::ApprovalDefinitionBinding;
use entities::payable::SupplierPayment;
use entities::work_item::{WorkItem, WorkItemCloseData, WorkItemStatus};
use id_generator::next_id;
use mongodb::Database;

use super::start_approval::load_bound_definition_graph;
use crate::approval::execution::authorization::{converge_eligibility, requires_blocked_cancel};
use crate::approval::execution::idempotency::normalize_idempotency_key;
use crate::approval::execution::{CancelExecutionInput, ExecutionCommandInput, PreparedExecution};
use crate::errors::{Error, Result};

/// 已加载的可撤回运行事实。
pub(super) struct LoadedCancelRuntime {
    /// 绑定定义图。
    pub graph: DefinitionGraph,
    /// 非终态实例。
    pub instance: ApprovalProcessInstance,
    /// 当前执行。
    pub current: ApprovalNodeExecution,
    /// 当前执行上的开放任务。
    pub open_tasks: Vec<WorkItem>,
}

/// 按主体加载 RUNNING/BLOCKED 实例、当前执行与开放任务。
///
/// 已 `APPROVED` 的实例必须拒绝。`BLOCKED` 时不得存在开放任务。
///
/// # 参数
/// * `db` - 数据库
/// * `binding` - 创建时冻结的定义绑定
/// * `subject` - 业务对象引用
/// * `subject_version` - 冻结提交版本
///
/// # 错误
/// 实例缺失、已终态、受阻仍有开放任务或仓储失败时返回错误。
pub(super) async fn load_cancel_runtime(
    db: &Database,
    binding: &ApprovalDefinitionBinding,
    subject: &bpm::SubjectRef,
    subject_version: u32,
) -> Result<LoadedCancelRuntime> {
    let instance = db
        .bpm_workflow()
        .find_non_terminal_by_subject(subject, subject_version, &mut NoTransaction)
        .await?
        .ok_or_else(|| Error::ConflictError("没有可撤回的审批实例".to_string()))?;
    ensure_instance_cancellable(&instance)?;
    let current = db
        .bpm_workflow()
        .find_current_execution(
            &ApprovalProcessInstanceId::new(instance.base.id.clone()),
            &mut NoTransaction,
        )
        .await?
        .ok_or_else(|| Error::ConflictError("审批实例缺少当前执行".to_string()))?;
    let open_tasks = db
        .work_items()
        .find_many(
            mongodb::bson::doc! {
                "approval_node_execution_id": &current.base.id,
                "status": WorkItemStatus::Open.as_str(),
            },
            &mut NoTransaction,
        )
        .await?;
    ensure_open_tasks_match_instance(&instance, open_tasks.len())?;
    Ok(LoadedCancelRuntime {
        graph: load_bound_definition_graph(db, binding).await?,
        instance,
        current,
        open_tasks,
    })
}

/// 已最终通过的实例不得撤回。
///
/// # 错误
/// 终态通过或状态不允许时返回冲突。
pub fn ensure_instance_cancellable(instance: &ApprovalProcessInstance) -> Result<()> {
    if instance.status == ApprovalProcessInstanceStatus::Approved {
        return Err(Error::ConflictError("已最终通过的审批实例不得撤回".to_string()));
    }
    if !matches!(
        instance.status,
        ApprovalProcessInstanceStatus::Running | ApprovalProcessInstanceStatus::Blocked
    ) {
        return Err(Error::ConflictError(
            "只有运行中或受阻的审批实例可以撤回".to_string(),
        ));
    }
    Ok(())
}

/// RUNNING 必须锁定开放任务；BLOCKED 必须证明没有开放任务。
///
/// # 错误
/// 任务数量与实例状态不一致时返回冲突。
fn ensure_open_tasks_match_instance(
    instance: &ApprovalProcessInstance,
    open_task_count: usize,
) -> Result<()> {
    match instance.status {
        ApprovalProcessInstanceStatus::Running => Ok(()),
        ApprovalProcessInstanceStatus::Blocked if open_task_count == 0 => Ok(()),
        ApprovalProcessInstanceStatus::Blocked => {
            Err(Error::ConflictError("受阻审批实例不得存在开放任务".to_string()))
        }
        _ => Err(Error::ConflictError(
            "只有运行中或受阻的审批实例可以撤回".to_string(),
        )),
    }
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
/// # 错误
/// 原因/幂等键非法、审批人引用无效或端口与 blocker 不匹配时返回错误。
pub fn build_supplier_payment_cancel_input(
    runtime: &LoadedCancelRuntime,
    reason: &str,
    actor_id: &str,
    idempotency_key: &str,
    receipt: Option<bpm::model::ApprovalCommandReceipt>,
    now: Instant,
) -> Result<CancelExecutionInput> {
    let reason = reason.trim();
    if reason.is_empty() {
        return Err(Error::ValidationError("撤回原因不能为空".to_string()));
    }
    let idempotency_key = normalize_idempotency_key(idempotency_key)?;
    let actor =
        ParticipantId::new(actor_id).map_err(|_| Error::ValidationError("撤回人引用无效".to_string()))?;
    let eligibility = cancel_eligibility(&runtime.current)?;
    let blocked_port = runtime.instance.blocker_code.is_some_and(requires_blocked_cancel);
    Ok(CancelExecutionInput {
        command: ExecutionCommandInput {
            graph: runtime.graph.clone(),
            current_eligibility: eligibility.clone(),
            next_eligibility: eligibility,
            receipt,
            idempotency_key,
            now: Timestamp::from_utc(now.as_utc()),
        },
        instance: runtime.instance.clone(),
        current: runtime.current.clone(),
        subject_version: runtime.instance.subject_version,
        expected_instance_version: runtime.instance.base.version,
        expected_execution_version: runtime.current.base.version,
        expected_task_version: runtime.open_tasks.first().map(|item| item.base.version),
        reason: reason.to_string(),
        actor,
        close_open_task: runtime.instance.status == ApprovalProcessInstanceStatus::Running,
        blocked_port,
        receipt_id: ApprovalCommandReceiptId::new(next_id()),
    })
}

/// 由当前执行构造取消资格，不得补默认办理人。
///
/// # 错误
/// 审批人引用或显示名为空时返回校验错误。
fn cancel_eligibility(current: &ApprovalNodeExecution) -> Result<Eligibility> {
    converge_eligibility(
        current.assignee_participant_id.as_str(),
        &current.assignee_name_snapshot,
        None,
    )
}

/// 在同一事务内应用取消计划、关闭任务并写回付款单。
///
/// # 参数
/// * `db` - 数据库
/// * `payment` - 已执行 `cancel_action` 的付款单
/// * `prepared` - `prepare_cancel` 结果
/// * `open_tasks` - 待关闭的开放任务
/// * `actor_id` - 撤回人
/// * `reason` - 撤回原因
/// * `now` - 调用方时间
/// * `audit` - 已构造审计
///
/// # 错误
/// CAS 冲突或仓储失败时返回错误，事务回滚。
#[allow(clippy::too_many_arguments)]
pub(super) async fn persist_supplier_payment_cancel(
    db: &Database,
    mut payment: SupplierPayment,
    prepared: PreparedExecution,
    open_tasks: Vec<WorkItem>,
    actor_id: &str,
    reason: &str,
    now: Instant,
    audit: entities::AuditLog,
) -> Result<()> {
    let db = db.clone();
    let client = db.client().clone();
    let actor_id = actor_id.to_string();
    let reason = reason.to_string();
    client
        .with_transaction(move |session| {
            Box::pin(async move {
                if let PreparedExecution::Apply(writes) = prepared {
                    persist_cancel_runtime(&db, &writes, &open_tasks, &actor_id, &reason, now, session)
                        .await?;
                }
                db.supplier_payments().update(&mut payment, session).await?;
                db.audit_logs().create(&audit, session).await?;
                Ok::<(), crate::errors::Error>(())
            })
        })
        .await
}

/// 将取消计划写入实例、执行、收据与任务。
///
/// # 错误
/// CAS 未应用或写入失败时返回错误。
async fn persist_cancel_runtime(
    db: &Database,
    writes: &crate::approval::execution::apply_plan::PlannedWrites,
    open_tasks: &[WorkItem],
    actor_id: &str,
    reason: &str,
    now: Instant,
    session: &mut mongodb::ClientSession,
) -> Result<()> {
    let expected_instance_version = writes
        .instance
        .base
        .version
        .checked_sub(1)
        .ok_or_else(|| Error::Internal("取消后实例版本非法".to_string()))?;
    let expected_execution_id = writes
        .updated_executions
        .first()
        .map(|item| bpm::ids::ApprovalNodeExecutionId::new(item.base.id.clone()))
        .ok_or_else(|| Error::Internal("取消计划缺少结束执行".to_string()))?;
    require_cas_applied(
        db.bpm_workflow()
            .advance_instance(
                &writes.instance,
                expected_instance_version,
                &expected_execution_id,
                &cancel_list_projection(now),
                session,
            )
            .await?,
        "审批实例",
    )?;
    for execution in &writes.updated_executions {
        let expected = execution
            .base
            .version
            .checked_sub(1)
            .ok_or_else(|| Error::Internal("取消后执行版本非法".to_string()))?;
        require_cas_applied(
            db.bpm_workflow()
                .end_active_execution(execution, expected, session)
                .await?,
            "审批执行",
        )?;
    }
    db.approval_command_receipts()
        .create(&writes.receipt, session)
        .await?;
    close_open_tasks(db, open_tasks, actor_id, reason, now, session).await
}

/// 关闭当前开放审批任务。
///
/// # 错误
/// 任务非开放或 CAS 失败时返回错误。
async fn close_open_tasks(
    db: &Database,
    open_tasks: &[WorkItem],
    actor_id: &str,
    reason: &str,
    now: Instant,
    session: &mut mongodb::ClientSession,
) -> Result<()> {
    for item in open_tasks {
        let mut item = item.clone();
        let expected = item.base.version;
        let execution_id = item
            .approval_node_execution_id
            .clone()
            .ok_or_else(|| Error::ConflictError("开放审批任务缺少节点执行引用".to_string()))?;
        item.close_by_approval_runtime(
            actor_id,
            WorkItemCloseData {
                close_reason: reason.to_string(),
            },
            now,
        )?;
        require_cas_applied(
            db.work_items()
                .close_approval_task(&item, expected, &execution_id, session)
                .await?,
            "审批任务",
        )?;
    }
    Ok(())
}

/// 取消后的有界列表投影：不得再展示当前审批人。
///
/// # 参数
/// * `now` - 状态变更时间
///
/// # 返回
/// 返回清空当前节点的列表投影。
fn cancel_list_projection(now: Instant) -> ApprovalInstanceListProjection {
    ApprovalInstanceListProjection {
        current_node_key: None,
        current_node_name: None,
        current_assignee_participant_id: None,
        current_assignee_name: None,
        latest_rejected_execution_id: None,
        latest_rejection_summary: None,
        last_status_changed_at: Some(now.unix_secs()),
    }
}

/// CAS 未应用时失败关闭。
///
/// # 错误
/// 未找到、版本冲突或状态已变时返回冲突。
fn require_cas_applied<T>(outcome: database::repository::bpm::CasWriteOutcome<T>, label: &str) -> Result<()> {
    match outcome {
        database::repository::bpm::CasWriteOutcome::Applied(_) => Ok(()),
        _ => Err(Error::ConflictError(format!(
            "{label}已被其他请求修改，请刷新后重试"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::ensure_instance_cancellable;
    use bpm::ids::{ApprovalProcessDefinitionId, ApprovalProcessInstanceId};
    use bpm::model::types::ApprovalProcessInstanceStatus;
    use bpm::model::{ApprovalProcessInstance, ParticipantId, ProcessKind, SubjectRef, Timestamp};

    fn instance(status: ApprovalProcessInstanceStatus) -> ApprovalProcessInstance {
        let mut item = ApprovalProcessInstance::start_running(
            ApprovalProcessInstanceId::new("inst-1"),
            ApprovalProcessDefinitionId::new("def-1"),
            1,
            ProcessKind::SupplierPayment,
            SubjectRef::new("supplier_payment", "sp-1").unwrap(),
            1,
            ParticipantId::new("u1").unwrap(),
            Timestamp::from_unix_secs(1).unwrap(),
        )
        .expect("实例夹具");
        item.status = status;
        item
    }

    /// 已最终通过不得撤回。
    #[test]
    fn approved_instance_cannot_cancel() {
        assert!(ensure_instance_cancellable(&instance(ApprovalProcessInstanceStatus::Approved)).is_err());
        assert!(ensure_instance_cancellable(&instance(ApprovalProcessInstanceStatus::Running)).is_ok());
    }
}
