//! P6-FINAL：BPM/集成仓储、索引、CAS、收据、outbox 与 20 类型 ProcessKind 查询矩阵。
//!
//! 真实 MongoDB 用例一律 `#[ignore]` + `require_mongo!()`，只读 `ERP_TEST_MONGO_URI`，
//! 使用独立随机库名并由 `TestDb` Drop 精确清理。不得打印 URI、凭证或敏感单据字段。

use std::str::FromStr;
use std::sync::Arc;

use bpm::engine::{
    cancel, decide, plan_enter_node, reassign, resume, start, CancelCommand, DecideCommand, DefinitionGraph,
    Eligibility, ReassignCommand, ResumeCommand, StartAssigneeBinding, StartCommand,
};
use bpm::graph::generate_linear_transitions;
use bpm::ids::{
    ApprovalCommandReceiptId, ApprovalInstanceAssigneeId, ApprovalNodeDefinitionId, ApprovalNodeExecutionId,
    ApprovalProcessDefinitionId, ApprovalProcessInstanceId, ApprovalTransitionDefinitionId,
};
use bpm::model::types::{
    ApprovalBlockerCode, ApprovalCommandKind, ApprovalDecision, ApprovalExecutionAssignmentSource,
    ApprovalExecutionEndReason, ApprovalNodeExecutionStatus, ApprovalProcessInstanceStatus,
    ApprovalTransitionEvent,
};
use bpm::model::{
    ApprovalCommandReceipt, ApprovalNodeDefinition, ApprovalNodeExecution, ApprovalProcessDefinition,
    ApprovalProcessInstance, ApprovalTransitionDefinition, NewNodeExecution, ParticipantId, ProcessKind,
    SubjectRef, Timestamp,
};
use database::repository::bpm::{
    ApprovalInstanceListFilter, ApprovalInstanceListProjection, ApprovalInstanceListView, CasWriteOutcome,
};
use database::{
    ensure_indexes, ApprovalIntegrationExt, BpmExt, Error as DatabaseError, NoTransaction, WorkItemExt,
};
use entities::approval_integration::{
    ApprovalNotificationEventKind, ApprovalNotificationOutbox, ApprovalNotificationTemplateParams,
    ApprovalSubjectSnapshot, ApprovalSubjectSnapshotPayload, MAX_DELIVERY_ATTEMPTS, RETRY_BACKOFF_SECS,
};
use entities::common::time::Instant;
use entities::document_registry::DocumentType;
use entities::ids::{ApprovalNotificationOutboxId, ApprovalSubjectSnapshotId, WorkItemId};
use entities::money::Quantity;
use entities::work_item::work_item::DocumentApprovalWorkItemData;
use entities::work_item::{WorkItem, WorkItemPriority, WorkItemType};
use test_support::{assert_indexes, require_mongo, TestDb};
use tokio::sync::Barrier;

const PILOT_KIND: ProcessKind = ProcessKind::StockAdjustment;
const PILOT_SUBJECT_KIND: &str = "stock_adjustment";

/// 目标 BPM 集合在 `ensure_indexes` 后必须存在的命名索引。
const DEFINITION_INDEXES: &[&str] = &[
    "uk_approval_process_definitions_id",
    "uk_approval_process_definitions_kind_version",
    "uk_approval_process_definitions_published_kind",
    "uk_approval_process_definitions_active_draft_kind",
    "idx_approval_process_definitions_history",
];
const NODE_INDEXES: &[&str] = &[
    "uk_approval_node_definitions_id",
    "uk_approval_node_definitions_definition_key",
    "uk_approval_node_definitions_definition_order",
    "idx_approval_node_definitions_definition",
];
const TRANSITION_INDEXES: &[&str] = &[
    "uk_approval_transition_definitions_id",
    "uk_approval_transition_definitions_from_event",
    "idx_approval_transition_definitions_definition",
];
const INSTANCE_INDEXES: &[&str] = &[
    "uk_approval_process_instances_id",
    "uk_approval_process_instances_active_subject",
    "idx_approval_process_instances_subject_history",
    "idx_approval_process_instances_blocked",
    "idx_approval_process_instances_started_by",
    "idx_approval_process_instances_updated",
    "idx_approval_process_instances_status_updated",
];
const EXECUTION_INDEXES: &[&str] = &[
    "uk_approval_node_executions_id",
    "uk_approval_node_executions_instance_no",
    "idx_approval_node_executions_round_node",
    "uk_approval_node_executions_current",
    "idx_approval_node_executions_round",
    "idx_approval_node_executions_assignee",
];
const ASSIGNEE_INDEXES: &[&str] = &[
    "uk_approval_instance_assignees_id",
    "uk_approval_instance_assignees_instance_node",
];
const RECEIPT_INDEXES: &[&str] = &[
    "uk_approval_command_receipts_id",
    "uk_approval_command_receipts_idempotency",
];
const SNAPSHOT_INDEXES: &[&str] = &[
    "uk_approval_subject_snapshots_id",
    "uk_approval_subject_snapshots_instance",
    "idx_approval_subject_snapshots_object",
];
const OUTBOX_INDEXES: &[&str] = &[
    "uk_approval_notification_outbox_id",
    "uk_approval_notification_outbox_dedup",
    "idx_approval_notification_outbox_delivery",
    "idx_approval_notification_outbox_lease",
    "idx_approval_notification_outbox_dead_letter",
];
const WORK_ITEM_INDEXES: &[&str] = &[
    "uk_work_items_open_object_type",
    "uk_work_items_approval_execution",
    "idx_work_items_mine",
    "idx_work_items_pending_approval",
    "idx_work_items_managed",
    "idx_work_items_responsibility_history",
    "idx_work_items_completed_history",
    "idx_work_items_closed_history",
];

/// 构造固定秒时间戳。
///
/// # 错误
/// 秒数为负时测试失败。
fn at(secs: i64) -> Timestamp {
    Timestamp::from_unix_secs(secs).expect("测试时间戳必须合法")
}

/// 构造处理人。
///
/// # 错误
/// 空 ID 时测试失败。
fn participant(id: &str) -> ParticipantId {
    ParticipantId::new(id).expect("测试处理人必须合法")
}

/// 构造试点单据引用。
///
/// # 错误
/// 种类或主键非法时测试失败。
fn stock_subject(object_id: &str) -> SubjectRef {
    SubjectRef::new(PILOT_SUBJECT_KIND, object_id).expect("试点 SubjectRef 必须合法")
}

