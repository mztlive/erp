//! 客户退款提交启动：加载定义图、构造 `prepare_start` 输入并持久化运行事实。

use bpm::engine::{DefinitionGraph, StartAssigneeBinding, TaskIntent};
use bpm::ids::{
    ApprovalCommandReceiptId, ApprovalInstanceAssigneeId, ApprovalNodeExecutionId, ApprovalProcessInstanceId,
};
use bpm::model::{ApprovalNodeExecution, ParticipantId, SubjectRef, Timestamp};
use database::repository::bpm::ApprovalInstanceListProjection;
use database::{
    AccessControlExt, ApprovalIntegrationExt, BpmExt, DocumentRegistryExt, Executor, NoTransaction,
    ReturnsExt, Transactional, WorkItemExt,
};
use entities::approval_integration::{ApprovalSubjectSnapshot, ApprovalSubjectSnapshotPayload};
use entities::common::time::Instant;
use entities::document_registry::business_document::ApprovalDefinitionBinding;
use entities::document_registry::DocumentType;
use entities::ids::{ApprovalSubjectSnapshotId, WorkItemId};
use entities::returns::{CustomerRefund, PaymentReversal, ReceiptReversal, SupplierRefund};
use entities::work_item::DocumentApprovalWorkItemData;
use entities::work_item::{WorkItem, WorkItemPriority};
use id_generator::next_id;
use mongodb::Database;

use super::adapter::{
    customer_refund_object_readable, payment_reversal_object_readable, receipt_reversal_object_readable,
    supplier_refund_object_readable,
};
use crate::approval::execution::authorization::{converge_eligibility, AuthorizationFailure};
use crate::approval::execution::idempotency::{
    normalize_idempotency_key, payload_conflict_error, start_identity, start_scope_candidates, ReceiptBranch,
};
use crate::approval::execution::{
    map_receipt_first_write_error, ExecutionCommandInput, PreparedExecution, StartExecutionInput,
};
use crate::approval::process_kind::process_kind_of;
use crate::approval::{
    approval_actor_is_active_with_executor, approval_document_action_scope_with_executor,
    approval_document_read_scope_with_executor,
};
use crate::audit::AuditActor;
use crate::errors::{Error, Result};
use crate::iam::SharedRbacService;

/// 在读取具体退款/冲正资源前先重验认证主体仍有效。
pub(super) async fn ensure_return_start_actor_active(
    db: &Database,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    if !approval_actor_is_active_with_executor(db, actor, executor).await? {
        return Err(Error::Forbidden("当前账号不可提交该退款或冲正单".to_string()));
    }
    Ok(())
}

/// 重放前在同一 fresh session 内重验账号、提交动作和对象读取 DataScope。
pub(super) async fn ensure_return_start_replay_authorized(
    db: &Database,
    rbac: &SharedRbacService,
    actor: &AuditActor,
    document_type: DocumentType,
    submit_permission: &str,
    organization_id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    ensure_return_start_actor_active(db, actor, executor).await?;
    let action_scope =
        approval_document_action_scope_with_executor(db, rbac, actor, submit_permission, executor).await?;
    let read_scope =
        approval_document_read_scope_with_executor(db, rbac, actor, document_type, executor).await?;
    if !action_scope.covers(organization_id) || !read_scope.covers(organization_id) {
        return Err(Error::Forbidden("无权提交该责任组织的退款或冲正单".to_string()));
    }
    Ok(())
}

/// 顺序重试先查当前已冻结版本；草稿首次/重提再查严格下一版本。
pub(super) fn replay_subject_versions(current: u32) -> Result<Vec<u32>> {
    let mut versions = Vec::with_capacity(2);
    if current > 0 {
        versions.push(current);
    }
    let next = current
        .checked_add(1)
        .ok_or_else(|| Error::ConflictError("审批主题版本已达上限".to_string()))?;
    if !versions.contains(&next) {
        versions.push(next);
    }
    Ok(versions)
}

/// 加载绑定定义图。缺失时失败关闭，不得用空图启动。
///
/// # 参数
/// * `db` - 数据库
/// * `binding` - 创建时冻结的定义绑定
///
/// # 返回
/// 返回已持久化的定义图。
///
/// # 错误
/// 定义不存在或仓储失败时返回冲突或仓储错误。
pub(super) async fn load_bound_definition_graph(
    db: &Database,
    binding: &ApprovalDefinitionBinding,
) -> Result<DefinitionGraph> {
    load_bound_definition_graph_with_executor(db, binding, &mut NoTransaction).await
}

/// 使用调用方执行器加载冻结绑定的定义图。
pub(super) async fn load_bound_definition_graph_with_executor(
    db: &Database,
    binding: &ApprovalDefinitionBinding,
    executor: &mut dyn Executor,
) -> Result<DefinitionGraph> {
    let graph = db
        .bpm_workflow()
        .load_definition_graph(&binding.approval_process_definition_id, executor)
        .await?
        .ok_or_else(|| Error::ConflictError("客户退款单绑定的审批定义不存在".to_string()))?;
    Ok(engine_graph(graph))
}

/// 将仓储定义图转为引擎定义图。字段一一对应，不得在此补默认节点。
///
/// # 参数
/// * `graph` - 仓储一次批量读取结果
///
/// # 返回
/// 返回引擎可消费的定义图。
fn engine_graph(graph: database::repository::bpm::DefinitionGraph) -> DefinitionGraph {
    DefinitionGraph {
        definition: graph.definition,
        nodes: graph.nodes,
        transitions: graph.transitions,
    }
}

/// 按精确单据类型依次读取当前 V3 与已知历史 StartApproval 作用域。
async fn load_start_receipt_for_document_type(
    db: &Database,
    document_type: DocumentType,
    subject: &SubjectRef,
    subject_version: u32,
    idempotency_key: &str,
) -> Result<Option<bpm::model::ApprovalCommandReceipt>> {
    let key = normalize_idempotency_key(idempotency_key)?;
    let process_kind = process_kind_of(document_type);
    let scopes = start_scope_candidates(
        process_kind.as_str(),
        subject.subject_kind(),
        subject.subject_id(),
        subject_version,
    )?;
    for scope in scopes {
        let receipt = db
            .bpm_workflow()
            .find_command_receipt(
                bpm::model::types::ApprovalCommandKind::StartApproval,
                &scope,
                &key,
                &mut NoTransaction,
            )
            .await?;
        if receipt.is_some() {
            return Ok(receipt);
        }
    }
    Ok(None)
}

