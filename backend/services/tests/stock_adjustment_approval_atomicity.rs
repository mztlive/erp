//! APP-S03/S04/S05、FUL-E08/E09：库存调整审批启动、最终过账与撤回的真实 MongoDB 副本集原子性验收。
//!
//! 用例只通过公开 Service 入口执行命令，并跨集合回读业务单据、BPM 运行事实、
//! WorkItem、命令收据、审计和通知 outbox。测试统一使用随机独立数据库；仅在
//! `ERP_TEST_MONGO_URI` 指向 MongoDB 7 单节点副本集时以 `--include-ignored` 执行。

use std::sync::Arc;

use bpm::ids::{
    ApprovalCommandReceiptId, ApprovalNodeDefinitionId, ApprovalNodeExecutionId, ApprovalProcessDefinitionId,
    ApprovalProcessInstanceId, ApprovalTransitionDefinitionId,
};
use bpm::model::types::{
    ApprovalAssigneeBindingSource, ApprovalBlockerCode, ApprovalCommandKind,
    ApprovalExecutionAssignmentSource, ApprovalNodeExecutionStatus, ApprovalProcessInstanceStatus,
    ApprovalTransitionEvent,
};
use bpm::model::{
    ApprovalCommandIdentity, ApprovalCommandReceipt, ApprovalInstanceAssignee, ApprovalNodeDefinition,
    ApprovalNodeExecution, ApprovalProcessDefinition, ApprovalProcessInstance, ApprovalTransitionDefinition,
    CanonicalCommandPayload, CommandPayloadField, IdempotencyKey, NewNodeDefinition, NewNodeExecution,
    NewProcessInstance, ParticipantId, ProcessKind, SubjectRef, Timestamp,
};
use casbin::Adapter;
use database::repository::bpm::ApprovalInstanceListProjection;
use database::{
    ensure_indexes, AccessControlExt, ApprovalIntegrationExt, BpmExt, DocumentRegistryExt, InventoryExt,
    MongoCasbinAdapter, NoTransaction, WorkItemExt,
};
use entities::access_control::{DataScope, DataScopeData, DataScopeSubjectType, DataScopeType};
use entities::approval_integration::{
    ApprovalNotificationEventKind, ApprovalNotificationOutbox, ApprovalNotificationTemplateParams,
    ApprovalSubjectCounterparty, ApprovalSubjectSnapshot, ApprovalSubjectSnapshotPayload,
};
use entities::common::time::Instant;
use entities::document_registry::business_document::ApprovalDefinitionBinding;
use entities::document_registry::{BusinessDocument, BusinessDocumentData, BusinessDocumentId, DocumentType};
use entities::ids::{
    ApprovalNotificationOutboxId, ApprovalSubjectSnapshotId, DataScopeId, SkuId, StockAdjustmentId,
    StockAdjustmentLineId, StockBalanceId, WorkItemId,
};
use entities::inventory::{
    AdjustmentReasonType, MovementDirection, MovementType, StockAdjustment, StockAdjustmentData,
    StockAdjustmentLine, StockAdjustmentLineData, StockAdjustmentState, StockBalance, StockBalanceData,
    StockMovement,
};
use entities::money::Quantity;
use entities::work_item::{
    AssignmentSource, DocumentApprovalWorkItemData, WorkItem, WorkItemPriority, WorkItemStatus, WorkItemType,
};
use entities::{
    AccountCore, AccountCoreData, AccountKind, AccountStatus, LoginAccount, Role, RoleData, Secret,
};
use mongodb::bson::raw::{cstr, RawDocumentBuf};
use mongodb::bson::{doc, Bson, Document};
use mongodb::options::{ClientOptions, IndexOptions};
use mongodb::{Client, Database, IndexModel};
use serde::de::DeserializeOwned;
use services::approval::execution::{ApprovalCommandOutcome, ApprovalRuntimeService};
use services::audit::AuditActor;
use services::iam::{self, subject};
use services::inventory::{
    CancelStockAdjustmentApprovalRequest, ExpectedStockBalanceVersion, InventoryService,
    StockAdjustmentLineUpdateInput, StockAdjustmentSubmitResultQuery, SubmitStockAdjustmentRequest,
};
use services::{ApprovalActionRegistry, Error, ErrorCode};
use sha2::{Digest, Sha256};
use test_support::{require_mongo, TestDb};
use tokio::sync::Barrier;

const STARTER: &str = "stock-adjustment-starter";
const APPROVER: &str = "stock-adjustment-approver";
const LATER_APPROVER: &str = "stock-adjustment-later";
const CANCEL_ROLE: &str = "stock-adj-cancel";
const APPROVER_ROLE: &str = "stock-adj-decide";
const LATER_APPROVER_ROLE: &str = "stock-adj-later";
const ORGANIZATION: &str = "warehouse-atomicity";
const DEFINITION_ID: &str = "definition-stock-adjustment-atomicity-v1";
const NODE_KEY: &str = "warehouse-review";
const LATER_NODE_KEY: &str = "finance-review";
const LINE_ID: &str = "line-stock-adjustment-atomicity";
const BALANCE_ID: &str = "balance-stock-adjustment-atomicity";
const SKU_ID: &str = "sku-stock-adjustment-atomicity";

/// 待验收实例的状态形状。
#[derive(Debug, Clone, Copy)]
enum RuntimeState {
    /// 正常运行，必须有且仅有一个开放任务。
    Running,
    /// 受阻实例，不得保留开放任务。
    Blocked(ApprovalBlockerCode),
}

/// 运行事实的稳定标识和调用方 CAS 令牌。
#[derive(Debug, Clone)]
struct RuntimeSeed {
    adjustment_id: String,
    instance_id: String,
    execution_id: String,
    task_id: Option<String>,
    subject_version: u32,
    adjustment_version: u64,
    instance_version: u64,
    execution_version: u64,
    task_version: Option<u64>,
}

/// 草稿提交所需的业务 CAS 基线。
#[derive(Debug, Clone)]
struct StartSeed {
    adjustment_id: String,
    line_id: String,
    balance_id: String,
    adjustment_version: u64,
    line_version: u64,
    balance_version: u64,
    target_subject_version: u32,
}

/// 跨集合回读的一致性快照，用于证明回放零重写和失败全回滚。
#[derive(Debug, Clone, PartialEq, Eq)]
struct StoredFacts {
    adjustment: StockAdjustment,
    instance: ApprovalProcessInstance,
    execution: ApprovalNodeExecution,
    task: Option<WorkItem>,
    cancel_receipt: Option<ApprovalCommandReceipt>,
    outbox: Option<ApprovalNotificationOutbox>,
    cancel_audit: Option<Document>,
    cancel_audits: u64,
}

/// 提交启动跨集合事实，用于证明成功形状、回放零写与失败全回滚。
#[derive(Debug, Clone, PartialEq, Eq)]
struct StartFacts {
    adjustment: StockAdjustment,
    line: StockAdjustmentLine,
    balance: StockBalance,
    instances: Vec<ApprovalProcessInstance>,
    executions: Vec<ApprovalNodeExecution>,
    assignees: Vec<ApprovalInstanceAssignee>,
    tasks: Vec<WorkItem>,
    snapshots: Vec<ApprovalSubjectSnapshot>,
    receipts: Vec<ApprovalCommandReceipt>,
    outbox: Vec<ApprovalNotificationOutbox>,
    audits: Vec<Document>,
}

/// 最终通过额外形成的库存事实和双审计。
#[derive(Debug, Clone, PartialEq, Eq)]
struct FinalApprovalFacts {
    chain: StartFacts,
    movements: Vec<StockMovement>,
    post_audits: Vec<Document>,
    decision_audits: Vec<Document>,
}

/// 一次性 failCommand 进入前的服务端可审计计数。
#[derive(Debug, Clone, Copy)]
struct ReceiptRaceObservation {
    failpoint_entries: i64,
}

/// 构造固定秒时间戳。
fn at(seconds: i64) -> Timestamp {
    Timestamp::from_unix_secs(seconds).expect("测试时间戳必须合法")
}

/// 构造可指定 scope/digest 的测试收据，覆盖当前 v3 writer 与历史字符串。
fn fixture_receipt(
    id: &str,
    kind: ApprovalCommandKind,
    scope: &str,
    key: &str,
    digest: &str,
    result_ref: &str,
    at: Timestamp,
) -> ApprovalCommandReceipt {
    let identity = ApprovalCommandIdentity::new(
        kind,
        "test.fixture",
        IdempotencyKey::parse(key).expect("测试幂等键必须有效"),
        CanonicalCommandPayload::new().field(CommandPayloadField::Text(scope)),
        CanonicalCommandPayload::new().field(CommandPayloadField::Text(digest)),
    )
    .expect("测试命令身份");
    let mut receipt =
        ApprovalCommandReceipt::new(ApprovalCommandReceiptId::new(id), &identity, result_ref, at)
            .expect("测试收据");
    receipt.scope_id = scope.to_string();
    receipt.payload_digest = digest.to_string();
    receipt
}

/// 构造 BPM 参与人。
fn participant(id: &str) -> ParticipantId {
    ParticipantId::new(id).expect("测试参与人必须合法")
}

/// 构造与当前认证身份一致的审计 actor。
fn actor() -> AuditActor {
    actor_for(STARTER)
}

/// 构造指定后台账号的审计 actor。
fn actor_for(id: &str) -> AuditActor {
    AuditActor::new(id.to_string(), format!("login-{id}"), AccountKind::Admin)
}

/// 构造指定当前状态的后台账号。
fn account(id: &str, status: AccountStatus) -> AccountCore {
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
            status,
            email: None,
            phone: None,
            avatar: None,
        },
    )
    .expect("测试账号")
}

/// 写入撤回与提交启动需要的账号、角色、动作权限和组织 DataScope。
async fn seed_authorization(db: &Database, state: RuntimeState) {
    let approver_status = match state {
        RuntimeState::Blocked(ApprovalBlockerCode::ApproverAccountInactive) => AccountStatus::Suspended,
        RuntimeState::Running | RuntimeState::Blocked(_) => AccountStatus::Active,
    };
    for account in [
        account(STARTER, AccountStatus::Active),
        account(APPROVER, approver_status),
        account(LATER_APPROVER, AccountStatus::Active),
    ] {
        db.accounts()
            .create(&account, &mut NoTransaction)
            .await
            .expect("写入撤回链账号");
    }
    for role_id in [CANCEL_ROLE, APPROVER_ROLE, LATER_APPROVER_ROLE] {
        let role = Role::new(
            role_id.to_string(),
            RoleData {
                name: role_id.to_string(),
                description: None,
                system: false,
            },
        )
        .expect("审批角色");
        db.roles()
            .create(&role, &mut NoTransaction)
            .await
            .expect("写入审批角色");
    }

    let mut adapter = MongoCasbinAdapter::new(db.clone());
    assert!(adapter
        .add_policies(
            "g",
            "g",
            vec![
                vec![
                    subject(AccountKind::Admin, STARTER),
                    format!("role:{CANCEL_ROLE}"),
                ],
                vec![
                    subject(AccountKind::Admin, APPROVER),
                    format!("role:{APPROVER_ROLE}"),
                ],
                vec![
                    subject(AccountKind::Admin, LATER_APPROVER),
                    format!("role:{LATER_APPROVER_ROLE}"),
                ],
            ],
        )
        .await
        .expect("绑定撤回与审批角色"));
    assert!(adapter
        .add_policies(
            "p",
            "p",
            vec![
                vec![
                    format!("role:{CANCEL_ROLE}"),
                    "approval_instance".to_string(),
                    "cancel".to_string(),
                ],
                vec![
                    format!("role:{CANCEL_ROLE}"),
                    "stock_adjustment".to_string(),
                    "detail".to_string(),
                ],
                vec![
                    format!("role:{CANCEL_ROLE}"),
                    "stock_adjustment".to_string(),
                    "submit".to_string(),
                ],
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
                    format!("role:{LATER_APPROVER_ROLE}"),
                    "approval_instance".to_string(),
                    "decide".to_string(),
                ],
                vec![
                    format!("role:{LATER_APPROVER_ROLE}"),
                    "stock_adjustment".to_string(),
                    "detail".to_string(),
                ],
            ],
        )
        .await
        .expect("写入撤回、提交、决定与对象读取权限"));
    for (scope_id, role_id) in [
        ("scope-stock-adjustment-canceller", CANCEL_ROLE),
        ("scope-stock-adjustment-approver", APPROVER_ROLE),
        ("scope-stock-adjustment-later", LATER_APPROVER_ROLE),
    ] {
        let scope = DataScope::new(
            DataScopeId::new(scope_id),
            DataScopeData {
                subject_type: DataScopeSubjectType::Role,
                subject_id: role_id.to_string(),
                scope_type: DataScopeType::Organization,
                scope_targets: vec![ORGANIZATION.to_string()],
            },
        )
        .expect("审批组织范围");
        db.data_scopes()
            .create(&scope, &mut NoTransaction)
            .await
            .expect("写入审批组织范围");
    }
}

/// 写入单节点已发布库存调整审批定义。
async fn seed_definition(db: &Database) {
    let mut definition = ApprovalProcessDefinition::new_draft(
        ApprovalProcessDefinitionId::new(DEFINITION_ID),
        ProcessKind::StockAdjustment,
        1,
        "库存调整原子性验收",
        NODE_KEY,
        participant("definition-admin"),
        at(1),
    )
    .expect("审批定义");
    definition
        .publish(participant("definition-admin"), at(2))
        .expect("发布审批定义");
    db.approval_process_definitions()
        .create(&definition, &mut NoTransaction)
        .await
        .expect("写入已发布定义");

    let node = ApprovalNodeDefinition::new(NewNodeDefinition {
        id: ApprovalNodeDefinitionId::new("node-stock-adjustment-atomicity"),
        process_definition_id: ApprovalProcessDefinitionId::new(DEFINITION_ID),
        node_key: NODE_KEY.to_string(),
        node_name: "仓储复核".to_string(),
        node_purpose: None,
        display_order: 1,
        assignee_participant_id: participant(APPROVER),
        assignee_label_snapshot: "库存复核人".to_string(),
        at: at(1),
    })
    .expect("审批节点");
    db.approval_node_definitions()
        .create(&node, &mut NoTransaction)
        .await
        .expect("写入审批节点");

    for transition in [
        ApprovalTransitionDefinition::to_approved(
            ApprovalTransitionDefinitionId::new("transition-stock-adjustment-approve"),
            ApprovalProcessDefinitionId::new(DEFINITION_ID),
            NODE_KEY,
            ApprovalTransitionEvent::Approve,
            at(1),
        )
        .expect("通过终态连线"),
        ApprovalTransitionDefinition::to_node(
            ApprovalTransitionDefinitionId::new("transition-stock-adjustment-reject"),
            ApprovalProcessDefinitionId::new(DEFINITION_ID),
            NODE_KEY,
            ApprovalTransitionEvent::Reject,
            NODE_KEY,
            at(1),
        )
        .expect("驳回重启连线"),
    ] {
        db.approval_transition_definitions()
            .create(&transition, &mut NoTransaction)
            .await
            .expect("写入审批连线");
    }
}

