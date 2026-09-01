//! APP-S01：审批运行读取入口的 actor、对象读取权与 DataScope 真实 MongoDB 授权矩阵。
//!
//! 用例统一使用随机独立库、真实 Casbin Mongo Adapter 与真实 Service；仅在
//! `ERP_TEST_MONGO_URI` 指向 MongoDB 7 单节点副本集时通过 `--include-ignored` 执行。

use std::collections::HashSet;
use std::str::FromStr;

use bpm::ids::{
    ApprovalCommandReceiptId, ApprovalNodeExecutionId, ApprovalProcessDefinitionId, ApprovalProcessInstanceId,
};
use bpm::model::types::{ApprovalBlockerCode, ApprovalCommandKind, ApprovalExecutionAssignmentSource};
use bpm::model::{
    ApprovalCommandIdentity, ApprovalCommandReceipt, ApprovalNodeExecution, ApprovalProcessInstance,
    CanonicalCommandPayload, CommandPayloadField, IdempotencyKey, NewNodeExecution, NewProcessInstance,
    ParticipantId, ProcessKind, SubjectRef, Timestamp,
};
use casbin::Adapter;
use database::repository::bpm::ApprovalInstanceListProjection;
use database::{
    ensure_indexes, AccessControlExt, ApprovalIntegrationExt, BpmExt, MongoCasbinAdapter, NoTransaction,
    WorkItemExt,
};
use entities::access_control::{DataScope, DataScopeData, DataScopeSubjectType, DataScopeType};
use entities::approval_integration::{ApprovalSubjectSnapshot, ApprovalSubjectSnapshotPayload};
use entities::common::time::Instant;
use entities::document_registry::DocumentType;
use entities::ids::{ApprovalSubjectSnapshotId, DataScopeId, WorkItemId};
use entities::money::Quantity;
use entities::work_item::{DocumentApprovalWorkItemData, WorkItem, WorkItemPriority, WorkItemType};
use entities::{
    AccountCore, AccountCoreData, AccountKind, AccountStatus, LoginAccount, Permission, Role, RoleData,
    RoleUpdate, Secret,
};
use services::approval::execution::{
    ApprovalRuntimeService, RuntimeInstanceListQuery, RuntimeInstanceListView,
};
use services::audit::AuditActor;
use services::iam::{self, subject};
use services::Error;
use test_support::{require_mongo, TestDb};

const STARTER: &str = "starter";
const APPROVER: &str = "approver";
const MANAGER: &str = "manager";
const COMPANY_MANAGER: &str = "company-manager";
const READER: &str = "reader";
const UNAUTHORIZED: &str = "unauthorized";
const WRONG_ORG: &str = "wrong-org";
const WRONG_USER_SCOPE: &str = "wrong-user-scope";
const NO_OBJECT_READ: &str = "no-object-read";
const DISABLED_RUNTIME_READER: &str = "disabled-runtime-reader";
const INACTIVE_STARTER: &str = "inactive-starter";
const INACTIVE_OWNER: &str = "inactive-owner";

const DISABLED_RUNTIME_ROLE: &str = "disabled-runtime-admin";
const ENABLED_OBJECT_READER_ROLE: &str = "enabled-object-reader";

const ORG_1: &str = "org-1";
const ORG_2: &str = "org-2";

const MAIN_INSTANCE: &str = "inst-main";
const WRONG_ORG_INSTANCE: &str = "inst-wrong-org";
const MISSING_SNAPSHOT_INSTANCE: &str = "inst-missing-snapshot";
const DRIFTED_SNAPSHOT_INSTANCE: &str = "inst-drifted-snapshot";
const BLOCKED_INSTANCE: &str = "inst-blocked";
const INACTIVE_STARTED_INSTANCE: &str = "inst-inactive-started";
const INACTIVE_MINE_INSTANCE: &str = "inst-inactive-mine";

/// 测试运行事实的快照形状。
#[derive(Debug, Clone, Copy)]
enum SnapshotFixture {
    /// 与实例主体严格一致。
    Exact,
    /// 缺失冻结快照。
    Missing,
    /// 快照业务对象 ID 与实例主体漂移。
    Drifted,
}