/// 在 fresh 事务快照内按完整 V3/legacy 身份回读已提交的退款/冲正启动结果。
#[allow(clippy::too_many_arguments)]
pub(super) async fn replay_return_start_with_executor(
    db: &Database,
    document_type: DocumentType,
    subject: &SubjectRef,
    subject_version: u32,
    idempotency_key: &str,
    binding: &ApprovalDefinitionBinding,
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<Option<String>> {
    let key = normalize_idempotency_key(idempotency_key)?;
    let process_kind = process_kind_of(document_type);
    let identity = start_identity(
        key,
        process_kind.as_str(),
        subject.subject_kind(),
        subject.subject_id(),
        subject_version,
        binding.approval_process_definition_id.as_ref(),
        binding.approval_definition_version,
        actor_id,
    )?;
    let mut receipt = None;
    for scope in identity.scope_candidates() {
        receipt = db
            .bpm_workflow()
            .find_command_receipt(
                bpm::model::types::ApprovalCommandKind::StartApproval,
                scope,
                identity.idempotency_key(),
                executor,
            )
            .await?;
        if receipt.is_some() {
            break;
        }
    }
    let receipt = match identity.classify(receipt.as_ref()) {
        ReceiptBranch::Fresh => return Ok(None),
        ReceiptBranch::PayloadConflict => return Err(payload_conflict_error()),
        ReceiptBranch::SamePayload(receipt) => receipt,
    };
    let instance = db
        .bpm_workflow()
        .find_instance_by_id(&ApprovalProcessInstanceId::new(&receipt.result_ref), executor)
        .await?
        .ok_or_else(|| Error::ConflictError("退款/冲正启动收据引用的审批实例不存在".to_string()))?;
    if instance.base.id != receipt.result_ref
        || instance.process_kind != process_kind
        || instance.subject.subject_kind() != subject.subject_kind()
        || instance.subject.subject_id() != subject.subject_id()
        || instance.subject_version != subject_version
        || instance.started_by.as_str() != actor_id
        || instance.process_definition_id != binding.approval_process_definition_id
        || instance.definition_version != binding.approval_definition_version
    {
        return Err(Error::ConflictError(
            "退款/冲正启动收据与冻结运行事实不一致".to_string(),
        ));
    }
    Ok(Some(instance.base.id))
}

/// 读取同载荷启动收据；不存在时返回 `None`。
///
/// # 参数
/// * `db` - 数据库
/// * `subject` - 业务对象引用
/// * `subject_version` - 冻结提交版本
/// * `idempotency_key` - 调用方幂等键
///
/// # 返回
/// 已提交收据或空。
///
/// # 错误
/// 幂等键非法或仓储失败时返回错误。
pub(super) async fn load_start_receipt(
    db: &Database,
    subject: &SubjectRef,
    subject_version: u32,
    idempotency_key: &str,
) -> Result<Option<bpm::model::ApprovalCommandReceipt>> {
    load_start_receipt_for_document_type(
        db,
        DocumentType::CustomerRefund,
        subject,
        subject_version,
        idempotency_key,
    )
    .await
}

/// 客户退款启动输入。
///
/// # 用途
/// 收拢 `build_customer_refund_start_input` 的定义图、绑定与提交人参数。
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
/// 审批人取自已发布节点，不接受客户端选择。
pub(super) struct CustomerRefundStartInput<'a> {
    /// 绑定定义图。
    pub graph: DefinitionGraph,
    /// 冻结绑定。
    pub binding: &'a ApprovalDefinitionBinding,
    /// 业务对象引用。
    pub subject: SubjectRef,
    /// 冻结提交版本。
    pub subject_version: u32,
    /// 提交人。
    pub actor_id: &'a str,
    /// 单据责任组织。
    pub organization_id: &'a str,
    /// 规范化前的幂等键。
    pub idempotency_key: &'a str,
    /// 已存在收据。
    pub receipt: Option<bpm::model::ApprovalCommandReceipt>,
    /// 调用方时间。
    pub now: Instant,
}

/// 由定义图与单据组织构造启动输入。
///
/// 审批人取自已发布节点，不接受客户端选择。对象读取权失败时收敛为 BLOCKED。
///
/// # 用途
/// 把启动参数收敛为引擎 `prepare_start` 输入。
///
/// # 参数
/// * `input` - 定义图、绑定、主体与提交人
///
/// # 返回
/// 返回可交给 `prepare_start` 的输入。
///
/// # 错误
/// 入口缺失、审批人非法、幂等键非法或读取权校验失败时返回错误。
///
/// # 关键业务约束
/// 定义版本必须与冻结绑定一致；对象读取权失败时收敛为 BLOCKED。
pub(super) fn build_customer_refund_start_input(
    input: CustomerRefundStartInput<'_>,
) -> Result<StartExecutionInput> {
    let CustomerRefundStartInput {
        graph,
        binding,
        subject,
        subject_version,
        actor_id,
        organization_id,
        idempotency_key,
        receipt,
        now,
    } = input;
    if graph.definition.definition_version != binding.approval_definition_version {
        return Err(Error::ConflictError(
            "客户退款单绑定定义版本与已加载定义不一致".to_string(),
        ));
    }
    let idempotency_key = normalize_idempotency_key(idempotency_key)?;
    let actor =
        ParticipantId::new(actor_id).map_err(|_| Error::ValidationError("提交人引用无效".to_string()))?;
    let timestamp = Timestamp::from_utc(now.as_utc());
    let bindings = start_bindings_from_graph(&graph, organization_id)?;
    let entry = graph
        .entry_node()
        .map_err(|_| Error::ConflictError("审批定义缺少入口节点".to_string()))?;
    let entry_eligibility = bindings
        .iter()
        .find(|item| item.node_key == entry.node_key)
        .map(|item| item.eligibility.clone())
        .ok_or_else(|| Error::ConflictError("入口节点缺少审批人绑定".to_string()))?;
    Ok(StartExecutionInput {
        command: ExecutionCommandInput {
            graph,
            current_eligibility: entry_eligibility.clone(),
            next_eligibility: entry_eligibility,
            receipt,
            idempotency_key,
            now: timestamp,
        },
        process_kind: process_kind_of(DocumentType::CustomerRefund),
        subject,
        subject_version,
        binding_id: binding.approval_process_definition_id.as_ref().to_string(),
        definition_version: binding.approval_definition_version,
        actor,
        instance_id: ApprovalProcessInstanceId::new(next_id()),
        entry_execution_id: ApprovalNodeExecutionId::new(next_id()),
        receipt_id: ApprovalCommandReceiptId::new(next_id()),
        bindings,
    })
}

/// 为定义全部节点冻结启动绑定，并按单据组织重验对象读取权。
///
/// # 参数
/// * `graph` - 定义图
/// * `organization_id` - 单据责任组织
///
/// # 返回
/// 返回与节点一一对应的绑定。
///
/// # 错误
/// 节点审批人引用非法或显示名为空时返回校验错误。
fn start_bindings_from_graph(
    graph: &DefinitionGraph,
    organization_id: &str,
) -> Result<Vec<StartAssigneeBinding>> {
    let mut bindings = Vec::with_capacity(graph.nodes.len());
    for node in &graph.nodes {
        let assignee = node.assignee_participant_id.as_str();
        let failure = match customer_refund_object_readable(organization_id, assignee) {
            Ok(true) => None,
            Ok(false) | Err(_) => Some(AuthorizationFailure::CannotReadSubject),
        };
        bindings.push(StartAssigneeBinding {
            id: ApprovalInstanceAssigneeId::new(next_id()),
            node_key: node.node_key.clone(),
            participant: node.assignee_participant_id.clone(),
            eligibility: converge_eligibility(assignee, &node.assignee_label_snapshot, failure)?,
        });
    }
    if bindings.is_empty() {
        return Err(Error::ConflictError(
            "审批定义没有节点，无法启动客户退款审批".to_string(),
        ));
    }
    Ok(bindings)
}