/// 写入入口与后续节点均完整连通的定义，专用于证明启动会校验全图候选人。
async fn seed_two_node_definition(db: &Database) {
    let mut definition = ApprovalProcessDefinition::new_draft(
        ApprovalProcessDefinitionId::new(DEFINITION_ID),
        ProcessKind::StockAdjustment,
        1,
        "库存调整全图候选验收",
        NODE_KEY,
        participant("definition-admin"),
        at(1),
    )
    .expect("两节点审批定义");
    definition
        .publish(participant("definition-admin"), at(2))
        .expect("发布两节点审批定义");
    db.approval_process_definitions()
        .create(&definition, &mut NoTransaction)
        .await
        .expect("写入两节点已发布定义");

    for node in [
        ApprovalNodeDefinition::new(NewNodeDefinition {
            id: ApprovalNodeDefinitionId::new("node-stock-adjustment-entry"),
            process_definition_id: ApprovalProcessDefinitionId::new(DEFINITION_ID),
            node_key: NODE_KEY.to_string(),
            node_name: "仓储复核".to_string(),
            node_purpose: None,
            display_order: 1,
            assignee_participant_id: participant(APPROVER),
            assignee_label_snapshot: "库存复核人".to_string(),
            at: at(1),
        })
        .expect("入口审批节点"),
        ApprovalNodeDefinition::new(NewNodeDefinition {
            id: ApprovalNodeDefinitionId::new("node-stock-adjustment-later"),
            process_definition_id: ApprovalProcessDefinitionId::new(DEFINITION_ID),
            node_key: LATER_NODE_KEY.to_string(),
            node_name: "财务复核".to_string(),
            node_purpose: None,
            display_order: 2,
            assignee_participant_id: participant(LATER_APPROVER),
            assignee_label_snapshot: "财务复核人".to_string(),
            at: at(1),
        })
        .expect("后续审批节点"),
    ] {
        db.approval_node_definitions()
            .create(&node, &mut NoTransaction)
            .await
            .expect("写入两节点审批节点");
    }

    for transition in [
        ApprovalTransitionDefinition::to_node(
            ApprovalTransitionDefinitionId::new("transition-entry-approve-later"),
            ApprovalProcessDefinitionId::new(DEFINITION_ID),
            NODE_KEY,
            ApprovalTransitionEvent::Approve,
            LATER_NODE_KEY,
            at(1),
        )
        .expect("入口通过到后续节点"),
        ApprovalTransitionDefinition::to_node(
            ApprovalTransitionDefinitionId::new("transition-entry-reject-entry"),
            ApprovalProcessDefinitionId::new(DEFINITION_ID),
            NODE_KEY,
            ApprovalTransitionEvent::Reject,
            NODE_KEY,
            at(1),
        )
        .expect("入口驳回回入口"),
        ApprovalTransitionDefinition::to_approved(
            ApprovalTransitionDefinitionId::new("transition-later-approve-terminal"),
            ApprovalProcessDefinitionId::new(DEFINITION_ID),
            LATER_NODE_KEY,
            ApprovalTransitionEvent::Approve,
            at(1),
        )
        .expect("后续通过终态"),
        ApprovalTransitionDefinition::to_node(
            ApprovalTransitionDefinitionId::new("transition-later-reject-entry"),
            ApprovalProcessDefinitionId::new(DEFINITION_ID),
            LATER_NODE_KEY,
            ApprovalTransitionEvent::Reject,
            NODE_KEY,
            at(1),
        )
        .expect("后续驳回回入口"),
    ] {
        db.approval_transition_definitions()
            .create(&transition, &mut NoTransaction)
            .await
            .expect("写入两节点审批连线");
    }
}

/// 写入审批中的库存调整单及其冻结定义绑定。
async fn seed_adjustment(db: &Database, adjustment_id: &str) -> StockAdjustment {
    let mut adjustment = StockAdjustment::new(
        StockAdjustmentId::new(adjustment_id),
        StockAdjustmentData {
            adjustment_no: format!("ADJ-{adjustment_id}"),
            warehouse_id: entities::ids::WarehouseId::new(ORGANIZATION),
            reason_type: AdjustmentReasonType::StockGain,
            prepared_by: STARTER.to_string(),
            note: Some("原子性验收".to_string()),
            occurred_at: Some(Instant::from_unix_secs(10)),
        },
        STARTER,
    )
    .expect("库存调整单");
    adjustment.start_approval().expect("进入审批中");
    db.stock_adjustments()
        .create(&adjustment, &mut NoTransaction)
        .await
        .expect("写入库存调整单");

    let mut document = BusinessDocument::new(
        BusinessDocumentId::new(adjustment_id),
        BusinessDocumentData {
            document_type: DocumentType::StockAdjustment,
            document_no: adjustment.adjustment_no.clone(),
        },
    )
    .expect("业务单据注册");
    document
        .bind_approval_definition(
            ApprovalDefinitionBinding::new(
                ApprovalProcessDefinitionId::new(DEFINITION_ID),
                1,
                Instant::from_unix_secs(2),
            )
            .expect("冻结定义绑定"),
        )
        .expect("绑定审批定义");
    db.business_documents()
        .create(&document, &mut NoTransaction)
        .await
        .expect("写入业务单据注册");
    adjustment
}

/// 写入精确 subject version 的实例、当前执行、启动收据、快照与可选开放任务。
async fn seed_runtime(
    db: &Database,
    adjustment: &StockAdjustment,
    subject_version: u32,
    instance_id: &str,
    execution_id: &str,
    task_id: &str,
    state: RuntimeState,
) -> RuntimeSeed {
    let instance_key = ApprovalProcessInstanceId::new(instance_id);
    let execution_key = ApprovalNodeExecutionId::new(execution_id);
    let mut instance = ApprovalProcessInstance::start_running(NewProcessInstance {
        id: instance_key.clone(),
        process_definition_id: ApprovalProcessDefinitionId::new(DEFINITION_ID),
        definition_version: 1,
        process_kind: ProcessKind::StockAdjustment,
        subject: SubjectRef::new(DocumentType::StockAdjustment.as_str(), &adjustment.base.id)
            .expect("库存调整主体"),
        subject_version,
        started_by: participant(STARTER),
        at: at(10),
    })
    .expect("审批实例");
    let mut execution = ApprovalNodeExecution::new_active(NewNodeExecution {
        id: execution_key.clone(),
        process_instance_id: instance_key.clone(),
        node_key: NODE_KEY.to_string(),
        node_name: "仓储复核".to_string(),
        round_no: 1,
        execution_no: 1,
        assignment_source: ApprovalExecutionAssignmentSource::Definition,
        replaces_execution_id: None,
        assignee_participant_id: participant(APPROVER),
        assignee_name_snapshot: "库存复核人".to_string(),
        at: at(10),
    })
    .expect("当前执行");
    instance
        .set_current_execution(execution_key.clone(), at(10))
        .expect("设置当前执行");
    if let RuntimeState::Blocked(code) = state {
        execution.block(code, at(11)).expect("阻塞当前执行");
        instance.enter_blocked(code, at(11)).expect("阻塞审批实例");
    }
    let start_receipt = fixture_receipt(
        &format!("receipt-start-{instance_id}"),
        ApprovalCommandKind::StartApproval,
        instance_id,
        &format!("start-{instance_id}"),
        &format!("start-digest-{instance_id}"),
        instance_id,
        at(10),
    );
    db.bpm_workflow()
        .create_bpm_runtime(
            &instance,
            &[],
            &execution,
            &start_receipt,
            &ApprovalInstanceListProjection {
                current_node_key: Some(NODE_KEY.to_string()),
                current_node_name: Some("仓储复核".to_string()),
                current_assignee_participant_id: Some(APPROVER.to_string()),
                current_assignee_name: Some("库存复核人".to_string()),
                last_status_changed_at: Some(match state {
                    RuntimeState::Running => 10,
                    RuntimeState::Blocked(_) => 11,
                }),
                ..ApprovalInstanceListProjection::default()
            },
            &mut NoTransaction,
        )
        .await
        .expect("写入审批运行事实");

    let snapshot = ApprovalSubjectSnapshot::new(
        ApprovalSubjectSnapshotId::new(format!("snapshot-{instance_id}")),
        instance_key,
        DocumentType::StockAdjustment,
        adjustment.base.id.clone(),
        subject_version,
        ApprovalSubjectSnapshotPayload {
            document_no: adjustment.adjustment_no.clone(),
            responsible_org_id: ORGANIZATION.to_string(),
            submitted_by: STARTER.to_string(),
            submitted_at: Instant::from_unix_secs(10),
            counterparty: Some(ApprovalSubjectCounterparty::Warehouse {
                warehouse_id: entities::ids::WarehouseId::new(ORGANIZATION),
            }),
            total_amount: None,
            total_quantity: Some("1".parse::<Quantity>().expect("测试数量")),
            line_count: 1,
        },
    )
    .expect("审批业务快照");
    db.approval_subject_snapshots()
        .create_immutable_snapshot(&snapshot, &mut NoTransaction)
        .await
        .expect("写入审批业务快照");

    let task = match state {
        RuntimeState::Running => {
            let task = WorkItem::new_document_approval(
                WorkItemId::new(task_id),
                DocumentApprovalWorkItemData {
                    approval_node_execution_id: execution_key,
                    business_object_type: DocumentType::StockAdjustment.as_str().to_string(),
                    business_object_id: adjustment.base.id.clone(),
                    subject_version: subject_version.to_string(),
                    owner_role: "stock_adjustment_approver".to_string(),
                    owner_organization_id: ORGANIZATION.to_string(),
                    owner_user_id: APPROVER.to_string(),
                    priority: WorkItemPriority::Normal,
                    due_at: None,
                },
                Instant::from_unix_secs(10),
            )
            .expect("开放审批任务");
            db.work_items()
                .create(&task, &mut NoTransaction)
                .await
                .expect("写入开放审批任务");
            Some(task)
        }
        RuntimeState::Blocked(_) => None,
    };

    RuntimeSeed {
        adjustment_id: adjustment.base.id.clone(),
        instance_id: instance.base.id.clone(),
        execution_id: execution.base.id.clone(),
        task_id: task.as_ref().map(|item| item.base.id.clone()),
        subject_version,
        adjustment_version: adjustment.base.version,
        instance_version: instance.base.version,
        execution_version: execution.base.version,
        task_version: task.as_ref().map(|item| item.base.version),
    }
}

/// 建立独立真实数据库和一条库存调整审批链。
async fn fixture(prefix: &str, state: RuntimeState) -> (TestDb, InventoryService, RuntimeSeed) {
    let fixture = TestDb::new(prefix).await.expect("测试数据库创建失败");
    ensure_indexes(fixture.db()).await.expect("索引创建失败");
    seed_authorization(fixture.db(), state).await;
    seed_definition(fixture.db()).await;
    let adjustment = seed_adjustment(fixture.db(), "adjustment-atomicity").await;
    let seed = seed_runtime(
        fixture.db(),
        &adjustment,
        adjustment.approval_subject_version,
        "instance-atomicity",
        "execution-atomicity",
        "task-atomicity",
        state,
    )
    .await;
    let service = InventoryService::new(
        fixture.db().clone(),
        iam::shared_rbac_service(fixture.db().clone()),
    );
    (fixture, service, seed)
}

/// 写入一条带明细、余额和冻结定义绑定的草稿库存调整单。
async fn seed_start_business(db: &Database, adjustment_id: &str) -> StartSeed {
    let adjustment = StockAdjustment::new(
        StockAdjustmentId::new(adjustment_id),
        StockAdjustmentData {
            adjustment_no: format!("ADJ-{adjustment_id}"),
            warehouse_id: entities::ids::WarehouseId::new(ORGANIZATION),
            reason_type: AdjustmentReasonType::StockGain,
            prepared_by: STARTER.to_string(),
            note: Some("提交前草稿".to_string()),
            occurred_at: Some(Instant::from_unix_secs(10)),
        },
        STARTER,
    )
    .expect("草稿库存调整单");
    let line = StockAdjustmentLine::new_for_reason(
        StockAdjustmentLineId::new(LINE_ID),
        AdjustmentReasonType::StockGain,
        StockAdjustmentLineData {
            stock_adjustment_id: StockAdjustmentId::new(adjustment_id),
            sku_id: SkuId::new(SKU_ID),
            quantity: "1".parse::<Quantity>().expect("草稿数量"),
            direction: MovementDirection::Increase,
        },
    )
    .expect("草稿库存调整明细");
    db.inventory()
        .create_stock_adjustment_with_lines(&adjustment, std::slice::from_ref(&line), &mut NoTransaction)
        .await
        .expect("写入草稿调整单与明细");

    let balance = StockBalance::new(
        StockBalanceId::new(BALANCE_ID),
        StockBalanceData {
            warehouse_id: entities::ids::WarehouseId::new(ORGANIZATION),
            sku_id: SkuId::new(SKU_ID),
            on_hand_quantity: "10".parse::<Quantity>().expect("账面数量"),
            reserved_quantity: "0".parse::<Quantity>().expect("预占数量"),
            available_quantity: "10".parse::<Quantity>().expect("可用数量"),
            last_movement_id: None,
        },
    )
    .expect("库存余额");
    db.stock_balances()
        .create(&balance, &mut NoTransaction)
        .await
        .expect("写入库存余额");

    let mut document = BusinessDocument::new(
        BusinessDocumentId::new(adjustment_id),
        BusinessDocumentData {
            document_type: DocumentType::StockAdjustment,
            document_no: adjustment.adjustment_no.clone(),
        },
    )
    .expect("草稿业务单据注册");
    document
        .bind_approval_definition(
            ApprovalDefinitionBinding::new(
                ApprovalProcessDefinitionId::new(DEFINITION_ID),
                1,
                Instant::from_unix_secs(2),
            )
            .expect("冻结提交定义绑定"),
        )
        .expect("绑定提交审批定义");
    db.business_documents()
        .create(&document, &mut NoTransaction)
        .await
        .expect("写入草稿业务单据注册");

    StartSeed {
        adjustment_id: adjustment.base.id,
        line_id: line.base.id,
        balance_id: balance.base.id,
        adjustment_version: adjustment.base.version,
        line_version: line.base.version,
        balance_version: balance.base.version,
        target_subject_version: 1,
    }
}

/// 建立提交启动专用的独立真实数据库。
async fn start_fixture(prefix: &str) -> (TestDb, InventoryService, StartSeed) {
    let fixture = TestDb::new(prefix).await.expect("测试数据库创建失败");
    ensure_indexes(fixture.db()).await.expect("索引创建失败");
    seed_authorization(fixture.db(), RuntimeState::Running).await;
    seed_definition(fixture.db()).await;
    let seed = seed_start_business(fixture.db(), "adjustment-start-atomicity").await;
    let service = InventoryService::new(
        fixture.db().clone(),
        iam::shared_rbac_service(fixture.db().clone()),
    );
    (fixture, service, seed)
}

/// 建立两节点定义的启动候选人全图校验数据库。
async fn candidate_start_fixture(prefix: &str) -> (TestDb, InventoryService, StartSeed) {
    let fixture = TestDb::new(prefix).await.expect("测试数据库创建失败");
    ensure_indexes(fixture.db()).await.expect("索引创建失败");
    seed_authorization(fixture.db(), RuntimeState::Running).await;
    seed_two_node_definition(fixture.db()).await;
    let seed = seed_start_business(fixture.db(), "adjustment-start-atomicity").await;
    let service = InventoryService::new(
        fixture.db().clone(),
        iam::shared_rbac_service(fixture.db().clone()),
    );
    (fixture, service, seed)
}

