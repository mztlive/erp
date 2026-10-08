//! 库存撤回的领域事实构造及跨域终态证明。

use bpm::ids::ApprovalNodeExecutionId;
use bpm::model::types::{ApprovalCommandKind, ApprovalNodeExecutionStatus, ApprovalProcessInstanceStatus};
use bpm::model::{ApprovalCommandReceipt, ApprovalNodeExecution, ApprovalProcessInstance, Timestamp};
use erp_inventory::entity::cancellation::{
    CancellationCommandIdentity, StockAdjustmentCancellation, StockCancellationTask,
};
use erp_inventory::repository::StockAdjustmentCancellationExt;
use erp_workflow::entity::work_item::{WorkItem, WorkItemStatus, WorkItemType};
use erp_workflow::repository::prelude::*;
use erp_workflow::{BpmExt, WorkItemExt};
use mongodb::Database;
use persistence_core::Executor;

use crate::{Error, Result};

/// 从审批回执精确投影库存领域消费的命令身份。
/// # 参数
/// * `receipt` - 同事务写入的原审批命令回执。
/// # 返回
/// 返回库存领域验证回放所需的完整命令身份。
///
/// # 错误
/// 不返回错误。
pub(super) fn cancellation_identity(receipt: &ApprovalCommandReceipt) -> CancellationCommandIdentity {
    CancellationCommandIdentity {
        receipt_id: receipt.base.id.clone(),
        scope_id: receipt.scope_id.clone(),
        idempotency_key: receipt.idempotency_key.to_string(),
        payload_digest: receipt.payload_digest.clone(),
    }
}

/// 同事务读取不可变撤回事实，缺失事实不得退回审计恢复。
/// # 参数
/// * `db` - 组合层数据库句柄。
/// * `instance` - 原已取消审批实例。
/// * `executor` - 当前事务的唯一执行器。
/// # 返回
/// 返回已证明原执行和全部历史任务的库存撤回事实。
/// # 错误
/// 事实缺失、终态不匹配或仓储读取失败时拒绝回放。
pub(super) async fn cancellation_fact(
    db: &Database,
    instance: &ApprovalProcessInstance,
    executor: &mut dyn Executor,
) -> Result<StockAdjustmentCancellation> {
    let fact = db
        .stock_adjustment_cancellations()
        .find_by_id(&instance.base.id, executor)
        .await?
        .ok_or_else(|| Error::ConflictError("库存调整撤回缺少结构化操作事实".into()))?;
    fact.validate()?;
    let execution_id = ApprovalNodeExecutionId::new(&fact.data.execution_id);
    let execution = db
        .bpm_workflow()
        .find_execution_by_id(&execution_id, executor)
        .await?
        .ok_or_else(invalid_terminal)?;
    let tasks = db.work_items().approval_tasks_for_execution(&execution_id, executor).await?;
    ensure_terminal(&fact, instance, &execution, &tasks)?;
    Ok(fact)
}

/// 证明保存的取消事实与实例、执行和原任务均为同一次提交。
fn ensure_terminal(
    fact: &StockAdjustmentCancellation,
    instance: &ApprovalProcessInstance,
    execution: &ApprovalNodeExecution,
    tasks: &[WorkItem],
) -> Result<()> {
    let data = &fact.data;
    let ended_at = Timestamp::from_utc(data.cancelled_at.as_utc());
    if instance.base.id != data.instance_id
        || instance.subject.subject_kind() != "stock_adjustment"
        || instance.subject.subject_id() != data.stock_adjustment_id
        || instance.subject_version != data.subject_version
        || instance.status != ApprovalProcessInstanceStatus::Cancelled
        || instance.base.is_deleted()
        || instance.current_node_execution_id.is_some()
        || instance.blocker_code.is_some()
        || Some(instance.base.version) != data.instance_version.checked_add(1)
        || instance.ended_at != Some(ended_at)
        || execution.base.id != data.execution_id
        || execution.process_instance_id.as_ref() != instance.base.id
        || execution.round_no != instance.current_round_no
        || execution.status != ApprovalNodeExecutionStatus::Cancelled
        || execution.base.is_deleted()
        || execution.blocker_code.map(|code| code.as_str()) != data.blocker_code.as_deref()
        || execution.blocker_code.is_some_and(|code| !code.allows_assignee_recovery())
        || Some(execution.base.version) != data.execution_version.checked_add(1)
        || execution.ended_at != Some(ended_at)
        || !tasks_match(fact, tasks)
    {
        return Err(invalid_terminal());
    }
    Ok(())
}

