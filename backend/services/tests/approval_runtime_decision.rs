//! APP-S03：审批最终决定授权、收据优先与并发回放的真实 MongoDB 验收。
//!
//! 用例使用随机独立库、真实 Casbin Mongo Adapter 和公开
//! `ApprovalRuntimeService::submit_decision` 端口。仅当 `ERP_TEST_MONGO_URI`
//! 指向 MongoDB 7 副本集时通过 `--include-ignored` 执行。

use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use bpm::engine::{start, DefinitionGraph, Eligibility, StartAssigneeBinding, StartCommand};
use bpm::ids::{
    ApprovalCommandReceiptId, ApprovalInstanceAssigneeId, ApprovalNodeDefinitionId, ApprovalNodeExecutionId,
    ApprovalProcessDefinitionId, ApprovalProcessInstanceId, ApprovalTransitionDefinitionId,
};
use bpm::model::types::{
    ApprovalAssigneeBindingSource, ApprovalBlockerCode, ApprovalCommandKind, ApprovalNodeExecutionStatus,
    ApprovalProcessInstanceStatus, ApprovalTransitionEvent,
};
use bpm::model::{
    ApprovalCommandReceipt, ApprovalNodeDefinition, ApprovalProcessDefinition, ApprovalTransitionDefinition,
    NewNodeDefinition, ParticipantId, ProcessKind, SubjectRef, Timestamp,
};
use casbin::Adapter;
use database::repository::bpm::ApprovalInstanceListProjection;
use database::{
    ensure_indexes, AccessControlExt, ApprovalIntegrationExt, BpmExt, Executor, MongoCasbinAdapter,
    NoTransaction, WorkItemExt,
};
use entities::access_control::{DataScope, DataScopeData, DataScopeSubjectType, DataScopeType};
use entities::approval_integration::{
    ApprovalNotificationEventKind, ApprovalNotificationOutbox, ApprovalNotificationTemplateParams,
    ApprovalSubjectSnapshot, ApprovalSubjectSnapshotPayload,
};
use entities::common::time::Instant;
use entities::document_registry::DocumentType;
use entities::ids::{ApprovalNotificationOutboxId, ApprovalSubjectSnapshotId, DataScopeId, WorkItemId};
use entities::money::Quantity;
use entities::work_item::{DocumentApprovalWorkItemData, WorkItem, WorkItemPriority, WorkItemStatus};
use entities::{
    AccountCore, AccountCoreData, AccountKind, AccountStatus, LoginAccount, Role, RoleData, RoleUpdate,
    Secret,
};
use mongodb::bson::{doc, Document};
use mongodb::Database;
use services::approval::execution::{ApprovalCommandOutcome, ApprovalRuntimeService};
use services::approval::policy::ApprovalDomainAction;
use services::approval::{
    ApprovalActionContext, ApprovalActionFuture, ApprovalCancelBlockedCommand, ApprovalDomainActionPort,
    ApprovalResumeCommand,
};
use services::audit::AuditActor;
use services::iam::{self, subject};
use services::{Error, ErrorCode};
use sha2::{Digest, Sha256};
use test_support::{require_mongo, TestDb};
use tokio::sync::Notify;

const SUBMITTER: &str = "decision-submitter";
const APPROVER: &str = "decision-approver";
const RUNTIME_ADMIN: &str = "decision-runtime-admin";
const WRONG_ORG_ADMIN: &str = "decision-wrong-org-admin";
const APPROVER_ROLE: &str = "decision-approver-role";
const APPROVER_DECIDE_ONLY_ROLE: &str = "decision-decide-only";
const RUNTIME_ADMIN_ROLE: &str = "decision-runtime-admin-role";
const WRONG_ORG_ADMIN_ROLE: &str = "decision-wrong-org-admin-role";
const ORG: &str = "decision-org";
const WRONG_ORG: &str = "decision-other-org";
const DEFINITION_ID: &str = "decision-definition";
const NODE_KEY: &str = "review";
const NODE_NAME: &str = "库存调整复核";
const APPROVER_NAME: &str = "库存审批人";
const ACTION_MARKERS: &str = "test_approval_decision_actions";

/// 并发测试在首个最终动作中暂停，使另一请求确实与未提交收据竞争。
#[derive(Default)]
struct DecisionGate {
    armed: AtomicBool,
    entered: Notify,
    release: Notify,
}

impl DecisionGate {
    fn armed() -> Self {
        Self {
            armed: AtomicBool::new(true),
            entered: Notify::new(),
            release: Notify::new(),
        }
    }
}

/// 仅在调用方事务会话中写入唯一动作标记，用于证明回放不重做、败者整体回滚。
struct MarkerActionPort {
    db: Database,
    gate: Option<Arc<DecisionGate>>,
}

impl ApprovalDomainActionPort for MarkerActionPort {
    fn execute<'a>(
        &'a self,
        action: ApprovalDomainAction,
        context: &'a ApprovalActionContext,
        actor: &'a AuditActor,
        executor: &'a mut dyn Executor,
    ) -> ApprovalActionFuture<'a> {
        Box::pin(async move {
            if !matches!(
                action,
                ApprovalDomainAction::StockAdjustmentPost
                    | ApprovalDomainAction::StockAdjustmentCancelApproval
            ) || context.business_object_type != DocumentType::StockAdjustment.as_str()
                || context.actor_id != actor.id()
            {
                return Err(Error::ConflictError("最终动作上下文不匹配".to_string()));
            }
            if let Some(gate) = &self.gate {
                if gate
                    .armed
                    .compare_exchange(true, false, Ordering::SeqCst, Ordering::SeqCst)
                    .is_ok()
                {
                    gate.entered.notify_one();
                    gate.release.notified().await;
                }
            }
            let session = executor
                .session()
                .ok_or_else(|| Error::Internal("测试最终动作缺少事务会话".to_string()))?;
            self.db
                .collection::<Document>(ACTION_MARKERS)
                .insert_one(doc! {
                    "_id": &context.approval_process_instance_id,
                    "instance_id": &context.approval_process_instance_id,
                    "execution_id": &context.approval_node_execution_id,
                    "work_item_id": &context.work_item_id,
                    "actor_id": actor.id(),
                    "idempotency_key": &context.idempotency_key,
                })
                .session(session)
                .await
                .map_err(database::Error::from)
                .map_err(Error::from)?;
            Ok(())
        })
    }
}

struct DecisionFixture {
    _test_db: TestDb,
    db: Database,
    service: Arc<ApprovalRuntimeService>,
    gate: Option<Arc<DecisionGate>>,
}

fn at(seconds: i64) -> Timestamp {
    Timestamp::from_unix_secs(seconds).expect("测试时间戳必须合法")
}

fn participant(id: &str) -> ParticipantId {
    ParticipantId::new(id).expect("测试参与人必须合法")
}

fn actor(id: &str) -> AuditActor {
    AuditActor::new(id.to_string(), format!("login-{id}"), AccountKind::Admin)
}

fn account(id: &str) -> AccountCore {
    AccountCore::new(
        id.to_string(),
        AccountCoreData {
            secret: Secret::new(
                LoginAccount::new(format!("login-{id}")).expect("测试登录账号"),
                "test-only-password",
            )
            .expect("测试凭证"),
            name: id.to_string(),
            kind: AccountKind::Admin,
            status: AccountStatus::Active,
            email: None,
            phone: None,
            avatar: None,
        },
    )
    .expect("测试账号")
}

fn organization_scope(id: &str, role_id: &str, organization_id: &str) -> DataScope {
    DataScope::new(
        DataScopeId::new(id),
        DataScopeData {
            subject_type: DataScopeSubjectType::Role,
            subject_id: role_id.to_string(),
            scope_type: DataScopeType::Organization,
            scope_targets: vec![organization_id.to_string()],
        },
    )
    .expect("角色组织范围")
}