/// 客户退款启动事务写入集合。
///
/// # 用途
/// 收拢提交后需一并写入的退款单、快照、启动计划与审计身份。
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
/// 运行事实、不可变快照与入口任务必须与单据迁移同事务。
pub(super) struct CustomerRefundStartPersistInput {
    /// 已进入 `IN_APPROVAL` 的退款单。
    pub refund: CustomerRefund,
    /// 审计操作人。
    pub actor: AuditActor,
    /// 退款单主键。
    pub id: String,
    /// 冻结快照载荷。
    pub snapshot_payload: ApprovalSubjectSnapshotPayload,
    /// `prepare_start` 结果。
    pub prepared: PreparedExecution,
    /// 合同签署的责任角色。
    pub owner_role: &'static str,
    /// 责任组织。
    pub organization_id: String,
    /// 调用方时间。
    pub now: Instant,
}

/// 在同一事务中写入单据迁移、快照、BPM 运行事实与入口任务。
///
/// # 用途
/// 提交启动后原子写入退款单、快照与运行事实。
///
/// # 参数
/// * `db` - 数据库
/// * `input` - 退款单、快照与启动计划
///
/// # 返回
/// 返回提交后的退款单实体，由调用方装配视图。
///
/// # 错误
/// 仓储写入失败或计划不完整时返回错误，事务回滚。
///
/// # 关键业务约束
/// Replay 不得重复写运行事实；Apply 必须写入快照与入口任务。
pub(super) async fn persist_customer_refund_start(
    db: &Database,
    input: CustomerRefundStartPersistInput,
) -> Result<CustomerRefund> {
    let CustomerRefundStartPersistInput {
        refund,
        actor,
        id,
        snapshot_payload,
        prepared,
        owner_role,
        organization_id,
        now,
    } = input;
    let PreparedExecution::Apply(writes) = prepared else {
        return Ok(refund);
    };
    let audit = actor.resource_log("customer_refund.submit", "customer_refund", id)?;
    let db = db.clone();
    let client = db.client().clone();
    client
        .with_transaction(move |session| {
            Box::pin(async move {
                db.bpm_workflow()
                    .insert_command_receipt(&writes.receipt, session)
                    .await
                    .map_err(map_receipt_first_write_error)?;
                let guarded = db
                    .business_documents()
                    .mark_approval_started(
                        writes.instance.subject.subject_id(),
                        DocumentType::CustomerRefund,
                        &writes.instance.process_definition_id,
                        writes.instance.definition_version,
                        now,
                        session,
                    )
                    .await?;
                if guarded.is_none() {
                    return Err(Error::ConflictError(
                        "客户退款单审批启动守卫冲突，请刷新后重试".to_string(),
                    ));
                }
                persist_runtime_writes(
                    &db,
                    &writes,
                    &snapshot_payload,
                    owner_role,
                    &organization_id,
                    now,
                    session,
                )
                .await?;
                let mut refund = refund;
                db.customer_refunds().update(&mut refund, session).await?;
                db.audit_logs().create(&audit, session).await?;
                Ok::<CustomerRefund, crate::errors::Error>(refund)
            })
        })
        .await
}

/// 将启动计划写入 BPM 集合、不可变快照和入口 WorkItem。
///
/// # 错误
/// 计划缺少入口执行或写入失败时返回错误。
pub(super) async fn persist_runtime_writes(
    db: &Database,
    writes: &crate::approval::execution::apply_plan::PlannedWrites,
    snapshot_payload: &ApprovalSubjectSnapshotPayload,
    owner_role: &str,
    organization_id: &str,
    now: Instant,
    session: &mut mongodb::ClientSession,
) -> Result<()> {
    let first = writes
        .created_executions
        .first()
        .ok_or_else(|| Error::Internal("启动计划缺少入口执行，不得提交客户退款".to_string()))?;
    db.bpm_workflow()
        .create_bpm_runtime_after_receipt(
            &writes.instance,
            &writes.created_assignees,
            first,
            &list_projection_from_execution(first, now),
            session,
        )
        .await?;
    let snapshot = ApprovalSubjectSnapshot::new(
        ApprovalSubjectSnapshotId::new(next_id()),
        ApprovalProcessInstanceId::new(writes.instance.base.id.clone()),
        DocumentType::CustomerRefund,
        writes.instance.subject.subject_id(),
        writes.instance.subject_version,
        snapshot_payload.clone(),
    )
    .map_err(|error| Error::ValidationError(error.to_string()))?;
    db.approval_subject_snapshots()
        .create_immutable_snapshot(&snapshot, session)
        .await?;
    persist_open_tasks(db, writes, owner_role, organization_id, now, session).await
}

/// 由入口执行构造有界列表投影。
///
/// # 参数
/// * `execution` - 入口执行
/// * `now` - 状态变更时间
///
/// # 返回
/// 返回启动时的列表投影。
fn list_projection_from_execution(
    execution: &ApprovalNodeExecution,
    now: Instant,
) -> ApprovalInstanceListProjection {
    ApprovalInstanceListProjection {
        current_node_key: Some(execution.node_key.clone()),
        current_node_name: Some(execution.node_name.clone()),
        current_assignee_participant_id: Some(execution.assignee_participant_id.as_str().to_string()),
        current_assignee_name: Some(execution.assignee_name_snapshot.clone()),
        latest_rejected_execution_id: None,
        latest_rejection_summary: None,
        last_status_changed_at: Some(now.unix_secs()),
    }
}

/// 将 `HumanTaskRequested` 映射为 `DOCUMENT_APPROVAL` 任务并写入。
///
/// # 错误
/// 责任人为空或仓储失败时返回错误。
async fn persist_open_tasks(
    db: &Database,
    writes: &crate::approval::execution::apply_plan::PlannedWrites,
    owner_role: &str,
    organization_id: &str,
    now: Instant,
    session: &mut mongodb::ClientSession,
) -> Result<()> {
    for intent in &writes.create_tasks {
        let TaskIntent::HumanTaskRequested {
            execution_id,
            assignee,
            ..
        } = intent
        else {
            continue;
        };
        let item = WorkItem::new_document_approval(
            WorkItemId::new(next_id()),
            DocumentApprovalWorkItemData {
                approval_node_execution_id: execution_id.clone(),
                business_object_type: DocumentType::CustomerRefund.as_str().to_string(),
                business_object_id: writes.instance.subject.subject_id().to_string(),
                subject_version: writes.instance.subject_version.to_string(),
                owner_role: owner_role.to_string(),
                owner_organization_id: organization_id.to_string(),
                owner_user_id: assignee.as_str().to_string(),
                priority: WorkItemPriority::Normal,
                due_at: None,
            },
            now,
        )?;
        db.work_items().create(&item, session).await?;
    }
    Ok(())
}

/// 读取供应商退款同载荷启动收据；不存在时返回 `None`。
///
/// # 参数
/// * `db` - 数据库
/// * `subject` - 业务对象引用
/// * `subject_version` - 冻结提交版本
/// * `idempotency_key` - 调用方幂等键
///
/// # 返回
/// 已提交收据或空。
///
/// # 错误
/// 幂等键非法或仓储失败时返回错误。
pub(super) async fn load_supplier_refund_start_receipt(
    db: &Database,
    subject: &SubjectRef,
    subject_version: u32,
    idempotency_key: &str,
) -> Result<Option<bpm::model::ApprovalCommandReceipt>> {
    load_start_receipt_for_document_type(
        db,
        DocumentType::SupplierRefund,
        subject,
        subject_version,
        idempotency_key,
    )
    .await
}