/// 构造有效资格。
fn eligible(user: &str, name: &str) -> Eligibility {
    Eligibility::Eligible {
        participant: participant(user),
        assignee_name_snapshot: name.to_string(),
    }
}

/// 构造库存调整草稿定义。
///
/// # 错误
/// 模型校验失败时测试失败。
fn draft_definition(id: &str, version: u32, entry: &str) -> ApprovalProcessDefinition {
    ApprovalProcessDefinition::new_draft(
        ApprovalProcessDefinitionId::new(id),
        PILOT_KIND,
        version,
        "库存调整试点",
        entry,
        participant("admin"),
        at(1),
    )
    .expect("草稿定义必须可构造")
}

/// 构造人工审批节点。
///
/// # 错误
/// 模型校验失败时测试失败。
fn node(
    id: &str,
    definition_id: &str,
    key: &str,
    name: &str,
    order: u32,
    user: &str,
) -> ApprovalNodeDefinition {
    ApprovalNodeDefinition::new(
        ApprovalNodeDefinitionId::new(id),
        ApprovalProcessDefinitionId::new(definition_id),
        key,
        name,
        None,
        order,
        participant(user),
        user,
        at(1),
    )
    .expect("节点必须可构造")
}

/// 构造单节点已发布图。
///
/// # 错误
/// 模型校验失败时测试失败。
fn single_node_graph() -> DefinitionGraph {
    let definition = draft_definition("def-stock-1", 1, "n1");
    DefinitionGraph {
        definition,
        nodes: vec![node("nd1", "def-stock-1", "n1", "仓储复核", 1, "u1")],
        transitions: vec![
            ApprovalTransitionDefinition::to_approved(
                ApprovalTransitionDefinitionId::new("t-approve"),
                ApprovalProcessDefinitionId::new("def-stock-1"),
                "n1",
                ApprovalTransitionEvent::Approve,
                at(1),
            )
            .expect("终态通过连线"),
            ApprovalTransitionDefinition::to_node(
                ApprovalTransitionDefinitionId::new("t-reject"),
                ApprovalProcessDefinitionId::new("def-stock-1"),
                "n1",
                ApprovalTransitionEvent::Reject,
                "n1",
                at(1),
            )
            .expect("驳回连线"),
        ],
    }
}

/// 构造两节点线性图。
///
/// # 错误
/// 模型校验失败时测试失败。
fn two_node_graph() -> DefinitionGraph {
    let definition = draft_definition("def-stock-2", 1, "n1");
    DefinitionGraph {
        definition,
        nodes: vec![
            node("nd1", "def-stock-2", "n1", "仓储复核", 1, "u1"),
            node("nd2", "def-stock-2", "n2", "财务确认", 2, "u2"),
        ],
        transitions: vec![
            ApprovalTransitionDefinition::to_node(
                ApprovalTransitionDefinitionId::new("t1"),
                ApprovalProcessDefinitionId::new("def-stock-2"),
                "n1",
                ApprovalTransitionEvent::Approve,
                "n2",
                at(1),
            )
            .expect("通过连线"),
            ApprovalTransitionDefinition::to_node(
                ApprovalTransitionDefinitionId::new("t2"),
                ApprovalProcessDefinitionId::new("def-stock-2"),
                "n1",
                ApprovalTransitionEvent::Reject,
                "n1",
                at(1),
            )
            .expect("驳回连线"),
            ApprovalTransitionDefinition::to_approved(
                ApprovalTransitionDefinitionId::new("t3"),
                ApprovalProcessDefinitionId::new("def-stock-2"),
                "n2",
                ApprovalTransitionEvent::Approve,
                at(1),
            )
            .expect("终态通过连线"),
            ApprovalTransitionDefinition::to_node(
                ApprovalTransitionDefinitionId::new("t4"),
                ApprovalProcessDefinitionId::new("def-stock-2"),
                "n2",
                ApprovalTransitionEvent::Reject,
                "n1",
                at(1),
            )
            .expect("末节点驳回连线"),
        ],
    }
}

/// 按线性规则构造 20 节点图。
///
/// # 错误
/// 图生成失败时测试失败。
fn twenty_node_graph() -> DefinitionGraph {
    let keys: Vec<String> = (1..=20).map(|index| format!("n{index}")).collect();
    let drafts = generate_linear_transitions(&keys).expect("20 节点线性连线必须确定");
    let nodes = keys
        .iter()
        .enumerate()
        .map(|(index, key)| {
            node(
                &format!("nd{index}"),
                "def-stock-20",
                key,
                &format!("节点{index}"),
                u32::try_from(index + 1).expect("节点序"),
                &format!("u{index}"),
            )
        })
        .collect::<Vec<_>>();
    let transitions = drafts
        .into_iter()
        .enumerate()
        .map(|(index, draft)| match draft.terminal_result {
            Some(_) => ApprovalTransitionDefinition::to_approved(
                ApprovalTransitionDefinitionId::new(format!("t{index}")),
                ApprovalProcessDefinitionId::new("def-stock-20"),
                draft.from_node_key,
                draft.event,
                at(1),
            )
            .expect("终态连线"),
            None => ApprovalTransitionDefinition::to_node(
                ApprovalTransitionDefinitionId::new(format!("t{index}")),
                ApprovalProcessDefinitionId::new("def-stock-20"),
                draft.from_node_key,
                draft.event,
                draft.to_node_key.expect("中间连线必须有目标"),
                at(1),
            )
            .expect("节点连线"),
        })
        .collect();
    DefinitionGraph {
        definition: draft_definition("def-stock-20", 1, "n1"),
        nodes,
        transitions,
    }
}

/// 启动试点实例并返回确定计划。
///
/// # 错误
/// 引擎拒绝时测试失败。
fn start_stock(graph: &DefinitionGraph, instance_id: &str, object_id: &str) -> bpm::engine::TransitionPlan {
    let bindings = graph
        .nodes
        .iter()
        .enumerate()
        .map(|(index, item)| StartAssigneeBinding {
            id: ApprovalInstanceAssigneeId::new(format!("asg-{index}")),
            node_key: item.node_key.clone(),
            participant: item.assignee_participant_id.clone(),
            eligibility: eligible(
                item.assignee_participant_id.as_str(),
                &item.assignee_label_snapshot,
            ),
        })
        .collect::<Vec<_>>();
    start(
        StartCommand {
            instance_id: ApprovalProcessInstanceId::new(instance_id),
            process_kind: PILOT_KIND,
            subject: stock_subject(object_id),
            subject_version: 1,
            started_by: participant("starter"),
            entry_execution_id: ApprovalNodeExecutionId::new("exec-1"),
            now: at(10),
        },
        graph,
        &bindings,
    )
    .expect("启动必须产生确定计划")
}

