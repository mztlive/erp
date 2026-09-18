//! 库存调整撤回运行事实加载与取消输入构图。
//!
//! 本模块只做撤回前读取（实例/运行事实/绑定）与 `prepare_document_cancel`
//! 输入构图；事务提交与恢复由 [`cancel_approval`] 入口编排，持久化由
//! [`cancel_persist`] 执行。

use application_core::AuditActor;
use bpm::engine::DefinitionGraph;
use bpm::ids::{ApprovalCommandReceiptId, ApprovalNodeExecutionId, ApprovalProcessInstanceId};
use bpm::model::types::{ApprovalCommandKind, ApprovalProcessInstanceStatus};
use bpm::model::{
    ApprovalCancellationTaskPolicy, ApprovalNodeExecution, ApprovalProcessInstance, IdempotencyKey,
    ParticipantId, Timestamp,
};
use erp_audit::AuditExt;
use erp_audit::repository::prelude::*;
use erp_core::common::time::Instant;
use erp_identity::SharedRbacService;
use erp_inventory::{CancelStockAdjustmentApprovalRequest, InventoryExt, StockAdjustmentView};
use erp_workflow::entity::approval_integration::ApprovalNotificationEventKind;
use erp_workflow::entity::document_registry::DocumentType;
use erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding;
use erp_workflow::entity::work_item::{AssignmentSource, WorkItem, WorkItemType};
use erp_workflow::ports::WorkflowAuthorizationPort;
use erp_workflow::repository::prelude::*;
use erp_workflow::service::approval::execution::authorization::{
    converge_eligibility, requires_blocked_cancel,
};
use erp_workflow::service::approval::execution::idempotency::{
    DocumentCancelIdentityParams, PreparedCommandIdentity, ReceiptBranch, document_cancel_identity,
    payload_conflict_error,
};
use erp_workflow::service::approval::execution::{
    CancelExecutionInput, ExecutionCommandInput, PlannedWrites,
};
use erp_workflow::service::approval::process_kind::process_kind_of;
use erp_workflow::service::approval::{
    approval_actor_is_active_with_executor, approval_cancel_scope_with_executor,
    approval_document_read_scope_with_executor, definition_management_visibility_with_executor,
};
use erp_workflow::{ApprovalIntegrationExt, BpmExt, WorkItemExt};
use id_generator::next_id;
use mongodb::Database;
use persistence_core::{Executor, NoTransaction, Transactional};

use super::InventoryAdjustmentService;
use super::adapter::stock_adjustment_adapter;
use super::approval_prepare::load_bound_definition_graph;
use crate::{Error, Result};

pub(crate) const STOCK_ADJUSTMENT_CANCEL_AUDIT_ACTION: &str = "stock_adjustment.cancel_approval";
pub(crate) const STOCK_ADJUSTMENT_AUDIT_RESOURCE: &str = "stock_adjustment";

/// 已加载的可撤回运行事实。
pub(crate) struct LoadedCancelRuntime {
    /// 绑定定义图。
    pub graph: DefinitionGraph,
    /// 非终态实例。
    pub instance: ApprovalProcessInstance,
    /// 当前执行。
    pub current: ApprovalNodeExecution,
    /// 当前实例决定的任务关闭策略。
    pub task_policy: ApprovalCancellationTaskPolicy,
    /// 当前执行上的全部开放任务。
    pub open_tasks: Vec<WorkItem>,
}

/// 普通撤回的授权身份，用于审计应急代办路径。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CancelAuthority {
    Submitter,
    RuntimeAdmin,
}

/// 事务内重新证明的普通撤回授权事实。
pub(crate) struct CancelAuthorization {
    pub(crate) authority: CancelAuthority,
    pub(crate) submitted_by: String,
    pub(crate) responsible_org_id: String,
}

impl CancelAuthority {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Submitter => "submitter",
            Self::RuntimeAdmin => "runtime_admin",
        }
    }
}

