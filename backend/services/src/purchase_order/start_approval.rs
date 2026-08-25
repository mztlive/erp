//! 采购单提交启动：加载定义图、构造 `prepare_start` 输入并持久化运行事实。

use bpm::engine::{DefinitionGraph, StartAssigneeBinding, TaskIntent};
use bpm::ids::{
    ApprovalCommandReceiptId, ApprovalInstanceAssigneeId, ApprovalNodeExecutionId, ApprovalProcessInstanceId,
};
use bpm::model::types::ApprovalCommandKind;
use bpm::model::{ApprovalNodeExecution, ParticipantId, SubjectRef, Timestamp};
use database::repository::bpm::ApprovalInstanceListProjection;
use database::{
    AccessControlExt, ApprovalIntegrationExt, BpmExt, DocumentRegistryExt, Executor, NoTransaction,
    PurchaseOrderExt, Transactional, WorkItemExt,
};
use entities::approval_integration::{ApprovalSubjectSnapshot, ApprovalSubjectSnapshotPayload};
use entities::common::time::Instant;
use entities::document_registry::business_document::ApprovalDefinitionBinding;
use entities::document_registry::{BusinessDocument, DocumentType};
use entities::ids::{ApprovalSubjectSnapshotId, WorkItemId};
use entities::purchase_order::{PurchaseOrder, PurchaseOrderSubmission, PurchaseOrderSubmissionLine};
use entities::work_item::DocumentApprovalWorkItemData;
use entities::work_item::{WorkItem, WorkItemPriority};
use id_generator::next_id;
use mongodb::{ClientSession, Database};

use super::adapter::purchase_order_object_readable;
use super::dto::SavePurchaseOrderLine;
use crate::approval::execution::authorization::{converge_eligibility, AuthorizationFailure};
use crate::approval::execution::idempotency::{normalize_idempotency_key, start_scope};
use crate::approval::execution::{ExecutionCommandInput, PreparedExecution, StartExecutionInput};
use crate::approval::process_kind::process_kind_of;
use crate::errors::{Error, Result};

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

/// 使用调用方执行器加载绑定定义图，供创建并提交的同一事务复用。
///
/// # 参数
/// * `db` - 数据库
/// * `binding` - 创建时冻结的定义绑定
/// * `executor` - 数据访问执行器
///
/// # 返回
/// 返回已持久化的定义图。
///
/// # 错误
/// 定义不存在或仓储失败时返回冲突或仓储错误。
///
/// # 关键业务约束
/// 新建采购单后立即提交时必须用同一事务会话读取绑定定义。
pub(super) async fn load_bound_definition_graph_with_executor(
    db: &Database,
    binding: &ApprovalDefinitionBinding,
    executor: &mut dyn Executor,
) -> Result<DefinitionGraph> {
    let graph = db
        .bpm_workflow()
        .load_definition_graph(&binding.approval_process_definition_id, executor)
        .await?
        .ok_or_else(|| Error::ConflictError("采购单绑定的审批定义不存在".to_string()))?;
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
    let key = normalize_idempotency_key(idempotency_key)?;
    let process_kind = process_kind_of(DocumentType::PurchaseOrder);
    let scope = start_scope(
        process_kind.as_str(),
        subject.subject_kind(),
        subject.subject_id(),
        subject_version,
    );
    Ok(db
        .bpm_workflow()
        .find_command_receipt(
            ApprovalCommandKind::StartApproval,
            &scope,
            &key,
            &mut NoTransaction,
        )
        .await?)
}

/// 采购单启动输入。
///
/// # 用途
/// 收拢 `build_purchase_order_start_input` 的定义图、绑定与提交人参数。
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
pub(super) struct PurchaseOrderStartInput<'a> {
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
pub(super) fn build_purchase_order_start_input(
    input: PurchaseOrderStartInput<'_>,
) -> Result<StartExecutionInput> {
    let PurchaseOrderStartInput {
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
            "采购单绑定定义版本与已加载定义不一致".to_string(),
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
        process_kind: process_kind_of(DocumentType::PurchaseOrder),
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
        let failure = match purchase_order_object_readable(organization_id, assignee) {
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
            "审批定义没有节点，无法启动采购单审批".to_string(),
        ));
    }
    Ok(bindings)
}

/// 采购单提交事务内需要一并写入的冻结提交。
///
/// # 用途
/// 收拢正式号、提交快照、启动计划与审计，供同一事务写入。
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
/// 运行事实、不可变快照与入口任务必须与提交快照同事务。
pub(super) struct PurchaseOrderStartPersistInput {
    /// 已进入审批中的采购单。
    pub order: PurchaseOrder,
    /// 已同步正式号的注册行。
    pub document: BusinessDocument,
    /// 已失效的旧草稿提交。
    pub superseded_draft: PurchaseOrderSubmission,
    /// 冻结提交头。
    pub submission: PurchaseOrderSubmission,
    /// 冻结提交行。
    pub submission_lines: Vec<PurchaseOrderSubmissionLine>,
    /// 提交时携带草稿补丁时，在同一事务推进 guard 并复核采购覆盖。
    pub procurement_guard: Option<PurchaseSubmitProcurementGuard>,
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
    /// 已构造审计。
    pub audit: entities::AuditLog,
}