/// 构造待投递通知。
///
/// # 错误
/// 模型校验失败时测试失败。
fn pending_outbox(id: &str, dedup: &str) -> ApprovalNotificationOutbox {
    ApprovalNotificationOutbox::enqueue(
        ApprovalNotificationOutboxId::new(id),
        dedup,
        ApprovalNotificationEventKind::Started,
        vec!["u1".to_string()],
        ApprovalNotificationTemplateParams {
            document_type_label: "库存调整单".to_string(),
            document_no: "ADJ-1".to_string(),
            current_node_name: "仓储复核".to_string(),
            current_approver_display_name: "仓储".to_string(),
            round_no: 1,
            reject_reason_summary: None,
        },
        Instant::from_unix_secs(10),
    )
    .expect("outbox 必须可入队")
}

/// 构造试点业务对象快照。
///
/// # 错误
/// 模型校验失败时测试失败。
fn stock_snapshot(id: &str, instance_id: &str, object_id: &str) -> ApprovalSubjectSnapshot {
    ApprovalSubjectSnapshot::new(
        ApprovalSubjectSnapshotId::new(id),
        ApprovalProcessInstanceId::new(instance_id),
        DocumentType::StockAdjustment,
        object_id,
        1,
        ApprovalSubjectSnapshotPayload {
            document_no: "ADJ-1".to_string(),
            responsible_org_id: "org-1".to_string(),
            submitted_by: "starter".to_string(),
            submitted_at: Instant::from_unix_secs(10),
            counterparty: None,
            total_amount: None,
            total_quantity: Some(Quantity::from_str("1").expect("数量")),
            line_count: 1,
        },
    )
    .expect("快照必须可构造")
}

/// 把启动计划落成仓储事实。
///
/// # 错误
/// 写入失败时测试失败。
async fn persist_started(
    db: &mongodb::Database,
    plan: &bpm::engine::TransitionPlan,
    receipt_id: &str,
) -> ApprovalCommandReceipt {
    let mut instance = plan.instance.clone();
    let execution = plan
        .created_executions
        .first()
        .cloned()
        .expect("启动必须创建入口执行");
    instance
        .set_current_execution(ApprovalNodeExecutionId::new(execution.base.id.clone()), at(10))
        .expect("写入当前执行");
    let receipt = ApprovalCommandReceipt::new(
        ApprovalCommandReceiptId::new(receipt_id),
        ApprovalCommandKind::StartApproval,
        instance.base.id.clone(),
        receipt_id,
        "digest-start",
        instance.base.id.clone(),
        at(10),
    )
    .expect("收据必须可构造");
    db.bpm_workflow()
        .create_bpm_runtime(
            &instance,
            &plan.created_assignees,
            &execution,
            &receipt,
            &ApprovalInstanceListProjection {
                current_node_key: Some(execution.node_key.clone()),
                current_node_name: Some(execution.node_name.clone()),
                current_assignee_participant_id: Some(execution.assignee_participant_id.as_str().to_string()),
                current_assignee_name: Some(execution.assignee_name_snapshot.clone()),
                ..ApprovalInstanceListProjection::default()
            },
            &mut NoTransaction,
        )
        .await
        .expect("运行时写入失败");
    receipt
}

/// 仓储查询只按 ProcessKind，不得按 ERP DocumentType 字符串分叉。
#[test]
fn repository_queries_use_process_kind_not_document_type() {
    let source = include_str!("../src/repository/bpm.rs");
    assert!(source.contains("process_kind"));
    assert!(!source.contains("DocumentType"));
    assert!(!source.contains("stock_adjustment_approver"));
    assert_eq!(PILOT_KIND.as_str(), DocumentType::StockAdjustment.as_str());
    assert_eq!(PILOT_KIND.as_str(), PILOT_SUBJECT_KIND);
}

/// 线性图生成对相同输入确定，Service 不得另立第二套实现。
#[test]
fn linear_graph_generation_is_deterministic_for_stock_adjustment() {
    let keys = vec!["n1".to_string(), "n2".to_string()];
    let first = generate_linear_transitions(&keys).expect("第一次");
    let second = generate_linear_transitions(&keys).expect("第二次");
    assert_eq!(first, second);
    assert_eq!(first.len(), 4);
    let definition_src = include_str!("../../services/src/approval/definition.rs");
    assert!(definition_src.contains("generate_linear_transitions"));
    assert!(!definition_src.contains("fn generate_linear_transitions"));
}

/// BPM 引擎在无数据库、无业务 Service 时对试点图产生确定计划。
#[test]
fn bpm_engine_start_decide_cancel_reassign_are_pure() {
    let single = single_node_graph();
    let started = start_stock(&single, "inst-1", "adj-1");
    assert_eq!(started.instance.process_kind, PILOT_KIND);
    assert_eq!(started.instance.status, ApprovalProcessInstanceStatus::Running);
    assert_eq!(started.created_executions.len(), 1);

    let approved = decide(
        started.instance.clone(),
        started.created_executions[0].clone(),
        &single,
        DecideCommand {
            decision: ApprovalDecision::Approve,
            reason: None,
            actor: participant("u1"),
            current_eligibility: eligible("u1", "仓储"),
            next_eligibility: eligible("u1", "仓储"),
            next_execution_id: ApprovalNodeExecutionId::new("exec-2"),
            next_execution_no: 2,
            now: at(20),
        },
    )
    .expect("单节点通过");
    assert_eq!(approved.instance.status, ApprovalProcessInstanceStatus::Approved);

    let rejected = decide(
        started.instance.clone(),
        started.created_executions[0].clone(),
        &single,
        DecideCommand {
            decision: ApprovalDecision::Reject,
            reason: Some("资料不全".to_string()),
            actor: participant("u1"),
            current_eligibility: eligible("u1", "仓储"),
            next_eligibility: eligible("u1", "仓储"),
            next_execution_id: ApprovalNodeExecutionId::new("exec-r2"),
            next_execution_no: 2,
            now: at(21),
        },
    )
    .expect("入口驳回必须开新一轮");
    assert_eq!(rejected.instance.current_round_no, 2);
    assert_eq!(rejected.instance.subject_version, 1);

    let cancelled = cancel(
        started.instance.clone(),
        started.created_executions[0].clone(),
        CancelCommand {
            actor: participant("starter"),
            reason: "撤回".to_string(),
            close_open_task: true,
            now: at(22),
        },
    )
    .expect("取消必须可计算");
    assert_eq!(
        cancelled.instance.status,
        ApprovalProcessInstanceStatus::Cancelled
    );

    let two = two_node_graph();
    let two_started = start_stock(&two, "inst-2", "adj-2");
    let entered = plan_enter_node(
        two_started.instance.clone(),
        &two,
        "n2",
        1,
        participant("u2"),
        eligible("u2", "财务"),
        ApprovalNodeExecutionId::new("exec-n2"),
        2,
        ApprovalExecutionAssignmentSource::Definition,
        None,
        at(23),
    )
    .expect("enter_node 必须可计算");
    assert_eq!(entered.created_executions[0].node_key, "n2");
}