/// 按请求中的稳定实例 ID 精确加载审批实例。
pub(crate) async fn load_cancel_instance(
    db: &Database,
    instance_id: &str,
) -> Result<ApprovalProcessInstance> {
    db.bpm_workflow()
        .find_instance_by_id(&ApprovalProcessInstanceId::new(instance_id), &mut NoTransaction)
        .await?
        .ok_or_else(|| Error::NotFound("审批实例不存在".to_string()))
}

/// 从候选实例加载 RUNNING/BLOCKED 当前执行与全部开放任务。
///
/// `RUNNING` 必须恰有一个开放任务，`BLOCKED` 必须没有开放任务。
pub(crate) async fn load_cancel_runtime(
    db: &Database,
    binding: &ApprovalDefinitionBinding,
    instance: ApprovalProcessInstance,
) -> Result<LoadedCancelRuntime> {
    let task_policy =
        instance.cancellation_task_policy().map_err(|error| Error::ConflictError(error.to_string()))?;
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

/// 在任何原状态或版本检查前解析已提交收据。
///
/// 实例 ID 直接来自强类型命令，因此即使单据已修改并以更高主题版本重新提交，
/// 仍能精确定位原命令作用域。收据读取和事务失败恢复都使用新会话。
pub(crate) async fn committed_cancel_replay(
    service: &InventoryAdjustmentService,
    id: &str,
    req: &CancelStockAdjustmentApprovalRequest,
    reason: &str,
    idempotency_key: &IdempotencyKey,
    actor: &AuditActor,
) -> Result<Option<StockAdjustmentView>> {
    let db = service.db.clone();
    let rbac = service.rbac.clone();
    let id = id.to_string();
    let req = req.clone();
    let reason = reason.to_string();
    let identity = document_cancel_identity(DocumentCancelIdentityParams {
        idempotency_key: idempotency_key.clone(),
        instance_id: &req.approval_process_instance_id,
        subject_version: req.expected_subject_version,
        expected_document_version: req.expected_version,
        expected_instance_version: req.expected_instance_version,
        expected_execution_version: req.expected_execution_version,
        expected_task_version: req.expected_task_version,
        reason: &reason,
        actor_id: actor.id(),
    })?;
    let actor = actor.clone();
    let client = db.client().clone();
    client
        .with_transaction(move |session| {
            Box::pin(async move {
                // 请求已携带稳定 instance scope，因此收据必须是快照内第一读。
                let receipt = find_cancel_receipt(&db, &identity, session).await?;
                let instance = db
                    .bpm_workflow()
                    .find_instance_by_id(
                        &ApprovalProcessInstanceId::new(&req.approval_process_instance_id),
                        session,
                    )
                    .await?
                    .ok_or_else(|| Error::NotFound("审批实例不存在".to_string()))?;
                ensure_cancel_instance_subject(&instance, &id, req.expected_subject_version)?;
                ensure_cancel_authorized_with_executor(&db, &rbac, &instance, &actor, session).await?;
                if instance.status == ApprovalProcessInstanceStatus::Cancelled {
                    let original_actor = committed_cancel_actor(&db, &id, &instance.base.id, session).await?;
                    if original_actor != actor.id() {
                        return Err(cancel_replay_actor_mismatch(&instance));
                    }
                }
                let Some(receipt) = receipt else {
                    return Ok(None);
                };
                if instance.status != ApprovalProcessInstanceStatus::Cancelled
                    || receipt.command_kind != ApprovalCommandKind::CancelApproval
                    || receipt.result_ref != instance.base.id
                {
                    return Err(Error::ConflictError("库存调整撤回收据与终态事实不一致".to_string()));
                }
                if !matches!(identity.classify(Some(&receipt)), ReceiptBranch::SamePayload(_)) {
                    return Err(payload_conflict_error().into());
                }
                let adjustment = db
                    .inventory()
                    .stock_adjustment(&id, session)
                    .await?
                    .ok_or_else(|| Error::NotFound("库存调整单不存在".to_string()))?;
                Ok(Some(adjustment.into()))
            })
        })
        .await
}

/// 按 V3、已知历史精确 scope 顺序读取普通撤回收据。
pub(crate) async fn find_cancel_receipt(
    db: &Database,
    identity: &PreparedCommandIdentity,
    executor: &mut dyn Executor,
) -> Result<Option<bpm::model::ApprovalCommandReceipt>> {
    for scope in identity.scope_candidates() {
        let receipt = db
            .bpm_workflow()
            .find_command_receipt(
                ApprovalCommandKind::CancelApproval,
                scope,
                identity.idempotency_key(),
                executor,
            )
            .await?;
        if receipt.is_some() {
            return Ok(receipt);
        }
    }
    Ok(None)
}

/// 从与取消收据同事务提交的不可变审计事实解析原命令操作人。
pub(crate) async fn committed_cancel_actor(
    db: &Database,
    adjustment_id: &str,
    instance_id: &str,
    executor: &mut dyn Executor,
) -> Result<String> {
    let actors = db
        .audit_logs()
        .list_successful_by_resource(STOCK_ADJUSTMENT_AUDIT_RESOURCE, adjustment_id, executor)
        .await?
        .into_iter()
        .filter(|audit| {
            audit.action == STOCK_ADJUSTMENT_CANCEL_AUDIT_ACTION
                && audit
                    .message
                    .as_deref()
                    .is_some_and(|message| cancel_audit_matches_instance(message, instance_id))
        })
        .map(|audit| audit.actor_id)
        .collect::<Vec<_>>();
    let [actor] = actors.as_slice() else {
        return Err(Error::ConflictError("库存调整撤回收据缺少唯一原命令操作人审计".to_string()));
    };
    Ok(actor.clone())
}

/// 构造不可歧义的取消审计实例前缀；原因仅追加在固定前缀之后。
pub(crate) fn cancel_audit_message_prefix(instance_id: &str) -> String {
    format!("instance={}:{} ", instance_id.len(), instance_id)
}

pub(crate) fn cancel_audit_matches_instance(message: &str, instance_id: &str) -> bool {
    message.starts_with(&cancel_audit_message_prefix(instance_id))
}

/// 非原命令操作人不得因收据是否存在获得不同错误投影。
pub(crate) fn cancel_replay_actor_mismatch(instance: &ApprovalProcessInstance) -> Error {
    match instance.cancellation_task_policy() {
        Err(error) => Error::ConflictError(error.to_string()),
        Ok(_) => Error::ConflictError("库存调整撤回收据与终态事实不一致".to_string()),
    }
}

const CANCEL_REPLAY_RECOVERY_ATTEMPTS: usize = 32;

/// 事务失败或结果未知后，以有限次新会话等待并发 winner 的收据可见。
pub(crate) async fn recover_cancel_replay(
    service: &InventoryAdjustmentService,
    id: &str,
    req: &CancelStockAdjustmentApprovalRequest,
    reason: &str,
    idempotency_key: &IdempotencyKey,
    actor: &AuditActor,
) -> Result<Option<StockAdjustmentView>> {
    for _ in 0..CANCEL_REPLAY_RECOVERY_ATTEMPTS {
        if let Some(view) = committed_cancel_replay(service, id, req, reason, idempotency_key, actor).await? {
            return Ok(Some(view));
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    Ok(None)
}

/// 校验实例确实属于路径中的库存调整单和命令冻结版本。
pub(crate) fn ensure_cancel_instance_subject(
    instance: &ApprovalProcessInstance,
    adjustment_id: &str,
    expected_subject_version: u32,
) -> Result<()> {
    if instance.process_kind != process_kind_of(DocumentType::StockAdjustment)
        || instance.subject.subject_kind() != DocumentType::StockAdjustment.as_str()
        || instance.subject.subject_id() != adjustment_id
        || instance.subject_version != expected_subject_version
    {
        return Err(Error::ConflictError("审批实例与库存调整撤回命令不一致".to_string()));
    }
    Ok(())
}

/// 校验实例流程种类与单据创建时冻结的定义绑定。
pub(crate) fn ensure_cancel_instance_binding(
    instance: &ApprovalProcessInstance,
    binding: &ApprovalDefinitionBinding,
) -> Result<()> {
    if instance.process_kind != process_kind_of(DocumentType::StockAdjustment)
        || instance.process_definition_id != binding.approval_process_definition_id
        || instance.definition_version != binding.approval_definition_version
    {
        return Err(Error::ConflictError("库存调整审批实例与冻结定义绑定不一致".to_string()));
    }
    Ok(())
}

/// 把普通业务撤回的唯一引擎通知意图规范为普通取消事件。
///
/// 人员失效实例仍走普通业务撤回，不得沿用受阻管理员取消的通知种类。
pub(crate) fn normalize_document_cancel_notification(writes: &mut PlannedWrites) -> Result<()> {
    let [intent] = writes.notifications.as_mut_slice() else {
        return Err(Error::Internal("库存调整普通撤回必须产生唯一取消通知意图".to_string()));
    };
    intent.event_kind = ApprovalNotificationEventKind::Cancelled;
    intent.dedup_key = format!("cancelled:{}:{}", writes.instance.base.id, writes.instance.current_round_no);
    Ok(())
}

/// 校验调用方持有的运行实例、执行与任务版本。
pub(crate) fn ensure_cancel_runtime_versions(
    runtime: &LoadedCancelRuntime,
    req: &CancelStockAdjustmentApprovalRequest,
    authorization: &CancelAuthorization,
) -> Result<()> {
    ensure_cancel_execution_identity(&runtime.instance, &runtime.current, runtime.task_policy)?;
    ensure_expected_version("审批实例", req.expected_instance_version, runtime.instance.base.version)?;
    ensure_expected_version("审批执行", req.expected_execution_version, runtime.current.base.version)?;
    match runtime.task_policy {
        ApprovalCancellationTaskPolicy::CloseOpenTask => {
            let expected = req
                .expected_task_version
                .ok_or_else(|| Error::ConflictError("运行中审批撤回必须提供开放任务版本".to_string()))?;
            let task = &runtime.open_tasks[0];
            ensure_open_task_matches_runtime(task, runtime, authorization)?;
            ensure_expected_version("审批任务", expected, task.base.version)
        },
        ApprovalCancellationTaskPolicy::NoOpenTask => {
            if req.expected_task_version.is_some() {
                return Err(Error::ConflictError("人员失效阻塞审批撤回的任务版本必须为空".to_string()));
            }
            Ok(())
        },
    }
}

/// 校验普通撤回实例与当前执行的身份、轮次、状态和 blocker 上下文。
pub(crate) fn ensure_cancel_execution_identity(
    instance: &ApprovalProcessInstance,
    current: &ApprovalNodeExecution,
    policy: ApprovalCancellationTaskPolicy,
) -> Result<()> {
    let identity_matches = current.process_instance_id.as_ref() == instance.base.id
        && instance.current_node_execution_id.as_ref().map(AsRef::as_ref) == Some(current.base.id.as_str())
        && current.round_no == instance.current_round_no;
    let state_matches = match policy {
        ApprovalCancellationTaskPolicy::CloseOpenTask => {
            instance.status == ApprovalProcessInstanceStatus::Running
                && current.status == bpm::model::types::ApprovalNodeExecutionStatus::Active
                && instance.blocker_code.is_none()
                && current.blocker_code.is_none()
        },
        ApprovalCancellationTaskPolicy::NoOpenTask => {
            instance.blocker_code.zip(current.blocker_code).is_some_and(|(instance_code, execution_code)| {
                instance.status == ApprovalProcessInstanceStatus::Blocked
                    && current.status == bpm::model::types::ApprovalNodeExecutionStatus::Blocked
                    && instance_code == execution_code
                    && !requires_blocked_cancel(instance_code)
            })
        },
    };
    if identity_matches && state_matches {
        return Ok(());
    }
    Err(Error::ConflictError("库存调整审批实例与当前执行不一致".to_string()))
}

/// 校验开放任务与当前实例、执行、对象和责任人完全一致。
pub(crate) fn ensure_open_task_matches_runtime(
    task: &WorkItem,
    runtime: &LoadedCancelRuntime,
    authorization: &CancelAuthorization,
) -> Result<()> {
    ensure_stock_adjustment_open_task_identity(
        task,
        &runtime.instance,
        &runtime.current,
        &authorization.responsible_org_id,
    )
}

/// 校验开放审批任务、当前执行、实例和冻结责任快照构成同一责任链。
pub(crate) fn ensure_stock_adjustment_open_task_identity(
    task: &WorkItem,
    instance: &ApprovalProcessInstance,
    current: &ApprovalNodeExecution,
    responsible_org_id: &str,
) -> Result<()> {
    let adapter = stock_adjustment_adapter()?;
    if instance.status != ApprovalProcessInstanceStatus::Running
        || instance.current_node_execution_id.as_ref().map(AsRef::as_ref) != Some(current.base.id.as_str())
        || current.status != bpm::model::types::ApprovalNodeExecutionStatus::Active
        || current.process_instance_id.as_ref() != instance.base.id
        || current.round_no != instance.current_round_no
        || task.work_item_type != WorkItemType::DocumentApproval
        || task.status != erp_workflow::entity::work_item::WorkItemStatus::Open
        || task.assignment_source != AssignmentSource::ApprovalRuntime
        || task.approval_node_execution_id.as_ref().map(AsRef::as_ref) != Some(current.base.id.as_str())
        || task.business_object_type != DocumentType::StockAdjustment.as_str()
        || task.business_object_id != instance.subject.subject_id()
        || task.subject_version != instance.subject_version.to_string()
        || task.owner_user_id.as_deref() != Some(current.assignee_participant_id.as_str())
        || task.owner_role != adapter.owner_role
        || task.owner_organization_id != responsible_org_id
    {
        return Err(Error::ConflictError("库存调整开放审批任务与当前运行事实不一致".to_string()));
    }
    Ok(())
}

pub(crate) fn ensure_expected_version(label: &str, expected: u64, actual: u64) -> Result<()> {
    if expected == actual {
        return Ok(());
    }
    Err(Error::ConflictError(format!("{label}版本已变化，请刷新后重试")))
}

pub(crate) fn normalize_cancel_reason(reason: &str) -> Result<String> {
    let reason = reason.trim();
    if reason.is_empty() {
        return Err(Error::ValidationError("撤回原因不能为空".to_string()));
    }
    Ok(reason.to_string())
}

/// 构造统一业务单据取消输入。
///
/// 普通撤回固定 `blocked_port=false`。人员失效允许走本端口；非人员一致性
/// blocker 由统一取消编排失败关闭，只能经运行管理员受阻取消入口处理。
pub(crate) fn build_stock_adjustment_cancel_input(
    runtime: &LoadedCancelRuntime,
    req: &CancelStockAdjustmentApprovalRequest,
    actor_id: &str,
    reason: &str,
    idempotency_key: &IdempotencyKey,
    receipt: Option<bpm::model::ApprovalCommandReceipt>,
    now: Instant,
) -> Result<CancelExecutionInput> {
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
        subject_version: req.expected_subject_version,
        expected_instance_version: req.expected_instance_version,
        expected_execution_version: req.expected_execution_version,
        expected_task_version: req.expected_task_version,
        reason: reason.to_string(),
        actor,
        close_open_task: runtime.task_policy.closes_open_task(),
        blocked_port: false,
        receipt_id: ApprovalCommandReceiptId::new(next_id()),
    })
}

/// 校验普通撤回的账号、动作权限、对象读取范围和提交人/运行管理员身份。
pub(crate) async fn ensure_cancel_authorized(
    service: &InventoryAdjustmentService,
    instance: &ApprovalProcessInstance,
    actor: &AuditActor,
) -> Result<CancelAuthorization> {
    ensure_cancel_authorized_with_executor(&service.db, &service.rbac, instance, actor, &mut NoTransaction)
        .await
}

/// 判断当前调用人是否可获得普通撤回动作投影。
///
/// 明确的授权拒绝映射为 `false`；仓储、政策登记或一致性错误继续上抛，禁止
/// 通过吞错把损坏事实伪装为“无动作”。
pub(crate) async fn actor_can_cancel(
    service: &InventoryAdjustmentService,
    instance: &ApprovalProcessInstance,
    actor: &AuditActor,
) -> Result<bool> {
    match ensure_cancel_authorized(service, instance, actor).await {
        Ok(_) => Ok(true),
        Err(Error::Forbidden(_)) => Ok(false),
        Err(error) => Err(error),
    }
}

/// 在调用方事务快照内重新校验普通撤回授权。
pub(crate) async fn ensure_cancel_authorized_with_executor(
    db: &Database,
    rbac: &SharedRbacService,
    instance: &ApprovalProcessInstance,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<CancelAuthorization> {
    if !approval_actor_is_active_with_executor(
        &crate::adapters::workflow::workflow_auth(db.clone(), rbac.clone()),
        actor,
        executor,
    )
    .await?
    {
        return Err(Error::Forbidden("当前账号不可执行库存调整审批撤回".to_string()));
    }
    let snapshot = db
        .approval_subject_snapshots()
        .find_by_process_instance_id(&instance.base.id, executor)
        .await?
        .ok_or_else(|| Error::ConflictError("审批实例缺少冻结业务快照".to_string()))?;
    snapshot
        .ensure_matches_runtime_subject(
            DocumentType::StockAdjustment,
            instance.subject.subject_id(),
            instance.subject_version,
        )
        .map_err(|_| Error::ConflictError("审批实例与冻结业务快照不一致".to_string()))?;
    let cancel_scope = approval_cancel_scope_with_executor(
        &crate::adapters::workflow::workflow_auth(db.clone(), rbac.clone()),
        actor,
        executor,
    )
    .await?;
    let read_scope = approval_document_read_scope_with_executor(
        &crate::adapters::workflow::workflow_auth(db.clone(), rbac.clone()),
        actor,
        DocumentType::StockAdjustment,
        executor,
    )
    .await?;
    let object = crate::adapters::workflow::workflow_auth(db.clone(), rbac.clone())
        .approval_scope_object(DocumentType::StockAdjustment, &snapshot.business_object_id, executor)
        .await?;
    if !cancel_scope.covers_object(&object) || !read_scope.covers_object(&object) {
        return Err(Error::Forbidden("无权撤回该责任组织的库存调整审批".to_string()));
    }
    if snapshot.payload.submitted_by == actor.id() {
        return Ok(CancelAuthorization {
            authority: CancelAuthority::Submitter,
            submitted_by: snapshot.payload.submitted_by,
            responsible_org_id: snapshot.payload.responsible_org_id,
        });
    }
    let visibility = definition_management_visibility_with_executor(
        &crate::adapters::workflow::workflow_auth(db.clone(), rbac.clone()),
        actor,
        executor,
    )
    .await?;
    if !visibility.runtime_admin_types().contains(&DocumentType::StockAdjustment) {
        return Err(Error::Forbidden("只有原提交人或库存调整审批运行管理员可以撤回".to_string()));
    }
    Ok(CancelAuthorization {
        authority: CancelAuthority::RuntimeAdmin,
        submitted_by: snapshot.payload.submitted_by,
        responsible_org_id: snapshot.payload.responsible_org_id,
    })
}
