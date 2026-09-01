//! APP-E02：定义管理命令规范 key、receipt-first 与新事务回放的真实 MongoDB 验收。
//!
//! 用例使用随机独立库、真实 Casbin Mongo Adapter 和公开
//! `ApprovalDefinitionService::create_definition_draft` 端口。仅当
//! `ERP_TEST_MONGO_URI` 指向启用 `enableTestCommands` 的 MongoDB 7 副本集时执行。

use casbin::Adapter;
use database::{ensure_indexes, AccessControlExt, MongoCasbinAdapter, NoTransaction};
use entities::document_registry::DocumentType;
use entities::{
    AccountCore, AccountCoreData, AccountKind, AccountStatus, LoginAccount, Permission, Role, RoleData,
    RoleUpdate, Secret,
};
use mongodb::bson::{doc, Bson, Document};
use mongodb::Database;
use services::approval::definition::ApprovalDefinitionService;
use services::approval::definition_dto::{CreateDefinitionDraftRequest, DraftSource};
use services::audit::AuditActor;
use services::iam::{self, subject};
use services::{Error, ErrorCode};
use test_support::{require_mongo, TestDb};

const ADMIN_ID: &str = "definition-admin";
const ADMIN_ROLE_ID: &str = "definition-admin-role";
const RECEIPTS: &str = "approval_command_receipts";
const DEFINITIONS: &str = "approval_process_definitions";
const AUDITS: &str = "audit_logs";

/// 定义命令全部持久化事实；同载荷回放和失败分支必须逐文档保持不变。
#[derive(Debug, Clone, PartialEq)]
struct DefinitionFacts {
    definitions: Vec<Document>,
    nodes: Vec<Document>,
    transitions: Vec<Document>,
    receipts: Vec<Document>,
    audits: Vec<Document>,
}

/// 构造与 Casbin subject 一致的有效后台管理员。
fn account() -> AccountCore {
    AccountCore::new(
        ADMIN_ID.to_string(),
        AccountCoreData {
            secret: Secret::new(
                LoginAccount::new(format!("login-{ADMIN_ID}")).expect("测试登录账号"),
                "test-only-password",
            )
            .expect("测试凭证"),
            name: "定义管理员".to_string(),
            kind: AccountKind::Admin,
            status: AccountStatus::Active,
            email: None,
            phone: None,
            avatar: None,
        },
    )
    .expect("测试管理员")
}

/// 构造认证操作人。
fn actor() -> AuditActor {
    AuditActor::new(
        ADMIN_ID.to_string(),
        format!("login-{ADMIN_ID}"),
        AccountKind::Admin,
    )
}

/// 构造仅携带定义管理字段的空草稿命令。
fn create_request(name: &str, key: &str) -> CreateDefinitionDraftRequest {
    CreateDefinitionDraftRequest {
        document_type: DocumentType::StockAdjustment,
        name: name.to_string(),
        draft_source: DraftSource::Empty,
        idempotency_key: key.to_string(),
    }
}

/// 写入启用角色、账号及精确类型级定义管理授权。
async fn seed_definition_admin(db: &Database) {
    db.accounts()
        .create(&account(), &mut NoTransaction)
        .await
        .expect("写入定义管理员");
    let role = Role::new(
        ADMIN_ROLE_ID.to_string(),
        RoleData {
            name: "定义管理员".to_string(),
            description: None,
            system: false,
        },
    )
    .expect("定义管理角色");
    db.roles()
        .create(&role, &mut NoTransaction)
        .await
        .expect("写入定义管理角色");

    let mut adapter = MongoCasbinAdapter::new(db.clone());
    assert!(adapter
        .add_policies(
            "g",
            "g",
            vec![vec![
                subject(AccountKind::Admin, ADMIN_ID),
                format!("role:{ADMIN_ROLE_ID}"),
            ]],
        )
        .await
        .expect("写入账号角色绑定"));
    assert!(adapter
        .add_policies(
            "p",
            "p",
            vec![vec![
                format!("role:{ADMIN_ROLE_ID}"),
                DocumentType::StockAdjustment.as_str().to_string(),
                "approval_definition_admin".to_string(),
            ]],
        )
        .await
        .expect("写入定义管理权限"));
}