/// 供应商退款启动输入。
///
/// # 用途
/// 收拢 `build_supplier_refund_start_input` 的定义图、绑定与提交人参数。
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
/// 审批人取自已发布节点，不接受客户端选择。
pub(super) struct SupplierRefundStartInput<'a> {
    /// 绑定定义图。
    pub graph: DefinitionGraph,
    /// 冻结绑定。
    pub binding: &'a ApprovalDefinitionBinding,
    /// 业务对象引用。
    pub subject: SubjectRef,
    /// 冻结提交版本。
    pub subject_version: u32,
    /// 提交人。
    pub actor_id: &'a str,
    /// 单据责任组织。
    pub organization_id: &'a str,
    /// 规范化前的幂等键。
    pub idempotency_key: &'a str,
    /// 已存在收据。
    pub receipt: Option<bpm::model::ApprovalCommandReceipt>,
    /// 调用方时间。
    pub now: Instant,
}

/// 由定义图与单据组织构造供应商退款启动输入。
///
/// 审批人取自已发布节点，不接受客户端选择。对象读取权失败时收敛为 BLOCKED。
///
/// # 用途
/// 把启动参数收敛为引擎 `prepare_start` 输入。
///
/// # 参数
/// * `input` - 定义图、绑定、主体与提交人
///
/// # 返回
/// 返回可交给 `prepare_start` 的输入。
///
/// # 错误
/// 入口缺失、审批人非法、幂等键非法或读取权校验失败时返回错误。
///
/// # 关键业务约束
/// 定义版本必须与冻结绑定一致；对象读取权失败时收敛为 BLOCKED。
pub(super) fn build_supplier_refund_start_input(
    input: SupplierRefundStartInput<'_>,
) -> Result<StartExecutionInput> {
    let SupplierRefundStartInput {
        graph,
        binding,
        subject,
        subject_version,
        actor_id,
        organization_id,
        idempotency_key,
        receipt,
        now,
    } = input;
    if graph.definition.definition_version != binding.approval_definition_version {
        return Err(Error::ConflictError(
            "供应商退款单绑定定义版本与已加载定义不一致".to_string(),
        ));
    }
    let idempotency_key = normalize_idempotency_key(idempotency_key)?;
    let actor =
        ParticipantId::new(actor_id).map_err(|_| Error::ValidationError("提交人引用无效".to_string()))?;
    let timestamp = Timestamp::from_utc(now.as_utc());
    let bindings = supplier_refund_start_bindings(&graph, organization_id)?;
    let entry = graph
        .entry_node()
        .map_err(|_| Error::ConflictError("审批定义缺少入口节点".to_string()))?;
    let entry_eligibility = bindings
        .iter()
        .find(|item| item.node_key == entry.node_key)
        .map(|item| item.eligibility.clone())
        .ok_or_else(|| Error::ConflictError("入口节点缺少审批人绑定".to_string()))?;
    Ok(StartExecutionInput {
        command: ExecutionCommandInput {
            graph,
            current_eligibility: entry_eligibility.clone(),
            next_eligibility: entry_eligibility,
            receipt,
            idempotency_key,
            now: timestamp,
        },
        process_kind: process_kind_of(DocumentType::SupplierRefund),
        subject,
        subject_version,
        binding_id: binding.approval_process_definition_id.as_ref().to_string(),
        definition_version: binding.approval_definition_version,
        actor,
        instance_id: ApprovalProcessInstanceId::new(next_id()),
        entry_execution_id: ApprovalNodeExecutionId::new(next_id()),
        receipt_id: ApprovalCommandReceiptId::new(next_id()),
        bindings,
    })
}

/// 为定义全部节点冻结供应商退款启动绑定，并按单据组织重验对象读取权。
///
/// # 参数
/// * `graph` - 定义图
/// * `organization_id` - 单据责任组织
///
/// # 返回
/// 返回与节点一一对应的绑定。
///
/// # 错误
/// 节点审批人引用非法或显示名为空时返回校验错误。
fn supplier_refund_start_bindings(
    graph: &DefinitionGraph,
    organization_id: &str,
) -> Result<Vec<StartAssigneeBinding>> {
    let mut bindings = Vec::with_capacity(graph.nodes.len());
    for node in &graph.nodes {
        let assignee = node.assignee_participant_id.as_str();
        let failure = match supplier_refund_object_readable(organization_id, assignee) {
            Ok(true) => None,
            Ok(false) | Err(_) => Some(AuthorizationFailure::CannotReadSubject),
        };
        bindings.push(StartAssigneeBinding {
            id: ApprovalInstanceAssigneeId::new(next_id()),
            node_key: node.node_key.clone(),
            participant: node.assignee_participant_id.clone(),
            eligibility: converge_eligibility(assignee, &node.assignee_label_snapshot, failure)?,
        });
    }
    if bindings.is_empty() {
        return Err(Error::ConflictError(
            "审批定义没有节点，无法启动供应商退款审批".to_string(),
        ));
    }
    Ok(bindings)
}

/// 供应商退款启动事务写入集合。
///
/// # 用途
/// 收拢提交后需一并写入的退款单、快照、启动计划与审计身份。
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
/// 运行事实、不可变快照与入口任务必须与单据迁移同事务。
pub(super) struct SupplierRefundStartPersistInput {
    /// 已进入 `IN_APPROVAL` 的退款单。
    pub refund: SupplierRefund,
    /// 审计操作人。
    pub actor: AuditActor,
    /// 退款单主键。
    pub id: String,
    /// 冻结快照载荷。
    pub snapshot_payload: ApprovalSubjectSnapshotPayload,
    /// `prepare_start` 结果。
    pub prepared: PreparedExecution,
    /// 合同签署的责任角色。
    pub owner_role: &'static str,
    /// 责任组织。
    pub organization_id: String,
    /// 调用方时间。
    pub now: Instant,
}

/// 在同一事务中写入供应商退款迁移、快照、BPM 运行事实与入口任务。
///
/// # 用途
/// 提交启动后原子写入退款单、快照与运行事实。
///
/// # 参数
/// * `db` - 数据库
/// * `input` - 退款单、快照与启动计划
///
/// # 返回
/// 返回提交后的退款单实体，由调用方装配视图。
///
/// # 错误
/// 仓储写入失败或计划不完整时返回错误，事务回滚。
///
/// # 关键业务约束
/// Replay 不得重复写运行事实；Apply 必须写入快照与入口任务。
pub(super) async fn persist_supplier_refund_start(
    db: &Database,
    input: SupplierRefundStartPersistInput,
) -> Result<SupplierRefund> {
    let SupplierRefundStartPersistInput {
        refund,
        actor,
        id,
        snapshot_payload,
        prepared,
        owner_role,
        organization_id,
        now,
    } = input;
    let PreparedExecution::Apply(writes) = prepared else {
        return Ok(refund);
    };
    let audit = actor.resource_log("supplier_refund.submit", "supplier_refund", id)?;
    let db = db.clone();
    let client = db.client().clone();
    client
        .with_transaction(move |session| {
            Box::pin(async move {
                db.bpm_workflow()
                    .insert_command_receipt(&writes.receipt, session)
                    .await
                    .map_err(map_receipt_first_write_error)?;
                let guarded = db
                    .business_documents()
                    .mark_approval_started(
                        writes.instance.subject.subject_id(),
                        DocumentType::SupplierRefund,
                        &writes.instance.process_definition_id,
                        writes.instance.definition_version,
                        now,
                        session,
                    )
                    .await?;
                if guarded.is_none() {
                    return Err(Error::ConflictError(
                        "供应商退款单审批启动守卫冲突，请刷新后重试".to_string(),
                    ));
                }
                persist_supplier_refund_runtime(
                    &db,
                    &writes,
                    &snapshot_payload,
                    owner_role,
                    &organization_id,
                    now,
                    session,
                )
                .await?;
                let mut refund = refund;
                db.supplier_refunds().update(&mut refund, session).await?;
                db.audit_logs().create(&audit, session).await?;
                Ok::<SupplierRefund, crate::errors::Error>(refund)
            })
        })
        .await
}