/// 提交时草稿补丁的采购覆盖校验上下文。
pub(super) struct PurchaseSubmitProcurementGuard {
    /// 服务端合并后的完整目标行。
    pub requested_lines: Vec<SavePurchaseOrderLine>,
    /// 补丁合并前的当前草稿行。
    pub existing_lines: Vec<PurchaseOrderSubmissionLine>,
    /// 推进销售采购 guard 的操作人。
    pub actor_id: String,
}

/// 在同一事务中写入正式号、提交快照、单据迁移、快照、BPM 运行事实与入口任务。
///
/// # 用途
/// 提交启动后原子写入采购单、提交快照与运行事实。
///
/// # 参数
/// * `db` - 数据库
/// * `input` - 提交写入集合
///
/// # 返回
/// 返回首个入口任务身份，无任务时为空。
///
/// # 错误
/// 仓储写入失败或计划不完整时返回错误，事务回滚。
///
/// # 关键业务约束
/// Replay 不得重复写运行事实；Apply 必须写入快照与入口任务。
pub(super) async fn persist_purchase_order_start(
    db: &Database,
    input: PurchaseOrderStartPersistInput,
) -> Result<Option<(String, u64)>> {
    let db = db.clone();
    let client = db.client().clone();
    client
        .with_transaction(move |session| {
            Box::pin(async move { persist_purchase_order_start_with_session(&db, input, session).await })
        })
        .await
}

/// 在调用方事务会话中写入正式号、提交快照、运行事实与入口任务。
///
/// # 参数
/// * `db` - 数据库
/// * `input` - 提交写入集合
/// * `session` - 已开启的 MongoDB 事务会话
///
/// # 返回
/// 返回首个入口任务身份，无任务时为空。
///
/// # 错误
/// 仓储写入失败或计划不完整时返回错误，由调用方事务回滚。
///
/// # 关键业务约束
/// 创建并提交必须复用建单事务，不得再开一层事务。
pub(super) async fn persist_purchase_order_start_with_session(
    db: &Database,
    input: PurchaseOrderStartPersistInput,
    session: &mut ClientSession,
) -> Result<Option<(String, u64)>> {
    let PurchaseOrderStartPersistInput {
        mut order,
        mut document,
        mut superseded_draft,
        submission,
        submission_lines,
        procurement_guard,
        snapshot_payload,
        prepared,
        owner_role,
        organization_id,
        now,
        audit,
    } = input;
    if let Some(guard) = procurement_guard {
        let coverage =
            super::draft_edit::advance_guard_and_load_coverage(db, &order, &guard.actor_id, session).await?;
        super::draft_edit::validate_procurement_line_edit(
            &guard.requested_lines,
            &guard.existing_lines,
            &coverage,
        )?;
    }
    db.purchase_order()
        .create_purchase_submission(&mut order, &submission, &submission_lines, session)
        .await?;
    db.purchase_order_submissions()
        .update(&mut superseded_draft, session)
        .await?;
    db.business_documents().update(&mut document, session).await?;
    let first_task = match prepared {
        PreparedExecution::Apply(writes) => {
            persist_runtime_writes(
                db,
                &writes,
                &snapshot_payload,
                owner_role,
                &organization_id,
                now,
                session,
            )
            .await?
        }
        PreparedExecution::Replay { .. } => None,
    };
    db.audit_logs().create(&audit, session).await?;
    Ok(first_task)
}

/// 将启动计划写入 BPM 集合、不可变快照和入口 WorkItem。
///
/// # 错误
/// 计划缺少入口执行或写入失败时返回错误。
async fn persist_runtime_writes(
    db: &Database,
    writes: &crate::approval::execution::apply_plan::PlannedWrites,
    snapshot_payload: &ApprovalSubjectSnapshotPayload,
    owner_role: &str,
    organization_id: &str,
    now: Instant,
    session: &mut mongodb::ClientSession,
) -> Result<Option<(String, u64)>> {
    let first = writes
        .created_executions
        .first()
        .ok_or_else(|| Error::Internal("启动计划缺少入口执行，不得提交采购单".to_string()))?;
    db.bpm_workflow()
        .create_bpm_runtime(
            &writes.instance,
            &writes.created_assignees,
            first,
            &writes.receipt,
            &list_projection_from_execution(first, now),
            session,
        )
        .await?;
    let snapshot = ApprovalSubjectSnapshot::new(
        ApprovalSubjectSnapshotId::new(next_id()),
        ApprovalProcessInstanceId::new(writes.instance.base.id.clone()),
        DocumentType::PurchaseOrder,
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
) -> Result<Option<(String, u64)>> {
    let mut first_task = None;
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
                business_object_type: DocumentType::PurchaseOrder.as_str().to_string(),
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
        if first_task.is_none() {
            first_task = Some((item.base.id.clone(), item.base.version));
        }
        db.work_items().create(&item, session).await?;
    }
    Ok(first_task)
}
