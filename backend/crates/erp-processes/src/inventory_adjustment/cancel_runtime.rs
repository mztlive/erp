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
    ApprovalCancellationTaskPolicy, ApprovalCommandReceipt, ApprovalNodeExecution, ApprovalProcessInstance,
    IdempotencyKey, ParticipantId, Timestamp,
};
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
    approval_action_roles_with_executor, approval_actor_is_active_with_executor,
    definition_management_visibility_with_executor,
};
use erp_workflow::{ApprovalIntegrationExt, BpmExt, WorkItemExt};
use id_generator::next_id;
use mongodb::Database;
use persistence_core::{Executor, NoTransaction, Transactional};

use super::InventoryAdjustmentService;
use super::adapter::stock_adjustment_adapter;
use super::approval_prepare::load_bound_definition_graph;
use super::cancel_facts::{cancellation_fact, ensure_receipt};
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
    /// 返回撤回授权身份的展示名称。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回原提交人或审批管理员的中文名称。
    ///
    /// # 错误
    /// 不返回错误。
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Submitter => "原提交人",
            Self::RuntimeAdmin => "审批管理员",
        }
    }
}

/// 按请求中的稳定实例 ID 精确加载审批实例。
///
/// # 参数
/// * `db` - 审批实例所在数据库。
/// * `instance_id` - 命令中的审批实例 ID。
///
/// # 返回
/// 返回该 ID 对应的审批实例。
///
/// # 错误
/// 实例不存在时返回 `NotFound`；仓储读取失败时返回对应错误。
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
///
/// # 参数
/// * `db` - 执行、任务与定义所在数据库。
/// * `binding` - 单据创建时冻结的定义绑定。
/// * `instance` - 已加载的候选实例。
///
/// # 返回
/// 返回定义图、实例、当前执行、任务关闭策略和全部开放任务。
///
/// # 错误
/// 取消策略、当前执行、开放任务数量或绑定定义不满足时返回 `ConflictError`。仓储读取失败时返回对应错误。
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
/// # 参数
/// * `service` - 库存调整用例及当前授权服务。
/// * `id` - 原调整单路径身份。
/// * `req` - 原实例及全部期望版本。
/// * `reason` - 已规范化撤回原因。
/// * `idempotency_key` - 已规范化幂等键。
/// * `actor` - 当前认证操作人。
/// # 返回
/// 原命令已提交时返回调整单视图，否则返回空值。
/// # 错误
/// 当前授权、原操作人、命令身份或取消终态不匹配时拒绝回放。
pub(crate) async fn committed_cancel_replay(
    service: &InventoryAdjustmentService,
    id: &str,
    req: &CancelStockAdjustmentApprovalRequest,
    reason: &str,
    idempotency_key: &IdempotencyKey,
    actor: &AuditActor,
) -> Result<Option<StockAdjustmentView>> {
    let input = CancelReplayCommand {
        id: id.to_string(),
        request: req.clone(),
        reason: reason.to_string(),
        actor: actor.clone(),
        identity: document_cancel_identity(DocumentCancelIdentityParams {
            idempotency_key: idempotency_key.clone(),
            instance_id: &req.approval_process_instance_id,
            subject_version: req.expected_subject_version,
            expected_document_version: req.expected_version,
            expected_instance_version: req.expected_instance_version,
            expected_execution_version: req.expected_execution_version,
            expected_task_version: req.expected_task_version,
            reason,
            actor_id: actor.id(),
        })?,
    };
    let db = service.db.clone();
    let rbac = service.rbac.clone();
    let client = db.client().clone();
    client
        .with_transaction(move |executor| {
            Box::pin(async move { replay_cancel_step(&db, &rbac, input, executor).await })
        })
        .await
}

/// 普通撤回的精确命令身份；不从当前单据反推原版本。
struct CancelReplayCommand {
    id: String,
    request: CancelStockAdjustmentApprovalRequest,
    reason: String,
    actor: AuditActor,
    identity: PreparedCommandIdentity,
}