/// 将供应商退款启动计划写入 BPM 集合、不可变快照和入口 WorkItem。
///
/// # 错误
/// 计划缺少入口执行或写入失败时返回错误。
pub(super) async fn persist_supplier_refund_runtime(
    db: &Database,
    writes: &crate::approval::execution::apply_plan::PlannedWrites,
    snapshot_payload: &ApprovalSubjectSnapshotPayload,
    owner_role: &str,
    organization_id: &str,
    now: Instant,
    session: &mut mongodb::ClientSession,
) -> Result<()> {
    let first = writes
        .created_executions
        .first()
        .ok_or_else(|| Error::Internal("启动计划缺少入口执行，不得提交供应商退款".to_string()))?;
    db.bpm_workflow()
        .create_bpm_runtime_after_receipt(
            &writes.instance,
            &writes.created_assignees,
            first,
            &list_projection_from_execution(first, now),
            session,
        )
        .await?;
    let snapshot = ApprovalSubjectSnapshot::new(
        ApprovalSubjectSnapshotId::new(next_id()),
        ApprovalProcessInstanceId::new(writes.instance.base.id.clone()),
        DocumentType::SupplierRefund,
        writes.instance.subject.subject_id(),
        writes.instance.subject_version,
        snapshot_payload.clone(),
    )
    .map_err(|error| Error::ValidationError(error.to_string()))?;
    db.approval_subject_snapshots()
        .create_immutable_snapshot(&snapshot, session)
        .await?;
    persist_supplier_refund_open_tasks(db, writes, owner_role, organization_id, now, session).await
}

/// 将供应商退款 `HumanTaskRequested` 映射为 `DOCUMENT_APPROVAL` 任务并写入。
///
/// # 错误
/// 责任人为空或仓储失败时返回错误。
async fn persist_supplier_refund_open_tasks(
    db: &Database,
    writes: &crate::approval::execution::apply_plan::PlannedWrites,
    owner_role: &str,
    organization_id: &str,
    now: Instant,
    session: &mut mongodb::ClientSession,
) -> Result<()> {
    for intent in &writes.create_tasks {
        let TaskIntent::HumanTaskRequested {
            execution_id,
            assignee,
            ..
        } = intent
        else {
            continue;
        };
        let item = WorkItem::new_document_approval(
            WorkItemId::new(next_id()),
            DocumentApprovalWorkItemData {
                approval_node_execution_id: execution_id.clone(),
                business_object_type: DocumentType::SupplierRefund.as_str().to_string(),
                business_object_id: writes.instance.subject.subject_id().to_string(),
                subject_version: writes.instance.subject_version.to_string(),
                owner_role: owner_role.to_string(),
                owner_organization_id: organization_id.to_string(),
                owner_user_id: assignee.as_str().to_string(),
                priority: WorkItemPriority::Normal,
                due_at: None,
            },
            now,
        )?;
        db.work_items().create(&item, session).await?;
    }
    Ok(())
}

/// 读取回款冲正同载荷启动收据；不存在时返回 `None`。
///
/// # 参数
/// * `db` - 数据库
/// * `subject` - 业务对象引用
/// * `subject_version` - 冻结提交版本
/// * `idempotency_key` - 调用方幂等键
///
/// # 返回
/// 已提交收据或空。
///
/// # 错误
/// 幂等键非法或仓储失败时返回错误。
pub(super) async fn load_receipt_reversal_start_receipt(
    db: &Database,
    subject: &SubjectRef,
    subject_version: u32,
    idempotency_key: &str,
) -> Result<Option<bpm::model::ApprovalCommandReceipt>> {
    load_start_receipt_for_document_type(
        db,
        DocumentType::ReceiptReversal,
        subject,
        subject_version,
        idempotency_key,
    )
    .await
}

/// 回款冲正启动输入。
///
/// # 用途
/// 收拢 `build_receipt_reversal_start_input` 的定义图、绑定与提交人参数。
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
/// 审批人取自已发布节点，不接受客户端选择。
pub(super) struct ReceiptReversalStartInput<'a> {
    /// 绑定定义图。
    pub graph: DefinitionGraph,
    /// 冻结绑定。
    pub binding: &'a ApprovalDefinitionBinding,
    /// 业务对象引用。
    pub subject: SubjectRef,
    /// 冻结提交版本。
    pub subject_version: u32,
    /// 提交人。
    pub actor_id: &'a str,
    /// 单据责任组织。
    pub organization_id: &'a str,
    /// 规范化前的幂等键。
    pub idempotency_key: &'a str,
    /// 已存在收据。
    pub receipt: Option<bpm::model::ApprovalCommandReceipt>,
    /// 调用方时间。
    pub now: Instant,
}

/// 由定义图与单据组织构造回款冲正启动输入。
///
/// 审批人取自已发布节点，不接受客户端选择。对象读取权失败时收敛为 BLOCKED。
///
/// # 用途
/// 把启动参数收敛为引擎 `prepare_start` 输入。
///
/// # 参数
/// * `input` - 定义图、绑定、主体与提交人
///
/// # 返回
/// 返回可交给 `prepare_start` 的输入。
///
/// # 错误
/// 入口缺失、审批人非法、幂等键非法或读取权校验失败时返回错误。
///
/// # 关键业务约束
/// 定义版本必须与冻结绑定一致；对象读取权失败时收敛为 BLOCKED。
pub(super) fn build_receipt_reversal_start_input(
    input: ReceiptReversalStartInput<'_>,
) -> Result<StartExecutionInput> {
    let ReceiptReversalStartInput {
        graph,
        binding,
        subject,
        subject_version,
        actor_id,
        organization_id,
        idempotency_key,
        receipt,
        now,
    } = input;
    if graph.definition.definition_version != binding.approval_definition_version {
        return Err(Error::ConflictError(
            "回款冲正单绑定定义版本与已加载定义不一致".to_string(),
        ));
    }
    let idempotency_key = normalize_idempotency_key(idempotency_key)?;
    let actor =
        ParticipantId::new(actor_id).map_err(|_| Error::ValidationError("提交人引用无效".to_string()))?;
    let timestamp = Timestamp::from_utc(now.as_utc());
    let bindings = receipt_reversal_start_bindings(&graph, organization_id)?;
    let entry = graph
        .entry_node()
        .map_err(|_| Error::ConflictError("审批定义缺少入口节点".to_string()))?;
    let entry_eligibility = bindings
        .iter()
        .find(|item| item.node_key == entry.node_key)
        .map(|item| item.eligibility.clone())
        .ok_or_else(|| Error::ConflictError("入口节点缺少审批人绑定".to_string()))?;
    Ok(StartExecutionInput {
        command: ExecutionCommandInput {
            graph,
            current_eligibility: entry_eligibility.clone(),
            next_eligibility: entry_eligibility,
            receipt,
            idempotency_key,
            now: timestamp,
        },
        process_kind: process_kind_of(DocumentType::ReceiptReversal),
        subject,
        subject_version,
        binding_id: binding.approval_process_definition_id.as_ref().to_string(),
        definition_version: binding.approval_definition_version,
        actor,
        instance_id: ApprovalProcessInstanceId::new(next_id()),
        entry_execution_id: ApprovalNodeExecutionId::new(next_id()),
        receipt_id: ApprovalCommandReceiptId::new(next_id()),
        bindings,
    })
}