async fn seed_authorization(db: &Database) {
    for account_id in [SUBMITTER, APPROVER, RUNTIME_ADMIN, WRONG_ORG_ADMIN] {
        db.accounts()
            .create(&account(account_id), &mut NoTransaction)
            .await
            .expect("写入审批决定账号");
    }
    for role_id in [
        APPROVER_ROLE,
        APPROVER_DECIDE_ONLY_ROLE,
        RUNTIME_ADMIN_ROLE,
        WRONG_ORG_ADMIN_ROLE,
    ] {
        let role = Role::new(
            role_id.to_string(),
            RoleData {
                name: role_id.to_string(),
                description: None,
                system: false,
            },
        )
        .expect("角色 fixture");
        db.roles()
            .create(&role, &mut NoTransaction)
            .await
            .expect("写入审批决定角色");
    }
    let mut adapter = MongoCasbinAdapter::new(db.clone());
    assert!(adapter
        .add_policies(
            "g",
            "g",
            vec![
                vec![
                    subject(AccountKind::Admin, APPROVER),
                    format!("role:{APPROVER_ROLE}")
                ],
                vec![
                    subject(AccountKind::Admin, APPROVER),
                    format!("role:{APPROVER_DECIDE_ONLY_ROLE}")
                ],
                vec![
                    subject(AccountKind::Admin, RUNTIME_ADMIN),
                    format!("role:{RUNTIME_ADMIN_ROLE}"),
                ],
                vec![
                    subject(AccountKind::Admin, WRONG_ORG_ADMIN),
                    format!("role:{WRONG_ORG_ADMIN_ROLE}"),
                ],
            ],
        )
        .await
        .expect("写入角色绑定"));
    assert!(adapter
        .add_policies(
            "p",
            "p",
            vec![
                vec![
                    format!("role:{APPROVER_ROLE}"),
                    "approval_instance".to_string(),
                    "decide".to_string(),
                ],
                vec![
                    format!("role:{APPROVER_ROLE}"),
                    "stock_adjustment".to_string(),
                    "detail".to_string(),
                ],
                vec![
                    format!("role:{APPROVER_DECIDE_ONLY_ROLE}"),
                    "approval_instance".to_string(),
                    "decide".to_string(),
                ],
                vec![
                    format!("role:{RUNTIME_ADMIN_ROLE}"),
                    "stock_adjustment".to_string(),
                    "approval_runtime_admin".to_string(),
                ],
                vec![
                    format!("role:{RUNTIME_ADMIN_ROLE}"),
                    "stock_adjustment".to_string(),
                    "detail".to_string(),
                ],
                vec![
                    format!("role:{RUNTIME_ADMIN_ROLE}"),
                    "approval_instance".to_string(),
                    "cancel_blocked".to_string(),
                ],
                vec![
                    format!("role:{RUNTIME_ADMIN_ROLE}"),
                    "approval_instance".to_string(),
                    "resume".to_string(),
                ],
                vec![
                    format!("role:{WRONG_ORG_ADMIN_ROLE}"),
                    "stock_adjustment".to_string(),
                    "approval_runtime_admin".to_string(),
                ],
                vec![
                    format!("role:{WRONG_ORG_ADMIN_ROLE}"),
                    "stock_adjustment".to_string(),
                    "detail".to_string(),
                ],
                vec![
                    format!("role:{WRONG_ORG_ADMIN_ROLE}"),
                    "approval_instance".to_string(),
                    "cancel_blocked".to_string(),
                ],
            ],
        )
        .await
        .expect("写入决定与运行管理权限"));
    for scope in [
        organization_scope("scope-decision-approver", APPROVER_ROLE, ORG),
        organization_scope(
            "scope-decision-approver-decide-only",
            APPROVER_DECIDE_ONLY_ROLE,
            ORG,
        ),
        organization_scope("scope-decision-runtime-admin", RUNTIME_ADMIN_ROLE, ORG),
        organization_scope("scope-decision-wrong-org-admin", WRONG_ORG_ADMIN_ROLE, WRONG_ORG),
    ] {
        db.data_scopes()
            .create(&scope, &mut NoTransaction)
            .await
            .expect("写入决定 DataScope");
    }
}

fn single_node_graph() -> DefinitionGraph {
    let mut definition = ApprovalProcessDefinition::new_draft(
        ApprovalProcessDefinitionId::new(DEFINITION_ID),
        ProcessKind::StockAdjustment,
        1,
        "库存调整决定测试",
        NODE_KEY,
        participant("definition-admin"),
        at(1),
    )
    .expect("草稿定义");
    definition
        .publish(participant("definition-admin"), at(2))
        .expect("发布定义");
    let node = ApprovalNodeDefinition::new(NewNodeDefinition {
        id: ApprovalNodeDefinitionId::new("decision-node"),
        process_definition_id: ApprovalProcessDefinitionId::new(DEFINITION_ID),
        node_key: NODE_KEY.to_string(),
        node_name: NODE_NAME.to_string(),
        node_purpose: None,
        display_order: 1,
        assignee_participant_id: participant(APPROVER),
        assignee_label_snapshot: APPROVER_NAME.to_string(),
        at: at(1),
    })
    .expect("单节点定义");
    let approve = ApprovalTransitionDefinition::to_approved(
        ApprovalTransitionDefinitionId::new("decision-approve"),
        ApprovalProcessDefinitionId::new(DEFINITION_ID),
        NODE_KEY,
        ApprovalTransitionEvent::Approve,
        at(1),
    )
    .expect("通过终点连线");
    let reject = ApprovalTransitionDefinition::to_node(
        ApprovalTransitionDefinitionId::new("decision-reject"),
        ApprovalProcessDefinitionId::new(DEFINITION_ID),
        NODE_KEY,
        ApprovalTransitionEvent::Reject,
        NODE_KEY,
        at(1),
    )
    .expect("驳回入口连线");
    DefinitionGraph {
        definition,
        nodes: vec![node],
        transitions: vec![approve, reject],
    }
}

async fn persist_graph(db: &Database, graph: &DefinitionGraph) {
    db.approval_process_definitions()
        .create(&graph.definition, &mut NoTransaction)
        .await
        .expect("写入决定定义");
    for node in &graph.nodes {
        db.approval_node_definitions()
            .create(node, &mut NoTransaction)
            .await
            .expect("写入决定节点");
    }
    for transition in &graph.transitions {
        db.approval_transition_definitions()
            .create(transition, &mut NoTransaction)
            .await
            .expect("写入决定连线");
    }
}

fn instance_id(name: &str) -> String {
    format!("decision-instance-{name}")
}

fn execution_id(name: &str) -> String {
    format!("decision-execution-{name}")
}

fn work_item_id(name: &str) -> String {
    format!("decision-work-item-{name}")
}

fn object_id(name: &str) -> String {
    format!("decision-adjustment-{name}")
}

fn document_no(name: &str) -> String {
    format!("ADJ-{name}")
}

async fn seed_runtime(db: &Database, graph: &DefinitionGraph, name: &str) {
    let instance = instance_id(name);
    let execution = execution_id(name);
    let object = object_id(name);
    let plan = start(
        StartCommand {
            instance_id: ApprovalProcessInstanceId::new(&instance),
            process_kind: ProcessKind::StockAdjustment,
            subject: SubjectRef::new(DocumentType::StockAdjustment.as_str(), &object).expect("库存调整主体"),
            subject_version: 1,
            started_by: participant(SUBMITTER),
            entry_execution_id: ApprovalNodeExecutionId::new(&execution),
            now: at(10),
        },
        graph,
        &[StartAssigneeBinding {
            id: ApprovalInstanceAssigneeId::new(format!("decision-assignee-{name}")),
            node_key: NODE_KEY.to_string(),
            participant: participant(APPROVER),
            eligibility: Eligibility::Eligible {
                participant: participant(APPROVER),
                assignee_name_snapshot: APPROVER_NAME.to_string(),
            },
        }],
    )
    .expect("启动运行计划");
    let current = plan.created_executions.first().expect("启动计划必须创建当前执行");
    let start_receipt = ApprovalCommandReceipt::new(
        ApprovalCommandReceiptId::new(format!("decision-start-receipt-{name}")),
        ApprovalCommandKind::StartApproval,
        &instance,
        format!("decision-start-key-{name}"),
        format!("decision-start-digest-{name}"),
        &instance,
        at(10),
    )
    .expect("启动收据");
    db.bpm_workflow()
        .create_bpm_runtime(
            &plan.instance,
            &plan.created_assignees,
            current,
            &start_receipt,
            &ApprovalInstanceListProjection {
                current_node_key: Some(current.node_key.clone()),
                current_node_name: Some(current.node_name.clone()),
                current_assignee_participant_id: Some(APPROVER.to_string()),
                current_assignee_name: Some(APPROVER_NAME.to_string()),
                last_status_changed_at: Some(10),
                ..ApprovalInstanceListProjection::default()
            },
            &mut NoTransaction,
        )
        .await
        .expect("写入决定运行事实");
    let snapshot = ApprovalSubjectSnapshot::new(
        ApprovalSubjectSnapshotId::new(format!("decision-snapshot-{name}")),
        ApprovalProcessInstanceId::new(&instance),
        DocumentType::StockAdjustment,
        &object,
        1,
        ApprovalSubjectSnapshotPayload {
            document_no: document_no(name),
            responsible_org_id: ORG.to_string(),
            submitted_by: SUBMITTER.to_string(),
            submitted_at: Instant::from_unix_secs(10),
            counterparty: None,
            total_amount: None,
            total_quantity: Some(Quantity::from_str("1").expect("测试数量")),
            line_count: 1,
        },
    )
    .expect("冻结快照");
    db.approval_subject_snapshots()
        .create_immutable_snapshot(&snapshot, &mut NoTransaction)
        .await
        .expect("写入冻结快照");
    let work_item = WorkItem::new_document_approval(
        WorkItemId::new(work_item_id(name)),
        DocumentApprovalWorkItemData {
            approval_node_execution_id: ApprovalNodeExecutionId::new(&execution),
            business_object_type: DocumentType::StockAdjustment.as_str().to_string(),
            business_object_id: object,
            subject_version: "1".to_string(),
            owner_role: "stock_adjustment_approver".to_string(),
            owner_organization_id: ORG.to_string(),
            owner_user_id: APPROVER.to_string(),
            priority: WorkItemPriority::Normal,
            due_at: None,
        },
        Instant::from_unix_secs(10),
    )
    .expect("审批任务");
    db.work_items()
        .create(&work_item, &mut NoTransaction)
        .await
        .expect("写入审批任务");
}

