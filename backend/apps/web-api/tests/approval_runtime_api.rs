//! D03 审批运行时与阻塞管理 HTTP 合同的真实 MongoDB 集成测试。

use std::{
    collections::BTreeSet,
    sync::{Arc, Mutex},
};

use config::{AppConfig, Config, DatabaseConfig, S3Config, SafeConfig};
use database::{
    AccessControlExt, ApprovalExt, Executor, NoTransaction, StartProcessingEligibility,
    StartProcessingOutcome, WorkItemExt,
};
use entities::{
    access_control::{DataScope, DataScopeData, DataScopeSubjectType, DataScopeType, UserRole, UserRoleData},
    approval::{
        ApprovalDecision, ApprovalInstance, ApprovalInstanceData, ApprovalInstanceStatus,
        ApprovalRuntimeKind, ApprovalStepInstance, ApprovalStepInstanceData, ApprovalStepStatus,
    },
    common::time::Instant,
    ids::{ApprovalInstanceId, ApprovalStepInstanceId, DataScopeId, UserRoleId, WorkItemId},
    work_item::{
        AssignmentMode, AssignmentSource, WorkItem, WorkItemData, WorkItemPriority, WorkItemStatus,
        WorkItemType,
    },
    Role, RoleData, RoleId,
};
use mongodb::{
    bson::{doc, Document},
    Database,
};
use serde_json::{json, Value};
use services::approval::{
    ensure_approval_definitions, ApprovalActionContext, ApprovalActionFuture, ApprovalBusinessAction,
    ApprovalDomainActionPort, ApprovalRuntimePort, ApprovalRuntimeView, InternalApprovalRuntime,
    StartApprovalCommand, SubmitDecisionCommand, CARD_SALES_APPROVAL,
};
use storage::{S3Storage, S3StorageConfig};
use test_support::{mint_jwt, require_mongo, seed_admin_account, TestApi, TestDb};
use web_api::{app_state::AppState, core::routes};

const JWT_SECRET: &str = "approval-runtime-api-test-secret-at-least-32-bytes";
const INSTANCE_ID: &str = "approval-instance-blocked-1";
const STEP_ID: &str = "approval-step-blocked-1";
const WORK_ITEM_ID: &str = "approval-work-item-blocked-1";
const OWNER_ORGANIZATION_ID: &str = "company";
const SALES_LEADER_ROLE: &str = "role-sales-leader";
const OPERATIONS_ROLE: &str = "role-operations";

/// 可记录调用并在指定强类型动作上失败的测试领域端口。
#[derive(Debug, Default)]
struct TestApprovalActionPort {
    fail_on: Option<ApprovalBusinessAction>,
    calls: Mutex<Vec<ApprovalBusinessAction>>,
}

impl TestApprovalActionPort {
    /// 构造仅在给定强类型动作上返回业务失败的端口。
    fn failing_on(action: ApprovalBusinessAction) -> Self {
        Self {
            fail_on: Some(action),
            calls: Mutex::new(Vec::new()),
        }
    }

    /// 返回指定动作的实际调用次数。
    fn call_count(&self, expected: ApprovalBusinessAction) -> usize {
        self.calls
            .lock()
            .expect("测试动作记录锁不得中毒")
            .iter()
            .filter(|action| **action == expected)
            .count()
    }
}

impl ApprovalDomainActionPort for TestApprovalActionPort {
    fn execute<'a>(
        &'a self,
        action: ApprovalBusinessAction,
        _context: &'a ApprovalActionContext,
        _executor: &'a mut dyn Executor,
    ) -> ApprovalActionFuture<'a> {
        Box::pin(async move {
            self.calls.lock().expect("测试动作记录锁不得中毒").push(action);
            if self.fail_on == Some(action) {
                return Err(services::Error::BusinessLogicError(
                    "测试领域动作故障".to_string(),
                ));
            }
            Ok(())
        })
    }
}

/// 完整运行时场景使用的三个隔离账号。
struct RuntimeActors {
    starter: String,
    manager: String,
    operator: String,
}