/// 单条运行实例的确定性 fixture 规格。
struct RuntimeFixture<'a> {
    instance_id: &'a str,
    object_id: &'a str,
    started_by: &'a str,
    approver: &'a str,
    organization_id: &'a str,
    snapshot: SnapshotFixture,
    blocked: bool,
    create_work_item: bool,
}

/// 构造固定秒时间戳。
fn at(seconds: i64) -> Timestamp {
    Timestamp::from_unix_secs(seconds).expect("测试时间戳必须合法")
}

/// 构造可指定 scope/digest 的测试收据。
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

/// 构造合法 BPM 参与人。
fn participant(id: &str) -> ParticipantId {
    ParticipantId::new(id).expect("测试参与人必须合法")
}

/// 构造与当前认证身份一致的审计 actor。
fn actor(id: &str) -> AuditActor {
    AuditActor::new(id.to_string(), format!("login-{id}"), AccountKind::Admin)
}

/// 构造用于账号状态重验的后台账号。
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

/// 插入全部授权矩阵账号；失效账号只改变当前持久化状态。
async fn seed_accounts(db: &mongodb::Database) {
    for (id, status) in [
        (STARTER, AccountStatus::Active),
        (APPROVER, AccountStatus::Active),
        (MANAGER, AccountStatus::Active),
        (COMPANY_MANAGER, AccountStatus::Active),
        (READER, AccountStatus::Active),
        (UNAUTHORIZED, AccountStatus::Active),
        (WRONG_ORG, AccountStatus::Active),
        (WRONG_USER_SCOPE, AccountStatus::Active),
        (NO_OBJECT_READ, AccountStatus::Active),
        (DISABLED_RUNTIME_READER, AccountStatus::Active),
        (INACTIVE_STARTER, AccountStatus::Suspended),
        (INACTIVE_OWNER, AccountStatus::Suspended),
    ] {
        db.accounts()
            .create(&account(id, status), &mut NoTransaction)
            .await
            .expect("插入授权矩阵账号");
    }
}

/// 构造角色或用户组织范围。
fn organization_scope(
    id: &str,
    subject_type: DataScopeSubjectType,
    subject_id: &str,
    organization_id: &str,
) -> DataScope {
    DataScope::new(
        DataScopeId::new(id),
        DataScopeData {
            subject_type,
            subject_id: subject_id.to_string(),
            scope_type: DataScopeType::Organization,
            scope_targets: vec![organization_id.to_string()],
        },
    )
    .expect("组织 DataScope fixture")
}

/// 构造角色公司级范围。
fn company_scope(id: &str, role_id: &str) -> DataScope {
    DataScope::new(
        DataScopeId::new(id),
        DataScopeData {
            subject_type: DataScopeSubjectType::Role,
            subject_id: role_id.to_string(),
            scope_type: DataScopeType::Company,
            scope_targets: Vec::new(),
        },
    )
    .expect("公司 DataScope fixture")
}

/// 从 Entity 注册表取得库存调整对象读取权限，不在测试中复制类型映射。
fn stock_adjustment_read_permission() -> (&'static str, &'static str) {
    let relation = WorkItemType::DocumentApproval
        .brief_relation(DocumentType::StockAdjustment.as_str())
        .expect("库存调整审批简报关系必须登记");
    assert_eq!(relation.read_permission, "stock_adjustment:detail");
    relation
        .read_permission
        .split_once(':')
        .expect("读取权限必须为 resource:action")
}