/// 写入无开放任务的结构性受阻实例，仅允许运行管理员走受阻取消。
async fn seed_blocked_runtime(db: &Database, graph: &DefinitionGraph, name: &str) {
    let instance = instance_id(name);
    let execution = execution_id(name);
    let object = object_id(name);
    let plan = start(
        StartCommand {
            instance_id: ApprovalProcessInstanceId::new(&instance),
            process_kind: ProcessKind::StockAdjustment,
            subject: SubjectRef::new(DocumentType::StockAdjustment.as_str(), &object)
                .expect("受阻库存调整主体"),
            subject_version: 1,
            started_by: participant(SUBMITTER),
            entry_execution_id: ApprovalNodeExecutionId::new(&execution),
            now: at(10),
        },
        graph,
        &[StartAssigneeBinding {
            id: ApprovalInstanceAssigneeId::new(format!("decision-assignee-{name}")),
            node_key: NODE_KEY.to_string(),
            participant: participant(APPROVER),
            eligibility: Eligibility::Eligible {
                participant: participant(APPROVER),
                assignee_name_snapshot: APPROVER_NAME.to_string(),
            },
        }],
    )
    .expect("受阻启动计划");
    let mut runtime_instance = plan.instance;
    let assignees = plan.created_assignees;
    let mut current = plan
        .created_executions
        .into_iter()
        .next()
        .expect("受阻 fixture 必须有当前执行");
    current
        .block(ApprovalBlockerCode::DefinitionGraphCorrupted, at(11))
        .expect("阻塞当前执行");
    runtime_instance
        .enter_blocked(ApprovalBlockerCode::DefinitionGraphCorrupted, at(11))
        .expect("阻塞实例");
    let start_receipt = ApprovalCommandReceipt::new(
        ApprovalCommandReceiptId::new(format!("decision-start-receipt-{name}")),
        ApprovalCommandKind::StartApproval,
        &instance,
        format!("decision-start-key-{name}"),
        format!("decision-start-digest-{name}"),
        &instance,
        at(10),
    )
    .expect("受阻启动收据");
    db.bpm_workflow()
        .create_bpm_runtime(
            &runtime_instance,
            &assignees,
            &current,
            &start_receipt,
            &ApprovalInstanceListProjection {
                current_node_key: Some(current.node_key.clone()),
                current_node_name: Some(current.node_name.clone()),
                current_assignee_participant_id: Some(APPROVER.to_string()),
                current_assignee_name: Some(APPROVER_NAME.to_string()),
                last_status_changed_at: Some(11),
                ..ApprovalInstanceListProjection::default()
            },
            &mut NoTransaction,
        )
        .await
        .expect("写入受阻运行事实");
    let snapshot = ApprovalSubjectSnapshot::new(
        ApprovalSubjectSnapshotId::new(format!("decision-snapshot-{name}")),
        ApprovalProcessInstanceId::new(&instance),
        DocumentType::StockAdjustment,
        &object,
        1,
        ApprovalSubjectSnapshotPayload {
            document_no: document_no(name),
            responsible_org_id: ORG.to_string(),
            submitted_by: SUBMITTER.to_string(),
            submitted_at: Instant::from_unix_secs(10),
            counterparty: None,
            total_amount: None,
            total_quantity: Some(Quantity::from_str("1").expect("受阻测试数量")),
            line_count: 1,
        },
    )
    .expect("受阻冻结快照");
    db.approval_subject_snapshots()
        .create_immutable_snapshot(&snapshot, &mut NoTransaction)
        .await
        .expect("写入受阻冻结快照");
}

async fn fixture(test_name: &str, runtime_names: &[&str], gated: bool) -> DecisionFixture {
    let test_db = TestDb::new(test_name).await.expect("创建决定测试数据库");
    ensure_indexes(test_db.db()).await.expect("创建索引");
    seed_authorization(test_db.db()).await;
    let graph = single_node_graph();
    persist_graph(test_db.db(), &graph).await;
    for name in runtime_names {
        seed_runtime(test_db.db(), &graph, name).await;
    }
    let db = test_db.db().clone();
    let gate = gated.then(|| Arc::new(DecisionGate::armed()));
    let port = Arc::new(MarkerActionPort {
        db: db.clone(),
        gate: gate.clone(),
    });
    let service = Arc::new(ApprovalRuntimeService::with_action_port(
        db.clone(),
        iam::shared_rbac_service(db.clone()),
        port,
    ));
    DecisionFixture {
        _test_db: test_db,
        db,
        service,
        gate,
    }
}

async fn task_version(db: &Database, name: &str) -> u64 {
    db.work_items()
        .find_document_approval_by_id(&work_item_id(name), &mut NoTransaction)
        .await
        .expect("读取审批任务")
        .expect("审批任务必须存在")
        .base
        .version
}

async fn action_count(db: &Database, name: &str) -> u64 {
    db.collection::<Document>(ACTION_MARKERS)
        .count_documents(doc! { "instance_id": instance_id(name) })
        .await
        .expect("统计最终动作")
}

async fn decision_receipt_count(db: &Database, name: &str) -> u64 {
    db.collection::<Document>("approval_command_receipts")
        .count_documents(doc! { "scope_id": execution_id(name) })
        .await
        .expect("统计决定收据")
}

async fn receipt_digest(db: &Database, scope_id: &str, command_kind: &str) -> String {
    db.collection::<Document>("approval_command_receipts")
        .find_one(doc! { "scope_id": scope_id, "command_kind": command_kind })
        .await
        .expect("读取命令收据")
        .expect("命令收据必须存在")
        .get_str("payload_digest")
        .expect("命令收据摘要必须为字符串")
        .to_string()
}

async fn replace_receipt_digest(db: &Database, scope_id: &str, command_kind: &str, digest: &str) {
    let result = db
        .collection::<Document>("approval_command_receipts")
        .update_one(
            doc! { "scope_id": scope_id, "command_kind": command_kind },
            doc! { "$set": { "payload_digest": digest } },
        )
        .await
        .expect("替换历史命令摘要");
    assert_eq!(result.matched_count, 1, "必须精确命中一条命令收据");
}

fn legacy_payload_digest(fields: &[&str]) -> String {
    let canonical = fields
        .iter()
        .map(|field| {
            let trimmed = field.trim();
            if trimmed.is_empty() {
                "NULL"
            } else {
                trimmed
            }
        })
        .collect::<Vec<_>>()
        .join("\u{1f}");
    hex::encode(Sha256::digest(canonical.as_bytes()))
}

fn legacy_decision_digest(
    work_item_id: &str,
    decision: &str,
    reason: Option<&str>,
    expected_task_version: u64,
    actor_id: &str,
) -> String {
    legacy_payload_digest(&[
        work_item_id,
        decision,
        reason.unwrap_or(""),
        &expected_task_version.to_string(),
        actor_id,
    ])
}

fn legacy_cancel_blocked_digest(
    blocker: &str,
    expected_instance_version: u64,
    expected_execution_version: u64,
    expected_task_version: Option<u64>,
    reason: &str,
    actor_id: &str,
) -> String {
    let task_version = expected_task_version
        .map(|value| value.to_string())
        .unwrap_or_default();
    legacy_payload_digest(&[
        blocker,
        &expected_instance_version.to_string(),
        &expected_execution_version.to_string(),
        &task_version,
        reason,
        actor_id,
    ])
}

async fn decision_audit_count(db: &Database, name: &str) -> u64 {
    db.collection::<Document>("audit_logs")
        .count_documents(doc! {
            "action": "approval.decide",
            "resource_id": instance_id(name),
        })
        .await
        .expect("统计决定审计")
}

async fn blocked_cancel_command(
    db: &Database,
    name: &str,
    actor_id: &str,
    key: &str,
    reason: &str,
) -> ApprovalCancelBlockedCommand {
    let instance = db
        .bpm_workflow()
        .find_instance_by_id(
            &ApprovalProcessInstanceId::new(instance_id(name)),
            &mut NoTransaction,
        )
        .await
        .expect("读取受阻取消实例")
        .expect("受阻取消实例必须存在");
    let execution = db
        .bpm_workflow()
        .find_execution_by_id(
            instance
                .current_node_execution_id
                .as_ref()
                .expect("受阻实例必须持有当前执行"),
            &mut NoTransaction,
        )
        .await
        .expect("读取受阻取消执行")
        .expect("受阻取消执行必须存在");
    ApprovalCancelBlockedCommand {
        approval_process_instance_id: instance.base.id,
        expected_instance_version: instance.base.version,
        expected_execution_version: execution.base.version,
        expected_task_version: None,
        reason: reason.to_string(),
        idempotency_key: key.to_string(),
        actor_id: actor_id.to_string(),
    }
}