/// 返回 JSON 对象的稳定键集合。
fn object_keys(value: &Value) -> BTreeSet<String> {
    value
        .as_object()
        .expect("合同投影必须是 JSON 对象")
        .keys()
        .cloned()
        .collect()
}

/// 实例、步骤与待办视图版本必须是可解析的整数字符串。
fn parse_version(value: &str) -> u64 {
    value.parse().expect("运行时版本必须是 u64 字符串")
}

/// 写入审批解析器所需的启用角色与公司级责任范围。
async fn seed_runtime_role(db: &Database, role_id: &str, name: &str) {
    let role = Role::new(
        role_id.to_string(),
        RoleData {
            name: name.to_string(),
            description: None,
            system: true,
        },
    )
    .expect("审批测试角色构造失败");
    db.roles()
        .create(&role, &mut NoTransaction)
        .await
        .expect("审批测试角色写入失败");
    let scope = DataScope::new(
        DataScopeId::new(format!("approval-runtime-scope-{role_id}")),
        DataScopeData {
            subject_type: DataScopeSubjectType::Role,
            subject_id: role_id.to_string(),
            scope_type: DataScopeType::Company,
            scope_targets: Vec::new(),
        },
    )
    .expect("审批角色公司级范围构造失败");
    db.data_scopes()
        .create(&scope, &mut NoTransaction)
        .await
        .expect("审批角色公司级范围写入失败");
}

/// 把账号绑定到指定审批角色，生效时间固定早于测试执行时间。
async fn bind_runtime_role(db: &Database, actor_id: &str, role_id: &str) {
    let binding = UserRole::new(
        UserRoleId::new(format!("approval-binding-{role_id}-{actor_id}")),
        UserRoleData {
            user_id: actor_id.to_string(),
            role_id: RoleId::parse(role_id).expect("审批角色 ID 必须合法"),
            effective_from: Instant::from_unix_secs(1),
            effective_to: None,
            assigned_by: "system:approval-runtime-test".to_string(),
        },
    )
    .expect("审批测试角色绑定构造失败");
    db.user_roles()
        .create(&binding, &mut NoTransaction)
        .await
        .expect("审批测试角色绑定写入失败");
}

/// 写入启动人、销售领导和运营处理人的最小责任事实。
async fn seed_runtime_actors(db: &Database, manager_is_operator: bool) -> RuntimeActors {
    seed_runtime_role(db, SALES_LEADER_ROLE, "审批测试销售领导").await;
    seed_runtime_role(db, OPERATIONS_ROLE, "审批测试运营").await;
    let starter = seed_admin_account(db).await.expect("审批启动人种子失败");
    let manager = seed_admin_account(db).await.expect("销售领导种子失败");
    let operator = seed_admin_account(db).await.expect("运营处理人种子失败");
    bind_runtime_role(db, &manager, SALES_LEADER_ROLE).await;
    bind_runtime_role(db, &operator, OPERATIONS_ROLE).await;
    if manager_is_operator {
        bind_runtime_role(db, &manager, OPERATIONS_ROLE).await;
    }
    RuntimeActors {
        starter,
        manager,
        operator,
    }
}

/// 构造唯一的审批启动命令。
fn start_command(object_id: &str, starter: &str, idempotency_key: &str) -> StartApprovalCommand {
    StartApprovalCommand {
        definition_key: CARD_SALES_APPROVAL.to_string(),
        business_object_type: "SALES_ORDER_SUBMISSION".to_string(),
        business_object_id: object_id.to_string(),
        subject_version: format!("{object_id}-v1"),
        owner_organization_id: OWNER_ORGANIZATION_ID.to_string(),
        started_by: starter.to_string(),
        idempotency_key: idempotency_key.to_string(),
    }
}