/// 写入 Casbin 角色绑定、对象读取/运行管理权限及各自 DataScope。
async fn seed_authorization(db: &mongodb::Database) {
    let role_bindings = [
        (MANAGER, "manager"),
        (COMPANY_MANAGER, "company-manager"),
        (READER, "reader"),
        (WRONG_ORG, "wrong-org"),
        (WRONG_USER_SCOPE, "wrong-user-scope"),
        (NO_OBJECT_READ, "no-object-read"),
        (DISABLED_RUNTIME_READER, DISABLED_RUNTIME_ROLE),
        (DISABLED_RUNTIME_READER, ENABLED_OBJECT_READER_ROLE),
    ];
    for (_, role_id) in role_bindings {
        let mut role = Role::new(
            role_id.to_string(),
            RoleData {
                name: role_id.to_string(),
                description: None,
                system: false,
            },
        )
        .expect("角色 fixture");
        if role_id == DISABLED_RUNTIME_ROLE {
            role.update(RoleUpdate {
                disabled: Some(true),
                ..RoleUpdate::default()
            })
            .expect("停用运行管理角色");
        }
        db.roles()
            .create(&role, &mut NoTransaction)
            .await
            .expect("写入启用角色");
    }
    let mut adapter = MongoCasbinAdapter::new(db.clone());
    let bindings = role_bindings
        .iter()
        .map(|(user_id, role_id)| vec![subject(AccountKind::Admin, user_id), format!("role:{role_id}")])
        .collect::<Vec<_>>();
    assert!(adapter
        .add_policies("g", "g", bindings)
        .await
        .expect("写入角色绑定"));

    let (read_resource, read_action) = stock_adjustment_read_permission();
    let mut policies = Vec::new();
    for role_id in ["manager", "company-manager", "wrong-org", "wrong-user-scope"] {
        policies.extend([
            vec![
                format!("role:{role_id}"),
                read_resource.to_string(),
                read_action.to_string(),
            ],
            vec![
                format!("role:{role_id}"),
                "stock_adjustment".to_string(),
                "approval_runtime_admin".to_string(),
            ],
        ]);
    }
    policies.push(vec![
        "role:reader".to_string(),
        read_resource.to_string(),
        read_action.to_string(),
    ]);
    policies.push(vec![
        "role:no-object-read".to_string(),
        "stock_adjustment".to_string(),
        "approval_runtime_admin".to_string(),
    ]);
    policies.extend([
        vec![
            format!("role:{DISABLED_RUNTIME_ROLE}"),
            "stock_adjustment".to_string(),
            "approval_runtime_admin".to_string(),
        ],
        vec![
            format!("role:{ENABLED_OBJECT_READER_ROLE}"),
            read_resource.to_string(),
            read_action.to_string(),
        ],
    ]);
    assert!(adapter
        .add_policies("p", "p", policies)
        .await
        .expect("写入读取与运行管理权限"));

    for scope in [
        organization_scope("scope-manager", DataScopeSubjectType::Role, "manager", ORG_1),
        company_scope("scope-company-manager", "company-manager"),
        organization_scope("scope-reader", DataScopeSubjectType::Role, "reader", ORG_1),
        organization_scope("scope-wrong-org", DataScopeSubjectType::Role, "wrong-org", ORG_2),
        organization_scope(
            "scope-wrong-user-role",
            DataScopeSubjectType::Role,
            "wrong-user-scope",
            ORG_1,
        ),
        organization_scope(
            "scope-wrong-user-self",
            DataScopeSubjectType::User,
            WRONG_USER_SCOPE,
            ORG_2,
        ),
        organization_scope(
            "scope-no-object-read",
            DataScopeSubjectType::Role,
            "no-object-read",
            ORG_1,
        ),
        organization_scope(
            "scope-enabled-object-reader",
            DataScopeSubjectType::Role,
            ENABLED_OBJECT_READER_ROLE,
            ORG_1,
        ),
    ] {
        db.data_scopes()
            .create(&scope, &mut NoTransaction)
            .await
            .expect("写入 DataScope");
    }
}

/// 构造与实例关联的库存调整冻结快照。
fn stock_snapshot(spec: &RuntimeFixture<'_>) -> ApprovalSubjectSnapshot {
    let object_id = match spec.snapshot {
        SnapshotFixture::Drifted => format!("{}-drift", spec.object_id),
        SnapshotFixture::Exact | SnapshotFixture::Missing => spec.object_id.to_string(),
    };
    ApprovalSubjectSnapshot::new(
        ApprovalSubjectSnapshotId::new(format!("snapshot-{}", spec.instance_id)),
        ApprovalProcessInstanceId::new(spec.instance_id),
        DocumentType::StockAdjustment,
        object_id,
        1,
        ApprovalSubjectSnapshotPayload {
            document_no: format!("ADJ-{}", spec.instance_id),
            responsible_org_id: spec.organization_id.to_string(),
            submitted_by: spec.started_by.to_string(),
            submitted_at: Instant::from_unix_secs(10),
            counterparty: None,
            total_amount: None,
            total_quantity: Some(Quantity::from_str("1").expect("测试数量")),
            line_count: 1,
        },
    )
    .expect("库存调整快照 fixture")
}

