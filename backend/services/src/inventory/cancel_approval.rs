//! 库存调整普通撤回：调用统一取消编排并原子持久化业务与 BPM 事实。

use bpm::engine::DefinitionGraph;
use bpm::ids::{ApprovalCommandReceiptId, ApprovalNodeExecutionId, ApprovalProcessInstanceId};
use bpm::model::types::{ApprovalCommandKind, ApprovalProcessInstanceStatus};
use bpm::model::{
    ApprovalCancellationTaskPolicy, ApprovalNodeExecution, ApprovalProcessInstance, ParticipantId, Timestamp,
};
use database::{
    AccessControlExt, ApprovalIntegrationExt, BpmExt, Executor, InventoryExt, NoTransaction, Transactional,
    WorkItemExt,
};
use entities::approval_integration::{
    ApprovalNotificationEventKind, ApprovalNotificationOutbox, ApprovalNotificationTemplateParams,
};
use entities::common::time::Instant;
use entities::document_registry::business_document::ApprovalDefinitionBinding;
use entities::document_registry::DocumentType;
use entities::ids::ApprovalNotificationOutboxId;
use entities::inventory::StockAdjustment;
use entities::work_item::{AssignmentSource, WorkItem, WorkItemType};
use id_generator::next_id;
use mongodb::Database;
use validator::Validate;

use super::adapter::{
    execute_stock_adjustment_domain_action, require_frozen_binding, stock_adjustment_adapter,
};
use super::dto::{CancelStockAdjustmentApprovalRequest, StockAdjustmentView};
use super::start_approval::load_bound_definition_graph;
use super::{load_approval_binding, InventoryService};
use crate::approval::execution::authorization::{converge_eligibility, requires_blocked_cancel};
use crate::approval::execution::idempotency::{
    document_cancel_digest, normalize_idempotency_key, payload_conflict_error,
};
use crate::approval::execution::{
    prepare_document_cancel, CancelExecutionInput, ExecutionCommandInput, PlannedWrites, PreparedExecution,
};
use crate::approval::process_kind::process_kind_of;
use crate::approval::{
    approval_actor_is_active_with_executor, approval_cancel_scope_with_executor,
    approval_document_read_scope_with_executor, definition_management_visibility_with_executor,
};
use crate::audit::AuditActor;
use crate::errors::{Error, Result};
use crate::iam::SharedRbacService;

const STOCK_ADJUSTMENT_CANCEL_AUDIT_ACTION: &str = "stock_adjustment.cancel_approval";
const STOCK_ADJUSTMENT_AUDIT_RESOURCE: &str = "stock_adjustment";