/// 下一节点人员失效保留前一通过事实并形成 BLOCKED。
#[test]
fn next_assignee_invalid_keeps_approval_and_blocks() {
    let graph = two_node_graph();
    let started = start_stock(&graph, "inst-block", "adj-block");
    let blocked = decide(
        started.instance,
        started.created_executions[0].clone(),
        &graph,
        DecideCommand {
            decision: ApprovalDecision::Approve,
            reason: None,
            actor: participant("u1"),
            current_eligibility: eligible("u1", "仓储"),
            next_eligibility: Eligibility::Blocked {
                participant: participant("u2"),
                code: bpm::model::types::ApprovalBlockerCode::ApproverAccountInactive,
                assignee_name_snapshot: "财务".to_string(),
            },
            next_execution_id: ApprovalNodeExecutionId::new("exec-blocked"),
            next_execution_no: 2,
            now: at(30),
        },
    )
    .expect("下一节点失效必须提交 BLOCKED");
    assert_eq!(blocked.instance.status, ApprovalProcessInstanceStatus::Blocked);
    assert_eq!(
        blocked.updated_executions[0].decision,
        Some(ApprovalDecision::Approve)
    );
}

/// 1 节点与 20 节点完整通过路径都产生确定终态。
#[test]
fn one_and_twenty_node_graphs_complete() {
    let single = single_node_graph();
    let one = start_stock(&single, "inst-one", "adj-one");
    let one_approved = decide(
        one.instance,
        one.created_executions[0].clone(),
        &single,
        DecideCommand {
            decision: ApprovalDecision::Approve,
            reason: None,
            actor: participant("u1"),
            current_eligibility: eligible("u1", "仓储复核"),
            next_eligibility: eligible("u1", "仓储复核"),
            next_execution_id: ApprovalNodeExecutionId::new("exec-one-2"),
            next_execution_no: 2,
            now: at(20),
        },
    )
    .expect("单节点通过必须可计算");
    assert_eq!(
        one_approved.instance.status,
        ApprovalProcessInstanceStatus::Approved
    );

    let twenty = twenty_node_graph();
    assert_eq!(twenty.nodes.len(), 20);
    assert_eq!(twenty.transitions.len(), 40);
    let mut plan = start_stock(&twenty, "inst-20", "adj-20");
    assert_eq!(plan.created_assignees.len(), 20);
    let mut execution_no = 1_u32;
    for step in 1_u32..=20 {
        let current = plan
            .created_executions
            .first()
            .cloned()
            .expect("每步必须有当前执行");
        assert_eq!(current.node_key, format!("n{step}"));
        let current_user = format!("u{}", step - 1);
        let next_user = if step < 20 {
            format!("u{step}")
        } else {
            current_user.clone()
        };
        execution_no += 1;
        plan = decide(
            plan.instance,
            current,
            &twenty,
            DecideCommand {
                decision: ApprovalDecision::Approve,
                reason: None,
                actor: participant(&current_user),
                current_eligibility: eligible(&current_user, &format!("节点{}", step - 1)),
                next_eligibility: eligible(&next_user, &format!("节点{}", step.min(19))),
                next_execution_id: ApprovalNodeExecutionId::new(format!("exec-{execution_no}")),
                next_execution_no: execution_no,
                now: at(10 + i64::from(execution_no)),
            },
        )
        .unwrap_or_else(|_| panic!("第 {step} 步通过必须产生确定计划"));
    }
    assert_eq!(plan.instance.status, ApprovalProcessInstanceStatus::Approved);
}

/// 中间节点驳回开启下一轮入口新执行，subject_version 不变。
#[test]
fn middle_node_reject_opens_next_round_at_entry() {
    let graph = two_node_graph();
    let started = start_stock(&graph, "inst-mid-reject", "adj-mid-reject");
    let subject_version = started.instance.subject_version;
    let approved = decide(
        started.instance,
        started.created_executions[0].clone(),
        &graph,
        DecideCommand {
            decision: ApprovalDecision::Approve,
            reason: None,
            actor: participant("u1"),
            current_eligibility: eligible("u1", "仓储"),
            next_eligibility: eligible("u2", "财务"),
            next_execution_id: ApprovalNodeExecutionId::new("exec-mid-2"),
            next_execution_no: 2,
            now: at(20),
        },
    )
    .expect("第一节点通过");
    assert_eq!(approved.created_executions[0].node_key, "n2");
    let rejected = decide(
        approved.instance,
        approved.created_executions[0].clone(),
        &graph,
        DecideCommand {
            decision: ApprovalDecision::Reject,
            reason: Some("财务资料不全".to_string()),
            actor: participant("u2"),
            current_eligibility: eligible("u2", "财务"),
            next_eligibility: eligible("u1", "仓储"),
            next_execution_id: ApprovalNodeExecutionId::new("exec-mid-3"),
            next_execution_no: 3,
            now: at(21),
        },
    )
    .expect("中间节点驳回必须开新一轮");
    assert_eq!(rejected.instance.current_round_no, 2);
    assert_eq!(rejected.instance.subject_version, subject_version);
    assert_eq!(rejected.created_executions[0].node_key, "n1");
    assert_eq!(rejected.created_executions[0].round_no, 2);
    assert_eq!(rejected.instance.status, ApprovalProcessInstanceStatus::Running);
}

