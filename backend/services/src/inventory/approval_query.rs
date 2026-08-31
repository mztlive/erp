//! 库存调整详情的审批运行摘要与 actor-aware 普通撤回令牌。

use bpm::ids::{ApprovalNodeExecutionId, ApprovalProcessInstanceId};
use bpm::model::types::{ApprovalNodeExecutionStatus, ApprovalProcessInstanceStatus};
use bpm::model::{ApprovalNodeExecution, ApprovalProcessInstance};
use database::{ApprovalIntegrationExt, BpmExt, NoTransaction, WorkItemExt};
use entities::document_registry::business_document::ApprovalDefinitionBinding;
use entities::document_registry::DocumentType;
use entities::inventory::{StockAdjustment, StockAdjustmentState};
use entities::work_item::WorkItem;

use super::adapter::{
    document_approval_view_with_history, require_frozen_binding, stock_adjustment_subject_ref,
    RECENT_HISTORY_LIMIT,
};
use super::cancel_approval::{
    actor_can_cancel, ensure_cancel_instance_binding, ensure_cancel_instance_subject,
    ensure_stock_adjustment_open_task_identity,
};
use super::dto::{
    CancelStockAdjustmentApprovalTokenView, DocumentApprovalHistoryItemView, DocumentApprovalHistoryPageView,
    DocumentApprovalInstanceView, DocumentApprovalView, SubmitStockAdjustmentApprovalTokenView,
};
use super::start_approval::actor_can_submit;
use super::InventoryService;
use crate::approval::execution::authorization::requires_blocked_cancel;
use crate::approval::execution::{
    history_item_from_execution, history_page_from, latest_rejection_reason, RuntimeHistoryItem,
};
use crate::approval::process_kind::process_kind_of;
use crate::audit::AuditActor;
use crate::errors::{Error, Result};

/// 加载库存调整详情的审批实例、历史与当前调用人撤回令牌。
pub(super) async fn load_document_approval(
    service: &InventoryService,
    adjustment: &StockAdjustment,
    binding: Option<&ApprovalDefinitionBinding>,
    actor: &AuditActor,
) -> Result<DocumentApprovalView> {
    let submit_command = submit_token(service, adjustment, binding, actor).await?;
    let subject = stock_adjustment_subject_ref(&adjustment.base.id)?;
    let Some(instance) = service
        .db
        .bpm_workflow()
        .find_latest_by_subject(&subject, &mut NoTransaction)
        .await?
    else {
        return Ok(document_approval_view_with_history(
            binding,
            None,
            Vec::new(),
            empty_history_page(),
            adjustment.status,
            submit_command,
            None,
        ));
    };
    project_runtime(service, adjustment, binding, instance, submit_command, actor).await
}

/// 按签署收据的 `result_ref` 精确投影审批实例。
///
/// 本入口不生成新的提交令牌。旧实例已取消、单据已回到草稿或重新提交时，
/// 响应仍只能展示原命令实例，不能回落到主题上的最新实例或泄露新命令动作。
pub(super) async fn load_document_approval_for_instance(
    service: &InventoryService,
    adjustment: &StockAdjustment,
    binding: Option<&ApprovalDefinitionBinding>,
    instance_id: &str,
    expected_subject_version: u32,
    actor: &AuditActor,
) -> Result<DocumentApprovalView> {
    let instance = service
        .db
        .bpm_workflow()
        .find_instance_by_id(&ApprovalProcessInstanceId::new(instance_id), &mut NoTransaction)
        .await?
        .ok_or_else(|| Error::ConflictError("库存调整签署结果引用的审批实例不存在".to_string()))?;
    if instance.base.id != instance_id
        || instance.process_kind != process_kind_of(DocumentType::StockAdjustment)
        || instance.subject.subject_kind() != DocumentType::StockAdjustment.as_str()
        || instance.subject.subject_id() != adjustment.base.id
        || instance.subject_version != expected_subject_version
    {
        return Err(Error::ConflictError(
            "库存调整签署结果与审批实例身份不一致".to_string(),
        ));
    }
    ensure_cancel_instance_binding(&instance, require_frozen_binding(binding)?)?;
    project_runtime(service, adjustment, binding, instance, None, actor).await
}