/// 开放任务关闭后版本增一；阻塞前已经关闭的任务必须保持原版本。
fn tasks_match(fact: &StockAdjustmentCancellation, tasks: &[WorkItem]) -> bool {
    let data = &fact.data;
    let mut actual = tasks
        .iter()
        .map(|task| StockCancellationTask { task_id: task.base.id.clone(), version: task.base.version })
        .collect::<Vec<_>>();
    actual.sort();
    if actual != data.historical_tasks
        || tasks.iter().any(|task| {
            task.base.is_deleted()
                || task.status == WorkItemStatus::Open
                || task.work_item_type != WorkItemType::DocumentApproval
                || task.approval_node_execution_id.as_ref().map(AsRef::as_ref)
                    != Some(data.execution_id.as_str())
                || task.business_object_type != "stock_adjustment"
                || task.business_object_id != data.stock_adjustment_id
                || task.subject_version != data.subject_version.to_string()
        })
    {
        return false;
    }
    match (data.task_id.as_deref(), data.task_version) {
        (None, None) => true,
        (Some(id), Some(version)) => tasks.iter().any(|task| {
            task.base.id == id
                && Some(task.base.version) == version.checked_add(1)
                && task.status == WorkItemStatus::Closed
                && task.approval_node_execution_id.as_ref().map(AsRef::as_ref)
                    == Some(data.execution_id.as_str())
                && task.business_object_type == "stock_adjustment"
                && task.business_object_id == data.stock_adjustment_id
                && task.subject_version == data.subject_version.to_string()
                && task.closed_by.as_deref() == Some(data.actor_id.as_str())
                && task.close_reason.as_deref() == Some(data.reason.as_str())
                && task.closed_at == Some(data.cancelled_at)
        }),
        _ => false,
    }
}

/// 原任务关闭时版本增一；已关闭的阻塞历史任务保持原版本。
/// # 参数
/// * `tasks` - 本次取消执行关联的全部历史任务。
/// # 返回
/// 返回按任务身份排序的预期持久化版本。
/// # 错误
/// 开放任务版本不能递增时拒绝构造事实。
pub(super) fn historical_task_facts(tasks: &[WorkItem]) -> Result<Vec<StockCancellationTask>> {
    let mut facts = tasks
        .iter()
        .map(|task| {
            let version = if task.status == WorkItemStatus::Open {
                task.base.version.checked_add(1).ok_or_else(invalid_terminal)?
            } else {
                task.base.version
            };
            Ok(StockCancellationTask { task_id: task.base.id.clone(), version })
        })
        .collect::<Result<Vec<_>>>()?;
    facts.sort();
    Ok(facts)
}

/// 原独立审批回执和库存事实必须指向相同命令。
/// # 参数
/// * `fact` - 库存领域拥有的撤回事实。
/// * `receipt` - 同执行器读取的原独立审批回执。
/// # 返回
/// 两者命令身份完全一致时返回空值。
/// # 错误
/// 回执已删除或命令类型、结果、身份不匹配时拒绝回放。
pub(super) fn ensure_receipt(
    fact: &StockAdjustmentCancellation,
    receipt: &ApprovalCommandReceipt,
) -> Result<()> {
    if receipt.base.is_deleted()
        || receipt.command_kind != ApprovalCommandKind::CancelApproval
        || receipt.result_ref != fact.data.instance_id
        || cancellation_identity(receipt) != fact.data.command
    {
        return Err(invalid_terminal());
    }
    Ok(())
}

/// 回执或取消终态不一致的稳定冲突。
fn invalid_terminal() -> Error {
    Error::ConflictError("库存调整撤回收据与终态事实不一致".into())
}

#[cfg(test)]
mod tests {
    use bpm::ids::{ApprovalCommandReceiptId, ApprovalProcessDefinitionId, ApprovalProcessInstanceId};
    use bpm::model::types::ApprovalExecutionAssignmentSource;
    use bpm::model::{
        IdempotencyKey, NewNodeExecution, NewProcessInstance, ParticipantId, ProcessKind, SubjectRef,
    };
    use erp_core::common::time::Instant;
    use erp_core::ids::WorkItemId;
    use erp_inventory::entity::cancellation::StockAdjustmentCancellationData;
    use erp_workflow::entity::work_item::{DocumentApprovalWorkItemData, WorkItemPriority};
    use erp_workflow::service::approval::execution::idempotency::{
        DocumentCancelIdentityParams, document_cancel_identity,
    };

    use super::*;