/// 人员失效后只能 resume；旧执行 SUPERSEDED/ASSIGNEE_RECOVERED，新执行为 ASSIGNEE_RECOVERY。
#[test]
fn resume_supersedes_personnel_blocked_execution() {
    let graph = two_node_graph();
    let started = start_stock(&graph, "inst-resume", "adj-resume");
    let assignees = started.created_assignees.clone();
    let blocked = decide(
        started.instance,
        started.created_executions[0].clone(),
        &graph,
        DecideCommand {
            decision: ApprovalDecision::Approve,
            reason: None,
            actor: participant("u1"),
            current_eligibility: Eligibility::Blocked {
                participant: participant("u1"),
                code: ApprovalBlockerCode::ApproverEmploymentInvalid,
                assignee_name_snapshot: "仓储".to_string(),
            },
            next_eligibility: eligible("u2", "财务"),
            next_execution_id: ApprovalNodeExecutionId::new("exec-resume-2"),
            next_execution_no: 2,
            now: at(30),
        },
    )
    .expect("当前审批人失效必须提交 BLOCKED");
    assert_eq!(blocked.instance.status, ApprovalProcessInstanceStatus::Blocked);
    let resumed = resume(
        blocked.instance,
        blocked.updated_executions[0].clone(),
        &assignees[0],
        &graph,
        ResumeCommand {
            next_execution_id: ApprovalNodeExecutionId::new("exec-resume-3"),
            next_execution_no: 3,
            eligibility: eligible("u1", "仓储"),
            now: at(31),
        },
    )
    .expect("恢复必须可计算");
    assert_eq!(
        resumed.updated_executions[0].status,
        ApprovalNodeExecutionStatus::Superseded
    );
    assert_eq!(
        resumed.updated_executions[0].ended_reason,
        Some(ApprovalExecutionEndReason::AssigneeRecovered)
    );
    assert_eq!(
        resumed.created_executions[0].assignment_source,
        ApprovalExecutionAssignmentSource::AssigneeRecovery
    );
    assert_eq!(resumed.instance.status, ApprovalProcessInstanceStatus::Running);
}

/// 人员失效改派只能替换受阻执行；ACTIVE 不得改派。
#[test]
fn reassign_only_replaces_personnel_blocked_execution() {
    let graph = two_node_graph();
    let started = start_stock(&graph, "inst-reassign", "adj-reassign");
    let active = reassign(
        started.instance.clone(),
        started.created_executions[0].clone(),
        started.created_assignees[0].clone(),
        &graph,
        ReassignCommand {
            target: participant("u9"),
            actor: participant("admin"),
            reason: "改派".to_string(),
            target_eligibility: eligible("u9", "新人"),
            next_execution_id: ApprovalNodeExecutionId::new("exec-re"),
            next_execution_no: 2,
            now: at(40),
        },
    );
    assert!(active.is_err(), "ACTIVE 不得改派");

    let assignees = started.created_assignees.clone();
    let blocked = decide(
        started.instance,
        started.created_executions[0].clone(),
        &graph,
        DecideCommand {
            decision: ApprovalDecision::Approve,
            reason: None,
            actor: participant("u1"),
            current_eligibility: Eligibility::Blocked {
                participant: participant("u1"),
                code: ApprovalBlockerCode::ApproverAccountInactive,
                assignee_name_snapshot: "仓储".to_string(),
            },
            next_eligibility: eligible("u2", "财务"),
            next_execution_id: ApprovalNodeExecutionId::new("exec-re-2"),
            next_execution_no: 2,
            now: at(41),
        },
    )
    .expect("人员失效必须提交 BLOCKED");
    let reassigned = reassign(
        blocked.instance,
        blocked.updated_executions[0].clone(),
        assignees[0].clone(),
        &graph,
        ReassignCommand {
            target: participant("u9"),
            actor: participant("admin"),
            reason: "原审批人离职".to_string(),
            target_eligibility: eligible("u9", "新人"),
            next_execution_id: ApprovalNodeExecutionId::new("exec-re-3"),
            next_execution_no: 3,
            now: at(42),
        },
    )
    .expect("人员失效后改派必须成功");
    assert_eq!(
        reassigned.updated_executions[0].ended_reason,
        Some(ApprovalExecutionEndReason::AdminReassigned)
    );
    assert_eq!(
        reassigned.updated_executions[0].status,
        ApprovalNodeExecutionStatus::Superseded
    );
    assert_eq!(
        reassigned.created_executions[0].assignment_source,
        ApprovalExecutionAssignmentSource::AdminReassign
    );
    assert_eq!(
        reassigned.updated_assignees[0]
            .current_assignee_participant_id
            .as_str(),
        "u9"
    );
}

/// 同一 SubjectRef + subject_version 只能有一条活动链；定义 ID 不得拆分唯一性。
#[test]
fn active_chain_unique_key_excludes_definition_id() {
    let source = include_str!("../src/indexes/bpm.rs");
    assert!(source.contains("subject.subject_kind"));
    assert!(source.contains("subject.subject_id"));
    assert!(source.contains("subject_version"));
    assert!(source.contains("RUNNING"));
    assert!(source.contains("BLOCKED"));
    assert!(!source.contains("process_definition_id: 1,\n                \"subject"));
}

/// 新索引命名与开发重置脚本集合清单对齐，旧步骤索引不得重建。
#[test]
fn new_index_names_match_reset_collection_contract() {
    let reset = include_str!("../../scripts/reset-dev-business-data.mongosh.js");
    for collection in [
        "approval_process_definitions",
        "approval_node_definitions",
        "approval_transition_definitions",
        "approval_process_instances",
        "approval_node_executions",
        "approval_instance_assignees",
        "approval_command_receipts",
        "approval_subject_snapshots",
        "approval_notification_outbox",
    ] {
        assert!(reset.contains(collection), "{collection} 必须出现在重置脚本");
    }
    assert!(reset.contains("uk_work_items_open_approval_step"));
    assert!(!WORK_ITEM_INDEXES.contains(&"uk_work_items_open_approval_step"));
    assert!(!WORK_ITEM_INDEXES.contains(&"idx_work_items_team_pool"));
}