/// 为定义全部节点冻结回款冲正启动绑定，并按单据组织重验对象读取权。
///
/// # 参数
/// * `graph` - 定义图
/// * `organization_id` - 单据责任组织
///
/// # 返回
/// 返回与节点一一对应的绑定。
///
/// # 错误
/// 节点审批人引用非法或显示名为空时返回校验错误。
fn receipt_reversal_start_bindings(
    graph: &DefinitionGraph,
    organization_id: &str,
) -> Result<Vec<StartAssigneeBinding>> {
    let mut bindings = Vec::with_capacity(graph.nodes.len());
    for node in &graph.nodes {
        let assignee = node.assignee_participant_id.as_str();
        let failure = match receipt_reversal_object_readable(organization_id, assignee) {
            Ok(true) => None,
            Ok(false) | Err(_) => Some(AuthorizationFailure::CannotReadSubject),
        };
        bindings.push(StartAssigneeBinding {
            id: ApprovalInstanceAssigneeId::new(next_id()),
            node_key: node.node_key.clone(),
            participant: node.assignee_participant_id.clone(),
            eligibility: converge_eligibility(assignee, &node.assignee_label_snapshot, failure)?,
        });
    }
    if bindings.is_empty() {
        return Err(Error::ConflictError(
            "审批定义没有节点，无法启动回款冲正审批".to_string(),
        ));
    }
    Ok(bindings)
}

/// 回款冲正启动事务写入集合。
///
/// # 用途
/// 收拢提交后需一并写入的冲正单、快照、启动计划与审计身份。
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
/// 运行事实、不可变快照与入口任务必须与单据迁移同事务。
pub(super) struct ReceiptReversalStartPersistInput {
    /// 已进入 `IN_APPROVAL` 的冲正单。
    pub reversal: ReceiptReversal,
    /// 审计操作人。
    pub actor: AuditActor,
    /// 冲正单主键。
    pub id: String,
    /// 冻结快照载荷。
    pub snapshot_payload: ApprovalSubjectSnapshotPayload,
    /// `prepare_start` 结果。
    pub prepared: PreparedExecution,
    /// 合同签署的责任角色。
    pub owner_role: &'static str,
    /// 责任组织。
    pub organization_id: String,
    /// 调用方时间。
    pub now: Instant,
}

/// 在同一事务中写入回款冲正迁移、快照、BPM 运行事实与入口任务。
///
/// # 用途
/// 提交启动后原子写入冲正单、快照与运行事实。
///
/// # 参数
/// * `db` - 数据库
/// * `input` - 冲正单、快照与启动计划
///
/// # 返回
/// 返回提交后的冲正单实体，由调用方装配视图。
///
/// # 错误
/// 仓储写入失败或计划不完整时返回错误，事务回滚。
///
/// # 关键业务约束
/// Replay 不得重复写运行事实；Apply 必须写入快照与入口任务。
pub(super) async fn persist_receipt_reversal_start(
    db: &Database,
    input: ReceiptReversalStartPersistInput,
) -> Result<ReceiptReversal> {
    let ReceiptReversalStartPersistInput {
        reversal,
        actor,
        id,
        snapshot_payload,
        prepared,
        owner_role,
        organization_id,
        now,
    } = input;
    let PreparedExecution::Apply(writes) = prepared else {
        return Ok(reversal);
    };
    let audit = actor.resource_log("receipt_reversal.submit", "receipt_reversal", id)?;
    let db = db.clone();
    let client = db.client().clone();
    client
        .with_transaction(move |session| {
            Box::pin(async move {
                db.bpm_workflow()
                    .insert_command_receipt(&writes.receipt, session)
                    .await
                    .map_err(map_receipt_first_write_error)?;
                let guarded = db
                    .business_documents()
                    .mark_approval_started(
                        writes.instance.subject.subject_id(),
                        DocumentType::ReceiptReversal,
                        &writes.instance.process_definition_id,
                        writes.instance.definition_version,
                        now,
                        session,
                    )
                    .await?;
                if guarded.is_none() {
                    return Err(Error::ConflictError(
                        "回款冲正单审批启动守卫冲突，请刷新后重试".to_string(),
                    ));
                }
                persist_receipt_reversal_runtime(
                    &db,
                    &writes,
                    &snapshot_payload,
                    owner_role,
                    &organization_id,
                    now,
                    session,
                )
                .await?;
                let mut reversal = reversal;
                db.receipt_reversals().update(&mut reversal, session).await?;
                db.audit_logs().create(&audit, session).await?;
                Ok::<ReceiptReversal, crate::errors::Error>(reversal)
            })
        })
        .await
}

/// 将回款冲正启动计划写入 BPM 集合、不可变快照和入口 WorkItem。
///
/// # 错误
/// 计划缺少入口执行或写入失败时返回错误。
pub(super) async fn persist_receipt_reversal_runtime(
    db: &Database,
    writes: &crate::approval::execution::apply_plan::PlannedWrites,
    snapshot_payload: &ApprovalSubjectSnapshotPayload,
    owner_role: &str,
    organization_id: &str,
    now: Instant,
    session: &mut mongodb::ClientSession,
) -> Result<()> {
    let first = writes
        .created_executions
        .first()
        .ok_or_else(|| Error::Internal("启动计划缺少入口执行，不得提交回款冲正".to_string()))?;
    db.bpm_workflow()
        .create_bpm_runtime_after_receipt(
            &writes.instance,
            &writes.created_assignees,
            first,
            &list_projection_from_execution(first, now),
            session,
        )
        .await?;
    let snapshot = ApprovalSubjectSnapshot::new(
        ApprovalSubjectSnapshotId::new(next_id()),
        ApprovalProcessInstanceId::new(writes.instance.base.id.clone()),
        DocumentType::ReceiptReversal,
        writes.instance.subject.subject_id(),
        writes.instance.subject_version,
        snapshot_payload.clone(),
    )
    .map_err(|error| Error::ValidationError(error.to_string()))?;
    db.approval_subject_snapshots()
        .create_immutable_snapshot(&snapshot, session)
        .await?;
    persist_receipt_reversal_open_tasks(db, writes, owner_role, organization_id, now, session).await
}