async fn resume_command(db: &Database, name: &str, key: &str) -> ApprovalResumeCommand {
    let instance = db
        .bpm_workflow()
        .find_instance_by_id(
            &ApprovalProcessInstanceId::new(instance_id(name)),
            &mut NoTransaction,
        )
        .await
        .expect("读取人员受阻实例")
        .expect("人员受阻实例必须存在");
    let execution_id = instance
        .current_node_execution_id
        .as_ref()
        .expect("人员受阻实例必须持有当前执行")
        .clone();
    let execution = db
        .bpm_workflow()
        .find_execution_by_id(&execution_id, &mut NoTransaction)
        .await
        .expect("读取人员受阻执行")
        .expect("人员受阻执行必须存在");
    let assignee = db
        .bpm_workflow()
        .find_assignee_for_node(
            &ApprovalProcessInstanceId::new(&instance.base.id),
            &execution.node_key,
            &mut NoTransaction,
        )
        .await
        .expect("读取人员受阻审批人绑定")
        .expect("人员受阻审批人绑定必须存在");
    let tasks = db
        .work_items()
        .approval_tasks_for_execution(&execution_id, &mut NoTransaction)
        .await
        .expect("读取人员受阻历史任务");
    let expected_closed_task_version = match tasks.as_slice() {
        [task] if task.status == WorkItemStatus::Closed => Some(task.base.version),
        [] => None,
        _ => panic!("人员受阻执行必须至多关联一个已关闭任务"),
    };
    ApprovalResumeCommand {
        approval_process_instance_id: instance.base.id,
        expected_instance_version: instance.base.version,
        expected_execution_version: execution.base.version,
        expected_assignment_version: assignee.base.version,
        expected_closed_task_version,
        idempotency_key: key.to_string(),
        actor_id: RUNTIME_ADMIN.to_string(),
    }
}

async fn resume_receipt_count(db: &Database, name: &str) -> u64 {
    db.collection::<Document>("approval_command_receipts")
        .count_documents(doc! {
            "scope_id": instance_id(name),
            "command_kind": "RESUME_APPROVER",
        })
        .await
        .expect("统计恢复收据")
}

async fn resume_audit_count(db: &Database, name: &str) -> u64 {
    db.collection::<Document>("audit_logs")
        .count_documents(doc! {
            "action": "approval.resume_current_approver",
            "resource_id": instance_id(name),
        })
        .await
        .expect("统计恢复审计")
}

async fn assert_resume_failure_preserves_blocked_runtime(db: &Database, name: &str) {
    let instance_id = ApprovalProcessInstanceId::new(instance_id(name));
    let expected_execution_id = ApprovalNodeExecutionId::new(execution_id(name));
    let instance = db
        .bpm_workflow()
        .find_instance_by_id(&instance_id, &mut NoTransaction)
        .await
        .expect("读取恢复失败后的实例")
        .expect("恢复失败后实例必须存在");
    assert_eq!(instance.status, ApprovalProcessInstanceStatus::Blocked);
    assert_eq!(
        instance.current_node_execution_id.as_ref(),
        Some(&expected_execution_id),
        "恢复失败不得替换实例当前执行",
    );

    let execution = db
        .bpm_workflow()
        .find_execution_by_id(&expected_execution_id, &mut NoTransaction)
        .await
        .expect("读取恢复失败后的受阻执行")
        .expect("恢复失败后受阻执行必须存在");
    assert_eq!(execution.status, ApprovalNodeExecutionStatus::Blocked);
    assert!(execution.ended_reason.is_none(), "恢复失败不得结束受阻执行");
    let execution_count = db
        .collection::<Document>("approval_node_executions")
        .count_documents(doc! { "process_instance_id": instance_id.as_ref() })
        .await
        .expect("统计恢复失败后的执行");
    assert_eq!(execution_count, 1, "恢复失败不得创建 replacement execution");

    let tasks = db
        .work_items()
        .approval_tasks_for_execution(&expected_execution_id, &mut NoTransaction)
        .await
        .expect("读取恢复失败后的旧任务");
    assert!(matches!(tasks.as_slice(), [task] if task.status == WorkItemStatus::Closed));
    let task_count = db
        .collection::<Document>("work_items")
        .count_documents(doc! { "business_object_id": object_id(name) })
        .await
        .expect("统计恢复失败后的任务");
    assert_eq!(task_count, 1, "恢复失败不得创建新审批任务");

    let assignee = db
        .bpm_workflow()
        .find_assignee_for_node(&instance_id, &execution.node_key, &mut NoTransaction)
        .await
        .expect("读取恢复失败后的实例审批人绑定")
        .expect("恢复失败后实例审批人绑定必须存在");
    assert_eq!(
        assignee.current_assignee_participant_id,
        assignee.definition_assignee_participant_id,
    );
    assert_eq!(
        assignee.assignment_source,
        ApprovalAssigneeBindingSource::Definition,
    );
    assert!(assignee.changed_by.is_none());
    assert!(assignee.changed_at.is_none());
    assert!(assignee.change_reason.is_none());
    let assignee_count = db
        .collection::<Document>("approval_instance_assignees")
        .count_documents(doc! { "process_instance_id": instance_id.as_ref() })
        .await
        .expect("统计恢复失败后的实例审批人绑定");
    assert_eq!(assignee_count, 1, "恢复失败不得创建或替换实例审批人绑定");
}

async fn blocked_cancel_receipt_count(db: &Database, name: &str) -> u64 {
    db.collection::<Document>("approval_command_receipts")
        .count_documents(doc! {
            "scope_id": instance_id(name),
            "command_kind": "CANCEL_BLOCKED",
        })
        .await
        .expect("统计受阻取消收据")
}

async fn blocked_cancel_audit_count(db: &Database, name: &str) -> u64 {
    db.collection::<Document>("audit_logs")
        .count_documents(doc! {
            "action": "approval.cancel_blocked",
            "resource_id": instance_id(name),
        })
        .await
        .expect("统计受阻取消审计")
}

async fn seed_blocked_cancel_outbox_conflict(db: &Database, name: &str) {
    let dedup = format!("blocked_cancelled:{}", instance_id(name));
    let conflict = ApprovalNotificationOutbox::enqueue(
        ApprovalNotificationOutboxId::new(format!("conflicting-outbox-{name}")),
        dedup,
        ApprovalNotificationEventKind::Started,
        vec![SUBMITTER.to_string()],
        ApprovalNotificationTemplateParams {
            document_type_label: "库存调整单".to_string(),
            document_no: document_no(name),
            current_node_name: NODE_NAME.to_string(),
            current_approver_display_name: APPROVER_NAME.to_string(),
            round_no: 1,
            reject_reason_summary: None,
        },
        Instant::from_unix_secs(12),
    )
    .expect("构造受阻取消 outbox 冲突");
    db.approval_notification_outbox()
        .create(&conflict, &mut NoTransaction)
        .await
        .expect("写入受阻取消 outbox 冲突");
}

async fn mismatch_blocked_execution_context(db: &Database, name: &str) {
    let result = db
        .collection::<Document>("approval_node_executions")
        .update_one(
            doc! { "id": execution_id(name) },
            doc! { "$set": { "blocker_code": ApprovalBlockerCode::OpenTaskConflict.as_str() } },
        )
        .await
        .expect("制造实例/执行 blocker 不一致");
    assert_eq!(result.matched_count, 1, "必须精确命中当前执行");
}

async fn corrupt_cancelled_instance_current_ref(db: &Database, name: &str) {
    let result = db
        .collection::<Document>("approval_process_instances")
        .update_one(
            doc! { "id": instance_id(name) },
            doc! { "$set": { "current_node_execution_id": execution_id(name) } },
        )
        .await
        .expect("破坏已取消实例终态引用");
    assert_eq!(result.matched_count, 1, "必须精确命中已取消实例");
}

async fn replace_execution_blocker(db: &Database, name: &str, blocker: ApprovalBlockerCode) {
    let result = db
        .collection::<Document>("approval_node_executions")
        .update_one(
            doc! { "id": execution_id(name) },
            doc! { "$set": { "blocker_code": blocker.as_str() } },
        )
        .await
        .expect("替换历史执行 blocker");
    assert_eq!(result.matched_count, 1, "必须精确命中历史执行");
}

async fn notification(db: &Database, dedup_key: &str) -> ApprovalNotificationOutbox {
    db.collection::<ApprovalNotificationOutbox>("approval_notification_outbox")
        .find_one(doc! { "dedup_key": dedup_key })
        .await
        .expect("读取决定通知")
        .expect("决定通知必须存在")
}

async fn disable_approver_role(db: &Database) {
    let mut role = db
        .roles()
        .find_by_id(APPROVER_ROLE, &mut NoTransaction)
        .await
        .expect("读取审批角色")
        .expect("审批角色必须存在");
    role.update(RoleUpdate {
        disabled: Some(true),
        ..RoleUpdate::default()
    })
    .expect("停用审批角色");
    db.roles()
        .update(&mut role, &mut NoTransaction)
        .await
        .expect("持久化审批角色停用");
}

