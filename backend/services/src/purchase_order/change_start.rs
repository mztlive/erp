//! 采购变更提交启动：加载定义图、构造 `prepare_start` 输入并持久化运行事实。

use bpm::engine::{plan_start, DefinitionGraph, StartBindingInput, StartPlanInput, TaskIntent};
use bpm::ids::{
    ApprovalCommandReceiptId, ApprovalInstanceAssigneeId, ApprovalNodeExecutionId, ApprovalProcessInstanceId,
};
use bpm::model::{ApprovalNodeExecution, ParticipantId, SubjectRef, Timestamp};
use database::repository::bpm::ApprovalInstanceListProjection;
use database::{
    AccessControlExt, ApprovalIntegrationExt, BpmExt, DocumentRegistryExt, Executor, NoTransaction,
    PurchaseOrderExt, Transactional, WorkItemExt,
};
use entities::approval_integration::{ApprovalSubjectSnapshot, ApprovalSubjectSnapshotPayload};
use entities::common::time::Instant;
use entities::document_registry::business_document::ApprovalDefinitionBinding;
use entities::document_registry::DocumentType;
use entities::ids::{ApprovalSubjectSnapshotId, WorkItemId};
use entities::purchase_order::{PurchaseChangeOrder, PurchaseChangeSubmission, PurchaseChangeSubmissionLine};
use entities::work_item::DocumentApprovalWorkItemData;
use entities::work_item::{WorkItem, WorkItemPriority};
use id_generator::next_id;
use mongodb::Database;

use super::change_adapter::purchase_change_order_object_readable;
use crate::approval::execution::authorization::{converge_eligibility, AuthorizationFailure};
use crate::approval::execution::idempotency::{
    normalize_idempotency_key, payload_conflict_error, start_identity, start_scope_candidates, ReceiptBranch,
};
use crate::approval::execution::start::map_engine_error;
use crate::approval::execution::{
    map_receipt_first_write_error, ExecutionCommandInput, PreparedExecution, StartExecutionInput,
};
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
    let graph = db
        .bpm_workflow()
        .load_definition_graph(&binding.approval_process_definition_id, &mut NoTransaction)
        .await?
        .ok_or_else(|| Error::ConflictError("采购变更单绑定的审批定义不存在".to_string()))?;
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
    let process_kind = process_kind_of(DocumentType::PurchaseChangeOrder);
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

/// 在 fresh 事务快照内按完整 V3/legacy 身份回读已提交的采购变更启动结果。
pub(super) async fn replay_purchase_change_start_with_executor(
    db: &Database,
    subject: &SubjectRef,
    subject_version: u32,
    idempotency_key: &str,
    binding: &ApprovalDefinitionBinding,
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<Option<String>> {
    let key = normalize_idempotency_key(idempotency_key)?;
    let process_kind = process_kind_of(DocumentType::PurchaseChangeOrder);
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
        .ok_or_else(|| Error::ConflictError("采购变更启动收据引用的审批实例不存在".to_string()))?;
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
            "采购变更启动收据与冻结运行事实不一致".to_string(),
        ));
    }
    Ok(Some(instance.base.id))
}