/// 停用当前定义管理员唯一授权角色，验证 replay 仍重验实时授权。
async fn disable_definition_admin_role(db: &Database) {
    let mut role = db
        .roles()
        .find_by_id(ADMIN_ROLE_ID, &mut NoTransaction)
        .await
        .expect("读取定义管理角色")
        .expect("定义管理角色必须存在");
    role.update(RoleUpdate {
        disabled: Some(true),
        ..RoleUpdate::default()
    })
    .expect("停用定义管理角色");
    db.roles()
        .update(&mut role, &mut NoTransaction)
        .await
        .expect("持久化角色停用");
}

/// 读取集合并按 `_id` 固定顺序，供逐文档比较零重写。
async fn documents(db: &Database, collection: &str, filter: Document) -> Vec<Document> {
    let mut cursor = db
        .collection::<Document>(collection)
        .find(filter)
        .sort(doc! { "_id": 1_i32 })
        .await
        .expect("读取定义命令事实");
    let mut documents = Vec::new();
    while cursor.advance().await.expect("推进定义命令事实游标") {
        documents.push(cursor.deserialize_current().expect("反序列化定义命令事实"));
    }
    documents
}

/// 按服务端记录顺序读取当前随机库 profiler。
async fn profiles(db: &Database) -> Vec<Document> {
    let mut cursor = db
        .collection::<Document>("system.profile")
        .find(doc! {})
        .sort(doc! { "$natural": 1_i32 })
        .await
        .expect("读取随机库 profiler");
    let mut profiles = Vec::new();
    while cursor.advance().await.expect("推进 profiler 游标") {
        profiles.push(cursor.deserialize_current().expect("反序列化 profiler"));
    }
    profiles
}

/// 读取定义、图、收据与定义审计的完整持久化快照。
async fn definition_facts(db: &Database) -> DefinitionFacts {
    DefinitionFacts {
        definitions: documents(db, DEFINITIONS, doc! {}).await,
        nodes: documents(db, "approval_node_definitions", doc! {}).await,
        transitions: documents(db, "approval_transition_definitions", doc! {}).await,
        receipts: documents(db, RECEIPTS, doc! {}).await,
        audits: documents(db, AUDITS, doc! { "action": "approval_definition.create_draft" }).await,
    }
}

/// 读取 failCommand 累计进入次数。
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

/// 在本随机库的指定集合拒绝下一条 insert。
async fn arm_insert_error(db: &Database, collection: &str, error_code: i32) -> i64 {
    let before = fail_command_entries(db).await;
    db.client()
        .database("admin")
        .run_command(doc! {
            "configureFailPoint": "failCommand",
            "mode": { "times": 1_i32 },
            "data": {
                "failCommands": ["insert"],
                "namespace": format!("{}.{}", db.name(), collection),
                "errorCode": error_code,
            },
        })
        .await
        .expect("独立副本集必须启用 enableTestCommands");
    before
}

/// 在本随机库阻塞第一条 receipt insert，使另一会话先提交真实唯一键胜者。
async fn arm_first_receipt_insert_block(db: &Database) -> i64 {
    let before = fail_command_entries(db).await;
    db.client()
        .database("admin")
        .run_command(doc! {
            "configureFailPoint": "failCommand",
            "mode": { "times": 1_i32 },
            "data": {
                "failCommands": ["insert"],
                "namespace": format!("{}.{}", db.name(), RECEIPTS),
                "blockConnection": true,
                "blockTimeMS": 3_000_i32,
            },
        })
        .await
        .expect("挂起首条 receipt insert");
    before
}

/// 等待本轮 failpoint 确实进入，禁止用调度概率冒充并发覆盖。
async fn wait_for_fail_command(db: &Database, before: i64) {
    db.client()
        .database("admin")
        .run_command(doc! {
            "waitForFailPoint": "failCommand",
            "timesEntered": before.checked_add(1).expect("failpoint 计数溢出"),
            "maxTimeMS": 10_000_i32,
        })
        .await
        .expect("目标 insert 必须进入 failpoint");
}

/// 关闭全局 failCommand；namespace 已限制在本随机库。
async fn disarm_fail_command(db: &Database) {
    db.client()
        .database("admin")
        .run_command(doc! {
            "configureFailPoint": "failCommand",
            "mode": "off",
        })
        .await
        .expect("关闭 failCommand");
}