async fn project_runtime(
    service: &InventoryService,
    adjustment: &StockAdjustment,
    binding: Option<&ApprovalDefinitionBinding>,
    instance: ApprovalProcessInstance,
    submit_command: Option<SubmitStockAdjustmentApprovalTokenView>,
    actor: &AuditActor,
) -> Result<DocumentApprovalView> {
    let instance_id = ApprovalProcessInstanceId::new(instance.base.id.clone());
    let current = service
        .db
        .bpm_workflow()
        .find_current_execution(&instance_id, &mut NoTransaction)
        .await?;
    let open_tasks = load_open_tasks(service, current.as_ref()).await?;
    let rows = service
        .db
        .bpm_workflow()
        .list_execution_history(
            &instance_id,
            None,
            (RECENT_HISTORY_LIMIT as u32).saturating_add(1),
            &mut NoTransaction,
        )
        .await?;
    let history = history_page_from(
        rows.iter().map(history_item_from_execution).collect(),
        RECENT_HISTORY_LIMIT as u32,
    );
    let cancel_command = cancel_token(
        service,
        adjustment,
        &instance,
        current.as_ref(),
        &open_tasks,
        binding,
        actor,
    )
    .await?;
    Ok(document_approval_view_with_history(
        binding,
        Some(instance_view(
            &instance,
            current.as_ref(),
            open_tasks.first(),
            latest_rejection_reason(&history.items),
        )),
        history.items.iter().map(history_item_view).collect(),
        DocumentApprovalHistoryPageView {
            next_cursor: history.next_cursor,
            has_more: history.has_more,
        },
        adjustment.status,
        submit_command,
        cancel_command,
    ))
}

async fn submit_token(
    service: &InventoryService,
    adjustment: &StockAdjustment,
    binding: Option<&ApprovalDefinitionBinding>,
    actor: &AuditActor,
) -> Result<Option<SubmitStockAdjustmentApprovalTokenView>> {
    if adjustment.status != StockAdjustmentState::Draft || binding.is_none() {
        return Ok(None);
    }
    if !actor_can_submit(&service.db, &service.rbac, adjustment, actor).await? {
        return Ok(None);
    }
    let expected_subject_version = adjustment
        .approval_subject_version
        .checked_add(1)
        .ok_or_else(|| Error::ConflictError("库存调整审批主题版本已达上限".to_string()))?;
    Ok(Some(SubmitStockAdjustmentApprovalTokenView {
        expected_version: adjustment.base.version.to_string(),
        expected_subject_version: expected_subject_version.to_string(),
    }))
}

async fn load_open_tasks(
    service: &InventoryService,
    current: Option<&ApprovalNodeExecution>,
) -> Result<Vec<WorkItem>> {
    let Some(current) = current else {
        return Ok(Vec::new());
    };
    Ok(service
        .db
        .work_items()
        .open_approval_tasks_for_execution(
            &ApprovalNodeExecutionId::new(current.base.id.clone()),
            &mut NoTransaction,
        )
        .await?)
}