/// 将回款冲正 `HumanTaskRequested` 映射为 `DOCUMENT_APPROVAL` 任务并写入。
///
/// # 错误
/// 责任人为空或仓储失败时返回错误。
async fn persist_receipt_reversal_open_tasks(
    db: &Database,
    writes: &crate::approval::execution::apply_plan::PlannedWrites,
    owner_role: &str,
    organization_id: &str,
    now: Instant,
    session: &mut mongodb::ClientSession,
) -> Result<()> {
    for intent in &writes.create_tasks {
        let TaskIntent::HumanTaskRequested {
            execution_id,
            assignee,
            ..
        } = intent
        else {
            continue;
        };
        let item = WorkItem::new_document_approval(
            WorkItemId::new(next_id()),
            DocumentApprovalWorkItemData {
                approval_node_execution_id: execution_id.clone(),
                business_object_type: DocumentType::ReceiptReversal.as_str().to_string(),
                business_object_id: writes.instance.subject.subject_id().to_string(),
                subject_version: writes.instance.subject_version.to_string(),
                owner_role: owner_role.to_string(),
                owner_organization_id: organization_id.to_string(),
                owner_user_id: assignee.as_str().to_string(),
                priority: WorkItemPriority::Normal,
                due_at: None,
            },
            now,
        )?;
        db.work_items().create(&item, session).await?;
    }
    Ok(())
}

/// 读取付款冲正同载荷启动收据；不存在时返回 `None`。
///
/// # 参数
/// * `db` - 数据库
/// * `subject` - 业务对象引用
/// * `subject_version` - 冻结提交版本
/// * `idempotency_key` - 调用方幂等键
///
/// # 返回
/// 已提交收据或空。
///
/// # 错误
/// 幂等键非法或仓储失败时返回错误。
pub(super) async fn load_payment_reversal_start_receipt(
    db: &Database,
    subject: &SubjectRef,
    subject_version: u32,
    idempotency_key: &str,
) -> Result<Option<bpm::model::ApprovalCommandReceipt>> {
    load_start_receipt_for_document_type(
        db,
        DocumentType::PaymentReversal,
        subject,
        subject_version,
        idempotency_key,
    )
    .await
}

/// 付款冲正启动输入。
///
/// # 用途
/// 收拢 `build_payment_reversal_start_input` 的定义图、绑定与提交人参数。
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
/// 审批人取自已发布节点，不接受客户端选择。
pub(super) struct PaymentReversalStartInput<'a> {
    /// 绑定定义图。
    pub graph: DefinitionGraph,
    /// 冻结绑定。
    pub binding: &'a ApprovalDefinitionBinding,
    /// 业务对象引用。
    pub subject: SubjectRef,
    /// 冻结提交版本。
    pub subject_version: u32,
    /// 提交人。
    pub actor_id: &'a str,
    /// 单据责任组织。
    pub organization_id: &'a str,
    /// 规范化前的幂等键。
    pub idempotency_key: &'a str,
    /// 已存在收据。
    pub receipt: Option<bpm::model::ApprovalCommandReceipt>,
    /// 调用方时间。
    pub now: Instant,
}

/// 由定义图与单据组织构造付款冲正启动输入。
///
/// 审批人取自已发布节点，不接受客户端选择。对象读取权失败时收敛为 BLOCKED。
///
/// # 用途
/// 把启动参数收敛为引擎 `prepare_start` 输入。
///
/// # 参数
/// * `input` - 定义图、绑定、主体与提交人
///
/// # 返回
/// 返回可交给 `prepare_start` 的输入。
///
/// # 错误
/// 入口缺失、审批人非法、幂等键非法或读取权校验失败时返回错误。
///
/// # 关键业务约束
/// 定义版本必须与冻结绑定一致；对象读取权失败时收敛为 BLOCKED。
pub(super) fn build_payment_reversal_start_input(
    input: PaymentReversalStartInput<'_>,
) -> Result<StartExecutionInput> {
    let PaymentReversalStartInput {
        graph,
        binding,
        subject,
        subject_version,
        actor_id,
        organization_id,
        idempotency_key,
        receipt,
        now,
    } = input;
    if graph.definition.definition_version != binding.approval_definition_version {
        return Err(Error::ConflictError(
            "付款冲正单绑定定义版本与已加载定义不一致".to_string(),
        ));
    }
    let idempotency_key = normalize_idempotency_key(idempotency_key)?;
    let actor =
        ParticipantId::new(actor_id).map_err(|_| Error::ValidationError("提交人引用无效".to_string()))?;
    let timestamp = Timestamp::from_utc(now.as_utc());
    let bindings = payment_reversal_start_bindings(&graph, organization_id)?;
    let entry = graph
        .entry_node()
        .map_err(|_| Error::ConflictError("审批定义缺少入口节点".to_string()))?;
    let entry_eligibility = bindings
        .iter()
        .find(|item| item.node_key == entry.node_key)
        .map(|item| item.eligibility.clone())
        .ok_or_else(|| Error::ConflictError("入口节点缺少审批人绑定".to_string()))?;
    Ok(StartExecutionInput {
        command: ExecutionCommandInput {
            graph,
            current_eligibility: entry_eligibility.clone(),
            next_eligibility: entry_eligibility,
            receipt,
            idempotency_key,
            now: timestamp,
        },
        process_kind: process_kind_of(DocumentType::PaymentReversal),
        subject,
        subject_version,
        binding_id: binding.approval_process_definition_id.as_ref().to_string(),
        definition_version: binding.approval_definition_version,
        actor,
        instance_id: ApprovalProcessInstanceId::new(next_id()),
        entry_execution_id: ApprovalNodeExecutionId::new(next_id()),
        receipt_id: ApprovalCommandReceiptId::new(next_id()),
        bindings,
    })
}

/// 为定义全部节点冻结付款冲正启动绑定，并按单据组织重验对象读取权。
///
/// # 参数
/// * `graph` - 定义图
/// * `organization_id` - 单据责任组织
///
/// # 返回
/// 返回与节点一一对应的绑定。
///
/// # 错误
/// 节点审批人引用非法或显示名为空时返回校验错误。
fn payment_reversal_start_bindings(
    graph: &DefinitionGraph,
    organization_id: &str,
) -> Result<Vec<StartAssigneeBinding>> {
    let mut bindings = Vec::with_capacity(graph.nodes.len());
    for node in &graph.nodes {
        let assignee = node.assignee_participant_id.as_str();
        let failure = match payment_reversal_object_readable(organization_id, assignee) {
            Ok(true) => None,
            Ok(false) | Err(_) => Some(AuthorizationFailure::CannotReadSubject),
        };
        bindings.push(StartAssigneeBinding {
            id: ApprovalInstanceAssigneeId::new(next_id()),
            node_key: node.node_key.clone(),
            participant: node.assignee_participant_id.clone(),
            eligibility: converge_eligibility(assignee, &node.assignee_label_snapshot, failure)?,
        });
    }
    if bindings.is_empty() {
        return Err(Error::ConflictError(
            "审批定义没有节点，无法启动付款冲正审批".to_string(),
        ));
    }
    Ok(bindings)
}