/// 构造完整且稳定的库存调整提交载荷。
fn submit_request(seed: &StartSeed, key: &str, note: &str) -> SubmitStockAdjustmentRequest {
    SubmitStockAdjustmentRequest {
        expected_version: seed.adjustment_version,
        expected_subject_version: seed.target_subject_version,
        reason_type: AdjustmentReasonType::StockGain,
        lines: vec![StockAdjustmentLineUpdateInput {
            line_id: seed.line_id.clone(),
            quantity: "2".to_string(),
            direction: Some(MovementDirection::Increase),
        }],
        balances: vec![ExpectedStockBalanceVersion {
            balance_id: seed.balance_id.clone(),
            expected_version: seed.balance_version,
        }],
        note: note.to_string(),
        occurred_at: 20,
        idempotency_key: key.to_string(),
    }
}

/// 复现历史通用 START 摘要，仅用于验证当前只读兼容分支。
fn legacy_start_digest() -> String {
    let canonical = [DEFINITION_ID, "1", "1", STARTER].join("\u{1f}");
    hex::encode(Sha256::digest(canonical.as_bytes()))
}

/// 通过 Mongo 驱动公开游标按 `id` 排序回读一组强类型事实。
async fn collect_models<T>(db: &Database, collection: &str, filter: Document) -> Vec<T>
where
    T: DeserializeOwned + Send + Sync + Unpin,
{
    let mut cursor = db
        .collection::<T>(collection)
        .find(filter)
        .sort(doc! { "id": 1 })
        .await
        .expect("查询跨集合事实");
    let mut facts = Vec::new();
    while cursor.advance().await.expect("推进跨集合事实游标") {
        facts.push(cursor.deserialize_current().expect("反序列化跨集合事实"));
    }
    facts
}

/// 回读提交启动涉及的全部业务、运行时、任务、收据、审计与通知事实。
async fn start_facts(db: &Database, seed: &StartSeed) -> StartFacts {
    StartFacts {
        adjustment: db
            .stock_adjustments()
            .find_by_id(&seed.adjustment_id, &mut NoTransaction)
            .await
            .expect("读取提交调整单")
            .expect("提交调整单必须存在"),
        line: db
            .stock_adjustment_lines()
            .find_by_id(&seed.line_id, &mut NoTransaction)
            .await
            .expect("读取提交明细")
            .expect("提交明细必须存在"),
        balance: db
            .stock_balances()
            .find_by_id(&seed.balance_id, &mut NoTransaction)
            .await
            .expect("读取提交余额")
            .expect("提交余额必须存在"),
        instances: collect_models(
            db,
            "approval_process_instances",
            doc! { "subject.subject_id": &seed.adjustment_id },
        )
        .await,
        executions: collect_models(db, "approval_node_executions", doc! {}).await,
        assignees: collect_models(db, "approval_instance_assignees", doc! {}).await,
        tasks: collect_models(
            db,
            "work_items",
            doc! { "business_object_id": &seed.adjustment_id },
        )
        .await,
        snapshots: collect_models(
            db,
            "approval_subject_snapshots",
            doc! { "business_object_id": &seed.adjustment_id },
        )
        .await,
        receipts: collect_models(db, "approval_command_receipts", doc! {}).await,
        outbox: collect_models(db, "approval_notification_outbox", doc! {}).await,
        audits: collect_models(
            db,
            "audit_logs",
            doc! {
                "action": "stock_adjustment.submit",
                "resource_id": &seed.adjustment_id,
            },
        )
        .await,
    }
}

/// 回读最终通过额外形成的库存流水、库存过账审计与审批决定审计。
async fn final_approval_facts(db: &Database, seed: &StartSeed) -> FinalApprovalFacts {
    FinalApprovalFacts {
        chain: start_facts(db, seed).await,
        movements: collect_models(
            db,
            "stock_movements",
            doc! { "source_document_id": &seed.adjustment_id },
        )
        .await,
        post_audits: collect_models(
            db,
            "audit_logs",
            doc! {
                "action": "stock_adjustment.post",
                "resource_id": &seed.adjustment_id,
            },
        )
        .await,
        decision_audits: collect_models(db, "audit_logs", doc! { "action": "approval.decide" }).await,
    }
}

/// 构造注入真实领域动作注册表的审批运行时服务。
fn production_runtime_service(db: &Database) -> ApprovalRuntimeService {
    let rbac = iam::shared_rbac_service(db.clone());
    let registry = Arc::new(ApprovalActionRegistry::new(db.clone(), Arc::clone(&rbac)));
    ApprovalRuntimeService::with_action_port(db.clone(), rbac, registry)
}

/// 为 failCommand 精确作用域创建带独立 application name 的数据库句柄。
async fn app_named_database(fixture: &TestDb, app_name: &str) -> Database {
    let uri = std::env::var("ERP_TEST_MONGO_URI").expect("真实 Mongo 测试连接串");
    let mut options = ClientOptions::parse(uri).await.expect("解析 Mongo 测试连接串");
    options.app_name = Some(app_name.to_string());
    Client::with_options(options)
        .expect("创建带 appName 的 Mongo client")
        .database(fixture.name())
}

#[derive(Debug)]
struct PolicyFindGate {
    preceding_find_count: u32,
    namespace_preceding_match_count: u32,
    first_transactional_collection: String,
    blocked_transactional_collection: String,
    policy_revision_find_position: u32,
}

/// 用独立成功链 profiler 定位第二条事务 find 在同 appName 全部 find 中的下标。
async fn locate_second_transactional_find(db: &Database, app_name: &str) -> PolicyFindGate {
    let mut cursor = db
        .collection::<Document>("system.profile")
        .find(doc! { "appName": app_name, "op": "query" })
        .sort(doc! { "$natural": 1_i32 })
        .await
        .expect("读取 policy drift trace find 顺序");
    let mut preceding = 0_u32;
    let mut first_transactional_find = None;
    let mut second_transactional_find = None;
    let mut policy_revision_find_position = None;
    let mut find_collections = Vec::new();
    while cursor.advance().await.expect("推进 policy drift trace profiler") {
        let entry = cursor
            .deserialize_current()
            .expect("反序列化 policy drift trace profiler");
        let command = entry.get_document("command").ok();
        let collection = command
            .and_then(|value| value.get_str("find").ok())
            .map(ToOwned::to_owned);
        let in_transaction = command.and_then(|value| value.get_bool("autocommit").ok()) == Some(false)
            && command.and_then(|value| value.get("lsid")).is_some()
            && command.and_then(|value| value.get("txnNumber")).is_some();
        if in_transaction {
            let collection = collection.clone().expect("事务 find collection");
            let lsid = command
                .and_then(|value| value.get_document("lsid").ok())
                .expect("事务 find lsid")
                .clone();
            let txn_number = command
                .and_then(|value| value.get("txnNumber"))
                .expect("事务 find txnNumber")
                .clone();
            if first_transactional_find.is_none() {
                first_transactional_find = Some((preceding, collection, lsid, txn_number));
            } else if second_transactional_find.is_none() {
                second_transactional_find = Some((preceding, collection, lsid, txn_number));
            }
        }
        if collection.as_deref() == Some("casbin_policy_state") && second_transactional_find.is_some() {
            policy_revision_find_position.get_or_insert(preceding);
        }
        find_collections.push(collection);
        preceding = preceding.checked_add(1).expect("find 命令计数溢出");
    }
    let (first_position, first_transactional_collection, first_lsid, first_txn_number) =
        first_transactional_find.expect("成功 trace 必须形成第一条带 lsid/txnNumber 的事务 find");
    let (second_position, blocked_transactional_collection, second_lsid, second_txn_number) =
        second_transactional_find.expect("成功 trace 必须形成第二条带 lsid/txnNumber 的事务 find");
    let policy_revision_find_position =
        policy_revision_find_position.expect("成功 trace 必须在授权快照前读取 casbin policy revision");
    assert!(
        first_position < second_position && second_position < policy_revision_find_position,
        "门闩必须位于首条事务读之后、授权快照 revision 读取之前"
    );
    assert_eq!(first_lsid, second_lsid, "门闩前后两条 find 必须属于同一 session");
    assert_eq!(
        first_txn_number, second_txn_number,
        "门闩前后两条 find 必须属于同一事务"
    );
    let namespace_preceding_match_count = u32::try_from(
        find_collections[..usize::try_from(second_position).expect("第二条 find 位置")]
            .iter()
            .filter(|candidate| candidate.as_deref() == Some(blocked_transactional_collection.as_str()))
            .count(),
    )
    .expect("namespace 前置 find 数量");
    PolicyFindGate {
        preceding_find_count: second_position,
        namespace_preceding_match_count,
        first_transactional_collection,
        blocked_transactional_collection,
        policy_revision_find_position,
    }
}

/// 读取 failCommand 累计实际进入次数。
async fn fail_command_entries(db: &Database) -> i64 {
    db.client()
        .database("admin")
        .run_command(doc! {
            "getParameter": 1_i32,
            "failpoint.failCommand": 1_i32,
        })
        .await
        .expect("读取 failCommand 状态")
        .get_document("failpoint.failCommand")
        .expect("failCommand 参数")
        .get_i64("timesEntered")
        .expect("failCommand timesEntered")
}

/// 在指定 appName 与 namespace 内跳过 trace 已证明的前置命令，再挂起目标 find。
async fn arm_policy_find_drift(
    db: &Database,
    app_name: &str,
    collection: &str,
    namespace_preceding_match_count: u32,
) -> i64 {
    let before = fail_command_entries(db).await;
    let namespace = format!("{}.{}", db.name(), collection);
    let mode = if namespace_preceding_match_count == 0 {
        doc! { "times": 1_i32 }
    } else {
        doc! {
            "skip": i32::try_from(namespace_preceding_match_count)
                .expect("namespace 前置 find 数量"),
        }
    };
    db.client()
        .database("admin")
        .run_command(doc! {
            "configureFailPoint": "failCommand",
            "mode": mode,
            "data": {
                "failCommands": ["find"],
                "appName": app_name,
                "namespace": namespace,
                "blockConnection": true,
                "blockTimeMS": 10_000_i32,
            },
        })
        .await
        .expect("挂起 policy drift 第二条事务 find");
    before
}

/// 由 MongoDB 原生 waitForFailPoint 证明目标 namespace 已进入 failpoint。
async fn wait_for_policy_find_failpoint(db: &Database, before: i64) -> bool {
    db.client()
        .database("admin")
        .run_command(doc! {
            "waitForFailPoint": "failCommand",
            "timesEntered": before + 1,
            "maxTimeMS": 5_000_i32,
        })
        .await
        .is_ok()
}

/// 关闭全局 failCommand。
async fn disable_fail_command(db: &Database) {
    db.client()
        .database("admin")
        .run_command(doc! {
            "configureFailPoint": "failCommand",
            "mode": "off",
        })
        .await
        .expect("关闭 failCommand");
}

/// 成功路径结束后同步删除测试数据库，避免异步 Drop 掩盖清理结果。
async fn cleanup(fixture: TestDb) {
    fixture.db().drop().await.expect("清理真实 Mongo 测试数据库");
    // `TestDb::Drop` 另起当前线程 runtime，再析构 Mongo client 时会触发驱动
    // connection requester 竞态；数据库已同步删除，禁止再执行该尽力清理器。
    std::mem::forget(fixture);
}

/// 读取 failpoint 进入次数，不读取或输出命令载荷。
async fn receipt_race_observation(db: &Database) -> ReceiptRaceObservation {
    let failpoint = db
        .client()
        .database("admin")
        .run_command(doc! {
            "getParameter": 1_i32,
            "failpoint.failCommand": 1_i32,
        })
        .await
        .expect("读取 failCommand 进入计数");
    let failpoint_entries = failpoint
        .get_document("failpoint.failCommand")
        .expect("failCommand 参数")
        .get_i64("timesEntered")
        .expect("failCommand timesEntered");
    ReceiptRaceObservation { failpoint_entries }
}

/// 阻塞并以 E11000 拒绝下一条 insert，强制另一会话先提交唯一收据。
async fn arm_receipt_duplicate_race(db: &Database) -> ReceiptRaceObservation {
    let before = receipt_race_observation(db).await;
    db.run_command(doc! { "profile": 2_i32, "slowms": 0_i32 })
        .await
        .expect("启用当前随机库 profiler");
    db.client()
        .database("admin")
        .run_command(doc! {
            "configureFailPoint": "failCommand",
            "mode": { "times": 1_i32 },
            "data": {
                "failCommands": ["insert"],
                "blockConnection": true,
                "blockTimeMS": 1_000_i32,
                "errorCode": 11_000_i32,
                "errorExtraInfo": {
                    "keyPattern": { "receipt_identity": 1_i32 },
                    "keyValue": { "receipt_identity": "forced-race-loser" },
                },
            },
        })
        .await
        .expect("独立副本集必须以 enableTestCommands=1 启动");
    before
}

/// 关闭独立测试副本集的一次性 failpoint 与当前随机库 profiler。
async fn disarm_receipt_duplicate_race(db: &Database) {
    db.client()
        .database("admin")
        .run_command(doc! {
            "configureFailPoint": "failCommand",
            "mode": "off",
        })
        .await
        .expect("关闭 receipt insert failpoint");
    db.run_command(doc! { "profile": 0_i32 })
        .await
        .expect("关闭当前随机库 profiler");
}

/// 只读取命令种类、错误码来源与计数，证明 loser E11000 后以新会话回放。
async fn assert_one_receipt_duplicate_profile(db: &Database, before: ReceiptRaceObservation) {
    let mut cursor = db
        .collection::<Document>("system.profile")
        .find(doc! {
            "ns": format!("{}.approval_command_receipts", db.name()),
            "command.insert": "approval_command_receipts",
            "ninserted": 1_i32,
        })
        .projection(doc! {
            "_id": 0_i32,
            "op": 1_i32,
            "ninserted": 1_i32,
        })
        .await
        .expect("读取 receipt insert profiler 证据");
    let mut evidence = Vec::new();
    while cursor.advance().await.expect("推进 profiler 证据") {
        evidence.push(cursor.deserialize_current().expect("反序列化 profiler 证据"));
    }
    assert_eq!(
        evidence.len(),
        1,
        "同键并发必须恰有一个成功 receipt insert winner"
    );
    assert_eq!(evidence[0].get_str("op").expect("profiler op"), "insert");
    assert_eq!(evidence[0].get_i32("ninserted").expect("profiler ninserted"), 1);

    let after = receipt_race_observation(db).await;
    assert_eq!(
        after.failpoint_entries,
        before.failpoint_entries + 1,
        "一次性 failCommand 必须只命中一个 loser insert 会话"
    );
    let failpoint = db
        .client()
        .database("admin")
        .run_command(doc! {
            "getParameter": 1_i32,
            "failpoint.failCommand": 1_i32,
        })
        .await
        .expect("回读 failCommand 错误码");
    let data = failpoint
        .get_document("failpoint.failCommand")
        .and_then(|state| state.get_document("data"))
        .expect("failCommand data");
    assert_eq!(data.get_i32("errorCode").expect("failCommand errorCode"), 11_000);
    assert_eq!(
        data.get_array("failCommands")
            .expect("failCommand command types")
            .as_slice(),
        &[Bson::String("insert".to_string())]
    );
}

/// 测试专用 raw drift 注入与恢复；不触碰实体版本，便于逐项验证失败零写。
async fn set_raw_field(db: &Database, collection: &str, id: &str, field: &str, value: Bson) {
    let mut set = Document::new();
    set.insert(field, value);
    let result = db
        .collection::<Document>(collection)
        .update_one(doc! { "id": id }, doc! { "$set": set })
        .await
        .expect("注入或恢复 identity drift");
    assert_eq!(result.matched_count, 1, "identity drift 目标必须唯一存在");
}