/// 采购变更启动输入。
///
/// # 用途
/// 收拢 `build_purchase_change_start_input` 的定义图、绑定与提交人参数。
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
pub(super) struct PurchaseChangeStartInput<'a> {
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
/// 计算逐节点对象读取授权结果并交给 BPM 通用启动计划，再把计划组装为引擎
/// `prepare_start` 输入。
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
/// 定义版本漂移、空节点、入口缺失与办理人校验由 BPM `plan_start` 失败关闭；
/// 对象读取权失败收敛为 BLOCKED 资格，由引擎启动校验统一拒绝。采购单与
/// 采购变更单必须复用同一 BPM 规则。
pub(super) fn build_purchase_change_start_input(
    input: PurchaseChangeStartInput<'_>,
) -> Result<StartExecutionInput> {
    let PurchaseChangeStartInput {
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
    let idempotency_key = normalize_idempotency_key(idempotency_key)?;
    let actor =
        ParticipantId::new(actor_id).map_err(|_| Error::ValidationError("提交人引用无效".to_string()))?;
    let timestamp = Timestamp::from_utc(now.as_utc());
    let binding_inputs = graph
        .nodes
        .iter()
        .map(|node| {
            let assignee = node.assignee_participant_id.as_str();
            let failure = match purchase_change_order_object_readable(organization_id, assignee) {
                Ok(true) => None,
                Ok(false) | Err(_) => Some(AuthorizationFailure::CannotReadSubject),
            };
            Ok(StartBindingInput {
                node_key: node.node_key.clone(),
                assignee_id: ApprovalInstanceAssigneeId::new(next_id()),
                eligibility: converge_eligibility(assignee, &node.assignee_label_snapshot, failure)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let plan = plan_start(StartPlanInput {
        graph: &graph,
        expected_definition_version: binding.approval_definition_version,
        bindings: binding_inputs,
    })
    .map_err(map_start_plan_error)?;
    Ok(StartExecutionInput {
        command: ExecutionCommandInput {
            graph,
            current_eligibility: plan.entry_eligibility.clone(),
            next_eligibility: plan.entry_eligibility,
            receipt,
            idempotency_key,
            now: timestamp,
        },
        process_kind: process_kind_of(DocumentType::PurchaseChangeOrder),
        subject,
        subject_version,
        binding_id: binding.approval_process_definition_id.as_ref().to_string(),
        definition_version: binding.approval_definition_version,
        actor,
        instance_id: ApprovalProcessInstanceId::new(next_id()),
        entry_execution_id: ApprovalNodeExecutionId::new(next_id()),
        receipt_id: ApprovalCommandReceiptId::new(next_id()),
        bindings: plan.bindings,
    })
}

/// 将启动计划错误映射为服务错误；计划前置条件保持冲突语义。
///
/// # 参数
/// * `error` - BPM 启动计划错误
///
/// # 返回
/// 计划前置条件返回冲突，其余错误按引擎错误映射。
fn map_start_plan_error(error: bpm::engine::EngineError) -> Error {
    match error {
        bpm::engine::EngineError::InvalidCommand(message) => Error::ConflictError(message.to_string()),
        other => map_engine_error(other),
    }
}

/// 采购变更提交事务内需要一并写入的冻结提交。
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
pub(super) struct PurchaseChangeStartPersistInput {
    /// 已进入审批中的变更单。
    pub change_order: PurchaseChangeOrder,
    /// 冻结提交头。
    pub submission: PurchaseChangeSubmission,
    /// 冻结提交行。
    pub submission_lines: Vec<PurchaseChangeSubmissionLine>,
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
/// 提交启动后原子写入变更单、提交快照与运行事实。
///
/// # 参数
/// * `db` - 数据库
/// * `input` - 提交写入集合
///
/// # 返回
/// 成功时无返回值。
///
/// # 错误
/// 仓储写入失败或计划不完整时返回错误，事务回滚。
///
/// # 关键业务约束
/// Replay 不得重复写运行事实；Apply 必须写入快照与入口任务。
pub(super) async fn persist_purchase_change_start(
    db: &Database,
    input: PurchaseChangeStartPersistInput,
) -> Result<()> {
    let db = db.clone();
    let client = db.client().clone();
    let PurchaseChangeStartPersistInput {
        mut change_order,
        submission,
        submission_lines,
        snapshot_payload,
        prepared,
        owner_role,
        organization_id,
        now,
        audit,
    } = input;
    let PreparedExecution::Apply(writes) = prepared else {
        return Ok(());
    };
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
                        DocumentType::PurchaseChangeOrder,
                        &writes.instance.process_definition_id,
                        writes.instance.definition_version,
                        now,
                        session,
                    )
                    .await?;
                if guarded.is_none() {
                    return Err(Error::ConflictError(
                        "采购变更单审批启动守卫冲突，请刷新后重试".to_string(),
                    ));
                }
                db.purchase_order()
                    .create_change_submission(&mut change_order, &submission, &submission_lines, session)
                    .await?;
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
                db.audit_logs().create(&audit, session).await?;
                Ok::<(), crate::errors::Error>(())
            })
        })
        .await
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
) -> Result<()> {
    let first = writes
        .created_executions
        .first()
        .ok_or_else(|| Error::Internal("启动计划缺少入口执行，不得提交采购变更单".to_string()))?;
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
        DocumentType::PurchaseChangeOrder,
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
                business_object_type: DocumentType::PurchaseChangeOrder.as_str().to_string(),
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
    use super::{build_purchase_change_start_input, PurchaseChangeStartInput};
    use bpm::graph::DefinitionGraph;
    use bpm::ids::{
        ApprovalCommandReceiptId, ApprovalNodeDefinitionId, ApprovalProcessDefinitionId,
        ApprovalTransitionDefinitionId,
    };
    use bpm::model::types::{ApprovalBlockerCode, ApprovalTransitionEvent};
    use bpm::model::{
        ApprovalNodeDefinition, ApprovalProcessDefinition, ApprovalTransitionDefinition, ParticipantId,
        ProcessKind, Timestamp,
    };
    use entities::common::time::Instant;
    use entities::document_registry::business_document::ApprovalDefinitionBinding;
    use entities::document_registry::DocumentType;

    use crate::approval::execution::idempotency::start_identity;
    use crate::approval::execution::{prepare_start, PreparedExecution};
    use crate::approval::process_kind::process_kind_of;
    use crate::errors::Error;

    fn node(
        id: &str,
        key: &str,
        name: &str,
        order: u32,
        user: &str,
        label: &str,
        at: Timestamp,
    ) -> ApprovalNodeDefinition {
        ApprovalNodeDefinition::new(bpm::model::NewNodeDefinition {
            id: ApprovalNodeDefinitionId::new(id),
            process_definition_id: ApprovalProcessDefinitionId::new("def"),
            node_key: key.into(),
            node_name: name.into(),
            node_purpose: None,
            display_order: order,
            assignee_participant_id: ParticipantId::new(user).unwrap(),
            assignee_label_snapshot: label.into(),
            at,
        })
        .unwrap()
    }

    fn two_node_graph() -> DefinitionGraph {
        let at = at(1);
        DefinitionGraph {
            definition: ApprovalProcessDefinition::new_draft(
                ApprovalProcessDefinitionId::new("def"),
                ProcessKind::PurchaseChangeOrder,
                1,
                "采购变更审批",
                "n1",
                ParticipantId::new("admin").unwrap(),
                at,
            )
            .unwrap(),
            nodes: vec![
                node("nd1", "n1", "采购确认", 1, "u1", "张三", at),
                node("nd2", "n2", "财务复核", 2, "u2", "李四", at),
            ],
            transitions: vec![
                ApprovalTransitionDefinition::to_node(
                    ApprovalTransitionDefinitionId::new("t1"),
                    ApprovalProcessDefinitionId::new("def"),
                    "n1",
                    ApprovalTransitionEvent::Approve,
                    "n2",
                    at,
                )
                .unwrap(),
                ApprovalTransitionDefinition::to_approved(
                    ApprovalTransitionDefinitionId::new("t2"),
                    ApprovalProcessDefinitionId::new("def"),
                    "n2",
                    ApprovalTransitionEvent::Approve,
                    at,
                )
                .unwrap(),
            ],
        }
    }

    fn empty_graph() -> DefinitionGraph {
        let at = at(1);
        DefinitionGraph {
            definition: ApprovalProcessDefinition::new_draft(
                ApprovalProcessDefinitionId::new("def"),
                ProcessKind::PurchaseChangeOrder,
                1,
                "采购变更审批",
                "n1",
                ParticipantId::new("admin").unwrap(),
                at,
            )
            .unwrap(),
            nodes: vec![],
            transitions: vec![],
        }
    }

    fn binding(definition_version: u32) -> ApprovalDefinitionBinding {
        ApprovalDefinitionBinding::new(
            bpm::ids::ApprovalProcessDefinitionId::new("def"),
            definition_version,
            Instant::from_unix_secs(1),
        )
        .unwrap()
    }

    fn at(secs: i64) -> Timestamp {
        Timestamp::from_unix_secs(secs).unwrap()
    }

    fn input<'a>(
        graph: DefinitionGraph,
        binding: &'a ApprovalDefinitionBinding,
        organization_id: &'a str,
    ) -> PurchaseChangeStartInput<'a> {
        PurchaseChangeStartInput {
            graph,
            binding,
            subject: bpm::model::SubjectRef::new("purchase_change_order", "co-1").unwrap(),
            subject_version: 1,
            actor_id: "starter",
            organization_id,
            idempotency_key: "key-1",
            receipt: None,
            now: Instant::from_unix_secs(10),
        }
    }

    /// 采购变更单启动输入复用 BPM 通用计划，与采购单规则一致。
    #[test]
    fn purchase_change_start_freezes_all_nodes_with_bpm_plan() {
        let graph = two_node_graph();
        let built = build_purchase_change_start_input(input(graph, &binding(1), "org-1")).unwrap();
        assert_eq!(built.bindings.len(), 2);
        assert_eq!(built.bindings[0].node_key, "n1");
        assert_eq!(built.bindings[0].participant.as_str(), "u1");
        assert_eq!(built.command.current_eligibility.participant().as_str(), "u1");
        assert_eq!(
            built.process_kind,
            process_kind_of(DocumentType::PurchaseChangeOrder)
        );
        assert_eq!(built.definition_version, 1);
    }

    /// 定义版本漂移时失败关闭。
    #[test]
    fn purchase_change_start_rejects_definition_version_drift() {
        let graph = two_node_graph();
        let error = build_purchase_change_start_input(input(graph, &binding(2), "org-1")).unwrap_err();
        assert!(
            matches!(error, Error::ConflictError(message) if message.contains("定义版本与冻结绑定不一致"))
        );
    }

    /// 空节点定义不得启动。
    #[test]
    fn purchase_change_start_rejects_empty_node_graph() {
        let error =
            build_purchase_change_start_input(input(empty_graph(), &binding(1), "org-1")).unwrap_err();
        assert!(matches!(error, Error::ConflictError(message) if message.contains("审批定义没有节点")));
    }

    /// 入口键缺失时失败关闭。
    #[test]
    fn purchase_change_start_rejects_missing_entry_node() {
        let mut graph = two_node_graph();
        graph.definition.entry_node_key = "missing".to_string();
        let error = build_purchase_change_start_input(input(graph, &binding(1), "org-1")).unwrap_err();
        assert!(matches!(error, Error::ConflictError(message) if message.contains("审批定义缺少入口节点")));
    }

    /// 对象读取失败收敛为 BLOCKED 资格，并由引擎启动校验统一拒绝。
    #[test]
    fn purchase_change_start_converges_read_failure_to_blocked() {
        let built = build_purchase_change_start_input(input(two_node_graph(), &binding(1), "")).unwrap();
        assert_eq!(
            built.command.current_eligibility.blocked_code(),
            Some(ApprovalBlockerCode::ApproverCannotReadSubject)
        );
        let error = prepare_start(built).unwrap_err();
        assert!(
            matches!(error, Error::ValidationError(message) if message.contains("启动时全部审批人必须有效"))
        );
    }

    /// 同键同载荷启动收据必须重放，不重复规划写入。
    #[test]
    fn purchase_change_start_replays_matching_receipt() {
        let identity = start_identity(
            bpm::model::IdempotencyKey::parse("key-1").unwrap(),
            process_kind_of(DocumentType::PurchaseChangeOrder).as_str(),
            "purchase_change_order",
            "co-1",
            1,
            "def",
            1,
            "starter",
        )
        .unwrap();
        let receipt = bpm::model::ApprovalCommandReceipt::new(
            ApprovalCommandReceiptId::new("r1"),
            identity.current(),
            "inst-1",
            at(10),
        )
        .unwrap();
        let mut built =
            build_purchase_change_start_input(input(two_node_graph(), &binding(1), "org-1")).unwrap();
        built.command.receipt = Some(receipt);
        assert!(matches!(
            prepare_start(built).unwrap(),
            PreparedExecution::Replay { .. }
        ));
    }
}