/// 付款冲正启动事务写入集合。
///
/// # 用途
/// 收拢提交后需一并写入的冲正单、快照、启动计划与审计身份。
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
/// 运行事实、不可变快照与入口任务必须与单据迁移同事务。
pub(super) struct PaymentReversalStartPersistInput {
    /// 已进入 `IN_APPROVAL` 的冲正单。
    pub reversal: PaymentReversal,
    /// 审计操作人。
    pub actor: AuditActor,
    /// 冲正单主键。
    pub id: String,
    /// 冻结快照载荷。
    pub snapshot_payload: ApprovalSubjectSnapshotPayload,
    /// `prepare_start` 结果。
    pub prepared: PreparedExecution,
    /// 合同签署的责任角色。
    pub owner_role: &'static str,
    /// 责任组织。
    pub organization_id: String,
    /// 调用方时间。
    pub now: Instant,
}

/// 在同一事务中写入付款冲正迁移、快照、BPM 运行事实与入口任务。
///
/// # 用途
/// 提交启动后原子写入冲正单、快照与运行事实。
///
/// # 参数
/// * `db` - 数据库
/// * `input` - 冲正单、快照与启动计划
///
/// # 返回
/// 返回提交后的冲正单实体，由调用方装配视图。
///
/// # 错误
/// 仓储写入失败或计划不完整时返回错误，事务回滚。
///
/// # 关键业务约束
/// Replay 不得重复写运行事实；Apply 必须写入快照与入口任务。
pub(super) async fn persist_payment_reversal_start(
    db: &Database,
    input: PaymentReversalStartPersistInput,
) -> Result<PaymentReversal> {
    let PaymentReversalStartPersistInput {
        reversal,
        actor,
        id,
        snapshot_payload,
        prepared,
        owner_role,
        organization_id,
        now,
    } = input;
    let PreparedExecution::Apply(writes) = prepared else {
        return Ok(reversal);
    };
    let audit = actor.resource_log("payment_reversal.submit", "payment_reversal", id)?;
    let db = db.clone();
    let client = db.client().clone();
    client
        .with_transaction(move |session| {
            Box::pin(async move {
                db.bpm_workflow()
                    .insert_command_receipt(&writes.receipt, session)
                    .await
                    .map_err(map_receipt_first_write_error)?;
                let guarded = db
                    .business_documents()
                    .mark_approval_started(
                        writes.instance.subject.subject_id(),
                        DocumentType::PaymentReversal,
                        &writes.instance.process_definition_id,
                        writes.instance.definition_version,
                        now,
                        session,
                    )
                    .await?;
                if guarded.is_none() {
                    return Err(Error::ConflictError(
                        "付款冲正单审批启动守卫冲突，请刷新后重试".to_string(),
                    ));
                }
                persist_payment_reversal_runtime(
                    &db,
                    &writes,
                    &snapshot_payload,
                    owner_role,
                    &organization_id,
                    now,
                    session,
                )
                .await?;
                let mut reversal = reversal;
                db.payment_reversals().update(&mut reversal, session).await?;
                db.audit_logs().create(&audit, session).await?;
                Ok::<PaymentReversal, crate::errors::Error>(reversal)
            })
        })
        .await
}

/// 将付款冲正启动计划写入 BPM 集合、不可变快照和入口 WorkItem。
///
/// # 错误
/// 计划缺少入口执行或写入失败时返回错误。
pub(super) async fn persist_payment_reversal_runtime(
    db: &Database,
    writes: &crate::approval::execution::apply_plan::PlannedWrites,
    snapshot_payload: &ApprovalSubjectSnapshotPayload,
    owner_role: &str,
    organization_id: &str,
    now: Instant,
    session: &mut mongodb::ClientSession,
) -> Result<()> {
    let first = writes
        .created_executions
        .first()
        .ok_or_else(|| Error::Internal("启动计划缺少入口执行，不得提交付款冲正".to_string()))?;
    db.bpm_workflow()
        .create_bpm_runtime_after_receipt(
            &writes.instance,
            &writes.created_assignees,
            first,
            &list_projection_from_execution(first, now),
            session,
        )
        .await?;
    let snapshot = ApprovalSubjectSnapshot::new(
        ApprovalSubjectSnapshotId::new(next_id()),
        ApprovalProcessInstanceId::new(writes.instance.base.id.clone()),
        DocumentType::PaymentReversal,
        writes.instance.subject.subject_id(),
        writes.instance.subject_version,
        snapshot_payload.clone(),
    )
    .map_err(|error| Error::ValidationError(error.to_string()))?;
    db.approval_subject_snapshots()
        .create_immutable_snapshot(&snapshot, session)
        .await?;
    persist_payment_reversal_open_tasks(db, writes, owner_role, organization_id, now, session).await
}

/// 将付款冲正 `HumanTaskRequested` 映射为 `DOCUMENT_APPROVAL` 任务并写入。
///
/// # 错误
/// 责任人为空或仓储失败时返回错误。
async fn persist_payment_reversal_open_tasks(
    db: &Database,
    writes: &crate::approval::execution::apply_plan::PlannedWrites,
    owner_role: &str,
    organization_id: &str,
    now: Instant,
    session: &mut mongodb::ClientSession,
) -> Result<()> {
    for intent in &writes.create_tasks {
        let TaskIntent::HumanTaskRequested {
            execution_id,
            assignee,
            ..
        } = intent
        else {
            continue;
        };
        let item = WorkItem::new_document_approval(
            WorkItemId::new(next_id()),
            DocumentApprovalWorkItemData {
                approval_node_execution_id: execution_id.clone(),
                business_object_type: DocumentType::PaymentReversal.as_str().to_string(),
                business_object_id: writes.instance.subject.subject_id().to_string(),
                subject_version: writes.instance.subject_version.to_string(),
                owner_role: owner_role.to_string(),
                owner_organization_id: organization_id.to_string(),
                owner_user_id: assignee.as_str().to_string(),
                priority: WorkItemPriority::Normal,
                due_at: None,
            },
            now,
        )?;
        db.work_items().create(&item, session).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::list_projection_from_execution;
    use bpm::ids::{ApprovalNodeExecutionId, ApprovalProcessInstanceId};
    use bpm::model::{ApprovalNodeExecution, NewNodeExecution, ParticipantId, Timestamp};
    use entities::common::time::Instant;

    fn execution() -> ApprovalNodeExecution {
        ApprovalNodeExecution::new_active(NewNodeExecution {
            id: ApprovalNodeExecutionId::new("e1"),
            process_instance_id: ApprovalProcessInstanceId::new("inst-1"),
            node_key: "n1".into(),
            node_name: "财务复核".into(),
            round_no: 1,
            execution_no: 1,
            assignment_source: bpm::model::types::ApprovalExecutionAssignmentSource::Definition,
            replaces_execution_id: None,
            assignee_participant_id: ParticipantId::new("u1").unwrap(),
            assignee_name_snapshot: "张三".into(),
            at: Timestamp::from_unix_secs(10).unwrap(),
        })
        .expect("入口执行夹具")
    }

    /// 列表投影必须来自入口执行，不得推断未知审批人。
    #[test]
    fn list_projection_copies_entry_assignee() {
        let projection = list_projection_from_execution(&execution(), Instant::from_unix_secs(10));
        assert_eq!(projection.current_node_key.as_deref(), Some("n1"));
        assert_eq!(projection.current_assignee_participant_id.as_deref(), Some("u1"));
        assert_eq!(projection.current_assignee_name.as_deref(), Some("张三"));
        assert_eq!(projection.last_status_changed_at, Some(10));
    }
}