/// 证明一次性故障精确作用于本随机库目标集合的 insert。
async fn assert_insert_fail_command_target(db: &Database, collection: &str, error_code: i32) {
    let state = db
        .client()
        .database("admin")
        .run_command(doc! {
            "getParameter": 1_i32,
            "failpoint.failCommand": 1_i32,
        })
        .await
        .expect("回读 failCommand 配置");
    let data = state
        .get_document("failpoint.failCommand")
        .and_then(|failpoint| failpoint.get_document("data"))
        .expect("failCommand data");
    assert_eq!(
        data.get_i32("errorCode").expect("failCommand errorCode"),
        error_code
    );
    assert_eq!(
        data.get_array("failCommands")
            .expect("failCommand command types")
            .as_slice(),
        &[Bson::String("insert".to_string())]
    );
    assert_eq!(
        data.get_str("namespace").expect("failCommand namespace"),
        format!("{}.{}", db.name(), collection)
    );
}

/// 返回 profiler 命令所属服务端会话和事务号。
fn transaction_identity(profile: &Document) -> (Document, Bson) {
    let command = profile.get_document("command").expect("profile command");
    (
        command.get_document("lsid").expect("profile lsid").clone(),
        command.get("txnNumber").expect("profile txnNumber").clone(),
    )
}

/// 证明 receipt insert 已作为本次失败事务的第一笔物理写进入服务端。
async fn assert_receipt_insert_was_transactional(db: &Database) {
    let profiles = profiles(db).await;
    let receipt_namespace = format!("{}.{}", db.name(), RECEIPTS);
    let receipt = profiles
        .iter()
        .position(|profile| {
            profile.get_str("ns").ok() == Some(receipt_namespace.as_str())
                && profile
                    .get_document("command")
                    .and_then(|command| command.get_str("insert"))
                    .ok()
                    == Some(RECEIPTS)
                && profile.get_i32("ninserted").ok() == Some(1)
        })
        .expect("事务必须先成功执行 receipt insert");
    let command = profiles[receipt]
        .get_document("command")
        .expect("receipt profile command");
    assert_eq!(command.get_bool("autocommit").ok(), Some(false));
    let _ = transaction_identity(&profiles[receipt]);
}

/// 读取 profiler 中的收据并发失败码。
///
/// 快照事务下唯一键竞争可能表现为 `DuplicateKey(11000)` 或
/// `WriteConflict(112)`；两者都必须退出失败事务后回读。
fn concurrent_receipt_insert_error(profile: &Document) -> Option<i32> {
    let code = match profile.get("errCode") {
        Some(Bson::Int32(code)) => Some(*code),
        Some(Bson::Int64(code)) => i32::try_from(*code).ok(),
        _ => None,
    };
    if matches!(code, Some(11_000 | 112)) {
        return code;
    }
    match profile
        .get_str("errName")
        .ok()
        .or_else(|| profile.get_str("codeName").ok())
    {
        Some("DuplicateKey") => Some(11_000),
        Some("WriteConflict") => Some(112),
        _ => None,
    }
}

/// 证明真实 receipt 唯一竞争败者退出失败事务后，以不同事务回读胜者。
async fn assert_duplicate_loser_replays_in_new_transaction(db: &Database) {
    let profiles = profiles(db).await;
    let receipt_namespace = format!("{}.{}", db.name(), RECEIPTS);
    let duplicate = profiles
        .iter()
        .enumerate()
        .rev()
        .find(|(_, profile)| {
            profile.get_str("ns").ok() == Some(receipt_namespace.as_str())
                && profile
                    .get_document("command")
                    .and_then(|command| command.get_str("insert"))
                    .ok()
                    == Some(RECEIPTS)
                && concurrent_receipt_insert_error(profile).is_some()
        })
        .expect("并发败者必须收到真实 receipt DuplicateKey 或 WriteConflict");
    let failed_transaction = transaction_identity(duplicate.1);
    let replay = profiles
        .iter()
        .skip(duplicate.0 + 1)
        .find(|profile| {
            profile.get_str("ns").ok() == Some(receipt_namespace.as_str())
                && profile
                    .get_document("command")
                    .and_then(|command| command.get_str("find"))
                    .ok()
                    == Some(RECEIPTS)
                && profile
                    .get_document("command")
                    .and_then(|command| command.get_bool("autocommit"))
                    .ok()
                    == Some(false)
        })
        .expect("败者必须在失败 insert 后事务化回读 receipt");
    assert_ne!(
        transaction_identity(replay),
        failed_transaction,
        "失败事务不得继续用于胜者回读"
    );
}