async fn cancel_token(
    service: &InventoryService,
    adjustment: &StockAdjustment,
    instance: &ApprovalProcessInstance,
    current: Option<&ApprovalNodeExecution>,
    open_tasks: &[WorkItem],
    binding: Option<&ApprovalDefinitionBinding>,
    actor: &AuditActor,
) -> Result<Option<CancelStockAdjustmentApprovalTokenView>> {
    if adjustment.status != StockAdjustmentState::InApproval
        || adjustment.approval_subject_version != instance.subject_version
    {
        return Ok(None);
    }
    ensure_cancel_instance_subject(instance, &adjustment.base.id, adjustment.approval_subject_version)?;
    ensure_cancel_instance_binding(instance, require_frozen_binding(binding)?)?;
    let snapshot = service
        .db
        .approval_subject_snapshots()
        .find_by_process_instance_id(&instance.base.id, &mut NoTransaction)
        .await?
        .ok_or_else(|| Error::ConflictError("审批实例缺少冻结业务快照".to_string()))?;
    snapshot
        .ensure_matches_runtime_subject(
            DocumentType::StockAdjustment,
            &adjustment.base.id,
            instance.subject_version,
        )
        .map_err(|_| Error::ConflictError("审批实例与冻结业务快照不一致".to_string()))?;
    let Some(current) = current else {
        return Ok(None);
    };
    if current.process_instance_id.as_ref() != instance.base.id
        || current.round_no != instance.current_round_no
        || instance.current_node_execution_id.as_ref().map(AsRef::as_ref) != Some(current.base.id.as_str())
    {
        return Err(Error::ConflictError(
            "库存调整审批当前执行与实例不一致".to_string(),
        ));
    }
    let expected_task_version = match instance.status {
        ApprovalProcessInstanceStatus::Running
            if current.status == ApprovalNodeExecutionStatus::Active && open_tasks.len() == 1 =>
        {
            ensure_stock_adjustment_open_task_identity(
                &open_tasks[0],
                instance,
                current,
                &snapshot.payload.responsible_org_id,
            )?;
            Some(open_tasks[0].base.version.to_string())
        }
        ApprovalProcessInstanceStatus::Blocked
            if current.status == ApprovalNodeExecutionStatus::Blocked
                && open_tasks.is_empty()
                && same_personnel_blocker(instance.blocker_code, current.blocker_code) =>
        {
            None
        }
        _ => return Ok(None),
    };
    if !actor_can_cancel(service, instance, actor).await? {
        return Ok(None);
    }
    Ok(Some(CancelStockAdjustmentApprovalTokenView {
        expected_version: adjustment.base.version.to_string(),
        approval_process_instance_id: instance.base.id.clone(),
        expected_subject_version: instance.subject_version.to_string(),
        expected_instance_version: instance.base.version.to_string(),
        expected_execution_version: current.base.version.to_string(),
        expected_task_version,
    }))
}

/// BLOCKED 普通撤回令牌只接受实例与执行两端完全一致的人员失效 blocker。
fn same_personnel_blocker(
    instance: Option<bpm::model::types::ApprovalBlockerCode>,
    execution: Option<bpm::model::types::ApprovalBlockerCode>,
) -> bool {
    instance
        .zip(execution)
        .is_some_and(|(instance, execution)| instance == execution && !requires_blocked_cancel(instance))
}

fn instance_view(
    instance: &ApprovalProcessInstance,
    current: Option<&ApprovalNodeExecution>,
    current_task: Option<&WorkItem>,
    latest_rejection: Option<String>,
) -> DocumentApprovalInstanceView {
    DocumentApprovalInstanceView {
        id: instance.base.id.clone(),
        status: instance.status.as_str().to_string(),
        current_round_no: instance.current_round_no,
        current_node: current.map(|execution| execution.node_key.clone()),
        current_assignee: current.map(|execution| execution.assignee_participant_id.as_str().to_string()),
        latest_rejection,
        subject_version: instance.subject_version.to_string(),
        instance_version: instance.base.version.to_string(),
        current_execution_id: current.map(|execution| execution.base.id.clone()),
        current_execution_version: current.map(|execution| execution.base.version.to_string()),
        current_task_id: current_task.map(|task| task.base.id.clone()),
        current_task_version: current_task.map(|task| task.base.version.to_string()),
    }
}

fn history_item_view(item: &RuntimeHistoryItem) -> DocumentApprovalHistoryItemView {
    DocumentApprovalHistoryItemView {
        execution_id: item.execution_id.clone(),
        round_no: item.round_no,
        node_key: item.node_key.clone(),
        result: item.result.clone(),
    }
}

fn empty_history_page() -> DocumentApprovalHistoryPageView {
    DocumentApprovalHistoryPageView {
        next_cursor: None,
        has_more: false,
    }
}

#[cfg(test)]
mod tests {
    use super::same_personnel_blocker;
    use bpm::model::types::ApprovalBlockerCode;

    /// BLOCKED 令牌必须要求实例与执行 blocker 同时存在、相等且属于人员失效。
    #[test]
    fn blocked_cancel_token_requires_matching_personnel_blocker() {
        let personnel = ApprovalBlockerCode::ApproverAccountInactive;
        assert!(same_personnel_blocker(Some(personnel), Some(personnel)));
        assert!(!same_personnel_blocker(Some(personnel), None));
        assert!(!same_personnel_blocker(None, Some(personnel)));
        assert!(!same_personnel_blocker(
            Some(personnel),
            Some(ApprovalBlockerCode::ApproverEmploymentInvalid)
        ));
        assert!(!same_personnel_blocker(
            Some(ApprovalBlockerCode::OpenTaskConflict),
            Some(ApprovalBlockerCode::OpenTaskConflict)
        ));
    }
}