/// 插入完整 BPM instance/execution、可选 snapshot 与可选当前 WorkItem。
async fn seed_runtime(db: &mongodb::Database, spec: RuntimeFixture<'_>) {
    let instance_id = ApprovalProcessInstanceId::new(spec.instance_id);
    let execution_id = ApprovalNodeExecutionId::new(format!("exec-{}", spec.instance_id));
    let mut instance = ApprovalProcessInstance::start_running(NewProcessInstance {
        id: instance_id.clone(),
        process_definition_id: ApprovalProcessDefinitionId::new("definition-stock-adjustment-v1"),
        definition_version: 1,
        process_kind: ProcessKind::StockAdjustment,
        subject: SubjectRef::new(DocumentType::StockAdjustment.as_str(), spec.object_id)
            .expect("库存调整主体"),
        subject_version: 1,
        started_by: participant(spec.started_by),
        at: at(10),
    })
    .expect("运行实例 fixture");
    let mut execution = ApprovalNodeExecution::new_active(NewNodeExecution {
        id: execution_id.clone(),
        process_instance_id: instance_id.clone(),
        node_key: "review".to_string(),
        node_name: "库存调整复核".to_string(),
        round_no: 1,
        execution_no: 1,
        assignment_source: ApprovalExecutionAssignmentSource::Definition,
        replaces_execution_id: None,
        assignee_participant_id: participant(spec.approver),
        assignee_name_snapshot: spec.approver.to_string(),
        at: at(10),
    })
    .expect("当前执行 fixture");
    instance
        .set_current_execution(execution_id.clone(), at(10))
        .expect("设置当前执行");
    if spec.blocked {
        execution
            .block(ApprovalBlockerCode::ApproverAccountInactive, at(11))
            .expect("阻塞执行");
        instance
            .enter_blocked(ApprovalBlockerCode::ApproverAccountInactive, at(11))
            .expect("阻塞实例");
    }
    let receipt = fixture_receipt(
        &format!("receipt-{}", spec.instance_id),
        ApprovalCommandKind::StartApproval,
        spec.instance_id,
        &format!("start-{}", spec.instance_id),
        "test-digest",
        spec.instance_id,
        at(10),
    );
    db.bpm_workflow()
        .create_bpm_runtime(
            &instance,
            &[],
            &execution,
            &receipt,
            &ApprovalInstanceListProjection {
                current_node_key: Some(execution.node_key.clone()),
                current_node_name: Some(execution.node_name.clone()),
                current_assignee_participant_id: Some(spec.approver.to_string()),
                current_assignee_name: Some(spec.approver.to_string()),
                last_status_changed_at: Some(if spec.blocked { 11 } else { 10 }),
                ..ApprovalInstanceListProjection::default()
            },
            &mut NoTransaction,
        )
        .await
        .expect("写入 BPM 运行事实");

    if !matches!(spec.snapshot, SnapshotFixture::Missing) {
        db.approval_subject_snapshots()
            .create_immutable_snapshot(&stock_snapshot(&spec), &mut NoTransaction)
            .await
            .expect("写入冻结快照");
    }
    if spec.create_work_item {
        let work_item = WorkItem::new_document_approval(
            WorkItemId::new(format!("work-item-{}", spec.instance_id)),
            DocumentApprovalWorkItemData {
                approval_node_execution_id: execution_id,
                business_object_type: DocumentType::StockAdjustment.as_str().to_string(),
                business_object_id: spec.object_id.to_string(),
                subject_version: "1".to_string(),
                owner_role: "stock_adjustment_approver".to_string(),
                owner_organization_id: spec.organization_id.to_string(),
                owner_user_id: spec.approver.to_string(),
                priority: WorkItemPriority::Normal,
                due_at: None,
            },
            Instant::from_unix_secs(10),
        )
        .expect("审批 WorkItem fixture");
        db.work_items()
            .create(&work_item, &mut NoTransaction)
            .await
            .expect("写入审批 WorkItem");
    }
}