/// 在当前 drift 已落库时，经生产注册表提交最终决定并证明全链零写。
async fn assert_final_identity_drift_is_zero_write(
    db: &Database,
    seed: &StartSeed,
    task_id: &str,
    task_version: u64,
    key: &str,
    label: &str,
) {
    let before = final_approval_facts(db, seed).await;
    let runtime = production_runtime_service(db);
    let result = runtime
        .submit_decision(
            &actor_for(APPROVER),
            task_id,
            "APPROVE",
            Some("identity drift 必须失败关闭"),
            task_version,
            key,
        )
        .await;
    assert!(result.is_err(), "{label} 漂移必须拒绝最终决定: {result:?}");
    assert_eq!(
        final_approval_facts(db, seed).await,
        before,
        "{label} 漂移不得写 movement、balance、document、BPM、task、receipt、outbox 或审计"
    );
}

/// 断言候选人校验失败时业务与审批集合保持零写，并立即清理独立数据库。
async fn assert_start_candidate_rejected(
    fixture: TestDb,
    service: InventoryService,
    seed: StartSeed,
    label: &str,
) {
    let before = start_facts(fixture.db(), &seed).await;
    let result = service
        .submit_stock_adjustment(
            &seed.adjustment_id,
            submit_request(&seed, &format!("candidate-{label}"), "候选人失败关闭"),
            &actor(),
        )
        .await;
    assert!(result.is_err(), "候选人 {label} 失效必须拒绝提交");
    assert_eq!(
        start_facts(fixture.db(), &seed).await,
        before,
        "候选人 {label} 校验失败不得改写业务、运行时、任务、快照、收据、审计或 outbox"
    );
    drop(service);
    cleanup(fixture).await;
}

/// 构造与 fixture 精确一致的普通撤回命令。
fn cancel_request(seed: &RuntimeSeed, key: &str, reason: &str) -> CancelStockAdjustmentApprovalRequest {
    CancelStockAdjustmentApprovalRequest {
        expected_version: seed.adjustment_version,
        approval_process_instance_id: seed.instance_id.clone(),
        expected_subject_version: seed.subject_version,
        expected_instance_version: seed.instance_version,
        expected_execution_version: seed.execution_version,
        expected_task_version: seed.task_version,
        reason: reason.to_string(),
        idempotency_key: key.to_string(),
    }
}

/// 断言两条不可观察分支向调用方暴露完全相同的服务错误投影。
fn assert_same_error_projection(left: &Error, right: &Error, label: &str) {
    assert_eq!(
        std::mem::discriminant(left),
        std::mem::discriminant(right),
        "{label} 必须映射同一 HTTP 状态类别"
    );
    assert_eq!(left.code(), right.code(), "{label} 稳定错误码必须相同");
    assert_eq!(left.to_string(), right.to_string(), "{label} 对外文案必须相同");
}

/// 回读普通撤回涉及的全部持久化事实。
async fn stored_facts(db: &Database, seed: &RuntimeSeed, key: &str) -> StoredFacts {
    let adjustment = db
        .stock_adjustments()
        .find_by_id(&seed.adjustment_id, &mut NoTransaction)
        .await
        .expect("读取库存调整单")
        .expect("库存调整单必须存在");
    let instance = db
        .bpm_workflow()
        .find_instance_by_id(
            &ApprovalProcessInstanceId::new(seed.instance_id.clone()),
            &mut NoTransaction,
        )
        .await
        .expect("读取审批实例")
        .expect("审批实例必须存在");
    let execution = db
        .bpm_workflow()
        .find_execution_by_id(
            &ApprovalNodeExecutionId::new(seed.execution_id.clone()),
            &mut NoTransaction,
        )
        .await
        .expect("读取审批执行")
        .expect("审批执行必须存在");
    let task = match seed.task_id.as_deref() {
        Some(task_id) => Some(
            db.work_items()
                .find_by_id(task_id, &mut NoTransaction)
                .await
                .expect("读取审批任务")
                .expect("审批任务必须存在"),
        ),
        None => None,
    };
    let cancel_receipt = db
        .bpm_workflow()
        .find_command_receipt(
            ApprovalCommandKind::CancelApproval,
            &seed.instance_id,
            &IdempotencyKey::parse(key).expect("测试幂等键必须有效"),
            &mut NoTransaction,
        )
        .await
        .expect("读取撤回收据");
    let outbox_id = format!("cancelled:{}:1", seed.instance_id);
    let outbox = db
        .approval_notification_outbox()
        .find_by_id(&outbox_id, &mut NoTransaction)
        .await
        .expect("读取撤回通知 outbox");
    let cancel_audits = db
        .collection::<Document>("audit_logs")
        .count_documents(doc! {
            "action": "stock_adjustment.cancel_approval",
            "resource_id": &seed.adjustment_id,
        })
        .await
        .expect("统计撤回审计");
    let cancel_audit = db
        .collection::<Document>("audit_logs")
        .find_one(doc! {
            "action": "stock_adjustment.cancel_approval",
            "resource_id": &seed.adjustment_id,
        })
        .await
        .expect("读取撤回审计");
    StoredFacts {
        adjustment,
        instance,
        execution,
        task,
        cancel_receipt,
        outbox,
        cancel_audit,
        cancel_audits,
    }
}

/// 断言一条普通撤回链的全部成功终态。
fn assert_cancelled(facts: &StoredFacts, seed: &RuntimeSeed, reason: &str) {
    assert_eq!(facts.adjustment.status, StockAdjustmentState::Draft);
    assert_eq!(facts.adjustment.approval_subject_version, seed.subject_version);
    assert_eq!(
        facts.adjustment.base.version,
        seed.adjustment_version.checked_add(1).expect("调整单版本递增")
    );
    assert_eq!(facts.instance.status, ApprovalProcessInstanceStatus::Cancelled);
    assert_eq!(facts.instance.subject_version, seed.subject_version);
    assert_eq!(
        facts.instance.base.version,
        seed.instance_version.checked_add(1).expect("实例版本递增")
    );
    assert_eq!(facts.execution.status, ApprovalNodeExecutionStatus::Cancelled);
    assert!(facts.execution.ended_at.is_some());
    assert_eq!(
        facts.execution.base.version,
        seed.execution_version.checked_add(1).expect("执行版本递增")
    );
    match (&seed.task_id, &facts.task) {
        (Some(_), Some(task)) => {
            assert_eq!(task.status, WorkItemStatus::Closed);
            assert_eq!(
                task.base.version,
                seed.task_version
                    .expect("运行实例必须有任务版本")
                    .checked_add(1)
                    .expect("任务版本递增")
            );
        }
        (None, None) => {}
        other => panic!("任务终态与运行形状不一致: {other:?}"),
    }
    let receipt = facts.cancel_receipt.as_ref().expect("撤回收据必须提交");
    assert_eq!(receipt.command_kind, ApprovalCommandKind::CancelApproval);
    assert_eq!(receipt.scope_id, seed.instance_id);
    assert_eq!(receipt.result_ref, seed.instance_id);
    let outbox = facts.outbox.as_ref().expect("撤回通知必须提交");
    assert_eq!(outbox.event_kind, ApprovalNotificationEventKind::Cancelled);
    assert_eq!(outbox.dedup_key, format!("cancelled:{}:1", seed.instance_id));
    assert_eq!(
        outbox.recipient_user_ids,
        vec![APPROVER.to_string(), STARTER.to_string()]
    );
    assert_eq!(facts.cancel_audits, 1);
    let expected_message = format!(
        "instance={}:{} authority=submitter reason={reason}",
        seed.instance_id.len(),
        seed.instance_id,
    );
    assert_eq!(
        facts
            .cancel_audit
            .as_ref()
            .and_then(|audit| audit.get_str("actor_id").ok()),
        Some(STARTER),
        "唯一撤回审计必须冻结原始撤回人"
    );
    assert_eq!(
        facts
            .cancel_audit
            .as_ref()
            .and_then(|audit| audit.get_str("message").ok()),
        Some(expected_message.as_str())
    );
}

/// 断言库存调整提交启动一次性形成全部签署事实。
fn assert_started(facts: &StartFacts, seed: &StartSeed, note: &str, key: &str) {
    assert_eq!(facts.adjustment.status, StockAdjustmentState::InApproval);
    assert_eq!(
        facts.adjustment.approval_subject_version,
        seed.target_subject_version
    );
    assert_eq!(facts.adjustment.base.version, seed.adjustment_version + 1);
    assert_eq!(facts.adjustment.reason_type, AdjustmentReasonType::StockGain);
    assert_eq!(facts.adjustment.note.as_deref(), Some(note));
    assert_eq!(facts.adjustment.occurred_at, Some(Instant::from_unix_secs(20)));
    assert_eq!(facts.line.base.version, seed.line_version + 1);
    assert_eq!(facts.line.quantity, "2".parse::<Quantity>().expect("提交数量"));
    assert_eq!(facts.line.direction, MovementDirection::Increase);
    assert_eq!(facts.balance.base.version, seed.balance_version);
    assert_eq!(
        facts.balance.on_hand_quantity,
        "10".parse::<Quantity>().expect("原余额")
    );

    let [instance] = facts.instances.as_slice() else {
        panic!("提交必须且只能形成一个审批实例: {:?}", facts.instances)
    };
    assert_eq!(instance.status, ApprovalProcessInstanceStatus::Running);
    assert_eq!(instance.process_kind, ProcessKind::StockAdjustment);
    assert_eq!(
        instance.subject.subject_kind(),
        DocumentType::StockAdjustment.as_str()
    );
    assert_eq!(instance.subject.subject_id(), seed.adjustment_id);
    assert_eq!(instance.subject_version, seed.target_subject_version);
    assert_eq!(instance.process_definition_id.as_ref(), DEFINITION_ID);
    assert_eq!(instance.definition_version, 1);
    assert_eq!(instance.started_by.as_str(), STARTER);
    assert_eq!(instance.current_round_no, 1);

    let [execution] = facts.executions.as_slice() else {
        panic!("提交必须且只能形成一个入口执行: {:?}", facts.executions)
    };
    assert_eq!(execution.process_instance_id.as_ref(), instance.base.id);
    assert_eq!(execution.status, ApprovalNodeExecutionStatus::Active);
    assert_eq!(execution.node_key, NODE_KEY);
    assert_eq!(execution.node_name, "仓储复核");
    assert_eq!(execution.round_no, 1);
    assert_eq!(execution.execution_no, 1);
    assert_eq!(
        execution.assignment_source,
        ApprovalExecutionAssignmentSource::Definition
    );
    assert_eq!(execution.assignee_participant_id.as_str(), APPROVER);
    assert_eq!(execution.assignee_name_snapshot, "库存复核人");
    assert_eq!(
        instance.current_node_execution_id.as_ref().map(AsRef::as_ref),
        Some(execution.base.id.as_str())
    );

    let [assignee] = facts.assignees.as_slice() else {
        panic!("提交必须且只能冻结一个实例审批人: {:?}", facts.assignees)
    };
    assert_eq!(assignee.process_instance_id.as_ref(), instance.base.id);
    assert_eq!(assignee.node_key, NODE_KEY);
    assert_eq!(assignee.definition_assignee_participant_id.as_str(), APPROVER);
    assert_eq!(assignee.current_assignee_participant_id.as_str(), APPROVER);
    assert_eq!(
        assignee.assignment_source,
        ApprovalAssigneeBindingSource::Definition
    );

    let [task] = facts.tasks.as_slice() else {
        panic!("提交必须且只能形成一个审批 WorkItem: {:?}", facts.tasks)
    };
    assert_eq!(task.work_item_type, WorkItemType::DocumentApproval);
    assert_eq!(task.status, WorkItemStatus::Open);
    assert_eq!(
        task.approval_node_execution_id.as_ref().map(AsRef::as_ref),
        Some(execution.base.id.as_str())
    );
    assert_eq!(task.business_object_type, DocumentType::StockAdjustment.as_str());
    assert_eq!(task.business_object_id, seed.adjustment_id);
    assert_eq!(task.subject_version, seed.target_subject_version.to_string());
    assert_eq!(task.owner_role, "stock_adjustment_approver");
    assert_eq!(task.owner_organization_id, ORGANIZATION);
    assert_eq!(task.owner_user_id.as_deref(), Some(APPROVER));
    assert_eq!(task.responsibility_actor_ids, vec![APPROVER.to_string()]);
    assert_eq!(task.assignment_source, AssignmentSource::ApprovalRuntime);
    assert_eq!(task.priority, WorkItemPriority::Normal);

    let [snapshot] = facts.snapshots.as_slice() else {
        panic!("提交必须且只能形成一个不可变快照: {:?}", facts.snapshots)
    };
    assert_eq!(snapshot.approval_process_instance_id.as_ref(), instance.base.id);
    assert_eq!(snapshot.document_type, DocumentType::StockAdjustment);
    assert_eq!(snapshot.business_object_id, seed.adjustment_id);
    assert_eq!(snapshot.subject_version, seed.target_subject_version);
    assert_eq!(
        snapshot.payload.document_no,
        format!("ADJ-{}", seed.adjustment_id)
    );
    assert_eq!(snapshot.payload.responsible_org_id, ORGANIZATION);
    assert_eq!(snapshot.payload.submitted_by, STARTER);
    assert_eq!(snapshot.payload.line_count, 1);
    assert_eq!(
        snapshot.payload.total_quantity,
        Some("2".parse::<Quantity>().expect("快照合计数量"))
    );
    assert_eq!(
        snapshot.payload.counterparty,
        Some(ApprovalSubjectCounterparty::Warehouse {
            warehouse_id: entities::ids::WarehouseId::new(ORGANIZATION),
        })
    );

    let [receipt] = facts.receipts.as_slice() else {
        panic!("提交必须且只能形成一条启动收据: {:?}", facts.receipts)
    };
    assert_eq!(receipt.command_kind, ApprovalCommandKind::StartApproval);
    assert_eq!(
        receipt.scope_id,
        format!(
            "stock_adjustment\u{1f}stock_adjustment\u{1f}{}\u{1f}{}",
            seed.adjustment_id, seed.target_subject_version
        )
    );
    assert_eq!(receipt.idempotency_key, key);
    assert_eq!(receipt.result_ref, instance.base.id);
    assert!(receipt.payload_digest.starts_with("v1:"));

    assert_eq!(facts.outbox.len(), 2);
    let started = facts
        .outbox
        .iter()
        .find(|item| item.event_kind == ApprovalNotificationEventKind::Started)
        .expect("Started outbox");
    let entered = facts
        .outbox
        .iter()
        .find(|item| item.event_kind == ApprovalNotificationEventKind::Entered)
        .expect("Entered outbox");
    assert_eq!(started.dedup_key, format!("started:{}", instance.base.id));
    assert_eq!(
        started.recipient_user_ids,
        vec![APPROVER.to_string(), STARTER.to_string()]
    );
    assert_eq!(entered.dedup_key, format!("entered:{}", execution.base.id));
    assert_eq!(entered.recipient_user_ids, vec![APPROVER.to_string()]);
    for outbox in [&started, &entered] {
        assert_eq!(outbox.template_params.document_type_label, "库存调整单");
        assert_eq!(
            outbox.template_params.document_no,
            format!("ADJ-{}", seed.adjustment_id)
        );
        assert_eq!(outbox.template_params.current_node_name, "仓储复核");
        assert_eq!(outbox.template_params.current_approver_display_name, "库存复核人");
        assert_eq!(outbox.template_params.round_no, 1);
        assert!(outbox.template_params.reject_reason_summary.is_none());
    }
    assert_eq!(facts.audits.len(), 1);
}