impl InventoryService {
    /// 撤回审批中的库存调整单，回到草稿且保持审批主题版本不变。
    ///
    /// 本方法是合同 §4.4.4 签署的普通撤回端口。它以审批实例作为稳定幂等
    /// 作用域，先回读收据，再校验完整运行时 CAS 和业务撤回规则。新命令在单一
    /// MongoDB 事务内写回业务单据、实例、执行、收据、全部开放任务和审计。
    ///
    /// # 参数
    /// * `id` - 库存调整单主键
    /// * `req` - 单据/运行事实期望版本、原因和幂等键
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回调用人当前可读的最新库存调整单视图；同载荷回放不重复写入。
    ///
    /// # 错误
    /// 非原提交人或无范围运行管理员、动作/对象范围不足、版本变化、非审批中、
    /// 原因或幂等键非法，或任一事务写入失败时返回错误。
    #[tracing::instrument(
        name = "inventory.stock_adjustment_cancel_approval",
        skip_all,
        fields(
            layer = "service",
            domain = "inventory",
            operation = "stock_adjustment_cancel_approval"
        )
    )]
    pub async fn cancel_stock_adjustment_approval(
        &self,
        id: &str,
        req: CancelStockAdjustmentApprovalRequest,
        actor: &AuditActor,
    ) -> Result<StockAdjustmentView> {
        req.validate()?;
        let reason = normalize_cancel_reason(&req.reason)?;
        let idempotency_key = normalize_idempotency_key(&req.idempotency_key)?;
        if let Some(view) = committed_cancel_replay(self, id, &req, &reason, &idempotency_key, actor).await? {
            return Ok(view);
        }

        let instance = load_cancel_instance(&self.db, &req.approval_process_instance_id).await?;
        ensure_cancel_instance_subject(&instance, id, req.expected_subject_version)?;
        let authorization = ensure_cancel_authorized(self, &instance, actor).await?;
        let mut adjustment = self.load_stock_adjustment(id).await?;
        ensure_expected_version("库存调整单", req.expected_version, adjustment.base.version)?;
        if adjustment.approval_subject_version != req.expected_subject_version {
            return Err(Error::ConflictError(
                "库存调整审批主题版本已变化，请刷新后重试".to_string(),
            ));
        }
        let binding = load_approval_binding(&self.db, id, &mut NoTransaction).await?;
        let binding = require_frozen_binding(binding.as_ref())?.clone();
        ensure_cancel_instance_binding(&instance, &binding)?;
        let runtime = load_cancel_runtime(&self.db, &binding, instance).await?;
        ensure_cancel_runtime_versions(&runtime, &req, &authorization)?;

        let now = Instant::now();
        let input = build_stock_adjustment_cancel_input(
            &runtime,
            &req,
            actor.id(),
            &reason,
            &idempotency_key,
            None,
            now,
        )?;
        let PreparedExecution::Apply(mut writes) = prepare_document_cancel(input, req.expected_version)?
        else {
            return Err(Error::Internal("新库存调整撤回不得进入回放分支".to_string()));
        };
        normalize_document_cancel_notification(&mut writes)?;
        let adapter = stock_adjustment_adapter()?;
        execute_stock_adjustment_domain_action(&mut adjustment, adapter.cancel_action)?;
        let current_approver_id = runtime.current.assignee_participant_id.as_str().to_string();
        let current_approver_name = runtime.current.assignee_name_snapshot.clone();
        let authorization_instance = runtime.instance.clone();
        let authorization_execution = runtime.current.clone();
        let document_no = adjustment.adjustment_no.clone();
        let result = persist_stock_adjustment_cancel(
            &self.db,
            StockAdjustmentCancelPersistInput {
                rbac: self.rbac.clone(),
                adjustment,
                writes,
                open_tasks: runtime.open_tasks,
                authorization_instance,
                authorization_execution,
                binding,
                actor: actor.clone(),
                reason: reason.clone(),
                current_approver_id,
                current_approver_name,
                document_no,
                now,
            },
        )
        .await;
        match result {
            Ok(adjustment) => Ok(adjustment.into()),
            Err(error) => {
                if let Some(view) =
                    recover_cancel_replay(self, id, &req, &reason, &idempotency_key, actor).await?
                {
                    return Ok(view);
                }
                Err(error)
            }
        }
    }
}

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
    /// 当前执行上的全部开放任务。
    pub open_tasks: Vec<WorkItem>,
}

/// 普通撤回的授权身份，用于审计应急代办路径。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CancelAuthority {
    Submitter,
    RuntimeAdmin,
}

/// 事务内重新证明的普通撤回授权事实。
struct CancelAuthorization {
    authority: CancelAuthority,
    submitted_by: String,
    responsible_org_id: String,
}

impl CancelAuthority {
    fn as_str(self) -> &'static str {
        match self {
            Self::Submitter => "submitter",
            Self::RuntimeAdmin => "runtime_admin",
        }
    }
}

/// 按请求中的稳定实例 ID 精确加载审批实例。
async fn load_cancel_instance(db: &Database, instance_id: &str) -> Result<ApprovalProcessInstance> {
    db.bpm_workflow()
        .find_instance_by_id(&ApprovalProcessInstanceId::new(instance_id), &mut NoTransaction)
        .await?
        .ok_or_else(|| Error::NotFound("审批实例不存在".to_string()))
}