/// 新会话首先读取回执；重验当前授权后再证明原 actor 和终态。
async fn replay_cancel_step(
    db: &Database,
    rbac: &SharedRbacService,
    input: CancelReplayCommand,
    executor: &mut dyn Executor,
) -> Result<Option<StockAdjustmentView>> {
    let receipt = find_cancel_receipt(db, &input.identity, executor).await?;
    let instance = db
        .bpm_workflow()
        .find_instance_by_id(
            &ApprovalProcessInstanceId::new(&input.request.approval_process_instance_id),
            executor,
        )
        .await?
        .ok_or_else(|| Error::NotFound("审批实例不存在".into()))?;
    ensure_cancel_instance_subject(&instance, &input.id, input.request.expected_subject_version)?;
    ensure_cancel_authorized_with_executor(db, rbac, &instance, &input.actor, executor).await?;
    ensure_replay_terminal(db, &instance, receipt.as_ref(), &input, executor).await?;
    let Some(receipt) = receipt else {
        return Ok(None);
    };
    if instance.status != ApprovalProcessInstanceStatus::Cancelled
        || receipt.command_kind != ApprovalCommandKind::CancelApproval
        || receipt.result_ref != instance.base.id
    {
        return Err(Error::ConflictError("库存调整撤回收据与终态事实不一致".into()));
    }
    if !matches!(input.identity.classify(Some(&receipt)), ReceiptBranch::SamePayload(_)) {
        return Err(payload_conflict_error().into());
    }
    let adjustment = db
        .inventory()
        .stock_adjustment(&input.id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("库存调整单不存在".into()))?;
    Ok(Some(adjustment.into()))
}

/// 未命中原键也先验证 actor；异载荷判断仅在原回执命中后执行。
async fn ensure_replay_terminal(
    db: &Database,
    instance: &ApprovalProcessInstance,
    receipt: Option<&ApprovalCommandReceipt>,
    input: &CancelReplayCommand,
    executor: &mut dyn Executor,
) -> Result<()> {
    if instance.status != ApprovalProcessInstanceStatus::Cancelled {
        return Ok(());
    }
    let fact = cancellation_fact(db, instance, executor).await?;
    if fact.data.actor_id != input.actor.id() {
        return Err(cancel_replay_actor_mismatch(instance));
    }
    if let Some(receipt) = receipt {
        if !fact.matches_request(&input.id, &input.request, input.actor.id(), &input.reason) {
            return Err(payload_conflict_error().into());
        }
        ensure_receipt(&fact, receipt)?;
    }
    Ok(())
}

/// 按 V3、已知历史精确 scope 顺序读取普通撤回收据。
///
/// # 参数
/// * `db` - 命令收据所在数据库。
/// * `identity` - 已规范化的普通撤回命令身份。
/// * `executor` - 调用方数据库快照。
///
/// # 返回
/// 命中第一个作用域时返回该收据；全部未命中时返回 `None`。
///
/// # 错误
/// 仓储读取失败时返回对应错误。
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

/// 非原命令操作人不得因收据是否存在获得不同错误投影。
///
/// # 参数
/// * `instance` - 已取消的审批实例。
///
/// # 返回
/// 返回稳定的 `ConflictError`。取消策略本身无法解析时，错误正文取该策略错误。
///
/// # 错误
/// 不返回错误。调用方把返回值当作错误使用。
pub(crate) fn cancel_replay_actor_mismatch(instance: &ApprovalProcessInstance) -> Error {
    match instance.cancellation_task_policy() {
        Err(error) => Error::ConflictError(error.to_string()),
        Ok(_) => Error::ConflictError("库存调整撤回收据与终态事实不一致".to_string()),
    }
}

const CANCEL_REPLAY_RECOVERY_ATTEMPTS: usize = 32;