/// 根据运行时最新视图构造带乐观锁的审批决定命令。
fn decision_command(
    view: &ApprovalRuntimeView,
    work_item_id: &str,
    task_version: u64,
    actor_id: &str,
    decision: ApprovalDecision,
    reason: Option<&str>,
    idempotency_key: &str,
) -> SubmitDecisionCommand {
    SubmitDecisionCommand {
        work_item_id: work_item_id.to_string(),
        approval_instance_id: view.instance.id.clone(),
        approval_step_instance_id: view.step.id.clone(),
        expected_task_version: task_version,
        expected_instance_version: parse_version(&view.instance.instance_version),
        expected_step_version: parse_version(&view.step.step_version),
        expected_subject_version: view.instance.subject_version.clone(),
        decision,
        reason: reason.map(str::to_string),
        actor_id: actor_id.to_string(),
        idempotency_key: idempotency_key.to_string(),
    }
}

/// 以视图中的当前 DIRECT 待办构造审批决定命令。
fn direct_decision_command(
    view: &ApprovalRuntimeView,
    actor_id: &str,
    decision: ApprovalDecision,
    reason: Option<&str>,
    idempotency_key: &str,
) -> SubmitDecisionCommand {
    let item = view.work_item.as_ref().expect("DIRECT 步骤必须存在待办");
    decision_command(
        view,
        &item.id,
        parse_version(&item.task_version),
        actor_id,
        decision,
        reason,
        idempotency_key,
    )
}

/// 原子开始处理运营 POOL 待办并返回已形成个人责任的最新事实。
async fn claim_operations_work_item(db: &Database, view: &ApprovalRuntimeView, actor_id: &str) -> WorkItem {
    let item = view.work_item.as_ref().expect("运营步骤必须存在 POOL 待办");
    assert_eq!(item.assignment_mode, AssignmentMode::Pool);
    let outcome = db
        .work_items()
        .start_processing(
            &item.id,
            parse_version(&item.task_version),
            StartProcessingEligibility {
                owner_role: &item.owner_role,
                owner_organization_id: &item.owner_organization_id,
            },
            actor_id,
            Instant::now(),
            &mut NoTransaction,
        )
        .await
        .expect("运营 POOL 待办开始处理失败");
    match outcome {
        StartProcessingOutcome::Started(item) => item,
        other => panic!("运营 POOL 待办未形成唯一个人责任: {other:?}"),
    }
}

/// 构造测试 AppState；对象存储客户端不会在本合同测试中发起网络请求。
fn test_app_state(db: Database, mongo_uri: String, db_name: String) -> AppState {
    let s3 = S3Config {
        bucket: "approval-runtime-test".to_string(),
        region: "us-east-1".to_string(),
        endpoint: Some("http://127.0.0.1:9000".to_string()),
        access_key_id: "test-access-key".to_string(),
        secret_access_key: "test-secret-key".to_string(),
        session_token: None,
        key_prefix: None,
        public_base_url: "http://127.0.0.1:9000/approval-runtime-test".to_string(),
        force_path_style: true,
    };
    let storage = S3Storage::new(S3StorageConfig {
        bucket: s3.bucket.clone(),
        region: s3.region.clone(),
        endpoint: s3.endpoint.clone(),
        access_key_id: s3.access_key_id.clone(),
        secret_access_key: s3.secret_access_key.clone(),
        session_token: s3.session_token.clone(),
        key_prefix: s3.key_prefix.clone(),
        public_base_url: s3.public_base_url.clone(),
        force_path_style: s3.force_path_style,
    })
    .expect("S3 测试配置必须合法");
    let config = Config {
        app: AppConfig {
            port: 0,
            secret: JWT_SECRET.to_string(),
        },
        database: DatabaseConfig {
            uri: mongo_uri,
            db_name,
        },
        s3,
    };
    AppState::new(db, SafeConfig::new(config), storage)
}

/// 构造 Casbin 权限规则文档。
fn casbin_permission(role_key: &str, action: &str) -> Document {
    let values = vec![
        role_key.to_string(),
        "approval_instance".to_string(),
        action.to_string(),
    ];
    doc! {
        "_id": format!("p\u{1f}p\u{1f}{}", values.join("\u{1f}")),
        "sec": "p",
        "ptype": "p",
        "values": values,
    }
}