/// 从候选实例加载 RUNNING/BLOCKED 当前执行与全部开放任务。
///
/// `RUNNING` 必须恰有一个开放任务，`BLOCKED` 必须没有开放任务。
pub(super) async fn load_cancel_runtime(
    db: &Database,
    binding: &ApprovalDefinitionBinding,
    instance: ApprovalProcessInstance,
) -> Result<LoadedCancelRuntime> {
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

/// 在任何原状态或版本检查前解析已提交收据。
///
/// 实例 ID 直接来自强类型命令，因此即使单据已修改并以更高主题版本重新提交，
/// 仍能精确定位原命令作用域。收据读取和事务失败恢复都使用新会话。
async fn committed_cancel_replay(
    service: &InventoryService,
    id: &str,
    req: &CancelStockAdjustmentApprovalRequest,
    reason: &str,
    idempotency_key: &str,
    actor: &AuditActor,
) -> Result<Option<StockAdjustmentView>> {
    let db = service.db.clone();
    let rbac = service.rbac.clone();
    let id = id.to_string();
    let req = req.clone();
    let reason = reason.to_string();
    let idempotency_key = idempotency_key.to_string();
    let actor = actor.clone();
    let client = db.client().clone();
    client
        .with_transaction(move |session| {
            Box::pin(async move {
                // 请求已携带稳定 instance scope，因此收据必须是快照内第一读。
                let receipt = db
                    .bpm_workflow()
                    .find_command_receipt(
                        ApprovalCommandKind::CancelApproval,
                        &req.approval_process_instance_id,
                        &idempotency_key,
                        session,
                    )
                    .await?;
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
                    || receipt.scope_id != instance.base.id
                    || receipt.result_ref != instance.base.id
                {
                    return Err(Error::ConflictError(
                        "库存调整撤回收据与终态事实不一致".to_string(),
                    ));
                }
                let digest = document_cancel_digest(
                    req.expected_subject_version,
                    req.expected_version,
                    req.expected_instance_version,
                    req.expected_execution_version,
                    req.expected_task_version,
                    &reason,
                    actor.id(),
                );
                receipt.reconcile(&digest).map_err(|_| payload_conflict_error())?;
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

/// 从与取消收据同事务提交的不可变审计事实解析原命令操作人。
async fn committed_cancel_actor(
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
        return Err(Error::ConflictError(
            "库存调整撤回收据缺少唯一原命令操作人审计".to_string(),
        ));
    };
    Ok(actor.clone())
}

/// 构造不可歧义的取消审计实例前缀；原因仅追加在固定前缀之后。
fn cancel_audit_message_prefix(instance_id: &str) -> String {
    format!("instance={}:{} ", instance_id.len(), instance_id)
}

fn cancel_audit_matches_instance(message: &str, instance_id: &str) -> bool {
    message.starts_with(&cancel_audit_message_prefix(instance_id))
}

/// 非原命令操作人不得因收据是否存在获得不同错误投影。
fn cancel_replay_actor_mismatch(instance: &ApprovalProcessInstance) -> Error {
    match instance.cancellation_task_policy() {
        Err(error) => Error::ConflictError(error.to_string()),
        Ok(_) => Error::ConflictError("库存调整撤回收据与终态事实不一致".to_string()),
    }
}

const CANCEL_REPLAY_RECOVERY_ATTEMPTS: usize = 32;

/// 事务失败或结果未知后，以有限次新会话等待并发 winner 的收据可见。
async fn recover_cancel_replay(
    service: &InventoryService,
    id: &str,
    req: &CancelStockAdjustmentApprovalRequest,
    reason: &str,
    idempotency_key: &str,
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
pub(super) fn ensure_cancel_instance_subject(
    instance: &ApprovalProcessInstance,
    adjustment_id: &str,
    expected_subject_version: u32,
) -> Result<()> {
    if instance.process_kind != process_kind_of(DocumentType::StockAdjustment)
        || instance.subject.subject_kind() != DocumentType::StockAdjustment.as_str()
        || instance.subject.subject_id() != adjustment_id
        || instance.subject_version != expected_subject_version
    {
        return Err(Error::ConflictError(
            "审批实例与库存调整撤回命令不一致".to_string(),
        ));
    }
    Ok(())
}

/// 校验实例流程种类与单据创建时冻结的定义绑定。
pub(super) fn ensure_cancel_instance_binding(
    instance: &ApprovalProcessInstance,
    binding: &ApprovalDefinitionBinding,
) -> Result<()> {
    if instance.process_kind != process_kind_of(DocumentType::StockAdjustment)
        || instance.process_definition_id != binding.approval_process_definition_id
        || instance.definition_version != binding.approval_definition_version
    {
        return Err(Error::ConflictError(
            "库存调整审批实例与冻结定义绑定不一致".to_string(),
        ));
    }
    Ok(())
}

/// 把普通业务撤回的唯一引擎通知意图规范为普通取消事件。
///
/// 人员失效实例仍走普通业务撤回，不得沿用受阻管理员取消的通知种类。
fn normalize_document_cancel_notification(writes: &mut PlannedWrites) -> Result<()> {
    let [intent] = writes.notifications.as_mut_slice() else {
        return Err(Error::Internal(
            "库存调整普通撤回必须产生唯一取消通知意图".to_string(),
        ));
    };
    intent.event_kind = ApprovalNotificationEventKind::Cancelled;
    intent.dedup_key = format!(
        "cancelled:{}:{}",
        writes.instance.base.id, writes.instance.current_round_no
    );
    Ok(())
}

/// 校验调用方持有的运行实例、执行与任务版本。
fn ensure_cancel_runtime_versions(
    runtime: &LoadedCancelRuntime,
    req: &CancelStockAdjustmentApprovalRequest,
    authorization: &CancelAuthorization,
) -> Result<()> {
    ensure_cancel_execution_identity(&runtime.instance, &runtime.current, runtime.task_policy)?;
    ensure_expected_version(
        "审批实例",
        req.expected_instance_version,
        runtime.instance.base.version,
    )?;
    ensure_expected_version(
        "审批执行",
        req.expected_execution_version,
        runtime.current.base.version,
    )?;
    match runtime.task_policy {
        ApprovalCancellationTaskPolicy::CloseOpenTask => {
            let expected = req
                .expected_task_version
                .ok_or_else(|| Error::ConflictError("运行中审批撤回必须提供开放任务版本".to_string()))?;
            let task = &runtime.open_tasks[0];
            ensure_open_task_matches_runtime(task, runtime, authorization)?;
            ensure_expected_version("审批任务", expected, task.base.version)
        }
        ApprovalCancellationTaskPolicy::NoOpenTask => {
            if req.expected_task_version.is_some() {
                return Err(Error::ConflictError(
                    "人员失效阻塞审批撤回的任务版本必须为空".to_string(),
                ));
            }
            Ok(())
        }
    }
}

/// 校验普通撤回实例与当前执行的身份、轮次、状态和 blocker 上下文。
fn ensure_cancel_execution_identity(
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
        }
        ApprovalCancellationTaskPolicy::NoOpenTask => instance
            .blocker_code
            .zip(current.blocker_code)
            .is_some_and(|(instance_code, execution_code)| {
                instance.status == ApprovalProcessInstanceStatus::Blocked
                    && current.status == bpm::model::types::ApprovalNodeExecutionStatus::Blocked
                    && instance_code == execution_code
                    && !requires_blocked_cancel(instance_code)
            }),
    };
    if identity_matches && state_matches {
        return Ok(());
    }
    Err(Error::ConflictError(
        "库存调整审批实例与当前执行不一致".to_string(),
    ))
}

/// 校验开放任务与当前实例、执行、对象和责任人完全一致。
fn ensure_open_task_matches_runtime(
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
pub(super) fn ensure_stock_adjustment_open_task_identity(
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
        || task.status != entities::work_item::WorkItemStatus::Open
        || task.assignment_source != AssignmentSource::ApprovalRuntime
        || task.approval_node_execution_id.as_ref().map(AsRef::as_ref) != Some(current.base.id.as_str())
        || task.business_object_type != DocumentType::StockAdjustment.as_str()
        || task.business_object_id != instance.subject.subject_id()
        || task.subject_version != instance.subject_version.to_string()
        || task.owner_user_id.as_deref() != Some(current.assignee_participant_id.as_str())
        || task.owner_role != adapter.owner_role
        || task.owner_organization_id != responsible_org_id
    {
        return Err(Error::ConflictError(
            "库存调整开放审批任务与当前运行事实不一致".to_string(),
        ));
    }
    Ok(())
}

fn ensure_expected_version(label: &str, expected: u64, actual: u64) -> Result<()> {
    if expected == actual {
        return Ok(());
    }
    Err(Error::ConflictError(format!("{label}版本已变化，请刷新后重试")))
}

fn normalize_cancel_reason(reason: &str) -> Result<String> {
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
pub(super) fn build_stock_adjustment_cancel_input(
    runtime: &LoadedCancelRuntime,
    req: &CancelStockAdjustmentApprovalRequest,
    actor_id: &str,
    reason: &str,
    idempotency_key: &str,
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
            idempotency_key: idempotency_key.to_string(),
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
async fn ensure_cancel_authorized(
    service: &InventoryService,
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
pub(super) async fn actor_can_cancel(
    service: &InventoryService,
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
async fn ensure_cancel_authorized_with_executor(
    db: &Database,
    rbac: &SharedRbacService,
    instance: &ApprovalProcessInstance,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<CancelAuthorization> {
    if !approval_actor_is_active_with_executor(db, actor, executor).await? {
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
    let cancel_scope = approval_cancel_scope_with_executor(db, rbac, actor, executor).await?;
    let read_scope =
        approval_document_read_scope_with_executor(db, rbac, actor, DocumentType::StockAdjustment, executor)
            .await?;
    if !cancel_scope.covers(&snapshot.payload.responsible_org_id)
        || !read_scope.covers(&snapshot.payload.responsible_org_id)
    {
        return Err(Error::Forbidden("无权撤回该责任组织的库存调整审批".to_string()));
    }
    if snapshot.payload.submitted_by == actor.id() {
        return Ok(CancelAuthorization {
            authority: CancelAuthority::Submitter,
            submitted_by: snapshot.payload.submitted_by,
            responsible_org_id: snapshot.payload.responsible_org_id,
        });
    }
    let visibility = definition_management_visibility_with_executor(db, rbac, actor, executor).await?;
    if !visibility
        .runtime_admin_types()
        .contains(&DocumentType::StockAdjustment)
    {
        return Err(Error::Forbidden(
            "只有原提交人或库存调整审批运行管理员可以撤回".to_string(),
        ));
    }
    Ok(CancelAuthorization {
        authority: CancelAuthority::RuntimeAdmin,
        submitted_by: snapshot.payload.submitted_by,
        responsible_org_id: snapshot.payload.responsible_org_id,
    })
}

/// 库存调整撤回事务写入集合。
struct StockAdjustmentCancelPersistInput {
    rbac: SharedRbacService,
    adjustment: StockAdjustment,
    writes: Box<PlannedWrites>,
    open_tasks: Vec<WorkItem>,
    authorization_instance: ApprovalProcessInstance,
    authorization_execution: ApprovalNodeExecution,
    binding: ApprovalDefinitionBinding,
    actor: AuditActor,
    reason: String,
    current_approver_id: String,
    current_approver_name: String,
    document_no: String,
    now: Instant,
}

/// 在同一事务内应用取消计划、关闭全部开放任务并写回库存调整单。
async fn persist_stock_adjustment_cancel(
    db: &Database,
    input: StockAdjustmentCancelPersistInput,
) -> Result<StockAdjustment> {
    let StockAdjustmentCancelPersistInput {
        rbac,
        mut adjustment,
        writes,
        open_tasks,
        authorization_instance,
        authorization_execution,
        binding,
        actor,
        reason,
        current_approver_id,
        current_approver_name,
        document_no,
        now,
    } = input;
    let db = db.clone();
    let client = db.client().clone();
    client
        .with_transaction(move |session| {
            Box::pin(async move {
                let persisted_adjustment = db
                    .inventory()
                    .stock_adjustment(&adjustment.base.id, session)
                    .await?
                    .ok_or_else(|| Error::NotFound("库存调整单不存在".to_string()))?;
                if persisted_adjustment.base.version != adjustment.base.version
                    || persisted_adjustment.approval_subject_version != authorization_instance.subject_version
                    || persisted_adjustment.status != entities::inventory::StockAdjustmentState::InApproval
                {
                    return Err(Error::ConflictError(
                        "库存调整单事务内版本或审批状态已变化".to_string(),
                    ));
                }
                let persisted_instance = db
                    .bpm_workflow()
                    .find_instance_by_id(
                        &ApprovalProcessInstanceId::new(authorization_instance.base.id.clone()),
                        session,
                    )
                    .await?
                    .ok_or_else(|| Error::ConflictError("库存调整审批实例不存在".to_string()))?;
                ensure_cancel_instance_subject(
                    &persisted_instance,
                    &adjustment.base.id,
                    authorization_instance.subject_version,
                )?;
                let persisted_binding = load_approval_binding(&db, &adjustment.base.id, session).await?;
                let persisted_binding = require_frozen_binding(persisted_binding.as_ref())?;
                ensure_cancel_instance_binding(&persisted_instance, persisted_binding)?;
                if persisted_binding != &binding || persisted_instance != authorization_instance {
                    return Err(Error::ConflictError(
                        "库存调整撤回事务内运行事实已变化".to_string(),
                    ));
                }
                let authorization =
                    ensure_cancel_authorized_with_executor(&db, &rbac, &persisted_instance, &actor, session)
                        .await?;
                let current_open_tasks = revalidate_cancel_open_tasks(
                    &db,
                    &persisted_instance,
                    &authorization_execution,
                    &open_tasks,
                    &authorization,
                    session,
                )
                .await?;
                let closed_tasks = WorkItem::close_all_for_approval_cancellation(
                    current_open_tasks,
                    actor.id(),
                    &reason,
                    now,
                )?;
                // 唯一收据必须是事务内第一笔写入：并发同键只有一个事务获得
                // 命令所有权；失败事务退出后由外层使用新会话回读并分类回放。
                db.bpm_workflow()
                    .insert_command_receipt(&writes.receipt, session)
                    .await?;
                db.stock_adjustments().update(&mut adjustment, session).await?;
                db.bpm_workflow()
                    .persist_cancelled_runtime_after_receipt(
                        &writes.instance,
                        &writes.updated_executions,
                        session,
                    )
                    .await?;
                db.work_items()
                    .persist_cancelled_approval_tasks(&closed_tasks, session)
                    .await?;
                persist_stock_adjustment_cancel_notifications(
                    &db,
                    &writes,
                    &authorization,
                    actor.id(),
                    &current_approver_id,
                    &current_approver_name,
                    &document_no,
                    now,
                    session,
                )
                .await?;
                let audit = actor.clone().resource_log_with_message(
                    STOCK_ADJUSTMENT_CANCEL_AUDIT_ACTION,
                    STOCK_ADJUSTMENT_AUDIT_RESOURCE,
                    adjustment.base.id.clone(),
                    Some(format!(
                        "{}authority={} reason={reason}",
                        cancel_audit_message_prefix(&authorization_instance.base.id),
                        authorization.authority.as_str()
                    )),
                )?;
                db.audit_logs().create(&audit, session).await?;
                Ok::<StockAdjustment, crate::errors::Error>(adjustment)
            })
        })
        .await
}

/// 在同一事务快照内重读并校验普通撤回需要关闭的开放任务。
async fn revalidate_cancel_open_tasks(
    db: &Database,
    instance: &ApprovalProcessInstance,
    expected_execution: &ApprovalNodeExecution,
    expected_tasks: &[WorkItem],
    authorization: &CancelAuthorization,
    executor: &mut dyn Executor,
) -> Result<Vec<WorkItem>> {
    let policy = instance
        .cancellation_task_policy()
        .map_err(|error| Error::ConflictError(error.to_string()))?;
    let execution_id = instance
        .current_node_execution_id
        .as_ref()
        .ok_or_else(|| Error::ConflictError("审批实例缺少当前执行".to_string()))?;
    let current_execution = db
        .bpm_workflow()
        .find_execution_by_id(execution_id, executor)
        .await?
        .ok_or_else(|| Error::ConflictError("库存调整审批执行不存在".to_string()))?;
    if &current_execution != expected_execution {
        return Err(Error::ConflictError("库存调整审批执行已变化".to_string()));
    }
    ensure_cancel_execution_identity(instance, &current_execution, policy)?;
    let tasks = db
        .work_items()
        .open_approval_tasks_for_execution(execution_id, executor)
        .await?;
    policy
        .ensure_open_task_count(tasks.len())
        .map_err(|error| Error::ConflictError(error.to_string()))?;
    if tasks.len() != expected_tasks.len()
        || tasks.iter().zip(expected_tasks).any(|(current, expected)| {
            current.base.id != expected.base.id || current.base.version != expected.base.version
        })
    {
        return Err(Error::ConflictError("库存调整开放审批任务已变化".to_string()));
    }
    for task in &tasks {
        ensure_stock_adjustment_open_task_identity(
            task,
            instance,
            &current_execution,
            &authorization.responsible_org_id,
        )?;
    }
    Ok(tasks)
}

/// 在普通撤回事务内追加唯一通知 outbox。
#[allow(clippy::too_many_arguments)]
async fn persist_stock_adjustment_cancel_notifications(
    db: &Database,
    writes: &PlannedWrites,
    authorization: &CancelAuthorization,
    actor_id: &str,
    current_approver_id: &str,
    current_approver_name: &str,
    document_no: &str,
    now: Instant,
    session: &mut mongodb::ClientSession,
) -> Result<()> {
    let [intent] = writes.notifications.as_slice() else {
        return Err(Error::Internal(
            "库存调整普通撤回必须产生唯一取消通知意图".to_string(),
        ));
    };
    let mut recipients = vec![
        current_approver_id.to_string(),
        authorization.submitted_by.clone(),
    ];
    if authorization.authority == CancelAuthority::RuntimeAdmin {
        recipients.push(actor_id.to_string());
    }
    recipients.sort();
    recipients.dedup();
    let expected_dedup_key = format!(
        "cancelled:{}:{}",
        writes.instance.base.id, writes.instance.current_round_no
    );
    if intent.event_kind != ApprovalNotificationEventKind::Cancelled || intent.dedup_key != expected_dedup_key
    {
        return Err(Error::Internal(
            "库存调整普通撤回通知意图与持久化合同不一致".to_string(),
        ));
    }
    let record = ApprovalNotificationOutbox::enqueue(
        ApprovalNotificationOutboxId::new(intent.dedup_key.clone()),
        intent.dedup_key.clone(),
        intent.event_kind,
        recipients,
        ApprovalNotificationTemplateParams {
            document_type_label: DocumentType::StockAdjustment.label().to_string(),
            document_no: document_no.to_string(),
            current_node_name: writes
                .updated_executions
                .first()
                .map(|execution| execution.node_name.clone())
                .unwrap_or_default(),
            current_approver_display_name: current_approver_name.to_string(),
            round_no: writes.instance.current_round_no,
            reject_reason_summary: None,
        },
        now,
    )
    .map_err(|error| Error::ValidationError(error.to_string()))?;
    db.approval_notification_outbox().create(&record, session).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use bpm::ids::{ApprovalProcessDefinitionId, ApprovalProcessInstanceId};
    use bpm::model::{NewProcessInstance, ParticipantId, ProcessKind, SubjectRef, Timestamp};

    use super::{cancel_audit_matches_instance, cancel_audit_message_prefix, cancel_replay_actor_mismatch};
    use crate::errors::Error;

    fn cancelled_instance() -> bpm::model::ApprovalProcessInstance {
        let mut instance = bpm::model::ApprovalProcessInstance::start_running(NewProcessInstance {
            id: ApprovalProcessInstanceId::new("instance-cancelled"),
            process_definition_id: ApprovalProcessDefinitionId::new("definition-1"),
            definition_version: 1,
            process_kind: ProcessKind::StockAdjustment,
            subject: SubjectRef::new("stock_adjustment", "adjustment-1").unwrap(),
            subject_version: 1,
            started_by: ParticipantId::new("submitter-1").unwrap(),
            at: Timestamp::from_unix_secs(1).unwrap(),
        })
        .unwrap();
        instance.cancel(Timestamp::from_unix_secs(2).unwrap()).unwrap();
        instance
    }

    #[test]
    fn cancel_audit_instance_prefix_has_unambiguous_boundaries() {
        let instance_id = "instance:1";
        let message = format!(
            "{}authority=runtime_admin reason=instance=8:spoofed ",
            cancel_audit_message_prefix(instance_id)
        );
        assert!(cancel_audit_matches_instance(&message, instance_id));
        assert!(!cancel_audit_matches_instance(&message, "instance"));
        assert!(!cancel_audit_matches_instance(&message, "instance:10"));
    }

    #[test]
    fn non_original_replay_uses_the_same_terminal_conflict_as_a_missing_key() {
        let instance = cancelled_instance();
        let missing_key_message = instance.cancellation_task_policy().unwrap_err().to_string();
        assert!(matches!(
            cancel_replay_actor_mismatch(&instance),
            Error::ConflictError(message) if message == missing_key_message
        ));
    }
}