async fn enable_approver_role(db: &Database) {
    let mut role = db
        .roles()
        .find_by_id(APPROVER_ROLE, &mut NoTransaction)
        .await
        .expect("读取审批角色")
        .expect("审批角色必须存在");
    role.update(RoleUpdate {
        disabled: Some(false),
        ..RoleUpdate::default()
    })
    .expect("启用审批角色");
    db.roles()
        .update(&mut role, &mut NoTransaction)
        .await
        .expect("持久化审批角色启用");
}

async fn set_role_organization_scope(db: &Database, role_id: &str, organization_id: &str) {
    let result = db
        .collection::<Document>("data_scopes")
        .update_one(
            doc! {
                "subject_type": DataScopeSubjectType::Role.as_str(),
                "subject_id": role_id,
            },
            doc! { "$set": { "scope_targets": [organization_id] } },
        )
        .await
        .expect("调整审批读取角色 Warehouse DataScope");
    assert_eq!(result.matched_count, 1, "必须精确命中审批读取角色范围");
}

async fn set_approver_read_scope(db: &Database, organization_id: &str) {
    set_role_organization_scope(db, APPROVER_ROLE, organization_id).await;
}

/// 当前账号模型只有 Admin 一种后台身份；通知候选仍须逐人检查真实权限。
#[test]
fn runtime_admin_candidate_account_kind_is_currently_closed_to_admin() {
    assert_eq!(AccountKind::parse("admin"), Ok(AccountKind::Admin));
    assert!(AccountKind::parse("employee").is_err());
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn final_decision_replays_after_terminal_task_and_rejects_changed_payload() {
    require_mongo!(async {
        let fixture = fixture(
            "approval_runtime_decision_replay",
            &["replay", "new-no-fallback", "legacy-replay"],
            false,
        )
        .await;
        let version = task_version(&fixture.db, "replay").await;
        let applied = fixture
            .service
            .submit_decision(
                &actor(APPROVER),
                &work_item_id("replay"),
                "APPROVE",
                Some("同意"),
                version,
                "decision-key-replay",
            )
            .await
            .expect("首次最终通过");
        assert_eq!(applied.outcome, ApprovalCommandOutcome::Applied);
        assert_eq!(
            applied.instance_status,
            ApprovalProcessInstanceStatus::Approved.as_str()
        );
        assert!(
            receipt_digest(&fixture.db, &execution_id("replay"), "SUBMIT_DECISION")
                .await
                .starts_with("v2:")
        );

        let ended_task = fixture
            .db
            .work_items()
            .find_document_approval_by_id(&work_item_id("replay"), &mut NoTransaction)
            .await
            .expect("读取已完成任务")
            .expect("历史任务必须保留");
        assert_eq!(ended_task.status, WorkItemStatus::Completed);
        let ended_task_version = ended_task.base.version;

        let replay = fixture
            .service
            .submit_decision(
                &actor(APPROVER),
                &work_item_id("replay"),
                "APPROVE",
                Some("同意"),
                version,
                "decision-key-replay",
            )
            .await
            .expect("终态任务同载荷必须收据优先回放");
        assert_eq!(replay.outcome, ApprovalCommandOutcome::IdempotentReplay);
        assert_eq!(task_version(&fixture.db, "replay").await, ended_task_version);

        let conflict = fixture
            .service
            .submit_decision(
                &actor(APPROVER),
                &work_item_id("replay"),
                "APPROVE",
                Some("改变载荷"),
                version,
                "decision-key-replay",
            )
            .await
            .expect_err("同键异载荷必须稳定冲突");
        assert_eq!(
            conflict.code(),
            Some(ErrorCode::ApprovalIdempotencyPayloadConflict)
        );

        assert_eq!(action_count(&fixture.db, "replay").await, 1);
        assert_eq!(decision_receipt_count(&fixture.db, "replay").await, 1);
        assert_eq!(decision_audit_count(&fixture.db, "replay").await, 1);
        let approved = notification(&fixture.db, &format!("approved:{}", execution_id("replay"))).await;
        let completed = notification(&fixture.db, &format!("completed:{}", instance_id("replay"))).await;
        for notice in [approved, completed] {
            assert_eq!(notice.recipient_user_ids, vec![SUBMITTER.to_string()]);
            assert_eq!(notice.template_params.document_no, document_no("replay"));
            assert_eq!(notice.template_params.current_node_name, NODE_NAME);
            assert_eq!(
                notice.template_params.current_approver_display_name,
                APPROVER_NAME
            );
        }

        let collision_version = task_version(&fixture.db, "new-no-fallback").await;
        fixture
            .service
            .submit_decision(
                &actor(APPROVER),
                &work_item_id("new-no-fallback"),
                "APPROVE",
                None,
                collision_version,
                "decision-key-new-no-fallback",
            )
            .await
            .expect("新摘要基准决定");
        assert_eq!(
            legacy_decision_digest(
                &work_item_id("new-no-fallback"),
                "APPROVE",
                None,
                collision_version,
                APPROVER,
            ),
            legacy_decision_digest(
                &work_item_id("new-no-fallback"),
                "APPROVE",
                Some("NULL"),
                collision_version,
                APPROVER,
            ),
            "字面 NULL 必须复现旧摘要碰撞"
        );
        let no_fallback = fixture
            .service
            .submit_decision(
                &actor(APPROVER),
                &work_item_id("new-no-fallback"),
                "APPROVE",
                Some("NULL"),
                collision_version,
                "decision-key-new-no-fallback",
            )
            .await
            .expect_err("V2 收据不得降级为碰撞的 legacy 回放");
        assert_eq!(
            no_fallback.code(),
            Some(ErrorCode::ApprovalIdempotencyPayloadConflict)
        );
        assert_eq!(action_count(&fixture.db, "new-no-fallback").await, 1);

        let legacy_version = task_version(&fixture.db, "legacy-replay").await;
        fixture
            .service
            .submit_decision(
                &actor(APPROVER),
                &work_item_id("legacy-replay"),
                "APPROVE",
                None,
                legacy_version,
                "decision-key-legacy-replay",
            )
            .await
            .expect("构造终态历史收据 fixture");
        let legacy_digest = legacy_decision_digest(
            &work_item_id("legacy-replay"),
            "APPROVE",
            None,
            legacy_version,
            APPROVER,
        );
        replace_receipt_digest(
            &fixture.db,
            &execution_id("legacy-replay"),
            "SUBMIT_DECISION",
            &legacy_digest,
        )
        .await;
        let legacy_collision = fixture
            .service
            .submit_decision(
                &actor(APPROVER),
                &work_item_id("legacy-replay"),
                "APPROVE",
                Some("NULL"),
                legacy_version,
                "decision-key-legacy-replay",
            )
            .await
            .expect_err("legacy NULL 摘要碰撞仍必须由终态 reason 精确拒绝");
        assert_eq!(
            legacy_collision.code(),
            Some(ErrorCode::ApprovalIdempotencyPayloadConflict)
        );
        let legacy_replay = fixture
            .service
            .submit_decision(
                &actor(APPROVER),
                &work_item_id("legacy-replay"),
                "APPROVE",
                None,
                legacy_version,
                "decision-key-legacy-replay",
            )
            .await
            .expect("完成 runtime identity 验证后允许 legacy 收据回放");
        assert_eq!(legacy_replay.outcome, ApprovalCommandOutcome::IdempotentReplay);
        assert_eq!(action_count(&fixture.db, "legacy-replay").await, 1);
        drop(fixture);
        tokio::time::sleep(Duration::from_millis(250)).await;
    });
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn concurrent_same_key_recovers_winner_and_different_keys_leave_one_commit() {
    require_mongo!(async {
        let same = fixture("approval_runtime_decision_same_key", &["same-key"], true).await;
        let same_gate = same.gate.clone().expect("同键并发门闩");
        let version = task_version(&same.db, "same-key").await;
        let first_service = Arc::clone(&same.service);
        let first = tokio::spawn(async move {
            first_service
                .submit_decision(
                    &actor(APPROVER),
                    &work_item_id("same-key"),
                    "APPROVE",
                    None,
                    version,
                    "decision-key-concurrent",
                )
                .await
        });
        same_gate.entered.notified().await;
        let second_service = Arc::clone(&same.service);
        let second = tokio::spawn(async move {
            second_service
                .submit_decision(
                    &actor(APPROVER),
                    &work_item_id("same-key"),
                    "APPROVE",
                    None,
                    version,
                    "decision-key-concurrent",
                )
                .await
        });
        for _ in 0..64 {
            tokio::task::yield_now().await;
        }
        same_gate.release.notify_one();
        let first = first.await.expect("首个同键任务").expect("首个同键结果");
        let second = second
            .await
            .expect("第二个同键任务")
            .expect("竞争败者必须以新会话回读胜者");
        let outcomes = [first.outcome, second.outcome];
        assert!(outcomes.contains(&ApprovalCommandOutcome::Applied));
        assert!(outcomes.contains(&ApprovalCommandOutcome::IdempotentReplay));
        assert_eq!(action_count(&same.db, "same-key").await, 1);
        assert_eq!(decision_receipt_count(&same.db, "same-key").await, 1);
        assert_eq!(decision_audit_count(&same.db, "same-key").await, 1);

        let different = fixture(
            "approval_runtime_decision_different_keys",
            &["different-keys"],
            true,
        )
        .await;
        let different_gate = different.gate.clone().expect("异键并发门闩");
        let version = task_version(&different.db, "different-keys").await;
        let first_service = Arc::clone(&different.service);
        let first = tokio::spawn(async move {
            first_service
                .submit_decision(
                    &actor(APPROVER),
                    &work_item_id("different-keys"),
                    "APPROVE",
                    None,
                    version,
                    "decision-key-a",
                )
                .await
        });
        different_gate.entered.notified().await;
        let second_service = Arc::clone(&different.service);
        let second = tokio::spawn(async move {
            second_service
                .submit_decision(
                    &actor(APPROVER),
                    &work_item_id("different-keys"),
                    "APPROVE",
                    None,
                    version,
                    "decision-key-b",
                )
                .await
        });
        let second = second.await.expect("第二个异键任务");
        different_gate.release.notify_one();
        let first = first.await.expect("首个异键任务");
        assert_eq!(
            usize::from(first.is_ok()) + usize::from(second.is_ok()),
            1,
            "不同幂等键竞争只允许一个事务成功"
        );
        assert_eq!(action_count(&different.db, "different-keys").await, 1);
        assert_eq!(decision_receipt_count(&different.db, "different-keys").await, 1);
        assert_eq!(decision_audit_count(&different.db, "different-keys").await, 1);
        drop(same);
        drop(different);
        tokio::time::sleep(Duration::from_millis(250)).await;
    });
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn authorization_drift_blocks_fresh_decision_and_hides_committed_receipt() {
    require_mongo!(async {
        let fixture = fixture(
            "approval_runtime_decision_auth_drift",
            &["authorized-first", "fresh-drift"],
            false,
        )
        .await;
        let original_version = task_version(&fixture.db, "authorized-first").await;
        fixture
            .service
            .submit_decision(
                &actor(APPROVER),
                &work_item_id("authorized-first"),
                "APPROVE",
                None,
                original_version,
                "decision-key-authorized",
            )
            .await
            .expect("授权漂移前首次决定");

        disable_approver_role(&fixture.db).await;
        let replay_denied = fixture
            .service
            .submit_decision(
                &actor(APPROVER),
                &work_item_id("authorized-first"),
                "APPROVE",
                None,
                original_version,
                "decision-key-authorized",
            )
            .await
            .expect_err("同载荷回放失权时必须隐藏收据存在性");
        let changed_denied = fixture
            .service
            .submit_decision(
                &actor(APPROVER),
                &work_item_id("authorized-first"),
                "APPROVE",
                Some("不得先比较摘要"),
                original_version,
                "decision-key-authorized",
            )
            .await
            .expect_err("异载荷也必须先隐藏失权调用方的收据存在性");
        let missing_denied = fixture
            .service
            .submit_decision(
                &actor(APPROVER),
                &work_item_id("authorized-first"),
                "APPROVE",
                None,
                original_version,
                "decision-key-missing",
            )
            .await
            .expect_err("缺失键与已存在键必须返回同一终态语义");
        for error in [&replay_denied, &changed_denied, &missing_denied] {
            assert_eq!(error.code(), Some(ErrorCode::ApprovalTaskNotOpen));
            assert_eq!(error.to_string(), replay_denied.to_string());
        }

        let outsider_existing = fixture
            .service
            .submit_decision(
                &actor(RUNTIME_ADMIN),
                &work_item_id("authorized-first"),
                "APPROVE",
                Some("非原 actor 不得探测摘要"),
                original_version,
                "decision-key-authorized",
            )
            .await
            .expect_err("非原 actor existing key 必须先按 Fresh owner 语义拒绝");
        let outsider_missing = fixture
            .service
            .submit_decision(
                &actor(RUNTIME_ADMIN),
                &work_item_id("authorized-first"),
                "APPROVE",
                None,
                original_version,
                "decision-key-outsider-missing",
            )
            .await
            .expect_err("非原 actor missing key 必须返回相同拒绝");
        assert!(matches!(&outsider_existing, Error::Forbidden(_)));
        assert!(matches!(&outsider_missing, Error::Forbidden(_)));
        assert_eq!(outsider_existing.to_string(), outsider_missing.to_string());
        assert_eq!(action_count(&fixture.db, "authorized-first").await, 1);
        assert_eq!(decision_receipt_count(&fixture.db, "authorized-first").await, 1);
        assert_eq!(decision_audit_count(&fixture.db, "authorized-first").await, 1);

        let fresh_version = task_version(&fixture.db, "fresh-drift").await;
        let blocked = fixture
            .service
            .submit_decision(
                &actor(APPROVER),
                &work_item_id("fresh-drift"),
                "APPROVE",
                None,
                fresh_version,
                "decision-key-fresh-drift",
            )
            .await
            .expect_err("新决定授权漂移必须提交受阻事实");
        assert_eq!(blocked.code(), Some(ErrorCode::ApprovalInstanceBlocked));
        let instance = fixture
            .db
            .bpm_workflow()
            .find_instance_by_id(
                &ApprovalProcessInstanceId::new(instance_id("fresh-drift")),
                &mut NoTransaction,
            )
            .await
            .expect("读取受阻实例")
            .expect("受阻实例必须存在");
        assert_eq!(instance.status, ApprovalProcessInstanceStatus::Blocked);
        let task = fixture
            .db
            .work_items()
            .find_document_approval_by_id(&work_item_id("fresh-drift"), &mut NoTransaction)
            .await
            .expect("读取受阻任务")
            .expect("受阻历史任务必须存在");
        assert_eq!(task.status, WorkItemStatus::Closed);
        assert_eq!(action_count(&fixture.db, "fresh-drift").await, 0);
        assert_eq!(decision_receipt_count(&fixture.db, "fresh-drift").await, 1);
        assert_eq!(decision_audit_count(&fixture.db, "fresh-drift").await, 1);

        let blocked_notice =
            notification(&fixture.db, &format!("blocked:{}", execution_id("fresh-drift"))).await;
        assert_eq!(blocked_notice.event_kind, ApprovalNotificationEventKind::Blocked);
        assert_eq!(
            blocked_notice.recipient_user_ids,
            vec![SUBMITTER.to_string(), RUNTIME_ADMIN.to_string()]
        );
        assert!(!blocked_notice
            .recipient_user_ids
            .iter()
            .any(|recipient| recipient == WRONG_ORG_ADMIN));
        assert_eq!(
            blocked_notice.template_params.document_no,
            document_no("fresh-drift")
        );
        assert_eq!(
            blocked_notice.template_params.current_approver_display_name,
            APPROVER_NAME
        );
        drop(fixture);
        tokio::time::sleep(Duration::from_millis(250)).await;
    });
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn stock_personnel_blocker_resume_revalidates_read_permission_and_warehouse_scope_in_transaction() {
    require_mongo!(async {
        let names = ["resume-success", "resume-wrong-org", "resume-lost-read"];
        let fixture = fixture("approval_runtime_stock_resume_scope", &names, false).await;
        disable_approver_role(&fixture.db).await;
        for name in names {
            let version = task_version(&fixture.db, name).await;
            let blocked = fixture
                .service
                .submit_decision(
                    &actor(APPROVER),
                    &work_item_id(name),
                    "APPROVE",
                    None,
                    version,
                    &format!("decision-key-{name}"),
                )
                .await
                .expect_err("缺库存读取权限必须提交人员受阻事实");
            assert_eq!(blocked.code(), Some(ErrorCode::ApprovalInstanceBlocked));
        }

        let lost_read = resume_command(&fixture.db, "resume-lost-read", "resume-key-lost-read").await;
        let lost_error = fixture
            .service
            .resume_current_approver(&actor(RUNTIME_ADMIN), lost_read)
            .await
            .expect_err("仅保留决定权限而缺 detail 权限时不得恢复");
        assert_eq!(
            lost_error.code(),
            Some(ErrorCode::ApprovalCurrentApproverNotRecovered)
        );
        assert_eq!(resume_receipt_count(&fixture.db, "resume-lost-read").await, 0);
        assert_eq!(resume_audit_count(&fixture.db, "resume-lost-read").await, 0);
        assert_resume_failure_preserves_blocked_runtime(&fixture.db, "resume-lost-read").await;

        enable_approver_role(&fixture.db).await;
        set_approver_read_scope(&fixture.db, WRONG_ORG).await;
        let wrong_org = resume_command(&fixture.db, "resume-wrong-org", "resume-key-wrong-org").await;
        let wrong_org_error = fixture
            .service
            .resume_current_approver(&actor(RUNTIME_ADMIN), wrong_org)
            .await
            .expect_err("detail 权限角色的 Warehouse DataScope 不覆盖冻结组织时不得恢复");
        assert_eq!(
            wrong_org_error.code(),
            Some(ErrorCode::ApprovalCurrentApproverNotRecovered)
        );
        assert_eq!(resume_receipt_count(&fixture.db, "resume-wrong-org").await, 0);
        assert_eq!(resume_audit_count(&fixture.db, "resume-wrong-org").await, 0);
        assert_resume_failure_preserves_blocked_runtime(&fixture.db, "resume-wrong-org").await;

        set_approver_read_scope(&fixture.db, ORG).await;
        let success = resume_command(&fixture.db, "resume-success", "resume-key-success").await;
        let view = fixture
            .service
            .resume_current_approver(&actor(RUNTIME_ADMIN), success)
            .await
            .expect("同角色 detail 权限与 Warehouse DataScope 均覆盖时必须恢复");
        assert_eq!(view.outcome, ApprovalCommandOutcome::Applied);
        assert_eq!(
            view.instance_status,
            ApprovalProcessInstanceStatus::Running.as_str()
        );
        assert_eq!(resume_receipt_count(&fixture.db, "resume-success").await, 1);
        assert_eq!(resume_audit_count(&fixture.db, "resume-success").await, 1);
        let instance = fixture
            .db
            .bpm_workflow()
            .find_instance_by_id(
                &ApprovalProcessInstanceId::new(instance_id("resume-success")),
                &mut NoTransaction,
            )
            .await
            .expect("读取恢复后实例")
            .expect("恢复后实例必须存在");
        let current_execution_id = instance
            .current_node_execution_id
            .as_ref()
            .expect("恢复后实例必须指向新执行");
        assert_ne!(
            current_execution_id.as_ref(),
            execution_id("resume-success"),
            "恢复必须创建新执行而不是重开旧执行",
        );
        let new_execution = fixture
            .db
            .bpm_workflow()
            .find_execution_by_id(current_execution_id, &mut NoTransaction)
            .await
            .expect("读取恢复后的新执行")
            .expect("恢复后的新执行必须存在");
        assert_eq!(
            new_execution.assignee_participant_id.as_str(),
            APPROVER,
            "恢复后的新执行只能属于原审批人",
        );
        assert_eq!(
            new_execution.assignment_source,
            bpm::model::types::ApprovalExecutionAssignmentSource::AssigneeRecovery,
        );
        let open_tasks = fixture
            .db
            .work_items()
            .open_approval_tasks_for_execution(current_execution_id, &mut NoTransaction)
            .await
            .expect("读取恢复后开放任务");
        assert_eq!(open_tasks.len(), 1, "恢复后新执行必须恰有一个开放任务");
        assert_eq!(open_tasks[0].owner_user_id.as_deref(), Some(APPROVER));

        let assignee = fixture
            .db
            .bpm_workflow()
            .find_assignee_for_node(
                &ApprovalProcessInstanceId::new(instance_id("resume-success")),
                &new_execution.node_key,
                &mut NoTransaction,
            )
            .await
            .expect("读取恢复后的实例审批人绑定")
            .expect("恢复后的实例审批人绑定必须存在");
        assert!(assignee.ensure_unchanged_from_definition().is_ok());
        assert_eq!(assignee.current_assignee_participant_id.as_str(), APPROVER);

        let entered_notice =
            notification(&fixture.db, &format!("entered:{}", current_execution_id.as_ref())).await;
        assert_eq!(entered_notice.event_kind, ApprovalNotificationEventKind::Entered);
        assert_eq!(entered_notice.recipient_user_ids, vec![APPROVER.to_string()]);
        let resumed_notice =
            notification(&fixture.db, &format!("resumed:{}", current_execution_id.as_ref())).await;
        assert_eq!(resumed_notice.event_kind, ApprovalNotificationEventKind::Resumed);
        assert_eq!(
            resumed_notice.recipient_user_ids,
            vec![APPROVER.to_string(), SUBMITTER.to_string()],
        );
        for notice in [&entered_notice, &resumed_notice] {
            assert_eq!(notice.template_params.document_no, document_no("resume-success"));
            assert_eq!(notice.template_params.current_node_name, NODE_NAME);
            assert_eq!(
                notice.template_params.current_approver_display_name,
                APPROVER_NAME,
            );
            assert_eq!(notice.template_params.round_no, new_execution.round_no);
        }
        drop(fixture);
        tokio::time::sleep(Duration::from_millis(250)).await;
    });
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn blocked_cancel_is_receipt_first_scope_safe_and_rolls_back_on_outbox_conflict() {
    require_mongo!(async {
        let fixture = fixture("approval_runtime_blocked_cancel", &[], false).await;
        let graph = single_node_graph();
        seed_blocked_runtime(&fixture.db, &graph, "cancel-success").await;
        seed_blocked_runtime(&fixture.db, &graph, "cancel-rollback").await;
        seed_blocked_runtime(&fixture.db, &graph, "cancel-mismatch").await;
        seed_blocked_runtime(&fixture.db, &graph, "cancel-v2-corrupt").await;
        seed_blocked_runtime(&fixture.db, &graph, "cancel-legacy-personnel").await;

        mismatch_blocked_execution_context(&fixture.db, "cancel-mismatch").await;
        let mismatch = blocked_cancel_command(
            &fixture.db,
            "cancel-mismatch",
            RUNTIME_ADMIN,
            "cancel-mismatch-key",
            "不一致必须拒绝",
        )
        .await;
        let mismatch_error = fixture
            .service
            .cancel_blocked(&actor(RUNTIME_ADMIN), mismatch)
            .await
            .expect_err("实例与执行 blocker 不一致必须零写冲突");
        assert!(matches!(mismatch_error, Error::ConflictError(_)));
        assert_eq!(action_count(&fixture.db, "cancel-mismatch").await, 0);
        assert_eq!(
            blocked_cancel_receipt_count(&fixture.db, "cancel-mismatch").await,
            0
        );
        assert_eq!(
            blocked_cancel_audit_count(&fixture.db, "cancel-mismatch").await,
            0
        );

        let corrupt_command = blocked_cancel_command(
            &fixture.db,
            "cancel-v2-corrupt",
            RUNTIME_ADMIN,
            "cancel-v2-corrupt-key",
            "V2 终态破坏",
        )
        .await;
        fixture
            .service
            .cancel_blocked(&actor(RUNTIME_ADMIN), corrupt_command.clone())
            .await
            .expect("先构造 V2 受阻取消收据");
        corrupt_cancelled_instance_current_ref(&fixture.db, "cancel-v2-corrupt").await;
        fixture
            .service
            .cancel_blocked(&actor(RUNTIME_ADMIN), corrupt_command)
            .await
            .expect_err("V2 收据也不得掩盖损坏的取消终态");
        assert_eq!(action_count(&fixture.db, "cancel-v2-corrupt").await, 1);

        let personnel_command = blocked_cancel_command(
            &fixture.db,
            "cancel-legacy-personnel",
            RUNTIME_ADMIN,
            "cancel-legacy-personnel-key",
            "人员 blocker 不得兼容",
        )
        .await;
        fixture
            .service
            .cancel_blocked(&actor(RUNTIME_ADMIN), personnel_command.clone())
            .await
            .expect("先构造受阻取消终态");
        let personnel_blocker = ApprovalBlockerCode::ApproverAccountInactive;
        replace_execution_blocker(&fixture.db, "cancel-legacy-personnel", personnel_blocker).await;
        replace_receipt_digest(
            &fixture.db,
            &instance_id("cancel-legacy-personnel"),
            "CANCEL_BLOCKED",
            &legacy_cancel_blocked_digest(
                personnel_blocker.as_str(),
                personnel_command.expected_instance_version,
                personnel_command.expected_execution_version,
                personnel_command.expected_task_version,
                &personnel_command.reason,
                RUNTIME_ADMIN,
            ),
        )
        .await;
        fixture
            .service
            .cancel_blocked(&actor(RUNTIME_ADMIN), personnel_command)
            .await
            .expect_err("legacy 人员 blocker 收据不得进入结构受阻取消回放");
        assert_eq!(action_count(&fixture.db, "cancel-legacy-personnel").await, 1);

        let wrong_org = blocked_cancel_command(
            &fixture.db,
            "cancel-success",
            WRONG_ORG_ADMIN,
            "cancel-wrong-org",
            "结构受损退出",
        )
        .await;
        let denied = fixture
            .service
            .cancel_blocked(&actor(WRONG_ORG_ADMIN), wrong_org)
            .await
            .expect_err("错误组织运行管理员必须失败关闭");
        assert!(matches!(denied, Error::Forbidden(_)));
        assert_eq!(action_count(&fixture.db, "cancel-success").await, 0);
        assert_eq!(
            blocked_cancel_receipt_count(&fixture.db, "cancel-success").await,
            0
        );

        let command = blocked_cancel_command(
            &fixture.db,
            "cancel-success",
            RUNTIME_ADMIN,
            "cancel-success-key",
            "结构受损\u{1f}退出",
        )
        .await;
        let applied = fixture
            .service
            .cancel_blocked(&actor(RUNTIME_ADMIN), command.clone())
            .await
            .expect("同组织真实运行管理员可受阻取消");
        assert_eq!(applied.outcome, ApprovalCommandOutcome::Applied);
        assert_eq!(
            applied.instance_status,
            ApprovalProcessInstanceStatus::Cancelled.as_str()
        );
        assert!(
            receipt_digest(&fixture.db, &instance_id("cancel-success"), "CANCEL_BLOCKED")
                .await
                .starts_with("v2:")
        );
        let replay = fixture
            .service
            .cancel_blocked(&actor(RUNTIME_ADMIN), command.clone())
            .await
            .expect("受阻取消终态后同载荷必须回放");
        assert_eq!(replay.outcome, ApprovalCommandOutcome::IdempotentReplay);

        let mut wrong_org_existing = command.clone();
        wrong_org_existing.actor_id = WRONG_ORG_ADMIN.to_string();
        wrong_org_existing.reason = "错误组织不得探测已有键".to_string();
        let wrong_org_existing_error = fixture
            .service
            .cancel_blocked(&actor(WRONG_ORG_ADMIN), wrong_org_existing.clone())
            .await
            .expect_err("错误组织 existing key 必须在摘要前拒绝");
        wrong_org_existing.idempotency_key = "cancel-wrong-org-missing".to_string();
        let wrong_org_missing_error = fixture
            .service
            .cancel_blocked(&actor(WRONG_ORG_ADMIN), wrong_org_existing.clone())
            .await
            .expect_err("错误组织 missing key 必须返回相同拒绝");
        assert!(matches!(&wrong_org_existing_error, Error::Forbidden(_)));
        assert!(matches!(&wrong_org_missing_error, Error::Forbidden(_)));
        assert_eq!(
            wrong_org_existing_error.to_string(),
            wrong_org_missing_error.to_string()
        );

        set_role_organization_scope(&fixture.db, WRONG_ORG_ADMIN_ROLE, ORG).await;
        wrong_org_existing.idempotency_key = command.idempotency_key.clone();
        let authorized_non_actor_existing = fixture
            .service
            .cancel_blocked(&actor(WRONG_ORG_ADMIN), wrong_org_existing.clone())
            .await
            .expect_err("已授权非原 actor existing key 必须复用 Fresh 终态冲突");
        wrong_org_existing.idempotency_key = "cancel-authorized-non-actor-missing".to_string();
        let authorized_non_actor_missing = fixture
            .service
            .cancel_blocked(&actor(WRONG_ORG_ADMIN), wrong_org_existing)
            .await
            .expect_err("已授权非原 actor missing key 必须返回相同终态冲突");
        assert!(matches!(
            &authorized_non_actor_existing,
            Error::ConflictError(message) if message == "审批实例版本已变化，请刷新后重试"
        ));
        assert!(matches!(
            &authorized_non_actor_missing,
            Error::ConflictError(message) if message == "审批实例版本已变化，请刷新后重试"
        ));
        assert_eq!(
            authorized_non_actor_existing.to_string(),
            authorized_non_actor_missing.to_string()
        );
        assert_ne!(
            authorized_non_actor_existing.code(),
            Some(ErrorCode::ApprovalIdempotencyPayloadConflict)
        );
        assert_eq!(action_count(&fixture.db, "cancel-success").await, 1);
        assert_eq!(
            blocked_cancel_receipt_count(&fixture.db, "cancel-success").await,
            1
        );
        assert_eq!(blocked_cancel_audit_count(&fixture.db, "cancel-success").await, 1);

        let legacy_digest = legacy_cancel_blocked_digest(
            ApprovalBlockerCode::DefinitionGraphCorrupted.as_str(),
            command.expected_instance_version,
            command.expected_execution_version,
            command.expected_task_version,
            &command.reason,
            RUNTIME_ADMIN,
        );
        replace_receipt_digest(
            &fixture.db,
            &instance_id("cancel-success"),
            "CANCEL_BLOCKED",
            &legacy_digest,
        )
        .await;
        let collision_actor_id = format!("退出\u{1f}{RUNTIME_ADMIN}");
        let mut legacy_collision_command = command.clone();
        legacy_collision_command.reason = "结构受损".to_string();
        legacy_collision_command.actor_id = collision_actor_id.clone();
        assert_eq!(
            legacy_digest,
            legacy_cancel_blocked_digest(
                ApprovalBlockerCode::DefinitionGraphCorrupted.as_str(),
                legacy_collision_command.expected_instance_version,
                legacy_collision_command.expected_execution_version,
                legacy_collision_command.expected_task_version,
                &legacy_collision_command.reason,
                &collision_actor_id,
            ),
            "分隔符迁移必须复现 legacy 摘要碰撞"
        );
        let legacy_collision = fixture
            .service
            .cancel_blocked(&actor(&collision_actor_id), legacy_collision_command)
            .await
            .expect_err("未授权非原 actor 必须在 legacy 摘要比较前拒绝");
        assert!(matches!(legacy_collision, Error::Forbidden(_)));
        let legacy_replay = fixture
            .service
            .cancel_blocked(&actor(RUNTIME_ADMIN), command.clone())
            .await
            .expect("受阻取消完成 runtime identity 验证后允许 legacy 收据回放");
        assert_eq!(legacy_replay.outcome, ApprovalCommandOutcome::IdempotentReplay);
        let mut changed = command;
        changed.reason = "改变受阻取消载荷".to_string();
        let conflict = fixture
            .service
            .cancel_blocked(&actor(RUNTIME_ADMIN), changed)
            .await
            .expect_err("受阻取消同键异载荷必须稳定冲突");
        assert_eq!(
            conflict.code(),
            Some(ErrorCode::ApprovalIdempotencyPayloadConflict)
        );
        assert_eq!(action_count(&fixture.db, "cancel-success").await, 1);
        assert_eq!(
            blocked_cancel_receipt_count(&fixture.db, "cancel-success").await,
            1
        );
        assert_eq!(blocked_cancel_audit_count(&fixture.db, "cancel-success").await, 1);
        let cancelled_notice = notification(
            &fixture.db,
            &format!("blocked_cancelled:{}", instance_id("cancel-success")),
        )
        .await;
        assert_eq!(
            cancelled_notice.event_kind,
            ApprovalNotificationEventKind::BlockedCancelled
        );
        assert_eq!(
            cancelled_notice.recipient_user_ids,
            vec![SUBMITTER.to_string(), RUNTIME_ADMIN.to_string()]
        );
        assert_eq!(
            cancelled_notice.template_params.document_no,
            document_no("cancel-success")
        );
        assert_eq!(cancelled_notice.template_params.current_node_name, NODE_NAME);
        assert_eq!(
            cancelled_notice.template_params.current_approver_display_name,
            APPROVER_NAME
        );

        seed_blocked_cancel_outbox_conflict(&fixture.db, "cancel-rollback").await;
        let rollback_command = blocked_cancel_command(
            &fixture.db,
            "cancel-rollback",
            RUNTIME_ADMIN,
            "cancel-rollback-key",
            "冲突必须回滚",
        )
        .await;
        fixture
            .service
            .cancel_blocked(&actor(RUNTIME_ADMIN), rollback_command)
            .await
            .expect_err("outbox 唯一键冲突必须使整个受阻取消事务回滚");
        let rolled_back_instance = fixture
            .db
            .bpm_workflow()
            .find_instance_by_id(
                &ApprovalProcessInstanceId::new(instance_id("cancel-rollback")),
                &mut NoTransaction,
            )
            .await
            .expect("读取回滚实例")
            .expect("回滚实例必须存在");
        assert_eq!(
            rolled_back_instance.status,
            ApprovalProcessInstanceStatus::Blocked
        );
        let rolled_back_execution = fixture
            .db
            .bpm_workflow()
            .find_execution_by_id(
                rolled_back_instance
                    .current_node_execution_id
                    .as_ref()
                    .expect("回滚后必须保留受阻执行引用"),
                &mut NoTransaction,
            )
            .await
            .expect("读取回滚执行")
            .expect("回滚执行必须存在");
        assert_eq!(rolled_back_execution.status, ApprovalNodeExecutionStatus::Blocked);
        assert_eq!(action_count(&fixture.db, "cancel-rollback").await, 0);
        assert_eq!(
            blocked_cancel_receipt_count(&fixture.db, "cancel-rollback").await,
            0
        );
        assert_eq!(
            blocked_cancel_audit_count(&fixture.db, "cancel-rollback").await,
            0
        );
        let preexisting = notification(
            &fixture.db,
            &format!("blocked_cancelled:{}", instance_id("cancel-rollback")),
        )
        .await;
        assert_eq!(preexisting.event_kind, ApprovalNotificationEventKind::Started);
        drop(fixture);
        tokio::time::sleep(Duration::from_millis(250)).await;
    });
}