/// 为测试管理员补齐审批诊断/恢复权限与公司级数据范围。
async fn grant_approval_management(db: &Database) -> String {
    let account_id = seed_admin_account(db).await.expect("测试管理员种子失败");
    let role = db
        .collection::<Role>("roles")
        .find_one(doc! { "name": "P0 测试管理员" })
        .await
        .expect("测试角色查询失败")
        .expect("测试角色不存在");
    let role_key = format!("role:{}", role.base.id);
    db.collection::<Document>("casbin_rules")
        .insert_many([
            casbin_permission(&role_key, "diagnose"),
            casbin_permission(&role_key, "recover"),
        ])
        .await
        .expect("审批管理权限写入失败");

    let scope = DataScope::new(
        DataScopeId::new("approval-runtime-company-scope"),
        DataScopeData {
            subject_type: DataScopeSubjectType::Role,
            subject_id: role.base.id,
            scope_type: DataScopeType::Company,
            scope_targets: Vec::new(),
        },
    )
    .expect("公司级数据范围构造失败");
    db.data_scopes()
        .create(&scope, &mut NoTransaction)
        .await
        .expect("公司级数据范围写入失败");
    account_id
}

/// 写入互相一致的阻塞实例、阻塞步骤与开放任务事实。
async fn seed_blocked_runtime(db: &Database, owner_user_id: &str) {
    let blocked_at = Instant::from_unix_secs(1_700_000_000);
    let step_id = ApprovalStepInstanceId::new(STEP_ID);
    let mut instance = ApprovalInstance::new(
        ApprovalInstanceId::new(INSTANCE_ID),
        ApprovalInstanceData {
            definition_key: services::approval::CARD_SALES_APPROVAL.to_string(),
            definition_version: services::approval::CARD_SALES_APPROVAL_VERSION,
            runtime_kind: ApprovalRuntimeKind::Internal,
            business_object_type: "SALES_ORDER_SUBMISSION".to_string(),
            business_object_id: "submission-blocked-1".to_string(),
            owner_organization_id: OWNER_ORGANIZATION_ID.to_string(),
            subject_version: "submission-v1".to_string(),
            start_idempotency_key: "start-blocked-1".to_string(),
            current_step_instance_id: step_id.clone(),
            external_instance_id: None,
            started_by: "requester-1".to_string(),
            started_at: Instant::from_unix_secs(1_699_999_900),
        },
    )
    .expect("阻塞审批实例构造失败");
    instance
        .block("APPROVAL_OWNER_ROLE_UNAVAILABLE", blocked_at)
        .expect("审批实例阻塞失败");

    let mut step = ApprovalStepInstance::new(
        step_id,
        ApprovalStepInstanceData {
            approval_instance_id: ApprovalInstanceId::new(INSTANCE_ID),
            step_key: services::approval::SALES_MANAGER_APPROVAL.to_string(),
            sequence_no: 1,
            initial_status: ApprovalStepStatus::Active,
            external_activity_id: None,
        },
    )
    .expect("阻塞审批步骤构造失败");
    step.block("APPROVAL_OWNER_ROLE_UNAVAILABLE", blocked_at)
        .expect("审批步骤阻塞失败");

    let work_item = WorkItem::new_at(
        WorkItemId::new(WORK_ITEM_ID),
        WorkItemData {
            work_item_type: WorkItemType::CardSalesManagerApproval,
            approval_step_instance_id: Some(STEP_ID.to_string()),
            business_object_type: "SALES_ORDER_SUBMISSION".to_string(),
            business_object_id: "submission-blocked-1".to_string(),
            subject_version: "submission-v1".to_string(),
            assignment_mode: AssignmentMode::Direct,
            owner_role: "role-sales-manager".to_string(),
            owner_organization_id: OWNER_ORGANIZATION_ID.to_string(),
            owner_user_id: Some(owner_user_id.to_string()),
            assignment_source: AssignmentSource::StepResolver,
            priority: WorkItemPriority::Normal,
            due_at: None,
            reason_code: Some("APPROVAL_OWNER_ROLE_UNAVAILABLE".to_string()),
            impact_summary: None,
        },
        Instant::from_unix_secs(1_699_999_900),
    )
    .expect("审批任务构造失败");

    db.approval_instances()
        .create(&instance, &mut NoTransaction)
        .await
        .expect("阻塞审批实例写入失败");
    db.approval_step_instances()
        .create(&step, &mut NoTransaction)
        .await
        .expect("阻塞审批步骤写入失败");
    db.work_items()
        .create(&work_item, &mut NoTransaction)
        .await
        .expect("审批任务写入失败");
}