/// 建立测试数据库、授权事实和全部运行时矩阵。
async fn fixture() -> (TestDb, ApprovalRuntimeService) {
    let fixture = TestDb::new("approval_runtime_service_auth")
        .await
        .expect("测试数据库创建失败");
    ensure_indexes(fixture.db()).await.expect("索引创建失败");
    seed_accounts(fixture.db()).await;
    seed_authorization(fixture.db()).await;
    for spec in [
        RuntimeFixture {
            instance_id: MAIN_INSTANCE,
            object_id: "adjustment-main",
            started_by: STARTER,
            approver: APPROVER,
            organization_id: ORG_1,
            snapshot: SnapshotFixture::Exact,
            blocked: false,
            create_work_item: true,
        },
        RuntimeFixture {
            instance_id: WRONG_ORG_INSTANCE,
            object_id: "adjustment-wrong-org",
            started_by: STARTER,
            approver: "other-approver",
            organization_id: ORG_2,
            snapshot: SnapshotFixture::Exact,
            blocked: false,
            create_work_item: false,
        },
        RuntimeFixture {
            instance_id: MISSING_SNAPSHOT_INSTANCE,
            object_id: "adjustment-missing-snapshot",
            started_by: STARTER,
            approver: "other-approver",
            organization_id: ORG_1,
            snapshot: SnapshotFixture::Missing,
            blocked: false,
            create_work_item: false,
        },
        RuntimeFixture {
            instance_id: DRIFTED_SNAPSHOT_INSTANCE,
            object_id: "adjustment-drifted-snapshot",
            started_by: STARTER,
            approver: "other-approver",
            organization_id: ORG_1,
            snapshot: SnapshotFixture::Drifted,
            blocked: false,
            create_work_item: false,
        },
        RuntimeFixture {
            instance_id: BLOCKED_INSTANCE,
            object_id: "adjustment-blocked",
            started_by: STARTER,
            approver: "other-approver",
            organization_id: ORG_1,
            snapshot: SnapshotFixture::Exact,
            blocked: true,
            create_work_item: false,
        },
        RuntimeFixture {
            instance_id: INACTIVE_STARTED_INSTANCE,
            object_id: "adjustment-inactive-started",
            started_by: INACTIVE_STARTER,
            approver: "other-approver",
            organization_id: ORG_1,
            snapshot: SnapshotFixture::Exact,
            blocked: false,
            create_work_item: false,
        },
        RuntimeFixture {
            instance_id: INACTIVE_MINE_INSTANCE,
            object_id: "adjustment-inactive-mine",
            started_by: STARTER,
            approver: INACTIVE_OWNER,
            organization_id: ORG_1,
            snapshot: SnapshotFixture::Exact,
            blocked: false,
            create_work_item: true,
        },
    ] {
        seed_runtime(fixture.db(), spec).await;
    }
    let rbac = iam::shared_rbac_service(fixture.db().clone());
    let service = ApprovalRuntimeService::new(fixture.db().clone(), rbac);
    (fixture, service)
}

/// 提取隐藏存在性的稳定 NotFound 文案。
fn hidden_message(error: Error) -> String {
    match error {
        Error::NotFound(message) => message,
        other => panic!("应返回隐藏存在性的 NotFound，实际为 {other:?}"),
    }
}

/// 构造固定视图的规范化查询。
fn list_query(view: RuntimeInstanceListView) -> RuntimeInstanceListQuery {
    RuntimeInstanceListQuery::prepare(
        view,
        Some(DocumentType::StockAdjustment.as_str().to_string()),
        None,
        None,
        Some(100),
        None,
    )
    .expect("列表查询 fixture")
}