/// 事务失败或结果未知后，以有限次新会话等待并发 winner 的收据可见。
///
/// # 参数
/// * `service` - 库存调整流程服务。
/// * `id` - 库存调整单主键。
/// * `req` - 原撤回请求中的实例与期望版本。
/// * `reason` - 已规范化撤回原因。
/// * `idempotency_key` - 已规范化幂等键。
/// * `actor` - 当前认证操作人。
///
/// # 返回
/// 限定次数内读到已提交结果时返回调整单视图；仍未出现时返回 `None`。
///
/// # 错误
/// 某次回放校验或仓储读取失败时返回对应错误，不再继续等待。
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
///
/// # 参数
/// * `instance` - 已加载的审批实例。
/// * `adjustment_id` - 路径中的库存调整单主键。
/// * `expected_subject_version` - 命令冻结的主题版本。
///
/// # 返回
/// 流程种类、主体和主题版本一致时无返回值。
///
/// # 错误
/// 任一身份不一致时返回 `ConflictError`。
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
///
/// # 参数
/// * `instance` - 已加载的审批实例。
/// * `binding` - 单据创建时冻结的定义绑定。
///
/// # 返回
/// 流程种类、定义标识和定义版本一致时无返回值。
///
/// # 错误
/// 任一绑定不一致时返回 `ConflictError`。
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
///
/// # 参数
/// * `writes` - 取消编排写出的计划；成功时改写其中唯一通知意图。
///
/// # 返回
/// 唯一通知已改为普通取消事件时无返回值。
///
/// # 错误
/// 通知意图不是恰好一条时返回 `Internal`。
pub(crate) fn normalize_document_cancel_notification(writes: &mut PlannedWrites) -> Result<()> {
    let [intent] = writes.notifications.as_mut_slice() else {
        return Err(Error::Internal("库存调整普通撤回必须产生唯一取消通知意图".to_string()));
    };
    intent.event_kind = ApprovalNotificationEventKind::Cancelled;
    intent.dedup_key = format!("cancelled:{}:{}", writes.instance.base.id, writes.instance.current_round_no);
    Ok(())
}

/// 校验调用方持有的运行实例、执行与任务版本。
///
/// # 参数
/// * `runtime` - 已加载的撤回运行事实。
/// * `req` - 含实例、执行和任务期望版本的撤回请求。
/// * `authorization` - 已证明的撤回授权，用于核对开放任务责任组织。
///
/// # 返回
/// 身份与版本全部匹配时无返回值。
///
/// # 错误
/// 执行身份不一致、期望版本变化，或任务策略与请求中的任务版本不匹配时返回 `ConflictError`。
///
/// # Panics
/// `CloseOpenTask` 分支直接取 `open_tasks[0]`。调用方必须已证明恰有一个开放任务，否则越界。
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
///
/// # 参数
/// * `instance` - 审批实例。
/// * `current` - 当前节点执行。
/// * `policy` - 当前实例决定的任务关闭策略。
///
/// # 返回
/// 运行中有开放任务，或人员可恢复的阻塞且两边 blocker 相同、没有开放任务时无返回值。
///
/// # 错误
/// 身份、轮次、状态或 blocker 不匹配时返回 `ConflictError`。
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
///
/// # 参数
/// * `task` - 待关闭的开放审批任务。
/// * `runtime` - 已加载的撤回运行事实。
/// * `authorization` - 已证明的撤回授权，提供责任组织。
///
/// # 返回
/// 任务与当前运行事实构成同一责任链时无返回值。
///
/// # 错误
/// 适配器登记不完整，或任务与实例、执行、责任组织不一致时返回对应错误。
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
///
/// # 参数
/// * `task` - 开放审批任务。
/// * `instance` - 运行中的审批实例。
/// * `current` - 当前节点执行。
/// * `responsible_org_id` - 冻结快照中的责任组织。
///
/// # 返回
/// 任务类型、状态、执行、对象、责任人和组织全部一致时无返回值。
///
/// # 错误
/// 适配器登记不完整时返回对应错误；任一身份不一致时返回 `ConflictError`。
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