/// 断言阻塞审批及其任务投影只包含合同允许的安全字段。
fn assert_safe_blocked_projection(value: &Value) {
    assert_eq!(
        object_keys(value),
        BTreeSet::from([
            "allowed_actions".to_string(),
            "approval_instance_id".to_string(),
            "blocked_at".to_string(),
            "blocker_code".to_string(),
            "blocker_message".to_string(),
            "business_object_label".to_string(),
            "current_step_instance_id".to_string(),
            "instance_version".to_string(),
            "step_version".to_string(),
            "work_item".to_string(),
        ])
    );
    assert!(value["instance_version"].is_string());
    assert!(value["step_version"].is_string());

    let work_item = &value["work_item"];
    assert_eq!(
        object_keys(work_item),
        BTreeSet::from([
            "assignment_mode".to_string(),
            "id".to_string(),
            "owner_organization_id".to_string(),
            "owner_role".to_string(),
            "owner_user_id".to_string(),
            "status".to_string(),
            "task_version".to_string(),
            "work_item_type".to_string(),
        ])
    );
    assert!(work_item["task_version"].is_string());
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn runtime_advances_manager_then_operations_and_replays_each_decision_once() {
    require_mongo!(async {
        let fixture = TestDb::new("approval_serial_replay")
            .await
            .expect("测试数据库创建失败");
        database::ensure_indexes(fixture.db())
            .await
            .expect("索引创建失败");
        ensure_approval_definitions(fixture.db())
            .await
            .expect("审批定义 bootstrap 失败");
        let actors = seed_runtime_actors(fixture.db(), false).await;
        let action_port = Arc::new(TestApprovalActionPort::default());
        let runtime = InternalApprovalRuntime::new(fixture.db().clone(), action_port.clone());

        let started = runtime
            .start_approval(start_command(
                "submission-serial-replay",
                &actors.starter,
                "start-serial-replay",
            ))
            .await
            .expect("审批启动失败");
        assert_eq!(started.instance.status, ApprovalInstanceStatus::Running);
        assert_eq!(started.step.status, ApprovalStepStatus::Active);
        assert_eq!(
            started.work_item.as_ref().unwrap().assignment_mode,
            AssignmentMode::Direct
        );
        assert_eq!(
            started.work_item.as_ref().unwrap().owner_user_id.as_deref(),
            Some(actors.manager.as_str())
        );

        let manager_command = direct_decision_command(
            &started,
            &actors.manager,
            ApprovalDecision::Approve,
            None,
            "manager-approve-replay",
        );
        let operations = runtime
            .submit_decision(manager_command.clone())
            .await
            .expect("销售领导通过失败");
        assert_eq!(operations.instance.status, ApprovalInstanceStatus::Running);
        assert_eq!(operations.step.status, ApprovalStepStatus::Active);
        assert_eq!(
            operations.work_item.as_ref().unwrap().assignment_mode,
            AssignmentMode::Pool
        );
        assert!(operations.work_item.as_ref().unwrap().owner_user_id.is_none());

        let manager_replay = runtime
            .submit_decision(manager_command)
            .await
            .expect("销售领导同幂等键重放失败");
        assert_eq!(manager_replay, operations);
        assert_eq!(
            action_port.call_count(ApprovalBusinessAction::RecordSalesManagerApproval),
            1
        );

        let claimed = claim_operations_work_item(fixture.db(), &operations, &actors.operator).await;
        let operations_command = decision_command(
            &operations,
            &claimed.base.id,
            claimed.base.version,
            &actors.operator,
            ApprovalDecision::Approve,
            None,
            "operations-approve-replay",
        );
        let approved = runtime
            .submit_decision(operations_command.clone())
            .await
            .expect("运营通过失败");
        assert_eq!(approved.instance.status, ApprovalInstanceStatus::Approved);
        assert_eq!(approved.step.status, ApprovalStepStatus::Approved);
        assert_eq!(
            approved.work_item.as_ref().unwrap().status,
            WorkItemStatus::Completed
        );

        let operations_replay = runtime
            .submit_decision(operations_command)
            .await
            .expect("运营同幂等键重放失败");
        assert_eq!(operations_replay, approved);
        assert_eq!(
            action_port.call_count(ApprovalBusinessAction::ApproveAndActivateCardSales),
            1
        );
    });
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn final_domain_action_failure_rolls_back_step_task_instance_and_receipt() {
    require_mongo!(async {
        let fixture = TestDb::new("approval_final_rollback")
            .await
            .expect("测试数据库创建失败");
        database::ensure_indexes(fixture.db())
            .await
            .expect("索引创建失败");
        ensure_approval_definitions(fixture.db())
            .await
            .expect("审批定义 bootstrap 失败");
        let actors = seed_runtime_actors(fixture.db(), false).await;
        let action_port = Arc::new(TestApprovalActionPort::failing_on(
            ApprovalBusinessAction::ApproveAndActivateCardSales,
        ));
        let runtime = InternalApprovalRuntime::new(fixture.db().clone(), action_port.clone());

        let started = runtime
            .start_approval(start_command(
                "submission-final-rollback",
                &actors.starter,
                "start-final-rollback",
            ))
            .await
            .expect("审批启动失败");
        let operations = runtime
            .submit_decision(direct_decision_command(
                &started,
                &actors.manager,
                ApprovalDecision::Approve,
                None,
                "manager-approve-before-rollback",
            ))
            .await
            .expect("销售领导通过失败");
        let claimed = claim_operations_work_item(fixture.db(), &operations, &actors.operator).await;
        let command = decision_command(
            &operations,
            &claimed.base.id,
            claimed.base.version,
            &actors.operator,
            ApprovalDecision::Approve,
            None,
            "operations-final-failure",
        );

        let error = runtime
            .submit_decision(command.clone())
            .await
            .expect_err("领域动作故障必须使最终通过失败");
        assert!(matches!(error, services::Error::BusinessLogicError(_)));
        assert_eq!(
            action_port.call_count(ApprovalBusinessAction::ApproveAndActivateCardSales),
            1
        );

        let instance = fixture
            .db()
            .approval_instances()
            .find_by_id(&operations.instance.id, &mut NoTransaction)
            .await
            .expect("审批实例回读失败")
            .expect("审批实例不得丢失");
        let step = fixture
            .db()
            .approval_step_instances()
            .find_by_id(&operations.step.id, &mut NoTransaction)
            .await
            .expect("审批步骤回读失败")
            .expect("审批步骤不得丢失");
        let item = fixture
            .db()
            .work_items()
            .find_by_id(&claimed.base.id, &mut NoTransaction)
            .await
            .expect("审批待办回读失败")
            .expect("审批待办不得丢失");
        assert_eq!(instance.status, ApprovalInstanceStatus::Running);
        assert_eq!(
            instance.current_step_instance_id.as_ref().unwrap().as_ref(),
            operations.step.id.as_str()
        );
        assert_eq!(step.status, ApprovalStepStatus::Active);
        assert!(step.decision.is_none());
        assert_eq!(item.status, WorkItemStatus::Open);
        assert_eq!(item.owner_user_id.as_deref(), Some(actors.operator.as_str()));
        assert_eq!(item.base.version, claimed.base.version);

        let retry_error = runtime
            .submit_decision(command)
            .await
            .expect_err("失败事务不得留下伪幂等 receipt");
        assert!(matches!(retry_error, services::Error::BusinessLogicError(_)));
        assert_eq!(
            action_port.call_count(ApprovalBusinessAction::ApproveAndActivateCardSales),
            2
        );
    });
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn separation_of_duties_rejects_previous_manager_on_operations_step() {
    require_mongo!(async {
        let fixture = TestDb::new("approval_operations_sod")
            .await
            .expect("测试数据库创建失败");
        database::ensure_indexes(fixture.db())
            .await
            .expect("索引创建失败");
        ensure_approval_definitions(fixture.db())
            .await
            .expect("审批定义 bootstrap 失败");
        let actors = seed_runtime_actors(fixture.db(), true).await;
        assert_ne!(actors.operator, actors.manager);
        let runtime =
            InternalApprovalRuntime::new(fixture.db().clone(), Arc::new(TestApprovalActionPort::default()));

        let started = runtime
            .start_approval(start_command(
                "submission-operations-sod",
                &actors.starter,
                "start-operations-sod",
            ))
            .await
            .expect("审批启动失败");
        let operations = runtime
            .submit_decision(direct_decision_command(
                &started,
                &actors.manager,
                ApprovalDecision::Approve,
                None,
                "manager-approve-sod",
            ))
            .await
            .expect("销售领导通过失败");
        let claimed = claim_operations_work_item(fixture.db(), &operations, &actors.manager).await;
        let error = runtime
            .submit_decision(decision_command(
                &operations,
                &claimed.base.id,
                claimed.base.version,
                &actors.manager,
                ApprovalDecision::Approve,
                None,
                "manager-cannot-approve-operations",
            ))
            .await
            .expect_err("前一步销售领导不得处理运营审批");
        assert!(matches!(error, services::Error::Forbidden(_)));

        let step = fixture
            .db()
            .approval_step_instances()
            .find_by_id(&operations.step.id, &mut NoTransaction)
            .await
            .expect("审批步骤回读失败")
            .expect("审批步骤不得丢失");
        let item = fixture
            .db()
            .work_items()
            .find_by_id(&claimed.base.id, &mut NoTransaction)
            .await
            .expect("审批待办回读失败")
            .expect("审批待办不得丢失");
        assert_eq!(step.status, ApprovalStepStatus::Active);
        assert_eq!(item.status, WorkItemStatus::Open);
        assert_eq!(item.base.version, claimed.base.version);
    });
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn rejection_is_terminal_and_never_creates_operations_work_item() {
    require_mongo!(async {
        let fixture = TestDb::new("approval_reject_terminal")
            .await
            .expect("测试数据库创建失败");
        database::ensure_indexes(fixture.db())
            .await
            .expect("索引创建失败");
        ensure_approval_definitions(fixture.db())
            .await
            .expect("审批定义 bootstrap 失败");
        let actors = seed_runtime_actors(fixture.db(), false).await;
        let action_port = Arc::new(TestApprovalActionPort::default());
        let runtime = InternalApprovalRuntime::new(fixture.db().clone(), action_port.clone());

        let started = runtime
            .start_approval(start_command(
                "submission-reject-terminal",
                &actors.starter,
                "start-reject-terminal",
            ))
            .await
            .expect("审批启动失败");
        let rejected = runtime
            .submit_decision(direct_decision_command(
                &started,
                &actors.manager,
                ApprovalDecision::RejectToApplicant,
                Some("资料不完整"),
                "manager-reject-terminal",
            ))
            .await
            .expect("销售领导驳回失败");
        assert_eq!(rejected.instance.status, ApprovalInstanceStatus::Rejected);
        assert_eq!(rejected.step.status, ApprovalStepStatus::Rejected);
        assert!(rejected.instance.current_step_instance_id.is_none());
        assert_eq!(
            rejected.work_item.as_ref().unwrap().status,
            WorkItemStatus::Completed
        );
        assert_eq!(
            action_port.call_count(ApprovalBusinessAction::RejectCardSalesBySalesManager),
            1
        );

        let steps = fixture
            .db()
            .approval_step_instances()
            .list_by_instance(
                &ApprovalInstanceId::new(rejected.instance.id.clone()),
                &mut NoTransaction,
            )
            .await
            .expect("审批步骤列表回读失败");
        assert_eq!(steps.len(), 2);
        let operations = steps
            .iter()
            .find(|step| step.sequence_no == 2)
            .expect("冻结定义必须包含运营步骤");
        assert_eq!(operations.status, ApprovalStepStatus::Waiting);
        let operations_item = fixture
            .db()
            .work_items()
            .find_one(
                doc! { "approval_step_instance_id": &operations.base.id },
                &mut NoTransaction,
            )
            .await
            .expect("运营待办查询失败");
        assert!(operations_item.is_none());
    });
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn blocked_management_routes_enforce_safe_contract_and_latest_conflict_projection() {
    require_mongo!(async {
        let fixture = TestDb::new("approval_runtime_api")
            .await
            .expect("测试数据库创建失败");
        database::ensure_indexes(fixture.db())
            .await
            .expect("索引创建失败");
        let account_id = grant_approval_management(fixture.db()).await;
        seed_blocked_runtime(fixture.db(), &account_id).await;
        let token = mint_jwt(&account_id, JWT_SECRET, 3_600).expect("测试 JWT 签发失败");
        let mongo_uri = std::env::var("ERP_TEST_MONGO_URI").expect("MongoDB 测试连接未配置");
        let state = test_app_state(fixture.db().clone(), mongo_uri, fixture.name().to_string());
        let api = TestApi::new(routes::create(state));

        let (status, body) = api
            .get(
                "/admin/approval-instances?status=BLOCKED&page=1&page_size=20",
                Some(&token),
            )
            .await;
        assert_eq!(status, 200, "阻塞审批列表请求失败: {body}");
        let item = &body["data"]["items"][0];
        assert_safe_blocked_projection(item);
        assert_eq!(item["work_item"]["task_version"], "1");
        assert_eq!(item["allowed_actions"], json!(["RETRY_CURRENT_STEP"]));

        let recover = json!({
            "current_step_instance_id": STEP_ID,
            "expected_instance_version": "999",
            "expected_step_version": "1",
            "expected_task_version": "1",
            "recovery_action": "RETRY_CURRENT_STEP",
            "reason": "责任角色已恢复",
            "idempotency_key": "recover-stale-1"
        });
        let (status, conflict) = api
            .post(
                &format!("/admin/approval-instances/{INSTANCE_ID}/recover"),
                Some(&token),
                Some(recover.clone()),
            )
            .await;
        assert_eq!(status, 409, "陈旧版本必须返回 409: {conflict}");
        assert_eq!(conflict["code"], "CONFLICT");
        assert_eq!(conflict["success"], false);
        assert_safe_blocked_projection(&conflict["data"]);
        assert_eq!(conflict["data"]["instance_version"], "1");
        assert_eq!(conflict["data"]["step_version"], "1");
        assert_eq!(conflict["data"]["work_item"]["task_version"], "1");

        for forbidden_query in ["decision=APPROVE", "target_user_id=forged-user"] {
            let (status, _) = api
                .get(
                    &format!(
                        "/admin/approval-instances?status=BLOCKED&page=1&page_size=20&{forbidden_query}"
                    ),
                    Some(&token),
                )
                .await;
            assert_eq!(status, 400, "未注册列表查询字段必须被拒绝: {forbidden_query}");
        }

        for (field, value) in [
            ("decision", json!("APPROVE")),
            ("target_user_id", json!("forged-user")),
        ] {
            let mut forged = recover.clone();
            forged
                .as_object_mut()
                .expect("恢复请求必须是 JSON 对象")
                .insert(field.to_string(), value);
            let (status, _) = api
                .post(
                    &format!("/admin/approval-instances/{INSTANCE_ID}/recover"),
                    Some(&token),
                    Some(forged),
                )
                .await;
            assert_eq!(status, 422, "未注册恢复字段必须被拒绝: {field}");
        }

        for legacy_path in [
            "/admin/work-items/legacy-work-item/claim",
            "/admin/work-items/legacy-work-item/complete",
        ] {
            let (status, _) = api.post(legacy_path, Some(&token), Some(json!({}))).await;
            assert_eq!(status, 404, "旧任务路由不得继续存在: {legacy_path}");
        }
    });
}