    fn fixture() -> (
        StockAdjustmentCancellation,
        ApprovalProcessInstance,
        ApprovalNodeExecution,
        WorkItem,
        ApprovalCommandReceipt,
    ) {
        let at = Timestamp::from_unix_secs(10).unwrap();
        let mut instance = ApprovalProcessInstance::start_running(NewProcessInstance {
            id: ApprovalProcessInstanceId::new("instance"),
            process_definition_id: ApprovalProcessDefinitionId::new("definition"),
            definition_version: 1,
            process_kind: ProcessKind::StockAdjustment,
            subject: SubjectRef::new("stock_adjustment", "adjustment").unwrap(),
            subject_version: 1,
            started_by: ParticipantId::new("actor").unwrap(),
            at,
        })
        .unwrap();
        let mut execution = ApprovalNodeExecution::new_active(NewNodeExecution {
            id: ApprovalNodeExecutionId::new("execution"),
            process_instance_id: instance.typed_id(),
            node_key: "review".into(),
            node_name: "复核".into(),
            round_no: 1,
            execution_no: 1,
            assignment_source: ApprovalExecutionAssignmentSource::Definition,
            replaces_execution_id: None,
            assignee_participant_id: ParticipantId::new("approver").unwrap(),
            assignee_name_snapshot: "审批人".into(),
            at,
        })
        .unwrap();
        instance.set_current_execution(execution.typed_id(), at).unwrap();
        let task = WorkItem::new_document_approval(
            WorkItemId::new("task"),
            DocumentApprovalWorkItemData {
                approval_node_execution_id: execution.typed_id(),
                business_object_type: "stock_adjustment".into(),
                business_object_id: "adjustment".into(),
                subject_version: "1".into(),
                owner_role: "stock_adjustment_approver".into(),
                owner_organization_id: "org".into(),
                owner_user_id: "approver".into(),
                priority: WorkItemPriority::Normal,
                due_at: None,
            },
            Instant::from_unix_secs(10),
        )
        .unwrap();
        let identity = document_cancel_identity(DocumentCancelIdentityParams {
            idempotency_key: IdempotencyKey::parse("key").unwrap(),
            instance_id: "instance",
            subject_version: 1,
            expected_document_version: 3,
            expected_instance_version: instance.base.version,
            expected_execution_version: execution.base.version,
            expected_task_version: Some(task.base.version),
            reason: "撤回",
            actor_id: "actor",
        })
        .unwrap();
        let ended = Timestamp::from_unix_secs(20).unwrap();
        let receipt = ApprovalCommandReceipt::new(
            ApprovalCommandReceiptId::new("receipt"),
            identity.current(),
            "instance",
            ended,
        )
        .unwrap();
        let fact = StockAdjustmentCancellation::new(StockAdjustmentCancellationData {
            schema_version: 1,
            stock_adjustment_id: "adjustment".into(),
            instance_id: "instance".into(),
            execution_id: "execution".into(),
            subject_version: 1,
            actor_id: "actor".into(),
            reason: "撤回".into(),
            command: cancellation_identity(&receipt),
            audit_event_id: "audit".into(),
            document_version: 3,
            instance_version: instance.base.version,
            execution_version: execution.base.version,
            task_id: Some(task.base.id.clone()),
            task_version: Some(task.base.version),
            historical_tasks: historical_task_facts(std::slice::from_ref(&task)).unwrap(),
            blocker_code: None,
            cancelled_at: Instant::from_unix_secs(20),
        })
        .unwrap();
        instance.cancel(ended).unwrap();
        execution.cancel(ended).unwrap();
        let mut task =
            task.close_for_approval_cancellation("actor", "撤回", Instant::from_unix_secs(20)).unwrap();
        task.base.version += 1;
        (fact, instance, execution, task, receipt)
    }

    #[test]
    fn stock_cancel_requires_exact_original_receipt_and_terminal_task() {
        let (fact, instance, execution, task, receipt) = fixture();
        assert!(ensure_receipt(&fact, &receipt).is_ok());
        assert!(ensure_terminal(&fact, &instance, &execution, std::slice::from_ref(&task)).is_ok());
        let mut wrong = receipt;
        wrong.payload_digest = "different".into();
        assert!(ensure_receipt(&fact, &wrong).is_err());
        for case in 0..10 {
            let mut wrong = task.clone();
            match case {
                0 => wrong.base.version += 1,
                1 => wrong.closed_by = Some("other".into()),
                2 => wrong.close_reason = Some("other".into()),
                3 => wrong.closed_at = Some(Instant::from_unix_secs(21)),
                4 => wrong.business_object_id = "foreign".into(),
                5 => wrong.status = WorkItemStatus::Open,
                6 => wrong.approval_node_execution_id = Some(ApprovalNodeExecutionId::new("foreign")),
                7 => wrong.subject_version = "2".into(),
                8 => wrong.business_object_type = "sales_order".into(),
                _ => wrong.work_item_type = WorkItemType::FulfillmentOperation,
            }
            assert!(ensure_terminal(&fact, &instance, &execution, &[wrong]).is_err(), "case {case}");
        }
        assert!(ensure_terminal(&fact, &instance, &execution, &[]).is_err());
        let mut wrong = execution;
        wrong.ended_at = Some(Timestamp::from_unix_secs(21).unwrap());
        assert!(ensure_terminal(&fact, &instance, &wrong, &[task]).is_err());
    }

    #[test]
    fn stock_cancel_retains_previously_closed_task_versions_and_rejects_extra_tasks() {
        let (mut fact, instance, execution, task, _) = fixture();
        fact.data.task_id = None;
        fact.data.task_version = None;
        fact.data.historical_tasks = historical_task_facts(std::slice::from_ref(&task)).unwrap();
        assert_eq!(fact.data.historical_tasks[0].version, task.base.version);
        assert!(ensure_terminal(&fact, &instance, &execution, std::slice::from_ref(&task)).is_ok());
        assert!(ensure_terminal(&fact, &instance, &execution, &[]).is_err());
        assert!(ensure_terminal(&fact, &instance, &execution, &[task.clone(), task]).is_err());
    }
}