/// 校验调用方持有的乐观锁版本仍等于已加载事实。
///
/// # 参数
/// * `label` - 写入冲突文案的事实名称。
/// * `expected` - 调用方持有的版本。
/// * `actual` - 当前加载到的版本。
///
/// # 返回
/// 两边版本相等时无返回值。
///
/// # 错误
/// 版本不同时返回 `ConflictError`。
pub(crate) fn ensure_expected_version(label: &str, expected: u64, actual: u64) -> Result<()> {
    if expected == actual {
        return Ok(());
    }
    Err(Error::ConflictError(format!("{label}版本已变化，请刷新后重试")))
}

/// 去掉撤回原因首尾空白，并拒绝空原因。
///
/// # 参数
/// * `reason` - 调用方提交的撤回原因。
///
/// # 返回
/// 返回去掉首尾空白后的原因。
///
/// # 错误
/// 去掉空白后为空时返回 `ValidationError`。
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
///
/// # 参数
/// * `runtime` - 已加载的撤回运行事实。
/// * `req` - 含期望主题、实例、执行和任务版本的撤回请求。
/// * `actor_id` - 撤回人账号 ID。
/// * `reason` - 已规范化撤回原因。
/// * `idempotency_key` - 已规范化幂等键。
/// * `receipt` - 已存在的撤回收据；新命令为空。
/// * `now` - 调用方时间。
///
/// # 返回
/// 返回 `blocked_port` 固定为 `false` 的取消执行输入。
///
/// # 错误
/// 撤回人引用无效，或当前执行人的资格无法收敛时返回 `ValidationError`。
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
///
/// # 参数
/// * `service` - 库存调整流程服务。
/// * `instance` - 待撤回的审批实例。
/// * `actor` - 当前认证操作人。
///
/// # 返回
/// 返回原提交人或运行管理员的授权事实。
///
/// # 错误
/// 与 `ensure_cancel_authorized_with_executor` 相同；本函数使用非事务快照。
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
///
/// # 参数
/// * `service` - 库存调整流程服务。
/// * `instance` - 待投影撤回动作的审批实例。
/// * `actor` - 当前认证操作人。
///
/// # 返回
/// 授权通过时返回 `true`；明确拒绝时返回 `false`。
///
/// # 错误
/// 授权读取失败且不是 `Forbidden` 时返回对应错误。
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
///
/// # 参数
/// * `db` - 快照与授权所在数据库。
/// * `rbac` - 动作权限与对象读取使用的 RBAC。
/// * `instance` - 待撤回的审批实例。
/// * `actor` - 当前认证操作人。
/// * `executor` - 调用方数据库快照。
///
/// # 返回
/// 原提交人返回 `Submitter`；具备运行管理员可见性且对象可读时返回 `RuntimeAdmin`。
///
/// # 错误
/// 账号未激活、缺少撤回或读取权限、不是原提交人且不是运行管理员或对象不可读时返回 `Forbidden`。缺少或对不上冻结快照时返回 `ConflictError`。授权或快照读取失败时返回对应错误。
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
    let auth = crate::adapters::workflow::workflow_auth(db.clone(), rbac.clone());
    let roles =
        approval_action_roles_with_executor(&auth, actor, "approval_instance:cancel", executor).await?;
    let read_roles =
        approval_action_roles_with_executor(&auth, actor, "approval_instance:read", executor).await?;
    if roles.is_empty() || read_roles.is_empty() {
        return Err(Error::Forbidden("当前账号缺少撤回审批或读取权限".to_string()));
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
    if !visibility.runtime_admin_types().contains(&DocumentType::StockAdjustment)
        || !auth
            .approval_source_readable(
                actor,
                DocumentType::StockAdjustment,
                &snapshot.business_object_id,
                executor,
            )
            .await?
    {
        return Err(Error::Forbidden("只有原提交人或库存调整审批运行管理员可以撤回".to_string()));
    }
    Ok(CancelAuthorization {
        authority: CancelAuthority::RuntimeAdmin,
        submitted_by: snapshot.payload.submitted_by,
        responsible_org_id: snapshot.payload.responsible_org_id,
    })
}