/// 将页面实例 ID 收敛为无序集合，避免测试依赖非目标排序细节。
fn instance_ids(page: &services::approval::execution::RuntimeInstanceListPage) -> HashSet<&str> {
    page.items.iter().map(|item| item.instance_id.as_str()).collect()
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn detail_history_and_recovery_hide_unauthorized_or_inactive_actors() {
    require_mongo!(async {
        let (_fixture, service) = fixture().await;

        assert_eq!(
            service
                .instance_detail(&actor(STARTER), MAIN_INSTANCE)
                .await
                .expect("有效发起人可读详情")
                .instance_id,
            MAIN_INSTANCE
        );
        assert_eq!(
            service
                .instance_detail(&actor(APPROVER), MAIN_INSTANCE)
                .await
                .expect("当前执行人与开放 WorkItem 一致时可读详情")
                .instance_id,
            MAIN_INSTANCE
        );
        assert_eq!(
            service
                .instance_detail(&actor(MANAGER), MAIN_INSTANCE)
                .await
                .expect("对象读取权与专属 DataScope 成立时可读详情")
                .instance_id,
            MAIN_INSTANCE
        );
        assert_eq!(
            service
                .instance_history(&actor(READER), MAIN_INSTANCE, None, 20)
                .await
                .expect("普通对象读取人可读历史")
                .items
                .len(),
            1
        );
        assert!(service
            .recovery_options(&actor(MANAGER), MAIN_INSTANCE)
            .await
            .is_ok());

        let unauthorized = hidden_message(
            service
                .instance_detail(&actor(UNAUTHORIZED), MAIN_INSTANCE)
                .await
                .expect_err("无权限主体必须隐藏实例"),
        );
        let guessed = hidden_message(
            service
                .instance_detail(&actor(UNAUTHORIZED), "inst-guessed")
                .await
                .expect_err("猜测 ID 必须隐藏实例"),
        );
        assert_eq!(unauthorized, guessed, "无权与不存在必须使用相同隐藏语义");

        for denied_actor in [WRONG_ORG, WRONG_USER_SCOPE, NO_OBJECT_READ] {
            hidden_message(
                service
                    .instance_detail(&actor(denied_actor), MAIN_INSTANCE)
                    .await
                    .expect_err("错误组织、用户范围交集或对象权限必须隐藏详情"),
            );
        }
        hidden_message(
            service
                .instance_history(&actor(UNAUTHORIZED), MAIN_INSTANCE, None, 20)
                .await
                .expect_err("无权主体必须隐藏历史"),
        );
        hidden_message(
            service
                .recovery_options(&actor(READER), MAIN_INSTANCE)
                .await
                .expect_err("无运行管理权的普通读取人必须隐藏恢复入口"),
        );
        hidden_message(
            service
                .recovery_options(&actor(WRONG_ORG), MAIN_INSTANCE)
                .await
                .expect_err("错误组织不得读取恢复选项"),
        );
        let inactive = hidden_message(
            service
                .instance_detail(&actor(INACTIVE_STARTER), INACTIVE_STARTED_INSTANCE)
                .await
                .expect_err("已停用发起人不得沿用历史身份读取详情"),
        );
        let inactive_guessed = hidden_message(
            service
                .instance_detail(&actor(INACTIVE_STARTER), "inst-guessed")
                .await
                .expect_err("已停用主体猜测 ID 仍使用隐藏语义"),
        );
        assert_eq!(inactive, inactive_guessed);
    });
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn started_managed_and_mine_views_apply_actor_permission_and_scope_contracts() {
    require_mongo!(async {
        let (_fixture, service) = fixture().await;

        let started = service
            .instance_list(&actor(STARTER), list_query(RuntimeInstanceListView::Started))
            .await
            .expect("有效发起人 Started 列表");
        let started_ids = instance_ids(&started);
        for expected in [
            MAIN_INSTANCE,
            WRONG_ORG_INSTANCE,
            MISSING_SNAPSHOT_INSTANCE,
            DRIFTED_SNAPSHOT_INSTANCE,
            BLOCKED_INSTANCE,
            INACTIVE_MINE_INSTANCE,
        ] {
            assert!(
                started_ids.contains(expected),
                "Started 必须保留本人发起实例 {expected}"
            );
        }
        assert!(!started_ids.contains(INACTIVE_STARTED_INSTANCE));

        let managed = service
            .instance_list(&actor(MANAGER), list_query(RuntimeInstanceListView::Managed))
            .await
            .expect("组织级运行管理员 Managed 列表");
        let managed_ids = instance_ids(&managed);
        assert!(managed_ids.contains(MAIN_INSTANCE));
        assert!(managed_ids.contains(BLOCKED_INSTANCE));
        assert!(!managed_ids.contains(WRONG_ORG_INSTANCE));
        assert!(!managed_ids.contains(MISSING_SNAPSHOT_INSTANCE));
        assert!(!managed_ids.contains(DRIFTED_SNAPSHOT_INSTANCE));

        let blocked = service
            .instance_list(&actor(MANAGER), list_query(RuntimeInstanceListView::Blocked))
            .await
            .expect("组织级运行管理员 Blocked 列表");
        assert_eq!(instance_ids(&blocked), HashSet::from([BLOCKED_INSTANCE]));

        let company_managed = service
            .instance_list(
                &actor(COMPANY_MANAGER),
                list_query(RuntimeInstanceListView::Managed),
            )
            .await
            .expect("公司级运行管理员 Managed 列表");
        let company_ids = instance_ids(&company_managed);
        assert!(company_ids.contains(MISSING_SNAPSHOT_INSTANCE));
        assert!(company_ids.contains(DRIFTED_SNAPSHOT_INSTANCE));
        assert!(company_ids.contains(WRONG_ORG_INSTANCE));

        let wrong_org = service
            .instance_list(&actor(WRONG_ORG), list_query(RuntimeInstanceListView::Managed))
            .await
            .expect("错误组织主体只能看到其被授权组织");
        let wrong_org_ids = instance_ids(&wrong_org);
        assert!(wrong_org_ids.contains(WRONG_ORG_INSTANCE));
        assert!(!wrong_org_ids.contains(MAIN_INSTANCE));

        for denied_actor in [READER, UNAUTHORIZED, WRONG_USER_SCOPE, NO_OBJECT_READ] {
            let denied = service
                .instance_list(&actor(denied_actor), list_query(RuntimeInstanceListView::Managed))
                .await
                .expect("未满足完整管理授权时返回空页");
            assert!(denied.items.is_empty(), "{denied_actor} 不得进入 Managed 列表");
            assert_eq!(denied.total, 0);
        }

        let mine = service
            .instance_list(&actor(APPROVER), list_query(RuntimeInstanceListView::Mine))
            .await
            .expect("有效当前责任人 Mine 列表");
        assert_eq!(instance_ids(&mine), HashSet::from([MAIN_INSTANCE]));

        hidden_message(
            service
                .instance_list(
                    &actor(INACTIVE_STARTER),
                    list_query(RuntimeInstanceListView::Started),
                )
                .await
                .expect_err("已停用发起人不得读取 Started 列表"),
        );
        hidden_message(
            service
                .instance_list(&actor(INACTIVE_OWNER), list_query(RuntimeInstanceListView::Mine))
                .await
                .expect_err("已停用责任人不得读取 Mine 列表"),
        );
    });
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn disabled_runtime_admin_policy_cannot_combine_with_enabled_object_reader() {
    require_mongo!(async {
        let (fixture, service) = fixture().await;
        let rbac = iam::shared_rbac_service(fixture.db().clone());
        let runtime_admin =
            Permission::parse("stock_adjustment:approval_runtime_admin").expect("运行管理权限 fixture");
        assert!(
            rbac.enforce(
                &subject(AccountKind::Admin, DISABLED_RUNTIME_READER),
                &runtime_admin,
            )
            .await
            .expect("读取停用角色残留的 actor 级 Casbin 授权"),
            "回归前提必须保留停用角色的残留 g/p 事实",
        );
        assert!(
            !rbac
                .enforce(&format!("role:{ENABLED_OBJECT_READER_ROLE}"), &runtime_admin)
                .await
                .expect("重验启用对象读取角色"),
            "启用对象读取角色不得自身具备运行管理权",
        );

        assert_eq!(
            service
                .instance_detail(&actor(DISABLED_RUNTIME_READER), MAIN_INSTANCE)
                .await
                .expect("启用对象读取角色仍可读普通详情")
                .instance_id,
            MAIN_INSTANCE
        );
        let disabled_runtime = service
            .instance_list(
                &actor(DISABLED_RUNTIME_READER),
                list_query(RuntimeInstanceListView::Managed),
            )
            .await
            .expect("停用运行管理角色不得与启用对象读取角色拼接");
        assert!(disabled_runtime.items.is_empty());
        assert_eq!(disabled_runtime.total, 0);
        hidden_message(
            service
                .recovery_options(&actor(DISABLED_RUNTIME_READER), MAIN_INSTANCE)
                .await
                .expect_err("停用角色残留 g/p 不得恢复运行管理权"),
        );
    });
}