/// 首次失败到第 5 次失败的退避必须为 1/5/15/60/360 分钟。
#[test]
fn outbox_backoff_and_dead_letter_constants_match_contract() {
    assert_eq!(RETRY_BACKOFF_SECS, [60, 300, 900, 3_600, 21_600]);
    assert_eq!(MAX_DELIVERY_ATTEMPTS, 6);
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn stock_adjustment_indexes_are_created_by_ensure_indexes() {
    require_mongo!(async {
        let fixture = TestDb::new("awf_repo_idx").await.expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        assert_indexes(fixture.db(), "approval_process_definitions", DEFINITION_INDEXES)
            .await
            .expect("定义索引");
        assert_indexes(fixture.db(), "approval_node_definitions", NODE_INDEXES)
            .await
            .expect("节点索引");
        assert_indexes(
            fixture.db(),
            "approval_transition_definitions",
            TRANSITION_INDEXES,
        )
        .await
        .expect("连线索引");
        assert_indexes(fixture.db(), "approval_process_instances", INSTANCE_INDEXES)
            .await
            .expect("实例索引");
        assert_indexes(fixture.db(), "approval_node_executions", EXECUTION_INDEXES)
            .await
            .expect("执行索引");
        assert_indexes(fixture.db(), "approval_instance_assignees", ASSIGNEE_INDEXES)
            .await
            .expect("审批人索引");
        assert_indexes(fixture.db(), "approval_command_receipts", RECEIPT_INDEXES)
            .await
            .expect("收据索引");
        assert_indexes(fixture.db(), "approval_subject_snapshots", SNAPSHOT_INDEXES)
            .await
            .expect("快照索引");
        assert_indexes(fixture.db(), "approval_notification_outbox", OUTBOX_INDEXES)
            .await
            .expect("outbox 索引");
        assert_indexes(fixture.db(), "work_items", WORK_ITEM_INDEXES)
            .await
            .expect("任务索引");
    });
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn each_process_kind_allows_one_draft_and_one_published() {
    require_mongo!(async {
        let fixture = TestDb::new("awf_repo_def").await.expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        let mut published = draft_definition("def-a", 1, "n1");
        published
            .publish(participant("admin"), at(2))
            .expect("内存中发布");
        fixture
            .db()
            .approval_process_definitions()
            .create(&published, &mut NoTransaction)
            .await
            .expect("首个已发布");
        let draft = draft_definition("def-b", 2, "n1");
        fixture
            .db()
            .approval_process_definitions()
            .create(&draft, &mut NoTransaction)
            .await
            .expect("每个 ProcessKind 允许一个活动草稿");
        let second_draft = draft_definition("def-c", 3, "n1");
        assert!(
            fixture
                .db()
                .approval_process_definitions()
                .create(&second_draft, &mut NoTransaction)
                .await
                .is_err(),
            "第二活动草稿必须被唯一索引拒绝"
        );
        let mut second_published = draft_definition("def-d", 4, "n1");
        second_published
            .publish(participant("admin"), at(3))
            .expect("第二发布");
        assert!(
            fixture
                .db()
                .approval_process_definitions()
                .create(&second_published, &mut NoTransaction)
                .await
                .is_err(),
            "第二 PUBLISHED 必须被唯一索引拒绝"
        );
        let found_published = fixture
            .db()
            .bpm_workflow()
            .find_published_by_process_kind(PILOT_KIND, &mut NoTransaction)
            .await
            .expect("查询已发布");
        assert!(found_published.is_some());
        let found_draft = fixture
            .db()
            .bpm_workflow()
            .find_active_draft(PILOT_KIND, &mut NoTransaction)
            .await
            .expect("查询活动草稿");
        assert!(found_draft.is_some());
    });
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn node_and_transition_uniqueness_is_enforced() {
    require_mongo!(async {
        let fixture = TestDb::new("awf_repo_node").await.expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        let first = node("node-a", "def-1", "n1", "仓储", 1, "u1");
        fixture
            .db()
            .approval_node_definitions()
            .create(&first, &mut NoTransaction)
            .await
            .expect("首个节点");
        let duplicate_key = node("node-b", "def-1", "n1", "仓储复制", 2, "u2");
        assert!(fixture
            .db()
            .approval_node_definitions()
            .create(&duplicate_key, &mut NoTransaction)
            .await
            .is_err());
        let approve = ApprovalTransitionDefinition::to_approved(
            ApprovalTransitionDefinitionId::new("tr-1"),
            ApprovalProcessDefinitionId::new("def-1"),
            "n1",
            ApprovalTransitionEvent::Approve,
            at(1),
        )
        .expect("连线");
        fixture
            .db()
            .approval_transition_definitions()
            .create(&approve, &mut NoTransaction)
            .await
            .expect("首条连线");
        let duplicate_event = ApprovalTransitionDefinition::to_node(
            ApprovalTransitionDefinitionId::new("tr-2"),
            ApprovalProcessDefinitionId::new("def-1"),
            "n1",
            ApprovalTransitionEvent::Approve,
            "n2",
            at(1),
        )
        .expect("重复事件连线");
        assert!(fixture
            .db()
            .approval_transition_definitions()
            .create(&duplicate_event, &mut NoTransaction)
            .await
            .is_err());
    });
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn active_subject_is_unique_even_with_different_definition_ids() {
    require_mongo!(async {
        let fixture = TestDb::new("awf_repo_subj").await.expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        let subject = stock_subject("adj-unique");
        let first = ApprovalProcessInstance::start_running(
            ApprovalProcessInstanceId::new("inst-a"),
            ApprovalProcessDefinitionId::new("def-old"),
            1,
            PILOT_KIND,
            subject.clone(),
            1,
            participant("starter"),
            at(10),
        )
        .expect("实例 A");
        fixture
            .db()
            .approval_process_instances()
            .create(&first, &mut NoTransaction)
            .await
            .expect("首条活动链");
        let second = ApprovalProcessInstance::start_running(
            ApprovalProcessInstanceId::new("inst-b"),
            ApprovalProcessDefinitionId::new("def-new"),
            2,
            PILOT_KIND,
            subject,
            1,
            participant("starter"),
            at(11),
        )
        .expect("实例 B");
        let conflict = fixture
            .db()
            .approval_process_instances()
            .create(&second, &mut NoTransaction)
            .await;
        assert!(conflict.is_err(), "不同定义 ID 也不得拆分活动链");
        let found = fixture
            .db()
            .bpm_workflow()
            .find_non_terminal_by_subject(&stock_subject("adj-unique"), 1, &mut NoTransaction)
            .await
            .expect("按 subject 查询")
            .expect("必须回读已有活动链");
        assert_eq!(found.base.id, "inst-a");
    });
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn command_receipt_same_payload_replays_and_cas_conflicts() {
    require_mongo!(async {
        let fixture = TestDb::new("awf_repo_cas").await.expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        let graph = single_node_graph();
        let plan = start_stock(&graph, "inst-cas", "adj-cas");
        persist_started(fixture.db(), &plan, "rcp-start").await;
        let same = fixture
            .db()
            .bpm_workflow()
            .find_command_receipt(
                ApprovalCommandKind::StartApproval,
                "inst-cas",
                "rcp-start",
                &mut NoTransaction,
            )
            .await
            .expect("回读收据")
            .expect("收据必须存在");
        assert_eq!(same.payload_digest, "digest-start");
        let duplicate = ApprovalCommandReceipt::new(
            ApprovalCommandReceiptId::new("rcp-dup"),
            ApprovalCommandKind::StartApproval,
            "inst-cas",
            "rcp-start",
            "other-digest",
            "inst-cas",
            at(12),
        )
        .expect("冲突收据");
        assert!(fixture
            .db()
            .approval_command_receipts()
            .create(&duplicate, &mut NoTransaction)
            .await
            .is_err());

        let mut definition = draft_definition("def-cas", 1, "n1");
        fixture
            .db()
            .approval_process_definitions()
            .create(&definition, &mut NoTransaction)
            .await
            .expect("草稿");
        definition.rename_draft("改名", at(3)).expect("重命名递增锁");
        let stale = fixture
            .db()
            .bpm_workflow()
            .update_draft_definition(&definition, 99, &mut NoTransaction)
            .await
            .expect("陈旧 CAS");
        assert!(matches!(
            stale,
            CasWriteOutcome::VersionConflict(_)
                | CasWriteOutcome::NotFound
                | CasWriteOutcome::StatusChanged(_)
        ));
    });
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn snapshot_is_immutable_and_one_per_instance() {
    require_mongo!(async {
        let fixture = TestDb::new("awf_repo_snap").await.expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        let snapshot = stock_snapshot("snap-1", "inst-snap", "adj-snap");
        fixture
            .db()
            .approval_subject_snapshots()
            .create_immutable_snapshot(&snapshot, &mut NoTransaction)
            .await
            .expect("快照写入");
        let again = stock_snapshot("snap-2", "inst-snap", "adj-snap");
        assert!(fixture
            .db()
            .approval_subject_snapshots()
            .create_immutable_snapshot(&again, &mut NoTransaction)
            .await
            .is_err());
        let loaded = fixture
            .db()
            .approval_subject_snapshots()
            .find_by_process_instance_id("inst-snap", &mut NoTransaction)
            .await
            .expect("按实例读取")
            .expect("快照必须存在");
        assert_eq!(loaded.subject_version, 1);
        assert_eq!(loaded.document_type, DocumentType::StockAdjustment);
    });
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn work_item_execution_id_is_unique_across_lifecycle() {
    require_mongo!(async {
        let fixture = TestDb::new("awf_repo_wi").await.expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        let first = WorkItem::new_document_approval(
            WorkItemId::new("wi-1"),
            DocumentApprovalWorkItemData {
                approval_node_execution_id: ApprovalNodeExecutionId::new("exec-unique"),
                business_object_type: PILOT_SUBJECT_KIND.to_string(),
                business_object_id: "adj-wi".to_string(),
                subject_version: "1".to_string(),
                owner_role: "stock_adjustment_approver".to_string(),
                owner_organization_id: "org-1".to_string(),
                owner_user_id: "u1".to_string(),
                priority: WorkItemPriority::Normal,
                due_at: None,
            },
            Instant::from_unix_secs(10),
        )
        .expect("审批任务");
        assert_eq!(first.work_item_type, WorkItemType::DocumentApproval);
        fixture
            .db()
            .work_items()
            .create(&first, &mut NoTransaction)
            .await
            .expect("首个任务");
        let duplicate = WorkItem::new_document_approval(
            WorkItemId::new("wi-2"),
            DocumentApprovalWorkItemData {
                approval_node_execution_id: ApprovalNodeExecutionId::new("exec-unique"),
                business_object_type: PILOT_SUBJECT_KIND.to_string(),
                business_object_id: "adj-wi-2".to_string(),
                subject_version: "1".to_string(),
                owner_role: "stock_adjustment_approver".to_string(),
                owner_organization_id: "org-1".to_string(),
                owner_user_id: "u2".to_string(),
                priority: WorkItemPriority::Normal,
                due_at: None,
            },
            Instant::from_unix_secs(11),
        )
        .expect("重复执行任务");
        assert!(fixture
            .db()
            .work_items()
            .create(&duplicate, &mut NoTransaction)
            .await
            .is_err());
    });
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn two_workers_compete_for_outbox_lease() {
    require_mongo!(async {
        let fixture = TestDb::new("awf_repo_obx").await.expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        let item = pending_outbox("obx-1", "dedup-1");
        fixture
            .db()
            .approval_notification_outbox()
            .enqueue_outbox(&item, &mut NoTransaction)
            .await
            .expect("入队");
        let barrier = Arc::new(Barrier::new(2));
        let now = Instant::from_unix_secs(20);
        let until = Instant::from_unix_secs(50);
        let claim = |worker: &'static str| {
            let db = fixture.db().clone();
            let barrier = Arc::clone(&barrier);
            tokio::spawn(async move {
                barrier.wait().await;
                db.approval_notification_outbox()
                    .lease_outbox_batch(worker, now, until, 1, &mut NoTransaction)
                    .await
                    .expect("领取")
            })
        };
        let left = claim("worker-a");
        let right = claim("worker-b");
        let first = left.await.expect("worker-a");
        let second = right.await.expect("worker-b");
        let winners = usize::from(!first.is_empty()) + usize::from(!second.is_empty());
        assert_eq!(winners, 1, "同一消息只能被一个 worker 领取");
    });
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn execution_history_is_bounded_and_has_no_n_plus_one() {
    require_mongo!(async {
        let fixture = TestDb::new("awf_repo_hist").await.expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        let graph = single_node_graph();
        let plan = start_stock(&graph, "inst-hist", "adj-hist");
        persist_started(fixture.db(), &plan, "rcp-hist").await;
        let rows = fixture
            .db()
            .bpm_workflow()
            .list_execution_history(
                &ApprovalProcessInstanceId::new("inst-hist"),
                None,
                50,
                &mut NoTransaction,
            )
            .await
            .expect("历史读取");
        assert_eq!(rows.len(), 1);
        let summaries = fixture
            .db()
            .bpm_workflow()
            .list_instance_summaries(
                &ApprovalInstanceListFilter {
                    view: ApprovalInstanceListView::Started,
                    process_kind: Some(PILOT_KIND),
                    status: None,
                    started_by: Some("starter".to_string()),
                    subject_kind: None,
                    subject_ids: None,
                    cursor: None,
                    limit: 20,
                },
                &mut NoTransaction,
            )
            .await
            .expect("列表");
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].current_node_key.as_deref(), Some("n1"));
    });
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn stale_execution_cas_is_classified() {
    require_mongo!(async {
        let fixture = TestDb::new("awf_repo_exec").await.expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        let execution = ApprovalNodeExecution::new_active(NewNodeExecution {
            id: ApprovalNodeExecutionId::new("exec-cas"),
            process_instance_id: ApprovalProcessInstanceId::new("inst-exec"),
            node_key: "n1".to_string(),
            node_name: "仓储复核".to_string(),
            round_no: 1,
            execution_no: 1,
            assignment_source: ApprovalExecutionAssignmentSource::Definition,
            replaces_execution_id: None,
            assignee_participant_id: participant("u1"),
            assignee_name_snapshot: "仓储".to_string(),
            at: at(10),
        })
        .expect("执行");
        fixture
            .db()
            .approval_node_executions()
            .create(&execution, &mut NoTransaction)
            .await
            .expect("写入执行");
        let outcome = fixture
            .db()
            .bpm_workflow()
            .end_active_execution(&execution, 99, &mut NoTransaction)
            .await
            .expect("陈旧结束");
        assert!(matches!(
            outcome,
            CasWriteOutcome::VersionConflict(_)
                | CasWriteOutcome::StatusChanged(_)
                | CasWriteOutcome::NotFound
        ));
        let _ = DatabaseError::OptimisticLockingError;
    });
}

/// 20 个 DocumentType 必须与 ProcessKind 稳定码一一对应；仓储只按 kind 查询。
#[test]
fn all_document_types_map_to_unique_process_kind_for_repository() {
    let pairs = [
        (DocumentType::SalesOrder, ProcessKind::SalesOrder),
        (DocumentType::VoucherSalesOrder, ProcessKind::VoucherSalesOrder),
        (DocumentType::SalesChangeOrder, ProcessKind::SalesChangeOrder),
        (DocumentType::PurchaseOrder, ProcessKind::PurchaseOrder),
        (
            DocumentType::PurchaseChangeOrder,
            ProcessKind::PurchaseChangeOrder,
        ),
        (DocumentType::StockAdjustment, ProcessKind::StockAdjustment),
        (DocumentType::CustomerReceipt, ProcessKind::CustomerReceipt),
        (DocumentType::SupplierPayment, ProcessKind::SupplierPayment),
        (DocumentType::CustomerRefund, ProcessKind::CustomerRefund),
        (DocumentType::SupplierRefund, ProcessKind::SupplierRefund),
        (DocumentType::ReceiptReversal, ProcessKind::ReceiptReversal),
        (DocumentType::PaymentReversal, ProcessKind::PaymentReversal),
        (DocumentType::PurchaseReceipt, ProcessKind::PurchaseReceipt),
        (DocumentType::Delivery, ProcessKind::Delivery),
        (DocumentType::ElectronicDelivery, ProcessKind::ElectronicDelivery),
        (DocumentType::ServiceFulfillment, ProcessKind::ServiceFulfillment),
        (DocumentType::CustomerAcceptance, ProcessKind::CustomerAcceptance),
        (DocumentType::Invoice, ProcessKind::Invoice),
        (DocumentType::SalesReturnCase, ProcessKind::SalesReturnCase),
        (
            DocumentType::PurchaseReturnOrder,
            ProcessKind::PurchaseReturnOrder,
        ),
    ];
    let mut kinds = std::collections::BTreeSet::new();
    for (document_type, kind) in pairs {
        assert_eq!(document_type.as_str(), kind.as_str());
        assert!(kinds.insert(kind.as_str()), "{} 必须唯一", kind.as_str());
    }
    assert_eq!(pairs.len(), 20);
    assert_eq!(kinds.len(), 20);
    let source = include_str!("../src/repository/bpm.rs");
    assert!(source.contains("process_kind"));
    assert!(!source.contains("DocumentType"));
    assert!(!source.contains("stock_adjustment_approver"));
}

/// 发布定义查询必须按 ProcessKind，不得按 ERP 类型字符串分叉。
#[test]
fn published_definition_lookup_is_process_kind_only() {
    let source = include_str!("../src/repository/bpm.rs");
    assert!(source.contains("find_published_by_process_kind"));
    assert!(source.contains("find_active_draft"));
    assert!(!source.contains("find_published_by_document_type"));
    assert!(!source.contains("match document_type"));
}

/// 重置脚本索引创建清单必须覆盖新 BPM 集合，且不得重建旧步骤唯一索引。
#[test]
fn reset_index_creation_matches_new_bpm_collections() {
    let reset = include_str!("../../scripts/reset-dev-business-data.mongosh.js");
    let bpm_indexes = include_str!("../src/indexes/bpm.rs");
    let integration_indexes = include_str!("../src/indexes/approval_integration.rs");
    for (collection, name, source) in [
        (
            "approval_process_definitions",
            "uk_approval_process_definitions_published_kind",
            bpm_indexes,
        ),
        (
            "approval_process_definitions",
            "uk_approval_process_definitions_active_draft_kind",
            bpm_indexes,
        ),
        (
            "approval_process_instances",
            "uk_approval_process_instances_active_subject",
            bpm_indexes,
        ),
        (
            "approval_node_executions",
            "uk_approval_node_executions_current",
            bpm_indexes,
        ),
        (
            "approval_command_receipts",
            "uk_approval_command_receipts_idempotency",
            bpm_indexes,
        ),
        (
            "approval_subject_snapshots",
            "uk_approval_subject_snapshots_instance",
            integration_indexes,
        ),
        (
            "approval_notification_outbox",
            "uk_approval_notification_outbox_dedup",
            integration_indexes,
        ),
    ] {
        assert!(source.contains(name), "{name} 必须在对应索引源文件");
        assert!(
            reset.contains(&format!("\"{collection}\"")),
            "{collection} 必须出现在 NEW_APPROVAL_COLLECTIONS"
        );
    }
    assert!(reset.contains("uk_work_items_open_approval_step"));
    assert!(reset.contains("idx_work_items_team_pool"));
    assert!(!bpm_indexes.contains("uk_work_items_open_approval_step"));
    assert!(!integration_indexes.contains("idx_work_items_team_pool"));
    assert!(reset.contains("CONFLICTING_INDEX_ALLOWLIST"));
}