/// 显式删除随机测试库，避免异步 Drop 留下残余。
async fn cleanup(fixture: TestDb) {
    fixture.db().drop().await.expect("清理随机测试数据库");
    std::mem::forget(fixture);
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向启用 enableTestCommands 的 MongoDB 副本集"]
async fn definition_create_is_receipt_first_replay_safe_and_concurrent_single_winner() {
    require_mongo!(async {
        let fixture = TestDb::new("approval_definition_e02")
            .await
            .expect("创建随机测试库");
        ensure_indexes(fixture.db()).await.expect("创建生产索引");
        seed_definition_admin(fixture.db()).await;
        let rbac = iam::shared_rbac_service(fixture.db().clone());
        let permission =
            Permission::parse("stock_adjustment:approval_definition_admin").expect("定义管理权限常量");
        assert!(rbac
            .enforce(&subject(AccountKind::Admin, ADMIN_ID), &permission)
            .await
            .expect("预热定义管理授权"));
        let service = std::sync::Arc::new(ApprovalDefinitionService::new(fixture.db().clone(), rbac));

        fixture
            .db()
            .run_command(doc! { "profile": 2_i32, "slowms": 0_i32 })
            .await
            .expect("启用随机库 profiler");
        let fault_before = arm_insert_error(fixture.db(), DEFINITIONS, 2).await;
        let failed = service
            .create_definition_draft(create_request("失败后回滚", "definition-rollback"), &actor())
            .await
            .expect_err("receipt 后的定义 insert 故障必须整笔失败");
        assert!(matches!(failed, Error::RepositoryError(_)));
        assert_eq!(
            fail_command_entries(fixture.db()).await,
            fault_before + 1,
            "definition insert 故障必须精确命中一次"
        );
        assert_insert_fail_command_target(fixture.db(), DEFINITIONS, 2).await;
        disarm_fail_command(fixture.db()).await;
        assert_receipt_insert_was_transactional(fixture.db()).await;
        assert_eq!(
            definition_facts(fixture.db()).await,
            DefinitionFacts {
                definitions: Vec::new(),
                nodes: Vec::new(),
                transitions: Vec::new(),
                receipts: Vec::new(),
                audits: Vec::new(),
            },
            "receipt 后任一写失败必须把 receipt 与全部业务事实一起回滚"
        );

        let race_before = arm_first_receipt_insert_block(fixture.db()).await;
        let first_service = std::sync::Arc::clone(&service);
        let first = tokio::spawn(async move {
            first_service
                .create_definition_draft(create_request("  并发定义  ", "  definition-race  "), &actor())
                .await
        });
        wait_for_fail_command(fixture.db(), race_before).await;
        let winner = service
            .create_definition_draft(create_request("并发定义", "definition-race"), &actor())
            .await
            .expect("未阻塞会话必须提交唯一胜者");
        let recovered = first
            .await
            .expect("并发任务不得 panic")
            .expect("败者必须退出失败事务后回读胜者");
        disarm_fail_command(fixture.db()).await;
        assert_eq!(recovered, winner);
        assert_duplicate_loser_replays_in_new_transaction(fixture.db()).await;

        let committed = definition_facts(fixture.db()).await;
        assert_eq!(committed.definitions.len(), 1);
        assert!(committed.nodes.is_empty());
        assert!(committed.transitions.is_empty());
        assert_eq!(committed.receipts.len(), 1);
        assert_eq!(committed.audits.len(), 1);

        let replay = service
            .create_definition_draft(create_request("并发定义", "\tdefinition-race\n"), &actor())
            .await
            .expect("空白等价 key 与同规范载荷必须回放");
        assert_eq!(replay, winner);
        assert_eq!(
            definition_facts(fixture.db()).await,
            committed,
            "同载荷 replay 必须逐文档零重写"
        );

        let conflict = service
            .create_definition_draft(create_request("异载荷定义", "definition-race"), &actor())
            .await
            .expect_err("同 scope+key 异载荷必须稳定冲突");
        assert_eq!(
            conflict.code(),
            Some(ErrorCode::ApprovalIdempotencyPayloadConflict)
        );
        assert_eq!(definition_facts(fixture.db()).await, committed);

        disable_definition_admin_role(fixture.db()).await;
        let revoked = service
            .create_definition_draft(create_request("并发定义", "definition-race"), &actor())
            .await
            .expect_err("当前失权后不得回放已有 receipt");
        assert!(matches!(revoked, Error::Forbidden(_)));
        assert_eq!(definition_facts(fixture.db()).await, committed);

        fixture
            .db()
            .run_command(doc! { "profile": 0_i32 })
            .await
            .expect("关闭随机库 profiler");
        drop(service);
        cleanup(fixture).await;
    });
}