/// 断言生产注册表最终通过一次性形成库存与审批两侧终态。
fn assert_final_approved(facts: &FinalApprovalFacts, seed: &StartSeed, decision_key: &str) {
    let chain = &facts.chain;
    assert_eq!(chain.adjustment.status, StockAdjustmentState::Posted);
    assert_eq!(
        chain.adjustment.approval_subject_version,
        seed.target_subject_version
    );
    assert_eq!(chain.adjustment.base.version, seed.adjustment_version + 2);
    assert_eq!(chain.line.base.version, seed.line_version + 1);
    assert_eq!(chain.balance.on_hand_quantity, "12".parse::<Quantity>().unwrap());
    assert_eq!(
        chain.balance.available_quantity,
        "12".parse::<Quantity>().unwrap()
    );

    let [movement] = facts.movements.as_slice() else {
        panic!("最终通过必须且只能形成一条库存流水: {:?}", facts.movements)
    };
    assert_eq!(movement.movement_type, MovementType::StockGain);
    assert_eq!(movement.direction, MovementDirection::Increase);
    assert_eq!(movement.quantity, "2".parse::<Quantity>().unwrap());
    assert_eq!(movement.source_document_id, seed.adjustment_id);
    assert_eq!(movement.source_line_id.as_deref(), Some(seed.line_id.as_str()));
    assert_eq!(movement.fact.recorded_by, APPROVER);
    assert_eq!(
        chain.balance.last_movement_id.as_ref().map(AsRef::as_ref),
        Some(movement.base.id.as_str())
    );

    let [instance] = chain.instances.as_slice() else {
        panic!("最终通过必须保留唯一审批实例: {:?}", chain.instances)
    };
    let [execution] = chain.executions.as_slice() else {
        panic!("最终通过必须保留唯一审批执行: {:?}", chain.executions)
    };
    let [task] = chain.tasks.as_slice() else {
        panic!("最终通过必须保留唯一审批任务: {:?}", chain.tasks)
    };
    assert_eq!(instance.status, ApprovalProcessInstanceStatus::Approved);
    assert!(instance.ended_at.is_some());
    assert_eq!(execution.status, ApprovalNodeExecutionStatus::Approved);
    assert_eq!(
        execution.decided_by.as_ref().map(ParticipantId::as_str),
        Some(APPROVER)
    );
    assert!(execution.decided_at.is_some());
    assert!(execution.ended_at.is_some());
    assert_eq!(task.status, WorkItemStatus::Completed);
    assert_eq!(task.completed_by.as_deref(), Some(APPROVER));

    assert_eq!(chain.receipts.len(), 2);
    let decision_receipt = chain
        .receipts
        .iter()
        .find(|receipt| receipt.command_kind == ApprovalCommandKind::SubmitDecision)
        .expect("最终通过决定收据");
    assert_eq!(decision_receipt.scope_id, execution.base.id);
    assert_eq!(decision_receipt.idempotency_key, decision_key);
    assert_eq!(decision_receipt.result_ref, instance.base.id);
    assert!(decision_receipt.payload_digest.starts_with("v2:"));

    assert_eq!(chain.outbox.len(), 4);
    let approved = chain
        .outbox
        .iter()
        .find(|item| item.event_kind == ApprovalNotificationEventKind::NodeApproved)
        .expect("NodeApproved outbox");
    let completed = chain
        .outbox
        .iter()
        .find(|item| item.event_kind == ApprovalNotificationEventKind::Completed)
        .expect("Completed outbox");
    assert_eq!(approved.dedup_key, format!("approved:{}", execution.base.id));
    assert_eq!(approved.recipient_user_ids, vec![STARTER.to_string()]);
    assert_eq!(completed.dedup_key, format!("completed:{}", instance.base.id));
    assert_eq!(completed.recipient_user_ids, vec![STARTER.to_string()]);
    for outbox in [approved, completed] {
        assert_eq!(outbox.template_params.document_type_label, "库存调整单");
        assert_eq!(
            outbox.template_params.document_no,
            format!("ADJ-{}", seed.adjustment_id)
        );
        assert_eq!(outbox.template_params.current_node_name, "仓储复核");
        assert_eq!(outbox.template_params.current_approver_display_name, "库存复核人");
        assert_eq!(outbox.template_params.round_no, 1);
    }
    assert_eq!(chain.audits.len(), 1, "提交审计必须只保留一条");
    assert_eq!(facts.post_audits.len(), 1, "库存过账审计必须一条");
    assert_eq!(facts.decision_audits.len(), 1, "审批决定审计必须一条");
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn submit_start_is_atomic_and_v1_replay_is_zero_write() {
    require_mongo!(async {
        let (fixture, service, seed) = start_fixture("stock_adj_start_atomic").await;
        let request = submit_request(&seed, "start-atomic", "提交冻结值");
        let initial = start_facts(fixture.db(), &seed).await;
        assert_eq!(initial.adjustment.status, StockAdjustmentState::Draft);
        assert_eq!(initial.adjustment.approval_subject_version, 0);
        assert!(initial.instances.is_empty());
        assert!(initial.receipts.is_empty());
        assert!(initial.outbox.is_empty());
        assert!(initial.audits.is_empty());

        let view = service
            .submit_stock_adjustment(&seed.adjustment_id, request.clone(), &actor())
            .await
            .expect("完整提交启动必须原子成功");
        assert_eq!(view.adjustment.status, StockAdjustmentState::InApproval);
        let committed = start_facts(fixture.db(), &seed).await;
        assert_started(&committed, &seed, "提交冻结值", "start-atomic");

        let existing_key_wrong_actor = service
            .submit_stock_adjustment(&seed.adjustment_id, request.clone(), &actor_for(APPROVER))
            .await
            .expect_err("非原提交人不得利用已存在 key 进入收据回放");
        let mut missing_key_request = request.clone();
        missing_key_request.idempotency_key = "start-nonstarter-missing".to_string();
        let missing_key_wrong_actor = service
            .submit_stock_adjustment(&seed.adjustment_id, missing_key_request, &actor_for(APPROVER))
            .await
            .expect_err("非原提交人使用不存在 key 也必须同样拒绝");
        let existing_message = match &existing_key_wrong_actor {
            Error::Forbidden(message) => message,
            other => panic!("已存在 key 的非原提交人必须 Forbidden: {other:?}"),
        };
        let missing_message = match &missing_key_wrong_actor {
            Error::Forbidden(message) => message,
            other => panic!("不存在 key 的非原提交人必须 Forbidden: {other:?}"),
        };
        assert_eq!(existing_message, "当前账号不可提交该库存调整单");
        assert_eq!(missing_message, existing_message);
        assert_eq!(existing_key_wrong_actor.code(), missing_key_wrong_actor.code());
        assert_eq!(
            start_facts(fixture.db(), &seed).await,
            committed,
            "非原提交人无论 key 是否存在都不得写入或泄露不同投影"
        );

        let replay = service
            .submit_stock_adjustment(&seed.adjustment_id, request.clone(), &actor())
            .await
            .expect("V1 同键同完整载荷必须回放成功");
        assert_eq!(replay.adjustment.status, StockAdjustmentState::InApproval);
        assert_eq!(
            start_facts(fixture.db(), &seed).await,
            committed,
            "V1 回放不得改写单据、明细、余额、BPM、任务、快照、收据、审计或 outbox"
        );

        let mut different = request;
        different.note = "同键不同完整载荷".to_string();
        let conflict = service
            .submit_stock_adjustment(&seed.adjustment_id, different, &actor())
            .await
            .expect_err("V1 同键异载荷不得降级为 legacy 回放");
        assert_eq!(
            conflict.code(),
            Some(ErrorCode::ApprovalIdempotencyPayloadConflict)
        );
        assert_eq!(
            start_facts(fixture.db(), &seed).await,
            committed,
            "V1 异载荷冲突不得改写任何已提交事实"
        );
        cleanup(fixture).await;
    });
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn submit_result_requires_exact_receipt_scope_and_key() {
    require_mongo!(async {
        let (fixture, service, seed) = start_fixture("stock_adj_submit_result").await;
        let key = "submit-result-exact";
        let request = submit_request(&seed, key, "提交结果精确回读");
        let initial = start_facts(fixture.db(), &seed).await;
        let missing = service
            .stock_adjustment_submit_result(
                &seed.adjustment_id,
                StockAdjustmentSubmitResultQuery {
                    expected_subject_version: seed.target_subject_version,
                    idempotency_key: key.to_string(),
                },
                &actor(),
            )
            .await
            .expect_err("没有精确收据时不得根据草稿状态推断提交成功");
        assert!(matches!(
            missing,
            Error::NotFound(ref message) if message == "库存调整提交结果不存在"
        ));
        assert_eq!(start_facts(fixture.db(), &seed).await, initial);

        let submitted = service
            .submit_stock_adjustment(&seed.adjustment_id, request.clone(), &actor())
            .await
            .expect("形成可查询的提交收据");
        let committed = start_facts(fixture.db(), &seed).await;
        let original_receipt = committed
            .receipts
            .iter()
            .find(|receipt| {
                receipt.command_kind == ApprovalCommandKind::StartApproval && receipt.idempotency_key == key
            })
            .expect("原提交启动收据必须存在");
        let original_instance_id = original_receipt.result_ref.clone();
        let submitted_runtime = submitted
            .approval
            .instance
            .as_ref()
            .expect("fresh 提交结果必须带审批实例");
        assert_eq!(submitted_runtime.id, original_instance_id);
        assert_eq!(submitted_runtime.subject_version, "1");
        let exact = service
            .stock_adjustment_submit_result(
                &seed.adjustment_id,
                StockAdjustmentSubmitResultQuery {
                    expected_subject_version: seed.target_subject_version,
                    idempotency_key: key.to_string(),
                },
                &actor(),
            )
            .await
            .expect("精确 subject version 与 key 必须命中原提交结果");
        assert_eq!(exact, submitted);
        let runtime = exact.approval.instance.as_ref().expect("提交结果必须带审批实例");
        assert_eq!(runtime.id, original_instance_id);
        assert_eq!(runtime.subject_version, seed.target_subject_version.to_string());
        assert_eq!(start_facts(fixture.db(), &seed).await, committed);

        let hidden = service
            .stock_adjustment_submit_result(
                &seed.adjustment_id,
                StockAdjustmentSubmitResultQuery {
                    expected_subject_version: seed.target_subject_version,
                    idempotency_key: key.to_string(),
                },
                &actor_for(APPROVER),
            )
            .await
            .expect_err("非原提交人即使具备 detail/read scope 也不得探测精确收据");
        assert!(matches!(
            hidden,
            Error::NotFound(ref message) if message == "库存调整提交结果不存在"
        ));
        assert_eq!(
            start_facts(fixture.db(), &seed).await,
            committed,
            "非原提交人精确 key 查询必须隐藏且零写"
        );

        for (label, query) in [
            (
                "wrong-key",
                StockAdjustmentSubmitResultQuery {
                    expected_subject_version: seed.target_subject_version,
                    idempotency_key: "submit-result-wrong".to_string(),
                },
            ),
            (
                "wrong-subject-version",
                StockAdjustmentSubmitResultQuery {
                    expected_subject_version: seed.target_subject_version + 1,
                    idempotency_key: key.to_string(),
                },
            ),
        ] {
            let error = service
                .stock_adjustment_submit_result(&seed.adjustment_id, query, &actor())
                .await
                .unwrap_err();
            assert!(
                matches!(error, Error::NotFound(ref message) if message == "库存调整提交结果不存在"),
                "{label} 必须返回精确 NotFound: {error:?}"
            );
            assert_eq!(
                start_facts(fixture.db(), &seed).await,
                committed,
                "{label} 查询不得改写业务或审批事实"
            );
        }

        let original_instance = committed
            .instances
            .iter()
            .find(|instance| instance.base.id == original_instance_id)
            .expect("原提交审批实例必须存在");
        let original_execution = committed
            .executions
            .iter()
            .find(|execution| execution.process_instance_id.as_ref() == original_instance_id)
            .expect("原提交当前执行必须存在");
        let original_task = committed
            .tasks
            .iter()
            .find(|task| {
                task.approval_node_execution_id.as_ref().map(AsRef::as_ref)
                    == Some(original_execution.base.id.as_str())
            })
            .expect("原提交开放任务必须存在");
        let cancel_seed = RuntimeSeed {
            adjustment_id: seed.adjustment_id.clone(),
            instance_id: original_instance_id.clone(),
            execution_id: original_execution.base.id.clone(),
            task_id: Some(original_task.base.id.clone()),
            subject_version: seed.target_subject_version,
            adjustment_version: committed.adjustment.base.version,
            instance_version: original_instance.base.version,
            execution_version: original_execution.base.version,
            task_version: Some(original_task.base.version),
        };
        service
            .cancel_stock_adjustment_approval(
                &seed.adjustment_id,
                cancel_request(&cancel_seed, "submit-result-cancel", "取消后重新提交"),
                &actor(),
            )
            .await
            .expect("原提交必须可正常撤回");

        let cancelled = start_facts(fixture.db(), &seed).await;
        assert_eq!(cancelled.adjustment.status, StockAdjustmentState::Draft);
        let resubmit_seed = StartSeed {
            adjustment_id: seed.adjustment_id.clone(),
            line_id: seed.line_id.clone(),
            balance_id: seed.balance_id.clone(),
            adjustment_version: cancelled.adjustment.base.version,
            line_version: cancelled.line.base.version,
            balance_version: cancelled.balance.base.version,
            target_subject_version: seed.target_subject_version + 1,
        };
        let resubmit_key = "submit-result-resubmitted";
        let resubmitted = service
            .submit_stock_adjustment(
                &seed.adjustment_id,
                submit_request(&resubmit_seed, resubmit_key, "重新提交形成新实例"),
                &actor(),
            )
            .await
            .expect("撤回后必须可形成新提交实例");
        let after_resubmit = start_facts(fixture.db(), &resubmit_seed).await;
        let new_receipt = after_resubmit
            .receipts
            .iter()
            .find(|receipt| {
                receipt.command_kind == ApprovalCommandKind::StartApproval
                    && receipt.idempotency_key == resubmit_key
            })
            .expect("重新提交启动收据必须存在");
        let new_instance_id = new_receipt.result_ref.clone();
        assert_ne!(new_instance_id, original_instance_id);
        let resubmitted_runtime = resubmitted
            .approval
            .instance
            .as_ref()
            .expect("fresh 重新提交必须带新审批实例");
        assert_eq!(resubmitted_runtime.id, new_instance_id);
        assert_eq!(resubmitted_runtime.subject_version, "2");
        let new_instance_before = after_resubmit
            .instances
            .iter()
            .find(|instance| instance.base.id == new_instance_id)
            .expect("新审批实例必须存在")
            .clone();
        let new_execution_before = after_resubmit
            .executions
            .iter()
            .find(|execution| execution.process_instance_id.as_ref() == new_instance_id)
            .expect("新审批执行必须存在");
        let new_task_before = after_resubmit
            .tasks
            .iter()
            .find(|task| {
                task.approval_node_execution_id.as_ref().map(AsRef::as_ref)
                    == Some(new_execution_before.base.id.as_str())
            })
            .expect("新审批开放任务必须存在")
            .clone();
        let current_adjustment_version = after_resubmit.adjustment.base.version;

        let old_replay = service
            .submit_stock_adjustment(&seed.adjustment_id, request, &actor())
            .await
            .expect("旧 key 必须按原收据精确回放");
        let old_replay_runtime = old_replay
            .approval
            .instance
            .as_ref()
            .expect("旧 key 回放必须带原审批实例");
        assert_eq!(old_replay_runtime.id, original_instance_id);
        assert_eq!(old_replay_runtime.subject_version, "1");
        assert!(old_replay.approval.submit_command.is_none());
        assert!(old_replay.approval.cancel_command.is_none());
        assert!(old_replay.approval.allowed_actions.is_empty());

        let old_result = service
            .stock_adjustment_submit_result(
                &seed.adjustment_id,
                StockAdjustmentSubmitResultQuery {
                    expected_subject_version: seed.target_subject_version,
                    idempotency_key: key.to_string(),
                },
                &actor(),
            )
            .await
            .expect("旧 key 提交结果必须按原收据精确回读");
        let old_result_runtime = old_result
            .approval
            .instance
            .as_ref()
            .expect("旧提交结果必须带原审批实例");
        assert_eq!(old_result_runtime.id, original_instance_id);
        assert_eq!(old_result_runtime.subject_version, "1");
        assert!(old_result.approval.submit_command.is_none());
        assert!(old_result.approval.cancel_command.is_none());
        assert!(old_result.approval.allowed_actions.is_empty());

        let new_result = service
            .stock_adjustment_submit_result(
                &seed.adjustment_id,
                StockAdjustmentSubmitResultQuery {
                    expected_subject_version: resubmit_seed.target_subject_version,
                    idempotency_key: resubmit_key.to_string(),
                },
                &actor(),
            )
            .await
            .expect("新 key 提交结果必须按新收据精确回读");
        let new_result_runtime = new_result
            .approval
            .instance
            .as_ref()
            .expect("新提交结果必须带新审批实例");
        assert_eq!(new_result_runtime.id, new_instance_id);
        assert_eq!(new_result_runtime.subject_version, "2");

        let after_old_receipt_reads = start_facts(fixture.db(), &resubmit_seed).await;
        assert_eq!(
            after_old_receipt_reads, after_resubmit,
            "旧 key replay、旧/新 submit-result 均不得改写当前业务、审批或收据事实"
        );
        assert_eq!(
            after_old_receipt_reads.adjustment.base.version,
            current_adjustment_version
        );
        assert_eq!(
            after_old_receipt_reads
                .instances
                .iter()
                .find(|instance| instance.base.id == new_instance_id),
            Some(&new_instance_before)
        );
        assert_eq!(
            after_old_receipt_reads
                .tasks
                .iter()
                .find(|task| task.base.id == new_task_before.base.id),
            Some(&new_task_before)
        );
        drop(service);
        cleanup(fixture).await;
    });
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn concurrent_same_key_submit_start_profiles_duplicate_loser_and_recovers() {
    require_mongo!(async {
        let (fixture, _service, seed) = start_fixture("stock_adj_start_race").await;
        let rbac = iam::shared_rbac_service(fixture.db().clone());
        let left_service = InventoryService::new(fixture.db().clone(), Arc::clone(&rbac));
        let right_service = InventoryService::new(fixture.db().clone(), rbac);
        let left_request = submit_request(&seed, "start-race", "并发同键提交");
        let right_request = left_request.clone();
        let left_actor = actor();
        let right_actor = actor();
        let barrier = Barrier::new(2);
        let observation = arm_receipt_duplicate_race(fixture.db()).await;
        let left = async {
            barrier.wait().await;
            left_service
                .submit_stock_adjustment(&seed.adjustment_id, left_request, &left_actor)
                .await
        };
        let right = async {
            barrier.wait().await;
            right_service
                .submit_stock_adjustment(&seed.adjustment_id, right_request, &right_actor)
                .await
        };
        let (left, right) = tokio::join!(left, right);
        assert!(left.is_ok(), "左侧提交应成功或新会话回放: {left:?}");
        assert!(right.is_ok(), "右侧提交应成功或新会话回放: {right:?}");
        assert_one_receipt_duplicate_profile(fixture.db(), observation).await;
        disarm_receipt_duplicate_race(fixture.db()).await;

        let committed = start_facts(fixture.db(), &seed).await;
        assert_started(&committed, &seed, "并发同键提交", "start-race");
        assert_eq!(committed.instances.len(), 1);
        assert_eq!(committed.executions.len(), 1);
        assert_eq!(committed.tasks.len(), 1);
        assert_eq!(committed.receipts.len(), 1);
        assert_eq!(committed.outbox.len(), 2);
        assert_eq!(committed.audits.len(), 1);
        cleanup(fixture).await;
    });
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn submit_start_candidate_failures_are_zero_write() {
    require_mongo!(async {
        {
            let (fixture, service, seed) = candidate_start_fixture("stock_adj_candidate_inactive").await;
            let mut approver = fixture
                .db()
                .accounts()
                .find_by_id(LATER_APPROVER, &mut NoTransaction)
                .await
                .expect("读取候选账号")
                .expect("候选账号存在");
            approver.status = AccountStatus::Suspended;
            fixture
                .db()
                .accounts()
                .update(&mut approver, &mut NoTransaction)
                .await
                .expect("停用候选账号");
            assert_start_candidate_rejected(fixture, service, seed, "inactive").await;
        }
        {
            let (fixture, service, seed) = candidate_start_fixture("stock_adj_candidate_decide").await;
            let mut adapter = MongoCasbinAdapter::new(fixture.db().clone());
            assert!(adapter
                .remove_policy(
                    "p",
                    "p",
                    vec![
                        format!("role:{LATER_APPROVER_ROLE}"),
                        "approval_instance".to_string(),
                        "decide".to_string(),
                    ],
                )
                .await
                .expect("移除候选决定权限"));
            assert_start_candidate_rejected(fixture, service, seed, "decide").await;
        }
        {
            let (fixture, service, seed) = candidate_start_fixture("stock_adj_candidate_read").await;
            let mut adapter = MongoCasbinAdapter::new(fixture.db().clone());
            assert!(adapter
                .remove_policy(
                    "p",
                    "p",
                    vec![
                        format!("role:{LATER_APPROVER_ROLE}"),
                        "stock_adjustment".to_string(),
                        "detail".to_string(),
                    ],
                )
                .await
                .expect("移除候选对象读取权限"));
            assert_start_candidate_rejected(fixture, service, seed, "object-read").await;
        }
        {
            let (fixture, service, seed) = candidate_start_fixture("stock_adj_candidate_scope").await;
            fixture
                .db()
                .collection::<Document>("data_scopes")
                .update_one(
                    doc! { "id": "scope-stock-adjustment-later" },
                    doc! { "$set": { "scope_targets": ["another-warehouse"] } },
                )
                .await
                .expect("把候选人 DataScope 移出单据组织");
            assert_start_candidate_rejected(fixture, service, seed, "data-scope").await;
        }
        {
            let (fixture, service, seed) = candidate_start_fixture("stock_adj_candidate_sod").await;
            fixture
                .db()
                .collection::<Document>("approval_node_definitions")
                .update_one(
                    doc! { "process_definition_id": DEFINITION_ID, "node_key": LATER_NODE_KEY },
                    doc! {
                        "$set": {
                            "assignee_participant_id": STARTER,
                            "assignee_label_snapshot": "提交人本人",
                        }
                    },
                )
                .await
                .expect("注入提交人与审批人相同的 SoD 配置");
            assert_start_candidate_rejected(fixture, service, seed, "separation-of-duties").await;
        }
    });
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn legacy_start_receipt_replays_only_exact_reconstructed_payload() {
    require_mongo!(async {
        let (fixture, service, seed) = start_fixture("stock_adj_start_legacy").await;
        let request = submit_request(&seed, "start-legacy", "历史冻结值");
        service
            .submit_stock_adjustment(&seed.adjustment_id, request.clone(), &actor())
            .await
            .expect("先形成完整启动结果");
        let fresh = start_facts(fixture.db(), &seed).await;
        let [receipt] = fresh.receipts.as_slice() else {
            panic!("必须形成唯一启动收据: {:?}", fresh.receipts)
        };
        fixture
            .db()
            .collection::<Document>("approval_command_receipts")
            .update_one(
                doc! { "id": &receipt.base.id },
                doc! { "$set": { "payload_digest": legacy_start_digest() } },
            )
            .await
            .expect("模拟无版本前缀历史启动收据");
        let legacy_committed = start_facts(fixture.db(), &seed).await;
        assert!(!legacy_committed.receipts[0].payload_digest.starts_with("v1:"));

        service
            .submit_stock_adjustment(&seed.adjustment_id, request.clone(), &actor())
            .await
            .expect("历史收据与完整成功结果一致时必须只读回放");
        assert_eq!(
            start_facts(fixture.db(), &seed).await,
            legacy_committed,
            "legacy 精确回放不得重写任何事实"
        );

        let mut variants = Vec::new();
        let mut changed_line = request.clone();
        changed_line.lines[0].quantity = "3".to_string();
        variants.push(("line", changed_line));
        let mut changed_balance = request.clone();
        changed_balance.balances[0].expected_version += 1;
        variants.push(("balance", changed_balance));
        let mut changed_note = request.clone();
        changed_note.note = "历史载荷变更".to_string();
        variants.push(("note", changed_note));
        let mut changed_time = request;
        changed_time.occurred_at += 1;
        variants.push(("occurred_at", changed_time));
        let mut omitted_direction = submit_request(&seed, "start-legacy", "历史冻结值");
        omitted_direction.lines[0].direction = None;
        variants.push(("direction", omitted_direction));
        for (field, variant) in variants {
            let result = service
                .submit_stock_adjustment(&seed.adjustment_id, variant, &actor())
                .await;
            let conflict = match result {
                Ok(_) => panic!("legacy {field} 漂移必须稳定冲突"),
                Err(error) => error,
            };
            assert_eq!(
                conflict.code(),
                Some(ErrorCode::ApprovalIdempotencyPayloadConflict),
                "legacy {field} 漂移必须使用幂等载荷冲突码"
            );
            assert_eq!(
                start_facts(fixture.db(), &seed).await,
                legacy_committed,
                "legacy {field} 漂移不得产生任何写入"
            );
        }
        cleanup(fixture).await;
    });
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn submit_start_outbox_failure_rolls_back_every_prior_write() {
    require_mongo!(async {
        let (fixture, service, seed) = start_fixture("stock_adj_start_rollback").await;
        for (kind, key) in [
            (ApprovalNotificationEventKind::Started, "preexisting-started"),
            (ApprovalNotificationEventKind::Entered, "preexisting-entered"),
        ] {
            let outbox = ApprovalNotificationOutbox::enqueue(
                ApprovalNotificationOutboxId::new(key),
                key,
                kind,
                vec!["preexisting-recipient".to_string()],
                ApprovalNotificationTemplateParams {
                    document_type_label: "库存调整单".to_string(),
                    document_no: format!("ADJ-{}", seed.adjustment_id),
                    current_node_name: "冲突注入".to_string(),
                    current_approver_display_name: "冲突注入".to_string(),
                    round_no: 1,
                    reject_reason_summary: None,
                },
                Instant::from_unix_secs(9),
            )
            .expect("预置启动通知");
            fixture
                .db()
                .approval_notification_outbox()
                .create(&outbox, &mut NoTransaction)
                .await
                .expect("写入预置启动通知");
        }
        fixture
            .db()
            .collection::<Document>("approval_notification_outbox")
            .create_index(
                IndexModel::builder()
                    .keys(doc! { "event_kind": 1 })
                    .options(
                        IndexOptions::builder()
                            .name("test_unique_start_event_kind".to_string())
                            .unique(true)
                            .build(),
                    )
                    .build(),
            )
            .await
            .expect("建立测试专用 outbox 冲突索引");
        let before = start_facts(fixture.db(), &seed).await;
        assert_eq!(before.outbox.len(), 2);
        assert!(before.receipts.is_empty());

        service
            .submit_stock_adjustment(
                &seed.adjustment_id,
                submit_request(&seed, "start-rollback", "注入启动 outbox 冲突"),
                &actor(),
            )
            .await
            .expect_err("通知 outbox 晚失败必须使整笔提交启动回滚");

        assert_eq!(
            start_facts(fixture.db(), &seed).await,
            before,
            "outbox 晚失败必须回滚先写的收据、BPM、快照、WorkItem、明细、单据和审计"
        );
        cleanup(fixture).await;
    });
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn production_registry_final_approve_posts_inventory_and_runtime_atomically() {
    require_mongo!(async {
        let (fixture, inventory, seed) = start_fixture("stock_adj_final_approve").await;
        inventory
            .submit_stock_adjustment(
                &seed.adjustment_id,
                submit_request(&seed, "start-before-final", "最终通过库存调整"),
                &actor(),
            )
            .await
            .expect("先通过生产提交入口启动审批");
        let started = start_facts(fixture.db(), &seed).await;
        let [task] = started.tasks.as_slice() else {
            panic!("最终决定前必须有唯一开放任务: {:?}", started.tasks)
        };
        let runtime = production_runtime_service(fixture.db());
        let outcome = runtime
            .submit_decision(
                &actor_for(APPROVER),
                &task.base.id,
                "APPROVE",
                Some("库存复核通过"),
                task.base.version,
                "final-approve",
            )
            .await
            .expect("生产注册表最终通过必须成功");
        assert_eq!(outcome.outcome, ApprovalCommandOutcome::Applied);
        assert_eq!(
            outcome.instance_status,
            ApprovalProcessInstanceStatus::Approved.as_str()
        );
        let committed = final_approval_facts(fixture.db(), &seed).await;
        assert_final_approved(&committed, &seed, "final-approve");

        let replay = runtime
            .submit_decision(
                &actor_for(APPROVER),
                &task.base.id,
                "APPROVE",
                Some("库存复核通过"),
                task.base.version,
                "final-approve",
            )
            .await
            .expect("最终决定同键同载荷必须收据优先回放");
        assert_eq!(replay.outcome, ApprovalCommandOutcome::IdempotentReplay);
        assert_eq!(
            final_approval_facts(fixture.db(), &seed).await,
            committed,
            "最终决定回放不得重复过账、改余额或重写审批事实"
        );
        drop(runtime);
        drop(inventory);
        cleanup(fixture).await;
    });
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn final_approve_outbox_conflict_rolls_back_inventory_and_runtime() {
    require_mongo!(async {
        let (fixture, inventory, seed) = start_fixture("stock_adj_final_rollback").await;
        inventory
            .submit_stock_adjustment(
                &seed.adjustment_id,
                submit_request(&seed, "start-before-rollback", "最终通过回滚基线"),
                &actor(),
            )
            .await
            .expect("先形成最终通过基线");
        let started = start_facts(fixture.db(), &seed).await;
        let [execution] = started.executions.as_slice() else {
            panic!("最终决定前必须有唯一活动执行: {:?}", started.executions)
        };
        let [task] = started.tasks.as_slice() else {
            panic!("最终决定前必须有唯一开放任务: {:?}", started.tasks)
        };
        let dedup_key = format!("approved:{}", execution.base.id);
        let conflicting = ApprovalNotificationOutbox::enqueue(
            ApprovalNotificationOutboxId::new(dedup_key.clone()),
            dedup_key,
            ApprovalNotificationEventKind::NodeApproved,
            vec!["preexisting-recipient".to_string()],
            ApprovalNotificationTemplateParams {
                document_type_label: "库存调整单".to_string(),
                document_no: format!("ADJ-{}", seed.adjustment_id),
                current_node_name: "最终通过冲突注入".to_string(),
                current_approver_display_name: "最终通过冲突注入".to_string(),
                round_no: 1,
                reject_reason_summary: None,
            },
            Instant::from_unix_secs(9),
        )
        .expect("预置最终通过 outbox");
        fixture
            .db()
            .approval_notification_outbox()
            .create(&conflicting, &mut NoTransaction)
            .await
            .expect("注入最终通过 outbox 唯一键冲突");
        let before = final_approval_facts(fixture.db(), &seed).await;
        assert!(before.movements.is_empty());
        assert!(before.post_audits.is_empty());
        assert!(before.decision_audits.is_empty());

        let runtime = production_runtime_service(fixture.db());
        runtime
            .submit_decision(
                &actor_for(APPROVER),
                &task.base.id,
                "APPROVE",
                Some("注入决定后通知失败"),
                task.base.version,
                "final-rollback",
            )
            .await
            .expect_err("领域过账后的 outbox 冲突必须回滚整笔最终决定");
        assert_eq!(
            final_approval_facts(fixture.db(), &seed).await,
            before,
            "outbox 晚失败必须回滚 movement、balance、document、BPM、task、receipt 与双审计"
        );
        drop(runtime);
        drop(inventory);
        cleanup(fixture).await;
    });
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn final_approve_inventory_failure_rolls_back_receipt_movement_and_runtime() {
    require_mongo!(async {
        let (fixture, inventory, seed) = start_fixture("stock_adj_final_inventory_fail").await;
        inventory
            .submit_stock_adjustment(
                &seed.adjustment_id,
                submit_request(&seed, "start-before-inventory-fail", "最终库存失败回滚"),
                &actor(),
            )
            .await
            .expect("先形成最终通过基线");
        let started = start_facts(fixture.db(), &seed).await;
        let [task] = started.tasks.as_slice() else {
            panic!("最终决定前必须有唯一开放任务: {:?}", started.tasks)
        };

        set_raw_field(
            fixture.db(),
            "stock_balances",
            &seed.balance_id,
            "sku_id",
            Bson::String("sku-without-balance".to_string()),
        )
        .await;
        let before = final_approval_facts(fixture.db(), &seed).await;
        assert!(before.movements.is_empty());
        let runtime = production_runtime_service(fixture.db());
        runtime
            .submit_decision(
                &actor_for(APPROVER),
                &task.base.id,
                "APPROVE",
                Some("库存余额维度缺失"),
                task.base.version,
                "final-inventory-fail",
            )
            .await
            .expect_err("receipt-first 后领域动作写流水再找不到余额时必须整事务回滚");
        assert_eq!(
            final_approval_facts(fixture.db(), &seed).await,
            before,
            "库存动作失败必须回滚决定收据、已插流水、单据、BPM、任务、outbox 与双审计"
        );
        drop(runtime);
        drop(inventory);
        cleanup(fixture).await;
    });
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn final_approve_identity_drift_matrix_is_zero_write() {
    require_mongo!(async {
        let (fixture, inventory, seed) = start_fixture("stock_adj_final_identity_drift").await;
        inventory
            .submit_stock_adjustment(
                &seed.adjustment_id,
                submit_request(&seed, "start-before-identity-drift", "最终身份漂移矩阵"),
                &actor(),
            )
            .await
            .expect("先形成最终决定运行事实");
        let original = final_approval_facts(fixture.db(), &seed).await;
        let [instance] = original.chain.instances.as_slice() else {
            panic!("必须有唯一实例: {:?}", original.chain.instances)
        };
        let [execution] = original.chain.executions.as_slice() else {
            panic!("必须有唯一执行: {:?}", original.chain.executions)
        };
        let [task] = original.chain.tasks.as_slice() else {
            panic!("必须有唯一任务: {:?}", original.chain.tasks)
        };
        let [snapshot] = original.chain.snapshots.as_slice() else {
            panic!("必须有唯一快照: {:?}", original.chain.snapshots)
        };
        let [assignee] = original.chain.assignees.as_slice() else {
            panic!("必须有唯一实例审批人: {:?}", original.chain.assignees)
        };
        let task_id = task.base.id.clone();
        let task_version = task.base.version;
        let drifts = vec![
            (
                "snapshot.subject_version",
                "approval_subject_snapshots",
                snapshot.base.id.clone(),
                "subject_version",
                Bson::Int32(2),
                Bson::Int32(1),
            ),
            (
                "instance.subject_id",
                "approval_process_instances",
                instance.base.id.clone(),
                "subject.subject_id",
                Bson::String("other-adjustment".to_string()),
                Bson::String(seed.adjustment_id.clone()),
            ),
            (
                "instance.subject_version",
                "approval_process_instances",
                instance.base.id.clone(),
                "subject_version",
                Bson::Int32(2),
                Bson::Int32(1),
            ),
            (
                "instance.current_execution",
                "approval_process_instances",
                instance.base.id.clone(),
                "current_node_execution_id",
                Bson::String("other-execution".to_string()),
                Bson::String(execution.base.id.clone()),
            ),
            (
                "execution.instance",
                "approval_node_executions",
                execution.base.id.clone(),
                "process_instance_id",
                Bson::String("other-instance".to_string()),
                Bson::String(instance.base.id.clone()),
            ),
            (
                "execution.assignee",
                "approval_node_executions",
                execution.base.id.clone(),
                "assignee_participant_id",
                Bson::String(LATER_APPROVER.to_string()),
                Bson::String(APPROVER.to_string()),
            ),
            (
                "instance_assignee.current",
                "approval_instance_assignees",
                assignee.base.id.clone(),
                "current_assignee_participant_id",
                Bson::String(LATER_APPROVER.to_string()),
                Bson::String(APPROVER.to_string()),
            ),
            (
                "task.execution",
                "work_items",
                task.base.id.clone(),
                "approval_node_execution_id",
                Bson::String("other-execution".to_string()),
                Bson::String(execution.base.id.clone()),
            ),
            (
                "task.business_object",
                "work_items",
                task.base.id.clone(),
                "business_object_id",
                Bson::String("other-adjustment".to_string()),
                Bson::String(seed.adjustment_id.clone()),
            ),
            (
                "task.subject_version",
                "work_items",
                task.base.id.clone(),
                "subject_version",
                Bson::String("2".to_string()),
                Bson::String("1".to_string()),
            ),
            (
                "task.assignment_source",
                "work_items",
                task.base.id.clone(),
                "assignment_source",
                Bson::String("SYSTEM_RULE".to_string()),
                Bson::String("APPROVAL_RUNTIME".to_string()),
            ),
            (
                "task.owner_role",
                "work_items",
                task.base.id.clone(),
                "owner_role",
                Bson::String("other-role".to_string()),
                Bson::String("stock_adjustment_approver".to_string()),
            ),
            (
                "task.owner_organization",
                "work_items",
                task.base.id.clone(),
                "owner_organization_id",
                Bson::String("other-organization".to_string()),
                Bson::String(ORGANIZATION.to_string()),
            ),
        ];

        for (index, (label, collection, id, field, drifted, restored)) in drifts.into_iter().enumerate() {
            set_raw_field(fixture.db(), collection, &id, field, drifted).await;
            assert_final_identity_drift_is_zero_write(
                fixture.db(),
                &seed,
                &task_id,
                task_version,
                &format!("final-identity-drift-{index}"),
                label,
            )
            .await;
            set_raw_field(fixture.db(), collection, &id, field, restored).await;
            assert_eq!(
                final_approval_facts(fixture.db(), &seed).await,
                original,
                "{label} 恢复后必须回到原始运行事实"
            );
        }
        drop(inventory);
        cleanup(fixture).await;
    });
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集且启用 enableTestCommands"]
async fn final_decision_policy_revision_drift_is_zero_write() {
    require_mongo!(async {
        const COMMAND_APP: &str = "stock-policy-drift-command";
        const TRACE_APP: &str = "stock-policy-drift-trace";

        // 独立成功链只用于定位门闩集合、事务顺序及该 namespace 内的前置匹配数。
        let (trace_fixture, trace_inventory, trace_seed) =
            start_fixture("stock_adj_policy_revision_trace").await;
        trace_inventory
            .submit_stock_adjustment(
                &trace_seed.adjustment_id,
                submit_request(&trace_seed, "start-policy-drift-trace", "策略漂移门闩定位"),
                &actor(),
            )
            .await
            .expect("形成 policy drift trace 运行事实");
        let trace_facts = start_facts(trace_fixture.db(), &trace_seed).await;
        let [trace_task] = trace_facts.tasks.as_slice() else {
            panic!("policy drift trace 必须有唯一任务: {:?}", trace_facts.tasks)
        };
        trace_fixture
            .db()
            .run_command(doc! { "profile": 2_i32, "slowms": 0_i32 })
            .await
            .expect("启用 policy drift trace profiler");
        let trace_db = app_named_database(&trace_fixture, TRACE_APP).await;
        production_runtime_service(&trace_db)
            .submit_decision(
                &actor_for(APPROVER),
                &trace_task.base.id,
                "APPROVE",
                Some("策略漂移门闩定位"),
                trace_task.base.version,
                "final-policy-drift-trace",
            )
            .await
            .expect("成功 trace 必须走完最终决定链");
        let gate = locate_second_transactional_find(trace_fixture.db(), TRACE_APP).await;
        eprintln!(
            "policy drift gate: preceding_find_count={}, namespace_preceding_match_count={}, first_txn_find={}, blocked_txn_find={}, policy_revision_find=casbin_policy_state@{}",
            gate.preceding_find_count,
            gate.namespace_preceding_match_count,
            gate.first_transactional_collection,
            gate.blocked_transactional_collection,
            gate.policy_revision_find_position,
        );
        trace_fixture
            .db()
            .run_command(doc! { "profile": 0_i32 })
            .await
            .expect("关闭 policy drift trace profiler");
        drop(trace_inventory);
        cleanup(trace_fixture).await;

        let (fixture, inventory, seed) = start_fixture("stock_adj_policy_revision_drift").await;
        inventory
            .submit_stock_adjustment(
                &seed.adjustment_id,
                submit_request(&seed, "start-before-policy-drift", "策略版本漂移基线"),
                &actor(),
            )
            .await
            .expect("先形成最终决定运行事实");
        let before = final_approval_facts(fixture.db(), &seed).await;
        let [task] = before.chain.tasks.as_slice() else {
            panic!("策略漂移前必须有唯一任务: {:?}", before.chain.tasks)
        };
        let task_id = task.base.id.clone();
        let task_version = task.base.version;
        let mut policy_adapter = MongoCasbinAdapter::new(fixture.db().clone());
        let revision_r = policy_adapter
            .policy_revision(&mut NoTransaction)
            .await
            .expect("读取策略 revision R");

        // 第一条事务 find 固定快照 R；挂起独立 trace 定位出的下一条事务 find。
        let command_db = app_named_database(&fixture, COMMAND_APP).await;
        let runtime = production_runtime_service(&command_db);
        let failpoint_before = arm_policy_find_drift(
            fixture.db(),
            COMMAND_APP,
            &gate.blocked_transactional_collection,
            gate.namespace_preceding_match_count,
        )
        .await;
        let command_task = tokio::spawn(async move {
            runtime
                .submit_decision(
                    &actor_for(APPROVER),
                    &task_id,
                    "APPROVE",
                    Some("策略版本必须与事务快照一致"),
                    task_version,
                    "final-policy-drift",
                )
                .await
        });
        let failpoint_hit = wait_for_policy_find_failpoint(fixture.db(), failpoint_before).await;
        if !failpoint_hit {
            disable_fail_command(fixture.db()).await;
            let unexpected = command_task.await.expect("等待未挂起的决定任务");
            drop(inventory);
            cleanup(fixture).await;
            panic!("指定 appName 的第二条 find 未命中 failpoint: {unexpected:?}");
        }
        let failpoint_delta = fail_command_entries(fixture.db()).await - failpoint_before;
        eprintln!(
            "policy drift namespace block confirmed by waitForFailPoint; timesEntered delta={failpoint_delta}"
        );

        let policy_change = policy_adapter
            .add_policy(
                "p",
                "p",
                vec![
                    format!("role:{CANCEL_ROLE}"),
                    "policy_revision_probe".to_string(),
                    "read".to_string(),
                ],
            )
            .await;
        let revision_r_plus_one = policy_adapter.policy_revision(&mut NoTransaction).await;
        let reloaded_revision = iam::shared_rbac_service(fixture.db().clone())
            .current_policy_revision()
            .await;
        disable_fail_command(fixture.db()).await;

        assert!(policy_change.expect("第二 policy adapter 提交 R+1"));
        let revision_r_plus_one = revision_r_plus_one.expect("读取策略 revision R+1");
        assert_eq!(revision_r_plus_one, revision_r + 1);
        assert_eq!(
            reloaded_revision.expect("第二 RBAC service reload R+1"),
            revision_r_plus_one
        );
        let command_result = command_task.await.expect("等待策略漂移决定任务");
        let error = command_result.expect_err("事务快照 R 与授权快照 R+1 不一致必须失败关闭");
        match error {
            Error::Rbac(message) => assert_eq!(message, "授权策略版本已变化，无法在当前事务中证明授权快照"),
            other => panic!("策略版本漂移必须返回稳定 RBAC 错误: {other:?}"),
        }
        assert_eq!(
            final_approval_facts(fixture.db(), &seed).await,
            before,
            "策略 revision 漂移必须在 receipt、库存和 BPM 首笔写前失败"
        );
        drop(inventory);
        cleanup(fixture).await;
    });
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn single_and_legacy_duplicate_created_at_receipts_read_through_repository() {
    require_mongo!(async {
        fn created_at_values(raw: &RawDocumentBuf) -> Vec<i64> {
            raw.iter()
                .map(|entry| entry.expect("解析 raw receipt BSON"))
                .filter(|(key, _)| *key == "created_at")
                .map(|(_, value)| value.as_i64().expect("created_at 必须是 i64"))
                .collect()
        }

        let fixture = TestDb::new("stock_adj_legacy_receipt")
            .await
            .expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");

        let current = fixture_receipt(
            "current-receipt-single-created-at",
            ApprovalCommandKind::CancelApproval,
            "current-instance",
            "current-key",
            "current-digest",
            "current-instance",
            at(20),
        );
        fixture
            .db()
            .bpm_workflow()
            .insert_command_receipt(&current, &mut NoTransaction)
            .await
            .expect("Repository 写入当前收据");
        let current_raw = fixture
            .db()
            .collection::<RawDocumentBuf>("approval_command_receipts")
            .find_one(doc! { "id": "current-receipt-single-created-at" })
            .await
            .expect("raw 回读当前收据")
            .expect("当前收据必须存在");
        assert_eq!(
            created_at_values(&current_raw),
            vec![20],
            "当前 Repository 新写 BSON 必须只含一个 created_at"
        );

        let mut legacy_raw = RawDocumentBuf::new();
        legacy_raw.append(cstr!("id"), "legacy-receipt-duplicate-created-at");
        legacy_raw.append(cstr!("version"), 1_i64);
        legacy_raw.append(cstr!("created_at"), 10_i64);
        legacy_raw.append(cstr!("created_at"), 10_i64);
        legacy_raw.append(cstr!("updated_at"), 10_i64);
        legacy_raw.append(cstr!("deleted_at"), 0_i64);
        legacy_raw.append(cstr!("command_kind"), "CANCEL_APPROVAL");
        legacy_raw.append(cstr!("scope_id"), "legacy-instance");
        legacy_raw.append(cstr!("idempotency_key"), "legacy-key");
        legacy_raw.append(cstr!("payload_digest"), "legacy-digest");
        legacy_raw.append(cstr!("result_ref"), "legacy-instance");
        fixture
            .db()
            .collection::<RawDocumentBuf>("approval_command_receipts")
            .insert_one(legacy_raw)
            .await
            .expect("raw BSON 写入双 created_at legacy 收据");
        let persisted_legacy_raw = fixture
            .db()
            .collection::<RawDocumentBuf>("approval_command_receipts")
            .find_one(doc! { "id": "legacy-receipt-duplicate-created-at" })
            .await
            .expect("raw 回读 legacy 收据")
            .expect("legacy 收据必须存在");
        assert_eq!(
            created_at_values(&persisted_legacy_raw),
            vec![10, 10],
            "MongoDB 必须真实保留两个同值 created_at"
        );

        let receipt = fixture
            .db()
            .bpm_workflow()
            .find_command_receipt(
                ApprovalCommandKind::CancelApproval,
                "legacy-instance",
                &IdempotencyKey::parse("legacy-key").expect("测试幂等键必须有效"),
                &mut NoTransaction,
            )
            .await
            .expect("Repository 回读 legacy 收据")
            .expect("legacy 收据必须存在");
        assert_eq!(receipt.base.id, "legacy-receipt-duplicate-created-at");
        assert_eq!(receipt.base.created_at, 10);
        assert_eq!(receipt.base.updated_at, 10);
        assert_eq!(receipt.command_kind, ApprovalCommandKind::CancelApproval);
        assert_eq!(receipt.payload_digest, "legacy-digest");
        cleanup(fixture).await;
    });
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn running_cancel_is_atomic_and_replay_stays_zero_write_after_resubmit() {
    require_mongo!(async {
        let (fixture, service, seed) = fixture("stock_adj_cancel_atomic", RuntimeState::Running).await;
        let request = cancel_request(&seed, "cancel-running", "提交人撤回");

        let initial = stored_facts(fixture.db(), &seed, "cancel-running").await;
        let mut stale_task = request.clone();
        stale_task.expected_task_version = stale_task
            .expected_task_version
            .and_then(|version| version.checked_add(1));
        service
            .cancel_stock_adjustment_approval(&seed.adjustment_id, stale_task, &actor())
            .await
            .expect_err("开放任务 CAS 漂移必须失败关闭");
        assert_eq!(
            stored_facts(fixture.db(), &seed, "cancel-running").await,
            initial,
            "运行 CAS 漂移不得产生业务、运行时、任务、收据、outbox 或审计写入"
        );

        let view = service
            .cancel_stock_adjustment_approval(&seed.adjustment_id, request.clone(), &actor())
            .await
            .expect("正常运行审批必须原子撤回");
        assert_eq!(view.status, StockAdjustmentState::Draft);
        let committed = stored_facts(fixture.db(), &seed, "cancel-running").await;
        assert_cancelled(&committed, &seed, "提交人撤回");

        let replay = service
            .cancel_stock_adjustment_approval(&seed.adjustment_id, request.clone(), &actor())
            .await
            .expect("同载荷必须回放成功");
        assert_eq!(replay.status, StockAdjustmentState::Draft);
        assert_eq!(
            stored_facts(fixture.db(), &seed, "cancel-running").await,
            committed,
            "同载荷回放不得改写任何撤回事实"
        );

        let mut different = request.clone();
        different.reason = "不同撤回原因".to_string();
        let conflict = service
            .cancel_stock_adjustment_approval(&seed.adjustment_id, different, &actor())
            .await
            .expect_err("同键异载荷必须稳定冲突");
        assert_eq!(
            conflict.code(),
            Some(ErrorCode::ApprovalIdempotencyPayloadConflict)
        );
        assert_eq!(
            stored_facts(fixture.db(), &seed, "cancel-running").await,
            committed,
            "异载荷冲突不得改写已提交事实"
        );

        let mut resubmitted = committed.adjustment.clone();
        assert_eq!(resubmitted.start_approval().expect("重新提交"), 2);
        fixture
            .db()
            .stock_adjustments()
            .update(&mut resubmitted, &mut NoTransaction)
            .await
            .expect("写回重新提交业务事实");
        let new_runtime = seed_runtime(
            fixture.db(),
            &resubmitted,
            2,
            "instance-resubmitted",
            "execution-resubmitted",
            "task-resubmitted",
            RuntimeState::Running,
        )
        .await;
        let original_before_replay = stored_facts(fixture.db(), &seed, "cancel-running").await;
        let new_before_replay = stored_facts(fixture.db(), &new_runtime, "unused-new-cancel-key").await;

        let replay_after_resubmit = service
            .cancel_stock_adjustment_approval(&seed.adjustment_id, request, &actor())
            .await
            .expect("原实例收据必须在重新提交后精确回放");
        assert_eq!(replay_after_resubmit.status, StockAdjustmentState::InApproval);
        assert_eq!(
            stored_facts(fixture.db(), &seed, "cancel-running").await,
            original_before_replay
        );
        assert_eq!(
            stored_facts(fixture.db(), &new_runtime, "unused-new-cancel-key").await,
            new_before_replay,
            "回放原实例不得触碰新主题版本运行事实"
        );
        cleanup(fixture).await;
    });
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn cancel_replay_key_existence_is_hidden_from_other_or_revoked_actor() {
    require_mongo!(async {
        let (fixture, service, seed) = fixture("stock_adj_cancel_key_oracle", RuntimeState::Running).await;
        let existing_request = cancel_request(&seed, "cancel-key-oracle", "撤回收据不可探测");
        service
            .cancel_stock_adjustment_approval(&seed.adjustment_id, existing_request.clone(), &actor())
            .await
            .expect("原提交人先形成唯一撤回收据与审计");
        let committed = stored_facts(fixture.db(), &seed, "cancel-key-oracle").await;
        assert_cancelled(&committed, &seed, "撤回收据不可探测");

        let mut adapter = MongoCasbinAdapter::new(fixture.db().clone());
        assert!(adapter
            .add_policy(
                "g",
                "g",
                vec![
                    subject(AccountKind::Admin, LATER_APPROVER),
                    format!("role:{CANCEL_ROLE}"),
                ],
            )
            .await
            .expect("为非原撤回人授予当前撤回权限"));
        assert!(adapter
            .add_policy(
                "p",
                "p",
                vec![
                    format!("role:{LATER_APPROVER_ROLE}"),
                    "stock_adjustment".to_string(),
                    "approval_runtime_admin".to_string(),
                ],
            )
            .await
            .expect("为非原撤回人授予库存调整运行管理权限"));
        let mut missing_request = existing_request.clone();
        missing_request.idempotency_key = "cancel-key-oracle-missing".to_string();
        let other_existing = service
            .cancel_stock_adjustment_approval(
                &seed.adjustment_id,
                existing_request.clone(),
                &actor_for(LATER_APPROVER),
            )
            .await
            .expect_err("非原撤回人不得回放已有收据");
        let other_missing = service
            .cancel_stock_adjustment_approval(
                &seed.adjustment_id,
                missing_request.clone(),
                &actor_for(LATER_APPROVER),
            )
            .await
            .expect_err("非原撤回人不得从缺失 key 探测终态");
        assert!(matches!(other_existing, Error::ConflictError(_)));
        assert_same_error_projection(
            &other_existing,
            &other_missing,
            "当前有权但非原撤回人 existing/missing key",
        );
        assert_eq!(
            stored_facts(fixture.db(), &seed, "cancel-key-oracle").await,
            committed,
            "非原撤回人探测 existing/missing key 必须零写"
        );

        assert!(adapter
            .remove_policy(
                "g",
                "g",
                vec![
                    subject(AccountKind::Admin, STARTER),
                    format!("role:{CANCEL_ROLE}"),
                ],
            )
            .await
            .expect("撤销原撤回人当前权限"));
        let revoked_existing = service
            .cancel_stock_adjustment_approval(&seed.adjustment_id, existing_request, &actor())
            .await
            .expect_err("当前失权的原撤回人不得回放已有收据");
        let revoked_missing = service
            .cancel_stock_adjustment_approval(&seed.adjustment_id, missing_request, &actor())
            .await
            .expect_err("当前失权的原撤回人不得从缺失 key 探测终态");
        assert!(matches!(revoked_existing, Error::Forbidden(_)));
        assert_same_error_projection(
            &revoked_existing,
            &revoked_missing,
            "当前失权原撤回人 existing/missing key",
        );
        assert_eq!(
            stored_facts(fixture.db(), &seed, "cancel-key-oracle").await,
            committed,
            "当前失权原撤回人探测 existing/missing key 必须零写"
        );
        cleanup(fixture).await;
    });
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn concurrent_same_key_cancel_profiles_duplicate_loser_and_recovers() {
    require_mongo!(async {
        let (fixture, _service, seed) = fixture("stock_adj_cancel_race", RuntimeState::Running).await;
        let rbac = iam::shared_rbac_service(fixture.db().clone());
        let left_service = InventoryService::new(fixture.db().clone(), Arc::clone(&rbac));
        let right_service = InventoryService::new(fixture.db().clone(), rbac);
        let left_request = cancel_request(&seed, "cancel-race", "并发同键撤回");
        let right_request = left_request.clone();
        let left_actor = actor();
        let right_actor = actor();
        let barrier = Barrier::new(2);
        let observation = arm_receipt_duplicate_race(fixture.db()).await;
        let left = async {
            barrier.wait().await;
            left_service
                .cancel_stock_adjustment_approval(&seed.adjustment_id, left_request, &left_actor)
                .await
        };
        let right = async {
            barrier.wait().await;
            right_service
                .cancel_stock_adjustment_approval(&seed.adjustment_id, right_request, &right_actor)
                .await
        };
        let (left, right) = tokio::join!(left, right);
        assert!(left.is_ok(), "左侧并发调用应成功或回放: {left:?}");
        assert!(right.is_ok(), "右侧并发调用应成功或回放: {right:?}");
        assert_one_receipt_duplicate_profile(fixture.db(), observation).await;
        disarm_receipt_duplicate_race(fixture.db()).await;

        let committed = stored_facts(fixture.db(), &seed, "cancel-race").await;
        assert_cancelled(&committed, &seed, "并发同键撤回");
        assert_eq!(
            fixture
                .db()
                .collection::<Document>("approval_command_receipts")
                .count_documents(doc! {
                    "command_kind": "CANCEL_APPROVAL",
                    "scope_id": &seed.instance_id,
                    "idempotency_key": "cancel-race",
                })
                .await
                .expect("统计撤回收据"),
            1,
            "并发同键只能提交一条撤回收据"
        );
        assert_eq!(committed.cancel_audits, 1);
        assert_eq!(
            fixture
                .db()
                .collection::<Document>("approval_notification_outbox")
                .count_documents(doc! { "dedup_key": format!("cancelled:{}:1", seed.instance_id) })
                .await
                .expect("统计撤回 outbox"),
            1
        );
        cleanup(fixture).await;
    });
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn personnel_blocked_uses_normal_cancel_but_structural_blocker_is_zero_write() {
    require_mongo!(async {
        let (personnel_fixture, personnel_service, personnel_seed) = fixture(
            "stock_adj_cancel_personnel",
            RuntimeState::Blocked(ApprovalBlockerCode::ApproverAccountInactive),
        )
        .await;
        assert!(personnel_seed.task_version.is_none());
        personnel_service
            .cancel_stock_adjustment_approval(
                &personnel_seed.adjustment_id,
                cancel_request(&personnel_seed, "cancel-personnel", "人员失效后撤回"),
                &actor(),
            )
            .await
            .expect("人员失效 BLOCKED 必须允许普通撤回");
        let personnel = stored_facts(personnel_fixture.db(), &personnel_seed, "cancel-personnel").await;
        assert_cancelled(&personnel, &personnel_seed, "人员失效后撤回");

        let (structural_fixture, structural_service, structural_seed) = fixture(
            "stock_adj_cancel_structural",
            RuntimeState::Blocked(ApprovalBlockerCode::OpenTaskConflict),
        )
        .await;
        let before = stored_facts(structural_fixture.db(), &structural_seed, "cancel-structural").await;
        let error = structural_service
            .cancel_stock_adjustment_approval(
                &structural_seed.adjustment_id,
                cancel_request(&structural_seed, "cancel-structural", "结构阻塞撤回"),
                &actor(),
            )
            .await
            .expect_err("结构 blocker 不得走普通撤回端口");
        assert!(matches!(
            error,
            Error::ConflictError(_) | Error::ValidationError(_)
        ));
        assert_eq!(
            stored_facts(structural_fixture.db(), &structural_seed, "cancel-structural").await,
            before,
            "结构 blocker 被拒绝时不得产生任何写入"
        );
        cleanup(personnel_fixture).await;
        cleanup(structural_fixture).await;
    });
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn outbox_dedup_conflict_rolls_back_business_runtime_task_receipt_and_audit() {
    require_mongo!(async {
        let (fixture, service, seed) = fixture("stock_adj_cancel_rollback", RuntimeState::Running).await;
        let dedup_key = format!("cancelled:{}:1", seed.instance_id);
        let conflicting_outbox = ApprovalNotificationOutbox::enqueue(
            ApprovalNotificationOutboxId::new(dedup_key.clone()),
            dedup_key.clone(),
            ApprovalNotificationEventKind::Cancelled,
            vec!["preexisting-recipient".to_string()],
            ApprovalNotificationTemplateParams {
                document_type_label: "库存调整单".to_string(),
                document_no: "PREEXISTING".to_string(),
                current_node_name: "冲突注入".to_string(),
                current_approver_display_name: "冲突注入".to_string(),
                round_no: 1,
                reject_reason_summary: None,
            },
            Instant::from_unix_secs(9),
        )
        .expect("冲突 outbox");
        fixture
            .db()
            .approval_notification_outbox()
            .create(&conflicting_outbox, &mut NoTransaction)
            .await
            .expect("预置 outbox 唯一键冲突");
        let before = stored_facts(fixture.db(), &seed, "cancel-rollback").await;
        assert!(before.cancel_receipt.is_none());
        assert_eq!(before.cancel_audits, 0);

        service
            .cancel_stock_adjustment_approval(
                &seed.adjustment_id,
                cancel_request(&seed, "cancel-rollback", "注入 outbox 冲突"),
                &actor(),
            )
            .await
            .expect_err("outbox 唯一键冲突必须使整笔撤回失败");

        assert_eq!(
            stored_facts(fixture.db(), &seed, "cancel-rollback").await,
            before,
            "outbox 写入失败必须回滚单据、运行时、任务、收据和审计"
        );
        let open_tasks = fixture
            .db()
            .work_items()
            .open_approval_tasks_for_execution(
                &ApprovalNodeExecutionId::new(seed.execution_id.clone()),
                &mut NoTransaction,
            )
            .await
            .expect("回读开放任务");
        assert_eq!(open_tasks.len(), 1);
        assert_eq!(open_tasks[0].status, WorkItemStatus::Open);
        cleanup(fixture).await;
    });
}
