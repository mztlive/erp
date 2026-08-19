//! 销售单提交启动：加载定义图、构造 `prepare_start` 输入并持久化运行事实。
//!
//! `SalesOrder` 与 `VoucherSalesOrder` 共用本模块，按 `DocumentType` 分派
//! `ProcessKind`、快照与任务归属，不得回退 `CARD_SALES_APPROVAL`。

use bpm::engine::{DefinitionGraph, StartAssigneeBinding, TaskIntent};
use bpm::ids::{
    ApprovalCommandReceiptId, ApprovalInstanceAssigneeId, ApprovalNodeExecutionId, ApprovalProcessInstanceId,
};
use bpm::model::types::ApprovalCommandKind;
use bpm::model::{ApprovalNodeExecution, ParticipantId, SubjectRef, Timestamp};
use database::repository::bpm::ApprovalInstanceListProjection;
use database::{
    AccessControlExt, ApprovalIntegrationExt, BpmExt, DocumentRegistryExt, NoTransaction, SalesOrderExt,
    Transactional, WorkItemExt,
};
use entities::approval_integration::{ApprovalSubjectSnapshot, ApprovalSubjectSnapshotPayload};
use entities::common::time::Instant;
use entities::document_registry::DocumentType;
use entities::ids::{ApprovalSubjectSnapshotId, WorkItemId};
use entities::sales_order::SalesOrder;
use entities::work_item::DocumentApprovalWorkItemData;
use entities::work_item::{WorkItem, WorkItemPriority};
use id_generator::next_id;
use mongodb::Database;

use super::adapter::sales_order_object_readable;
use super::dto::SubmissionView;
use crate::approval::execution::authorization::{converge_eligibility, AuthorizationFailure};
use crate::approval::execution::idempotency::{normalize_idempotency_key, start_scope};
use crate::approval::execution::{ExecutionCommandInput, PreparedExecution, StartExecutionInput};
use crate::approval::process_kind::process_kind_of;
use crate::errors::{Error, Result};
use entities::document_registry::business_document::ApprovalDefinitionBinding;

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
    let graph = db
        .bpm_workflow()
        .load_definition_graph(&binding.approval_process_definition_id, &mut NoTransaction)
        .await?
        .ok_or_else(|| Error::ConflictError("销售单绑定的审批定义不存在".to_string()))?;
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
/// * `document_type` - 按业务性质分派的单据类型
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
    document_type: DocumentType,
    subject: &SubjectRef,
    subject_version: u32,
    idempotency_key: &str,
) -> Result<Option<bpm::model::ApprovalCommandReceipt>> {
    let key = normalize_idempotency_key(idempotency_key)?;
    let process_kind = process_kind_of(document_type);
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

/// 销售单启动输入。
///
/// # 用途
/// 收拢 `build_sales_order_start_input` 的定义图、绑定、单据类型与提交人参数。
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
/// 审批人取自已发布节点，不接受客户端选择；`document_type` 必须按业务性质分派。
pub(super) struct SalesOrderStartInput<'a> {
    /// 绑定定义图。
    pub graph: DefinitionGraph,
    /// 冻结绑定。
    pub binding: &'a ApprovalDefinitionBinding,
    /// 按业务性质分派的单据类型。
    pub document_type: DocumentType,
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
/// * `input` - 定义图、绑定、单据类型、主体与提交人
///
/// # 返回
/// 返回可交给 `prepare_start` 的输入。
///
/// # 错误
/// 入口缺失、审批人非法、幂等键非法或读取权校验失败时返回错误。
///
/// # 关键业务约束
/// 定义版本必须与冻结绑定一致；对象读取权失败时收敛为 BLOCKED。
pub(super) fn build_sales_order_start_input(input: SalesOrderStartInput<'_>) -> Result<StartExecutionInput> {
    let SalesOrderStartInput {
        graph,
        binding,
        document_type,
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
            "销售单绑定定义版本与已加载定义不一致".to_string(),
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
        process_kind: process_kind_of(document_type),
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
        let failure = match sales_order_object_readable(organization_id, assignee) {
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
            "审批定义没有节点，无法启动销售单审批".to_string(),
        ));
    }
    Ok(bindings)
}

/// 销售提交事务内需要一并写入的冻结提交。
///
/// # 用途
/// 收拢提交快照、启动计划、责任组织与审计，供同一事务写入。
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
pub(super) struct SalesOrderStartPersistInput {
    /// 已进入审批中的销售单。
    pub order: SalesOrder,
    /// 已锁定的工作副本。
    pub working_copy: entities::sales_order::SalesOrderWorkingCopy,
    /// 冻结提交头。
    pub submission: entities::sales_order::SalesOrderSubmission,
    /// 冻结提交行。
    pub submission_lines: Vec<entities::sales_order::SalesOrderSubmissionLine>,
    /// 工作流动作。
    pub workflow_action: entities::document_registry::WorkflowAction,
    /// 按业务性质分派的单据类型。
    pub document_type: DocumentType,
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

/// 在同一事务中写入提交快照、单据迁移、快照、BPM 运行事实与入口任务。
///
/// # 用途
/// 提交启动后原子写入销售单、提交快照与运行事实。
///
/// # 参数
/// * `db` - 数据库
/// * `input` - 提交写入集合
///
/// # 返回
/// 返回提交快照视图。
///
/// # 错误
/// 仓储写入失败或计划不完整时返回错误，事务回滚。
///
/// # 关键业务约束
/// Replay 不得重复写运行事实；Apply 必须写入快照与入口任务。
pub(super) async fn persist_sales_order_start(
    db: &Database,
    input: SalesOrderStartPersistInput,
) -> Result<SubmissionView> {
    let db = db.clone();
    let client = db.client().clone();
    let SalesOrderStartPersistInput {
        mut order,
        mut working_copy,
        submission,
        submission_lines,
        workflow_action,
        document_type,
        snapshot_payload,
        prepared,
        owner_role,
        organization_id,
        now,
        audit,
    } = input;
    let submission_view = super::mapper::submission_view(submission.clone(), submission_lines.clone());
    client
        .with_transaction(move |session| {
            Box::pin(async move {
                db.sales_order()
                    .submit_working_copy(&mut working_copy, &submission, &submission_lines, session)
                    .await?;
                db.sales_orders().update(&mut order, session).await?;
                db.workflow_actions().create(&workflow_action, session).await?;
                match prepared {
                    PreparedExecution::Apply(writes) => {
                        persist_runtime_writes(
                            &db,
                            &writes,
                            SalesOrderRuntimeWriteInput {
                                document_type,
                                snapshot_payload: &snapshot_payload,
                                owner_role,
                                organization_id: &organization_id,
                                now,
                            },
                            session,
                        )
                        .await?;
                    }
                    PreparedExecution::Replay { .. } => {}
                }
                db.audit_logs().create(&audit, session).await?;
                Ok::<(), crate::errors::Error>(())
            })
        })
        .await?;
    Ok(submission_view)
}

/// 销售单启动运行事实写入上下文。
///
/// # 用途
/// 收拢快照、单据类型、责任组织与时间，供 `persist_runtime_writes` 使用。
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
/// `document_type` 必须与提交分派一致，不得在此改写。
struct SalesOrderRuntimeWriteInput<'a> {
    /// 按业务性质分派的单据类型。
    document_type: DocumentType,
    /// 冻结快照载荷。
    snapshot_payload: &'a ApprovalSubjectSnapshotPayload,
    /// 责任角色。
    owner_role: &'a str,
    /// 责任组织。
    organization_id: &'a str,
    /// 调用方时间。
    now: Instant,
}

/// 将启动计划写入 BPM 集合、不可变快照和入口 WorkItem。
///
/// # 用途
/// 把启动计划落到 BPM 运行事实、不可变快照与入口任务。
///
/// # 参数
/// * `db` - 数据库
/// * `writes` - 启动写入集合
/// * `input` - 快照、单据类型与责任组织
/// * `session` - 当前事务
///
/// # 返回
/// 成功时无返回值。
///
/// # 错误
/// 计划缺少入口执行或写入失败时返回错误。
///
/// # 关键业务约束
/// 缺少入口执行时不得提交销售单。
async fn persist_runtime_writes(
    db: &Database,
    writes: &crate::approval::execution::apply_plan::PlannedWrites,
    input: SalesOrderRuntimeWriteInput<'_>,
    session: &mut mongodb::ClientSession,
) -> Result<()> {
    let SalesOrderRuntimeWriteInput {
        document_type,
        snapshot_payload,
        owner_role,
        organization_id,
        now,
    } = input;
    let first = writes
        .created_executions
        .first()
        .ok_or_else(|| Error::Internal("启动计划缺少入口执行，不得提交销售单".to_string()))?;
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
        document_type,
        writes.instance.subject.subject_id(),
        writes.instance.subject_version,
        snapshot_payload.clone(),
    )
    .map_err(|error| Error::ValidationError(error.to_string()))?;
    db.approval_subject_snapshots()
        .create_immutable_snapshot(&snapshot, session)
        .await?;
    persist_open_tasks(
        db,
        writes,
        document_type,
        owner_role,
        organization_id,
        now,
        session,
    )
    .await
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
/// # 参数
/// * `db` - 数据库
/// * `writes` - 启动写入
/// * `document_type` - 按业务性质分派的单据类型
/// * `owner_role` - 责任角色
/// * `organization_id` - 责任组织
/// * `now` - 创建时间
/// * `session` - 当前事务
///
/// # 错误
/// 责任人为空或仓储失败时返回错误。
async fn persist_open_tasks(
    db: &Database,
    writes: &crate::approval::execution::apply_plan::PlannedWrites,
    document_type: DocumentType,
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
                business_object_type: document_type.as_str().to_string(),
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
            node_name: "采购确认".into(),
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
