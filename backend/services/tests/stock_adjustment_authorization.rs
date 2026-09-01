//! FUL-S01/S02/S03：库存调整读写与库存余额投影的真实 MongoDB 授权验收。
//!
//! 用例使用随机独立数据库、真实 Casbin Mongo Adapter 和公开 `InventoryService`；
//! 仅在 `ERP_TEST_MONGO_URI` 指向 MongoDB 7 单节点副本集时通过
//! `--include-ignored` 串行执行。

use std::collections::HashSet;
use std::str::FromStr;

use bpm::ids::{ApprovalNodeDefinitionId, ApprovalProcessDefinitionId, ApprovalTransitionDefinitionId};
use bpm::model::types::ApprovalTransitionEvent;
use bpm::model::{
    ApprovalNodeDefinition, ApprovalProcessDefinition, ApprovalTransitionDefinition, NewNodeDefinition,
    ParticipantId, ProcessKind, Timestamp,
};
use casbin::Adapter;
use database::{
    ensure_indexes, AccessControlExt, BpmExt, DocumentRegistryExt, InventoryExt, MongoCasbinAdapter,
    NoTransaction, WarehouseExt,
};
use entities::access_control::{DataScope, DataScopeData, DataScopeSubjectType, DataScopeType};
use entities::common::time::Instant;
use entities::document_registry::business_document::ApprovalDefinitionBinding;
use entities::document_registry::{BusinessDocument, BusinessDocumentData, BusinessDocumentId, DocumentType};
use entities::ids::{
    DataScopeId, SkuId, StockAdjustmentId, StockAdjustmentLineId, StockBalanceId, WarehouseId,
};
use entities::inventory::{
    AdjustmentReasonType, MovementDirection, StockAdjustment, StockAdjustmentData, StockAdjustmentLine,
    StockAdjustmentLineData, StockBalance, StockBalanceData,
};
use entities::money::Quantity;
use entities::warehouse::warehouse_entity::WarehouseData;
use entities::warehouse::{EnableStatus, Warehouse};
use entities::{
    AccountCore, AccountCoreData, AccountCoreUpdate, AccountKind, AccountStatus, LoginAccount, Permission,
    Role, RoleData, RoleUpdate, Secret,
};
use mongodb::bson::{doc, Document};
use mongodb::options::ClientOptions;
use mongodb::{Client, Database};
use services::audit::AuditActor;
use services::iam::{self, subject};
use services::inventory::{
    CreateStockAdjustmentRequest, InventoryService, StockAdjustmentLineInput, StockAdjustmentLineUpdateInput,
    StockAdjustmentListParams, StockBalanceListParams, UpdateStockAdjustmentRequest,
};
use services::Error;
use test_support::{require_mongo, TestDb};

const COMPANY: &str = "company-user";
const SINGLE: &str = "single-user";
const EMPTY: &str = "empty-user";
const WRONG: &str = "wrong-user";
const DETAIL_ONLY: &str = "detail-user";
const CREATE_ONLY: &str = "create-user";
const DISABLED: &str = "disabled-user";
const SPLIT: &str = "split-user";
const LIST_SPLIT: &str = "list-split";
const BALANCE_SCOPED: &str = "balance-scoped";
const BALANCE_ONLY: &str = "balance-only";
const BALANCE_INTERSECTION: &str = "balance-intersect";
const BALANCE_MULTI: &str = "balance-multi";
const BALANCE_SEPARATE: &str = "balance-separate";
const ADJUSTMENT_INTERSECTION: &str = "adjustment-intersect";
const APPROVER: &str = "approver";

const ROLE_COMPANY: &str = "adj-company";
const ROLE_SINGLE: &str = "adj-single";
const ROLE_EMPTY: &str = "adj-empty";
const ROLE_WRONG: &str = "adj-wrong";
const ROLE_DETAIL: &str = "adj-detail";
const ROLE_CREATE: &str = "adj-create";
const ROLE_DISABLED: &str = "adj-disabled";
const ROLE_SPLIT_PERMISSION: &str = "adj-split-d";
const ROLE_SPLIT_SCOPE: &str = "adj-split-w";
const ROLE_LIST_DISABLED: &str = "adj-list-off";
const ROLE_DETAIL_ENABLED: &str = "adj-detail-on";
const ROLE_BALANCE_COMPANY: &str = "bal-company";
const ROLE_BALANCE_A: &str = "bal-a";
const ROLE_BALANCE_B: &str = "bal-b";
const ROLE_BALANCE_INTERSECTION: &str = "bal-intersect";
const ROLE_BALANCE_LIST_A: &str = "bal-list-a";
const ROLE_BALANCE_DETAIL_B: &str = "bal-detail-b";
const ROLE_ADJUSTMENT_A: &str = "adj-a";
const ROLE_ADJUSTMENT_B: &str = "adj-b";
const ROLE_ADJUSTMENT_INTERSECTION: &str = "adj-intersect";
const ROLE_APPROVER: &str = "adj-approver";

const WAREHOUSE_A: &str = "warehouse-a";
const WAREHOUSE_B: &str = "warehouse-b";
const WAREHOUSE_C: &str = "warehouse-c";
const SKU_A: &str = "sku-shared";
const SKU_A_SECOND: &str = "sku-z";
const SKU_B: &str = "sku-shared";
const DEFINITION_ID: &str = "definition-stock-adjustment-auth-v1";
const NODE_KEY: &str = "authorization-review";
const CREATE_ACTION: &str = "CREATE_ADJUSTMENT";

/// 真实库内两仓业务事实的稳定主键。
struct FixtureSeed {
    adjustment_a1: String,
    adjustment_a2: String,
    adjustment_b1: String,
    adjustment_b2: String,
    balance_a: StockBalance,
    balance_a_second: StockBalance,
    balance_b: StockBalance,
}

/// 创建失败前后必须完全一致的持久化集合快照。
#[derive(Debug, Clone, PartialEq)]
struct WriteFacts {
    adjustments: Vec<Document>,
    lines: Vec<Document>,
    balances: Vec<Document>,
    documents: Vec<Document>,
    audits: Vec<Document>,
}

/// 构造固定秒时间戳。
fn at(seconds: i64) -> Timestamp {
    Timestamp::from_unix_secs(seconds).expect("测试时间戳必须合法")
}

/// 构造审批参与人。
fn participant(id: &str) -> ParticipantId {
    ParticipantId::new(id).expect("测试参与人必须合法")
}

/// 构造与持久化账号一致的审计身份。
fn actor(id: &str) -> AuditActor {
    AuditActor::new(id.to_string(), format!("login-{id}"), AccountKind::Admin)
}

/// 构造当前启用的后台账号。
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

/// 构造角色 DataScope。
fn scope(id: &str, role_id: &str, scope_type: DataScopeType, targets: &[&str]) -> DataScope {
    DataScope::new(
        DataScopeId::new(id),
        DataScopeData {
            subject_type: DataScopeSubjectType::Role,
            subject_id: role_id.to_string(),
            scope_type,
            scope_targets: targets.iter().map(|target| (*target).to_string()).collect(),
        },
    )
    .expect("DataScope fixture")
}

/// 构造与角色范围取交集的用户级 Warehouse DataScope。
fn user_scope(id: &str, user_id: &str, targets: &[&str]) -> DataScope {
    DataScope::new(
        DataScopeId::new(id),
        DataScopeData {
            subject_type: DataScopeSubjectType::User,
            subject_id: user_id.to_string(),
            scope_type: DataScopeType::Organization,
            scope_targets: targets.iter().map(|target| (*target).to_string()).collect(),
        },
    )
    .expect("用户 DataScope fixture")
}

/// 向测试策略集合追加一条角色权限。
fn grant(policies: &mut Vec<Vec<String>>, role_id: &str, resource: &str, action: &str) {
    policies.push(vec![
        format!("role:{role_id}"),
        resource.to_string(),
        action.to_string(),
    ]);
}

/// 写入账号、启用/停用角色、真实 Casbin 残留策略和逐角色 DataScope。
async fn seed_authorization(db: &Database) {
    for id in [
        COMPANY,
        SINGLE,
        EMPTY,
        WRONG,
        DETAIL_ONLY,
        CREATE_ONLY,
        DISABLED,
        SPLIT,
        LIST_SPLIT,
        BALANCE_SCOPED,
        BALANCE_ONLY,
        BALANCE_INTERSECTION,
        BALANCE_MULTI,
        BALANCE_SEPARATE,
        ADJUSTMENT_INTERSECTION,
        APPROVER,
    ] {
        db.accounts()
            .create(&account(id), &mut NoTransaction)
            .await
            .expect("写入授权矩阵账号");
    }

    for role_id in [
        ROLE_COMPANY,
        ROLE_SINGLE,
        ROLE_EMPTY,
        ROLE_WRONG,
        ROLE_DETAIL,
        ROLE_CREATE,
        ROLE_DISABLED,
        ROLE_SPLIT_PERMISSION,
        ROLE_SPLIT_SCOPE,
        ROLE_LIST_DISABLED,
        ROLE_DETAIL_ENABLED,
        ROLE_BALANCE_COMPANY,
        ROLE_BALANCE_A,
        ROLE_BALANCE_B,
        ROLE_BALANCE_INTERSECTION,
        ROLE_BALANCE_LIST_A,
        ROLE_BALANCE_DETAIL_B,
        ROLE_ADJUSTMENT_A,
        ROLE_ADJUSTMENT_B,
        ROLE_ADJUSTMENT_INTERSECTION,
        ROLE_APPROVER,
    ] {
        let mut role = Role::new(
            role_id.to_string(),
            RoleData {
                name: role_id.to_string(),
                description: None,
                system: false,
            },
        )
        .expect("授权角色 fixture");
        if matches!(role_id, ROLE_DISABLED | ROLE_LIST_DISABLED) {
            role.update(RoleUpdate {
                disabled: Some(true),
                ..RoleUpdate::default()
            })
            .expect("停用残留授权角色");
        }
        db.roles()
            .create(&role, &mut NoTransaction)
            .await
            .expect("写入授权角色");
    }

    let bindings = [
        (COMPANY, ROLE_COMPANY),
        (SINGLE, ROLE_SINGLE),
        (EMPTY, ROLE_EMPTY),
        (WRONG, ROLE_WRONG),
        (DETAIL_ONLY, ROLE_DETAIL),
        (CREATE_ONLY, ROLE_CREATE),
        (DISABLED, ROLE_DISABLED),
        (SPLIT, ROLE_SPLIT_PERMISSION),
        (SPLIT, ROLE_SPLIT_SCOPE),
        (LIST_SPLIT, ROLE_LIST_DISABLED),
        (LIST_SPLIT, ROLE_DETAIL_ENABLED),
        (BALANCE_SCOPED, ROLE_BALANCE_COMPANY),
        (BALANCE_SCOPED, ROLE_ADJUSTMENT_A),
        (BALANCE_ONLY, ROLE_BALANCE_A),
        (BALANCE_ONLY, ROLE_ADJUSTMENT_B),
        (BALANCE_INTERSECTION, ROLE_BALANCE_INTERSECTION),
        (BALANCE_MULTI, ROLE_BALANCE_A),
        (BALANCE_MULTI, ROLE_BALANCE_B),
        (BALANCE_SEPARATE, ROLE_BALANCE_LIST_A),
        (BALANCE_SEPARATE, ROLE_BALANCE_DETAIL_B),
        (ADJUSTMENT_INTERSECTION, ROLE_ADJUSTMENT_INTERSECTION),
        (APPROVER, ROLE_APPROVER),
    ];
    let mut adapter = MongoCasbinAdapter::new(db.clone());
    assert!(adapter
        .add_policies(
            "g",
            "g",
            bindings
                .iter()
                .map(|(user_id, role_id)| {
                    vec![subject(AccountKind::Admin, user_id), format!("role:{role_id}")]
                })
                .collect(),
        )
        .await
        .expect("写入授权矩阵角色绑定"));

    let mut policies = Vec::new();
    for role_id in [
        ROLE_COMPANY,
        ROLE_SINGLE,
        ROLE_EMPTY,
        ROLE_WRONG,
        ROLE_DISABLED,
        ROLE_ADJUSTMENT_A,
        ROLE_ADJUSTMENT_B,
        ROLE_ADJUSTMENT_INTERSECTION,
    ] {
        for action in ["list", "detail", "create", "update"] {
            grant(&mut policies, role_id, "stock_adjustment", action);
        }
    }
    grant(&mut policies, ROLE_DETAIL, "stock_adjustment", "detail");
    for action in ["list", "create", "update"] {
        grant(&mut policies, ROLE_CREATE, "stock_adjustment", action);
    }
    grant(&mut policies, ROLE_SPLIT_PERMISSION, "stock_adjustment", "detail");
    grant(&mut policies, ROLE_SPLIT_PERMISSION, "stock_balance", "detail");
    for action in ["list", "create", "update"] {
        grant(&mut policies, ROLE_SPLIT_SCOPE, "stock_adjustment", action);
    }
    grant(&mut policies, ROLE_LIST_DISABLED, "stock_adjustment", "list");
    grant(&mut policies, ROLE_DETAIL_ENABLED, "stock_adjustment", "detail");
    for role_id in [
        ROLE_COMPANY,
        ROLE_SINGLE,
        ROLE_EMPTY,
        ROLE_WRONG,
        ROLE_DETAIL,
        ROLE_CREATE,
        ROLE_DISABLED,
        ROLE_BALANCE_COMPANY,
        ROLE_BALANCE_A,
        ROLE_BALANCE_B,
        ROLE_BALANCE_INTERSECTION,
    ] {
        for action in ["list", "detail"] {
            grant(&mut policies, role_id, "stock_balance", action);
        }
    }
    grant(&mut policies, ROLE_BALANCE_LIST_A, "stock_balance", "list");
    grant(&mut policies, ROLE_BALANCE_DETAIL_B, "stock_balance", "detail");
    grant(&mut policies, ROLE_APPROVER, "approval_instance", "decide");
    grant(&mut policies, ROLE_APPROVER, "stock_adjustment", "detail");
    assert!(adapter
        .add_policies("p", "p", policies)
        .await
        .expect("写入库存调整与余额权限"));

    for data_scope in [
        scope("scope-adj-company", ROLE_COMPANY, DataScopeType::Company, &[]),
        scope(
            "scope-adj-single",
            ROLE_SINGLE,
            DataScopeType::Organization,
            &[WAREHOUSE_A],
        ),
        scope(
            "scope-adj-wrong",
            ROLE_WRONG,
            DataScopeType::Organization,
            &[WAREHOUSE_C],
        ),
        scope(
            "scope-adj-detail",
            ROLE_DETAIL,
            DataScopeType::Organization,
            &[WAREHOUSE_A],
        ),
        scope(
            "scope-adj-create",
            ROLE_CREATE,
            DataScopeType::Organization,
            &[WAREHOUSE_A],
        ),
        scope("scope-adj-disabled", ROLE_DISABLED, DataScopeType::Company, &[]),
        scope(
            "scope-adj-split-detail",
            ROLE_SPLIT_PERMISSION,
            DataScopeType::Organization,
            &[WAREHOUSE_A],
        ),
        scope(
            "scope-adj-split-write",
            ROLE_SPLIT_SCOPE,
            DataScopeType::Organization,
            &[WAREHOUSE_A],
        ),
        scope(
            "scope-adj-list-disabled",
            ROLE_LIST_DISABLED,
            DataScopeType::Organization,
            &[WAREHOUSE_A],
        ),
        scope(
            "scope-adj-detail-enabled",
            ROLE_DETAIL_ENABLED,
            DataScopeType::Organization,
            &[WAREHOUSE_A],
        ),
        scope(
            "scope-bal-company",
            ROLE_BALANCE_COMPANY,
            DataScopeType::Company,
            &[],
        ),
        scope(
            "scope-bal-a",
            ROLE_BALANCE_A,
            DataScopeType::Organization,
            &[WAREHOUSE_A],
        ),
        scope(
            "scope-bal-b",
            ROLE_BALANCE_B,
            DataScopeType::Organization,
            &[WAREHOUSE_B],
        ),
        scope(
            "scope-bal-intersect",
            ROLE_BALANCE_INTERSECTION,
            DataScopeType::Company,
            &[],
        ),
        user_scope("scope-user-intersect", BALANCE_INTERSECTION, &[WAREHOUSE_A]),
        scope(
            "scope-bal-list-a",
            ROLE_BALANCE_LIST_A,
            DataScopeType::Organization,
            &[WAREHOUSE_A],
        ),
        scope(
            "scope-bal-detail-b",
            ROLE_BALANCE_DETAIL_B,
            DataScopeType::Organization,
            &[WAREHOUSE_B],
        ),
        scope(
            "scope-adj-a",
            ROLE_ADJUSTMENT_A,
            DataScopeType::Organization,
            &[WAREHOUSE_A],
        ),
        scope(
            "scope-adj-b",
            ROLE_ADJUSTMENT_B,
            DataScopeType::Organization,
            &[WAREHOUSE_B],
        ),
        scope(
            "scope-adj-intersect",
            ROLE_ADJUSTMENT_INTERSECTION,
            DataScopeType::Company,
            &[],
        ),
        user_scope(
            "scope-adj-user-intersect",
            ADJUSTMENT_INTERSECTION,
            &[WAREHOUSE_A],
        ),
        scope("scope-adj-approver", ROLE_APPROVER, DataScopeType::Company, &[]),
    ] {
        db.data_scopes()
            .create(&data_scope, &mut NoTransaction)
            .await
            .expect("写入逐角色 Warehouse DataScope");
    }
}

/// 写入单节点已发布库存调整审批定义，保证创建成功路径只受本批授权规则影响。
async fn seed_definition(db: &Database) {
    let mut definition = ApprovalProcessDefinition::new_draft(
        ApprovalProcessDefinitionId::new(DEFINITION_ID),
        ProcessKind::StockAdjustment,
        1,
        "库存调整授权验收",
        NODE_KEY,
        participant("definition-admin"),
        at(1),
    )
    .expect("审批定义 fixture");
    definition
        .publish(participant("definition-admin"), at(2))
        .expect("发布审批定义");
    db.approval_process_definitions()
        .create(&definition, &mut NoTransaction)
        .await
        .expect("写入已发布定义");

    let node = ApprovalNodeDefinition::new(NewNodeDefinition {
        id: ApprovalNodeDefinitionId::new("node-stock-adjustment-auth"),
        process_definition_id: ApprovalProcessDefinitionId::new(DEFINITION_ID),
        node_key: NODE_KEY.to_string(),
        node_name: "库存调整授权复核".to_string(),
        node_purpose: None,
        display_order: 1,
        assignee_participant_id: participant(APPROVER),
        assignee_label_snapshot: "授权验收审批人".to_string(),
        at: at(1),
    })
    .expect("审批节点 fixture");
    db.approval_node_definitions()
        .create(&node, &mut NoTransaction)
        .await
        .expect("写入审批节点");

    for transition in [
        ApprovalTransitionDefinition::to_approved(
            ApprovalTransitionDefinitionId::new("transition-stock-adjustment-auth-approve"),
            ApprovalProcessDefinitionId::new(DEFINITION_ID),
            NODE_KEY,
            ApprovalTransitionEvent::Approve,
            at(1),
        )
        .expect("通过终态连线"),
        ApprovalTransitionDefinition::to_node(
            ApprovalTransitionDefinitionId::new("transition-stock-adjustment-auth-reject"),
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

/// 写入一条冻结定义绑定的草稿库存调整单。
async fn seed_adjustment(
    db: &Database,
    id: &str,
    adjustment_no: &str,
    warehouse_id: &str,
    sku_id: &str,
) -> String {
    let adjustment = StockAdjustment::new(
        StockAdjustmentId::new(id),
        StockAdjustmentData {
            adjustment_no: adjustment_no.to_string(),
            warehouse_id: WarehouseId::new(warehouse_id),
            reason_type: AdjustmentReasonType::StockGain,
            prepared_by: COMPANY.to_string(),
            note: Some("授权范围 fixture".to_string()),
            occurred_at: Some(Instant::from_unix_secs(10)),
        },
        COMPANY,
    )
    .expect("库存调整单 fixture");
    let line = StockAdjustmentLine::new_for_reason(
        StockAdjustmentLineId::new(format!("line-{id}")),
        AdjustmentReasonType::StockGain,
        StockAdjustmentLineData {
            stock_adjustment_id: StockAdjustmentId::new(id),
            sku_id: SkuId::new(sku_id),
            quantity: Quantity::from_str("1").expect("测试数量"),
            direction: MovementDirection::Increase,
        },
    )
    .expect("库存调整明细 fixture");
    db.inventory()
        .create_stock_adjustment_with_lines(&adjustment, std::slice::from_ref(&line), &mut NoTransaction)
        .await
        .expect("写入库存调整表头与明细");

    let mut document = BusinessDocument::new(
        BusinessDocumentId::new(id),
        BusinessDocumentData {
            document_type: DocumentType::StockAdjustment,
            document_no: adjustment_no.to_string(),
        },
    )
    .expect("业务单据注册 fixture");
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
    adjustment.base.id
}

/// 写入指定仓库与 SKU 的库存余额。
async fn seed_balance(db: &Database, id: &str, warehouse_id: &str, sku_id: &str) -> StockBalance {
    let balance = StockBalance::new(
        StockBalanceId::new(id),
        StockBalanceData {
            warehouse_id: WarehouseId::new(warehouse_id),
            sku_id: SkuId::new(sku_id),
            on_hand_quantity: Quantity::from_str("10").expect("账面数量"),
            reserved_quantity: Quantity::from_str("0").expect("预占数量"),
            available_quantity: Quantity::from_str("10").expect("可用数量"),
            last_movement_id: None,
        },
    )
    .expect("库存余额 fixture");
    db.stock_balances()
        .create(&balance, &mut NoTransaction)
        .await
        .expect("写入库存余额");
    balance
}

/// 写入创建命令会在事务内重验的仓库稳定身份。
async fn seed_warehouse(db: &Database, id: &str) {
    let warehouse = Warehouse::new(
        WarehouseId::new(id),
        WarehouseData {
            warehouse_code: id.to_string(),
            status: EnableStatus::Active,
            inbound_handler_user_id: None,
            outbound_handler_user_id: None,
        },
        "authorization-fixture",
    )
    .expect("仓库 fixture");
    db.warehouses()
        .create(&warehouse, &mut NoTransaction)
        .await
        .expect("写入仓库稳定身份");
}

/// 建立一套两仓、四单、三余额的真实 Mongo 授权 fixture。
async fn fixture(prefix: &str) -> (TestDb, InventoryService, FixtureSeed) {
    let fixture = TestDb::new(prefix).await.expect("测试数据库创建失败");
    ensure_indexes(fixture.db()).await.expect("索引创建失败");
    seed_authorization(fixture.db()).await;
    seed_definition(fixture.db()).await;
    seed_warehouse(fixture.db(), WAREHOUSE_A).await;
    seed_warehouse(fixture.db(), WAREHOUSE_B).await;
    let balance_a = seed_balance(fixture.db(), "balance-a", WAREHOUSE_A, SKU_A).await;
    let balance_a_second = seed_balance(fixture.db(), "balance-a2", WAREHOUSE_A, SKU_A_SECOND).await;
    let balance_b = seed_balance(fixture.db(), "balance-b", WAREHOUSE_B, SKU_B).await;
    // 相同 created_at 下按唯一 id 形成 A/B/A/B 交错；若 Repository 先分页再做
    // Warehouse scope 过滤，A 仓 page_size=1 的第二页会错误变为空页。
    let adjustment_a1 =
        seed_adjustment(fixture.db(), "adjustment-01-a", "ADJ-A-01", WAREHOUSE_A, SKU_A).await;
    let adjustment_a2 =
        seed_adjustment(fixture.db(), "adjustment-03-a", "ADJ-A-02", WAREHOUSE_A, SKU_A).await;
    let adjustment_b1 =
        seed_adjustment(fixture.db(), "adjustment-02-b", "ADJ-B-01", WAREHOUSE_B, SKU_B).await;
    let adjustment_b2 =
        seed_adjustment(fixture.db(), "adjustment-04-b", "ADJ-B-02", WAREHOUSE_B, SKU_B).await;
    let fixed_created_at = fixture
        .db()
        .collection::<Document>("stock_adjustments")
        .update_many(doc! {}, doc! { "$set": { "created_at": 10_i64 } })
        .await
        .expect("冻结相同库存调整主排序键");
    assert_eq!(fixed_created_at.modified_count, 4);
    let service = InventoryService::new(
        fixture.db().clone(),
        iam::shared_rbac_service(fixture.db().clone()),
    );
    (
        fixture,
        service,
        FixtureSeed {
            adjustment_a1,
            adjustment_a2,
            adjustment_b1,
            adjustment_b2,
            balance_a,
            balance_a_second,
            balance_b,
        },
    )
}

/// 构造库存调整分页参数。
fn adjustment_params(warehouse_id: Option<&str>, page: u64, page_size: u32) -> StockAdjustmentListParams {
    StockAdjustmentListParams {
        warehouse_id: warehouse_id.map(WarehouseId::new),
        status: None,
        page: Some(page),
        page_size: Some(page_size),
        sort_by: Some("created_at".to_string()),
        sort_dir: Some("asc".to_string()),
    }
}

/// 构造余额分页参数。
fn balance_params(warehouse_id: Option<&str>, page: u64, page_size: u32) -> StockBalanceListParams {
    StockBalanceListParams {
        warehouse_id: warehouse_id.map(WarehouseId::new),
        sku_id: None,
        page: Some(page),
        page_size: Some(page_size),
        sort_by: Some("sku_id".to_string()),
        sort_dir: Some("asc".to_string()),
    }
}

/// 构造一条合法创建命令。
fn create_request(
    adjustment_no: &str,
    warehouse_id: &str,
    sku_id: &str,
    balance: &StockBalance,
) -> CreateStockAdjustmentRequest {
    CreateStockAdjustmentRequest {
        balance_id: balance.base.id.clone(),
        expected_balance_version: balance.base.version,
        adjustment_no: adjustment_no.to_string(),
        warehouse_id: WarehouseId::new(warehouse_id),
        reason_type: AdjustmentReasonType::StockGain,
        lines: vec![StockAdjustmentLineInput {
            sku_id: SkuId::new(sku_id),
            quantity: Quantity::from_str("1").expect("创建数量"),
            direction: MovementDirection::Increase,
        }],
        note: Some("FUL-S01 授权验收".to_string()),
        occurred_at: Some(20),
    }
}

/// 构造一条同时更新表头与明细的库存调整命令。
fn update_request(adjustment_id: &str, expected_version: u64, note: &str) -> UpdateStockAdjustmentRequest {
    UpdateStockAdjustmentRequest {
        version: expected_version,
        reason_type: None,
        lines: Some(vec![StockAdjustmentLineUpdateInput {
            line_id: format!("line-{adjustment_id}"),
            quantity: "3".to_string(),
            direction: Some(MovementDirection::Increase),
        }]),
        note: Some(note.to_string()),
        occurred_at: Some(30),
    }
}

/// 把既有账号状态改为暂停，模拟命令到达前身份漂移。
async fn suspend_account(db: &Database, account_id: &str) {
    let mut row = db
        .accounts()
        .find_by_id(account_id, &mut NoTransaction)
        .await
        .expect("读取待暂停账号")
        .expect("待暂停账号必须存在");
    row.update(AccountCoreUpdate {
        status: Some(AccountStatus::Suspended),
        ..AccountCoreUpdate::default()
    })
    .expect("暂停账号 fixture");
    db.accounts()
        .update(&mut row, &mut NoTransaction)
        .await
        .expect("持久化账号暂停状态");
}

/// 为单条策略漂移命令创建带独立 application name 的数据库句柄。
async fn app_named_database(fixture: &TestDb, app_name: &str) -> Database {
    let uri = std::env::var("ERP_TEST_MONGO_URI").expect("真实 Mongo 测试连接串");
    let mut options = ClientOptions::parse(uri).await.expect("解析 Mongo 测试连接串");
    options.app_name = Some(app_name.to_string());
    Client::with_options(options)
        .expect("创建带 appName 的 Mongo client")
        .database(fixture.name())
}

/// 读取 failCommand 的累计进入次数，只记录计数而不输出业务载荷。
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

/// 精确挂起指定 appName 对策略版本集合的下一条 find。
async fn arm_policy_revision_find(db: &Database, app_name: &str) -> i64 {
    let before = fail_command_entries(db).await;
    db.client()
        .database("admin")
        .run_command(doc! {
            "configureFailPoint": "failCommand",
            "mode": { "times": 1_i32 },
            "data": {
                "failCommands": ["find"],
                "appName": app_name,
                "namespace": format!("{}.casbin_policy_state", db.name()),
                "blockConnection": true,
                "blockTimeMS": 10_000_i32,
            },
        })
        .await
        .expect("挂起策略版本 find");
    before
}

/// 由 MongoDB 原生 waitForFailPoint 证明目标策略版本读已被挂起。
async fn wait_for_policy_revision_find(db: &Database, before: i64) -> bool {
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

/// 关闭本测试独占 Mongo 实例上的 failCommand。
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

/// 用独立 adapter 提交 R+1，供事务快照 R 与进程授权快照 R+1 对照。
async fn advance_policy_revision(db: &Database, marker: &str) -> std::result::Result<(u64, u64), String> {
    let mut adapter = MongoCasbinAdapter::new(db.clone());
    let before = adapter
        .policy_revision(&mut NoTransaction)
        .await
        .map_err(|error| format!("读取策略 revision R 失败: {error}"))?;
    let inserted = adapter
        .add_policy(
            "p",
            "p",
            vec![
                format!("role:{ROLE_COMPANY}"),
                marker.to_string(),
                "read".to_string(),
            ],
        )
        .await
        .map_err(|error| format!("提交策略 revision R+1 失败: {error}"))?;
    if !inserted {
        return Err("策略 revision probe 已存在".to_string());
    }
    let after = adapter
        .policy_revision(&mut NoTransaction)
        .await
        .map_err(|error| format!("读取策略 revision R+1 失败: {error}"))?;
    Ok((before, after))
}

/// 按主键排序回读原始 BSON 集合，证明失败没有部分写或已有事实改写。
async fn raw_documents(db: &Database, collection: &str) -> Vec<Document> {
    let mut cursor = db
        .collection::<Document>(collection)
        .find(doc! {})
        .sort(doc! { "id": 1 })
        .await
        .expect("回读授权写事实");
    let mut rows = Vec::new();
    while cursor.advance().await.expect("推进授权写事实游标") {
        rows.push(cursor.deserialize_current().expect("反序列化授权写事实"));
    }
    rows
}

/// 回读创建可能影响的全部业务与审计集合。
async fn write_facts(db: &Database) -> WriteFacts {
    WriteFacts {
        adjustments: raw_documents(db, "stock_adjustments").await,
        lines: raw_documents(db, "stock_adjustment_lines").await,
        balances: raw_documents(db, "stock_balances").await,
        documents: raw_documents(db, "business_documents").await,
        audits: raw_documents(db, "audit_logs").await,
    }
}

/// 把列表主键收敛为稳定集合。
fn adjustment_ids(items: &[services::inventory::StockAdjustmentView]) -> HashSet<&str> {
    items.iter().map(|item| item.id.as_str()).collect()
}

/// 把余额列表主键收敛为稳定集合。
fn balance_ids(items: &[services::inventory::StockBalanceView]) -> HashSet<&str> {
    items.iter().map(|item| item.id.as_str()).collect()
}

/// 断言对象详情越权与猜测不存在对象使用同一隐藏语义。
fn hidden_message(error: Error) -> String {
    match error {
        Error::NotFound(message) => message,
        other => panic!("对象详情越权必须隐藏为 NotFound，实际为 {other:?}"),
    }
}

/// 断言当前余额行是否只获得唯一受支持的创建动作。
fn assert_create_action(actions: &[String], expected: bool, label: &str) {
    if expected {
        assert_eq!(actions, [CREATE_ACTION.to_string()], "{label}");
    } else {
        assert!(actions.is_empty(), "{label}: {actions:?}");
    }
}

/// 显式删除随机测试库，避免进程退出时异步 Drop 尚未收敛。
async fn cleanup(fixture: TestDb) {
    fixture.db().drop().await.expect("清理随机测试数据库");
    // 数据库已同步删除；避免 `TestDb::Drop` 在独立线程 runtime 析构 Mongo
    // client 时触发 driver connection requester 竞态噪声。
    std::mem::forget(fixture);
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn adjustment_list_filters_scope_before_page_and_total() {
    require_mongo!(async {
        let (fixture, service, seed) = fixture("stock_adj_auth_list").await;

        let company = service
            .stock_adjustment_list(&adjustment_params(None, 1, 100), &actor(COMPANY))
            .await
            .expect("Company 范围必须读取全部库存调整");
        assert_eq!(company.total, 4);
        assert_eq!(
            adjustment_ids(&company.items),
            HashSet::from([
                seed.adjustment_a1.as_str(),
                seed.adjustment_a2.as_str(),
                seed.adjustment_b1.as_str(),
                seed.adjustment_b2.as_str(),
            ])
        );
        let company_page_1 = service
            .stock_adjustment_list(&adjustment_params(None, 1, 2), &actor(COMPANY))
            .await
            .expect("相同 created_at 的 Company 第一页");
        let company_page_2 = service
            .stock_adjustment_list(&adjustment_params(None, 2, 2), &actor(COMPANY))
            .await
            .expect("相同 created_at 的 Company 第二页");
        assert_eq!(company_page_1.total, 4);
        assert_eq!(company_page_2.total, 4);
        let first_ids = adjustment_ids(&company_page_1.items);
        let second_ids = adjustment_ids(&company_page_2.items);
        assert!(first_ids.is_disjoint(&second_ids));
        assert_eq!(
            first_ids.union(&second_ids).copied().collect::<HashSet<_>>(),
            HashSet::from([
                seed.adjustment_a1.as_str(),
                seed.adjustment_a2.as_str(),
                seed.adjustment_b1.as_str(),
                seed.adjustment_b2.as_str(),
            ]),
            "相同 created_at 必须用稳定 id 次排序避免跨页重复或遗漏"
        );

        let single_page_1 = service
            .stock_adjustment_list(&adjustment_params(None, 1, 1), &actor(SINGLE))
            .await
            .expect("单仓范围第一页");
        let single_page_2 = service
            .stock_adjustment_list(&adjustment_params(None, 2, 1), &actor(SINGLE))
            .await
            .expect("单仓范围第二页");
        let single_page_3 = service
            .stock_adjustment_list(&adjustment_params(None, 3, 1), &actor(SINGLE))
            .await
            .expect("单仓范围越界页");
        assert_eq!(single_page_1.total, 2);
        assert_eq!(single_page_2.total, 2);
        assert_eq!(single_page_3.total, 2);
        assert_eq!(single_page_1.items[0].id, seed.adjustment_a1);
        assert_eq!(single_page_2.items[0].id, seed.adjustment_a2);
        assert!(single_page_3.items.is_empty());
        assert_eq!(single_page_1.page, 1);
        assert_eq!(single_page_2.page, 2);
        assert_eq!(single_page_1.page_size, 1);

        let explicit_wrong_warehouse = service
            .stock_adjustment_list(&adjustment_params(Some(WAREHOUSE_B), 1, 100), &actor(SINGLE))
            .await
            .expect("请求仓库与授权范围取空交集应返回空页");
        assert!(explicit_wrong_warehouse.items.is_empty());
        assert_eq!(explicit_wrong_warehouse.total, 0);

        let detail_only = service
            .stock_adjustment_list(&adjustment_params(None, 1, 100), &actor(DETAIL_ONLY))
            .await
            .expect("只有 detail、缺 list 时列表必须为空");
        assert_eq!(detail_only.total, 0);
        assert!(detail_only.items.is_empty());

        let user_intersection = service
            .stock_adjustment_list(&adjustment_params(None, 1, 100), &actor(ADJUSTMENT_INTERSECTION))
            .await
            .expect("调整单列表必须把 Company 角色范围与用户 A 仓范围取交集");
        assert_eq!(user_intersection.total, 2);
        assert_eq!(
            adjustment_ids(&user_intersection.items),
            HashSet::from([seed.adjustment_a1.as_str(), seed.adjustment_a2.as_str()])
        );

        let rbac = iam::shared_rbac_service(fixture.db().clone());
        for actor_id in [SPLIT, LIST_SPLIT] {
            for permission in ["stock_adjustment:list", "stock_adjustment:detail"] {
                assert!(
                    rbac.enforce(
                        &subject(AccountKind::Admin, actor_id),
                        &Permission::parse(permission).expect("列表联合权限"),
                    )
                    .await
                    .expect("读取拆分角色 Casbin 前提"),
                    "{actor_id} actor 级 {permission} 必须为 true"
                );
            }
        }
        for denied_actor in [
            EMPTY,
            WRONG,
            DETAIL_ONLY,
            CREATE_ONLY,
            DISABLED,
            SPLIT,
            LIST_SPLIT,
        ] {
            let denied = service
                .stock_adjustment_list(&adjustment_params(None, 1, 100), &actor(denied_actor))
                .await
                .expect("无同一启用角色 list+detail+scope 时列表必须为空");
            assert!(denied.items.is_empty(), "{denied_actor} 不得读取列表");
            assert_eq!(denied.total, 0, "{denied_actor} 不得泄露 total");
        }

        cleanup(fixture).await;
    });
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn adjustment_detail_hides_wrong_scope_missing_permission_and_residual_roles() {
    require_mongo!(async {
        let (fixture, service, seed) = fixture("stock_adj_auth_detail").await;
        let before = write_facts(fixture.db()).await;

        assert_eq!(
            service
                .stock_adjustment_detail(&seed.adjustment_b1, &actor(COMPANY))
                .await
                .expect("Company 范围可读任意仓详情")
                .adjustment
                .id,
            seed.adjustment_b1
        );
        assert_eq!(
            service
                .stock_adjustment_detail(&seed.adjustment_a1, &actor(SINGLE))
                .await
                .expect("单仓范围可读本仓详情")
                .adjustment
                .id,
            seed.adjustment_a1
        );
        assert_eq!(
            service
                .stock_adjustment_detail(&seed.adjustment_a1, &actor(DETAIL_ONLY))
                .await
                .expect("detail-only 仍可读命中范围详情")
                .adjustment
                .id,
            seed.adjustment_a1
        );
        for read_actor in [SPLIT, LIST_SPLIT, ADJUSTMENT_INTERSECTION] {
            assert_eq!(
                service
                    .stock_adjustment_detail(&seed.adjustment_a1, &actor(read_actor))
                    .await
                    .expect("启用 detail 角色命中 A 仓时必须可读 exact detail")
                    .adjustment
                    .id,
                seed.adjustment_a1,
                "{read_actor}"
            );
        }

        let hidden = hidden_message(
            service
                .stock_adjustment_detail(&seed.adjustment_b1, &actor(SINGLE))
                .await
                .expect_err("单仓主体不得读取另一仓详情"),
        );
        let guessed = hidden_message(
            service
                .stock_adjustment_detail("adjustment-guessed", &actor(SINGLE))
                .await
                .expect_err("猜测对象 ID 必须隐藏"),
        );
        assert_eq!(hidden, guessed, "越权与不存在不得泄露不同语义");

        let intersection_hidden = hidden_message(
            service
                .stock_adjustment_detail(&seed.adjustment_b1, &actor(ADJUSTMENT_INTERSECTION))
                .await
                .expect_err("用户范围与 Company 角色范围交集不得越过 A 仓"),
        );
        assert_eq!(intersection_hidden, guessed);

        for denied_actor in [EMPTY, WRONG, CREATE_ONLY, DISABLED] {
            let denied = hidden_message(
                service
                    .stock_adjustment_detail(&seed.adjustment_a1, &actor(denied_actor))
                    .await
                    .expect_err("空范围、错误范围、缺 detail 或残留角色必须隐藏详情"),
            );
            assert_eq!(
                denied, guessed,
                "{denied_actor} 的详情拒绝必须与 guessed missing 使用同一 NotFound 文案"
            );
        }
        assert_eq!(
            write_facts(fixture.db()).await,
            before,
            "全部详情授权分支必须只读且不得产生审计或业务写入"
        );

        cleanup(fixture).await;
    });
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn company_and_single_role_can_create_only_in_covered_warehouse() {
    require_mongo!(async {
        let (fixture, service, seed) = fixture("stock_adj_auth_create_ok").await;
        let before = write_facts(fixture.db()).await;

        let company_created = service
            .create_stock_adjustment(
                create_request("ADJ-AUTH-COMPANY", WAREHOUSE_B, SKU_B, &seed.balance_b),
                &actor(COMPANY),
            )
            .await
            .expect("Company 同角色 detail+create+scope 必须可创建");
        assert_eq!(company_created.adjustment.warehouse_id, WAREHOUSE_B);

        let single_created = service
            .create_stock_adjustment(
                create_request("ADJ-AUTH-SINGLE", WAREHOUSE_A, SKU_A, &seed.balance_a),
                &actor(SINGLE),
            )
            .await
            .expect("单仓同角色 detail+create+scope 必须可在本仓创建");
        assert_eq!(single_created.adjustment.warehouse_id, WAREHOUSE_A);

        let intersected_created = service
            .create_stock_adjustment(
                create_request("ADJ-AUTH-INTERSECT", WAREHOUSE_A, SKU_A, &seed.balance_a),
                &actor(ADJUSTMENT_INTERSECTION),
            )
            .await
            .expect("用户 A 仓范围与 Company 角色范围交集内必须可创建");
        assert_eq!(intersected_created.adjustment.warehouse_id, WAREHOUSE_A);

        let after = write_facts(fixture.db()).await;
        assert_eq!(after.adjustments.len(), before.adjustments.len() + 3);
        assert_eq!(after.lines.len(), before.lines.len() + 3);
        assert_eq!(after.documents.len(), before.documents.len() + 3);
        assert!(after.audits.len() >= before.audits.len() + 3);
        assert_eq!(after.balances, before.balances, "创建不得改写库存余额");

        cleanup(fixture).await;
    });
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn create_denials_and_balance_dimension_mismatch_are_zero_write() {
    require_mongo!(async {
        let (fixture, service, seed) = fixture("stock_adj_auth_create_deny").await;
        let rbac = iam::shared_rbac_service(fixture.db().clone());
        for actor_id in [DISABLED, SPLIT] {
            for permission in [
                Permission::parse("stock_adjustment:detail").expect("detail 权限"),
                Permission::parse("stock_adjustment:create").expect("create 权限"),
                Permission::parse("stock_adjustment:update").expect("update 权限"),
            ] {
                assert!(
                    rbac.enforce(&subject(AccountKind::Admin, actor_id), &permission)
                        .await
                        .expect("读取残留 Casbin actor 级授权"),
                    "{actor_id} 的原始 Casbin 前提必须仍为 true"
                );
            }
        }

        let baseline = write_facts(fixture.db()).await;
        let denied = [
            (
                "out-of-scope",
                SINGLE,
                create_request("ADJ-DENY-SCOPE", WAREHOUSE_B, SKU_B, &seed.balance_b),
            ),
            (
                "empty-scope",
                EMPTY,
                create_request("ADJ-DENY-EMPTY", WAREHOUSE_A, SKU_A, &seed.balance_a),
            ),
            (
                "wrong-scope",
                WRONG,
                create_request("ADJ-DENY-WRONG", WAREHOUSE_A, SKU_A, &seed.balance_a),
            ),
            (
                "missing-create",
                DETAIL_ONLY,
                create_request("ADJ-DENY-CREATE", WAREHOUSE_A, SKU_A, &seed.balance_a),
            ),
            (
                "missing-detail",
                CREATE_ONLY,
                create_request("ADJ-DENY-DETAIL", WAREHOUSE_A, SKU_A, &seed.balance_a),
            ),
            (
                "disabled-residual",
                DISABLED,
                create_request("ADJ-DENY-DISABLED", WAREHOUSE_A, SKU_A, &seed.balance_a),
            ),
            (
                "split-role",
                SPLIT,
                create_request("ADJ-DENY-SPLIT", WAREHOUSE_A, SKU_A, &seed.balance_a),
            ),
            (
                "list-detail-without-create",
                LIST_SPLIT,
                create_request("ADJ-DENY-LIST-SPLIT", WAREHOUSE_A, SKU_A, &seed.balance_a),
            ),
            (
                "user-role-intersection",
                ADJUSTMENT_INTERSECTION,
                create_request("ADJ-DENY-USER-SCOPE", WAREHOUSE_B, SKU_B, &seed.balance_b),
            ),
        ];
        for (label, actor_id, request) in denied {
            let error = service
                .create_stock_adjustment(request, &actor(actor_id))
                .await
                .expect_err("创建授权失败必须关闭");
            assert!(
                matches!(error, Error::Forbidden(_)),
                "{label} 必须返回 Forbidden，实际为 {error:?}"
            );
            assert_eq!(
                write_facts(fixture.db()).await,
                baseline,
                "{label} 必须在调整单、明细、注册、审计或余额首笔写入前失败"
            );
        }

        let dimension_error = service
            .create_stock_adjustment(
                create_request("ADJ-DENY-DIMENSION", WAREHOUSE_A, SKU_A, &seed.balance_a_second),
                &actor(COMPANY),
            )
            .await
            .expect_err("同仓不同 SKU 的余额与请求维度不一致必须失败");
        assert!(matches!(dimension_error, Error::ValidationError(_)));
        assert_eq!(
            write_facts(fixture.db()).await,
            baseline,
            "余额维度失败必须回滚表头、明细、注册、绑定审计与创建审计"
        );

        let mut stale_balance =
            create_request("ADJ-DENY-BALANCE-VERSION", WAREHOUSE_A, SKU_A, &seed.balance_a);
        stale_balance.expected_balance_version += 1;
        let stale_error = service
            .create_stock_adjustment(stale_balance, &actor(COMPANY))
            .await
            .expect_err("库存余额版本过期必须失败");
        assert!(matches!(stale_error, Error::ConflictError(_)));
        assert_eq!(
            write_facts(fixture.db()).await,
            baseline,
            "余额版本过期必须在全部业务与审计首写前失败"
        );

        let mut missing_balance =
            create_request("ADJ-DENY-MISSING-BALANCE", WAREHOUSE_A, SKU_A, &seed.balance_a);
        missing_balance.balance_id = StockBalanceId::new("balance-guessed").to_string();
        let missing_balance_error = service
            .create_stock_adjustment(missing_balance, &actor(COMPANY))
            .await
            .expect_err("不存在的余额必须隐藏为 NotFound");
        hidden_message(missing_balance_error);
        assert_eq!(
            write_facts(fixture.db()).await,
            baseline,
            "余额不存在必须在全部业务与审计首写前失败"
        );

        let missing_warehouse_error = service
            .create_stock_adjustment(
                create_request("ADJ-DENY-MISSING-WAREHOUSE", WAREHOUSE_C, SKU_A, &seed.balance_a),
                &actor(COMPANY),
            )
            .await
            .expect_err("Company 范围也不得在不存在仓库创建");
        hidden_message(missing_warehouse_error);
        assert_eq!(
            write_facts(fixture.db()).await,
            baseline,
            "仓库不存在必须在余额读取和全部业务与审计首写前失败"
        );

        suspend_account(fixture.db(), SINGLE).await;
        let inactive_error = service
            .create_stock_adjustment(
                create_request("ADJ-DENY-INACTIVE", WAREHOUSE_A, SKU_A, &seed.balance_a),
                &actor(SINGLE),
            )
            .await
            .expect_err("暂停账号不得创建库存调整单");
        assert!(matches!(inactive_error, Error::Forbidden(_)));
        assert_eq!(
            write_facts(fixture.db()).await,
            baseline,
            "账号暂停必须在全部业务与审计首写前失败"
        );

        let removed_definition = fixture
            .db()
            .collection::<Document>("approval_process_definitions")
            .delete_one(doc! { "id": DEFINITION_ID })
            .await
            .expect("删除测试发布定义以注入绑定失败");
        assert_eq!(removed_definition.deleted_count, 1);
        service
            .create_stock_adjustment(
                create_request("ADJ-DENY-BINDING", WAREHOUSE_A, SKU_A, &seed.balance_a),
                &actor(COMPANY),
            )
            .await
            .expect_err("调整单与明细写入后的定义绑定失败必须回滚");
        assert_eq!(
            write_facts(fixture.db()).await,
            baseline,
            "绑定失败必须回滚先写入的 adjustment、lines、document、audit，余额保持不变"
        );

        cleanup(fixture).await;
    });
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn company_and_single_role_update_header_lines_and_audit_atomically() {
    require_mongo!(async {
        let (fixture, service, seed) = fixture("stock_adj_auth_update_ok").await;
        let adjustment_a = fixture
            .db()
            .stock_adjustments()
            .find_by_id(&seed.adjustment_a1, &mut NoTransaction)
            .await
            .expect("读取 A 仓调整单")
            .expect("A 仓调整单必须存在");
        let adjustment_b = fixture
            .db()
            .stock_adjustments()
            .find_by_id(&seed.adjustment_b1, &mut NoTransaction)
            .await
            .expect("读取 B 仓调整单")
            .expect("B 仓调整单必须存在");
        let adjustment_a_second = fixture
            .db()
            .stock_adjustments()
            .find_by_id(&seed.adjustment_a2, &mut NoTransaction)
            .await
            .expect("读取第二张 A 仓调整单")
            .expect("第二张 A 仓调整单必须存在");
        let before = write_facts(fixture.db()).await;

        let company_updated = service
            .update_stock_adjustment(
                &seed.adjustment_b1,
                update_request(
                    &seed.adjustment_b1,
                    adjustment_b.base.version,
                    "Company 更新 B 仓",
                ),
                &actor(COMPANY),
            )
            .await
            .expect("Company 同角色 detail+update+scope 必须可更新");
        assert_eq!(company_updated.note.as_deref(), Some("Company 更新 B 仓"));
        assert_eq!(
            company_updated.version,
            (adjustment_b.base.version + 1).to_string()
        );

        let single_updated = service
            .update_stock_adjustment(
                &seed.adjustment_a1,
                update_request(&seed.adjustment_a1, adjustment_a.base.version, "单仓更新 A 仓"),
                &actor(SINGLE),
            )
            .await
            .expect("单仓同角色 detail+update+scope 必须可更新本仓");
        assert_eq!(single_updated.note.as_deref(), Some("单仓更新 A 仓"));
        assert_eq!(
            single_updated.version,
            (adjustment_a.base.version + 1).to_string()
        );

        let intersected_updated = service
            .update_stock_adjustment(
                &seed.adjustment_a2,
                update_request(
                    &seed.adjustment_a2,
                    adjustment_a_second.base.version,
                    "用户与角色范围交集更新 A 仓",
                ),
                &actor(ADJUSTMENT_INTERSECTION),
            )
            .await
            .expect("用户 A 仓范围与 Company 角色范围交集内必须可更新");
        assert_eq!(
            intersected_updated.note.as_deref(),
            Some("用户与角色范围交集更新 A 仓")
        );

        for adjustment_id in [&seed.adjustment_a1, &seed.adjustment_a2, &seed.adjustment_b1] {
            let line = fixture
                .db()
                .stock_adjustment_lines()
                .find_by_id(&format!("line-{adjustment_id}"), &mut NoTransaction)
                .await
                .expect("读取更新后明细")
                .expect("更新后明细必须存在");
            assert_eq!(line.quantity, Quantity::from_str("3").expect("更新后数量"));
        }

        let after = write_facts(fixture.db()).await;
        assert_eq!(after.adjustments.len(), before.adjustments.len());
        assert_eq!(after.lines.len(), before.lines.len());
        assert_ne!(after.adjustments, before.adjustments);
        assert_ne!(after.lines, before.lines);
        assert_eq!(after.documents, before.documents);
        assert_eq!(after.balances, before.balances);
        assert_eq!(after.audits.len(), before.audits.len() + 3);

        cleanup(fixture).await;
    });
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn update_auth_identity_line_and_version_failures_are_hidden_or_zero_write() {
    require_mongo!(async {
        let (fixture, service, seed) = fixture("stock_adj_auth_update_deny").await;
        let adjustment_a = fixture
            .db()
            .stock_adjustments()
            .find_by_id(&seed.adjustment_a1, &mut NoTransaction)
            .await
            .expect("读取 A 仓调整单")
            .expect("A 仓调整单必须存在");
        let adjustment_b = fixture
            .db()
            .stock_adjustments()
            .find_by_id(&seed.adjustment_b1, &mut NoTransaction)
            .await
            .expect("读取 B 仓调整单")
            .expect("B 仓调整单必须存在");
        let baseline = write_facts(fixture.db()).await;

        let hidden = hidden_message(
            service
                .update_stock_adjustment(
                    &seed.adjustment_b1,
                    update_request(&seed.adjustment_b1, adjustment_b.base.version, "越权更新"),
                    &actor(SINGLE),
                )
                .await
                .expect_err("单仓主体不得更新另一仓"),
        );
        let guessed = hidden_message(
            service
                .update_stock_adjustment(
                    "adjustment-guessed",
                    update_request("adjustment-guessed", 1, "猜测更新"),
                    &actor(SINGLE),
                )
                .await
                .expect_err("猜测调整单 ID 必须隐藏"),
        );
        assert_eq!(hidden, guessed, "更新越权与不存在不得泄露不同语义");
        assert_eq!(write_facts(fixture.db()).await, baseline);

        let intersection_hidden = hidden_message(
            service
                .update_stock_adjustment(
                    &seed.adjustment_b1,
                    update_request(&seed.adjustment_b1, adjustment_b.base.version, "用户范围交集越权"),
                    &actor(ADJUSTMENT_INTERSECTION),
                )
                .await
                .expect_err("用户范围与角色 Company 范围交集不得越过 A 仓"),
        );
        assert_eq!(intersection_hidden, guessed);
        assert_eq!(write_facts(fixture.db()).await, baseline);

        for denied_actor in [
            EMPTY,
            WRONG,
            DETAIL_ONLY,
            CREATE_ONLY,
            DISABLED,
            SPLIT,
            LIST_SPLIT,
        ] {
            let denied = hidden_message(
                service
                    .update_stock_adjustment(
                        &seed.adjustment_a1,
                        update_request(&seed.adjustment_a1, adjustment_a.base.version, "拒绝更新"),
                        &actor(denied_actor),
                    )
                    .await
                    .expect_err("缺权限、范围或启用角色必须隐藏更新对象"),
            );
            assert_eq!(
                denied, guessed,
                "{denied_actor} 的更新拒绝必须与 guessed missing 使用同一 NotFound 文案"
            );
            assert_eq!(
                write_facts(fixture.db()).await,
                baseline,
                "{denied_actor} 更新拒绝不得改写 header、lines 或 audit"
            );
        }

        let stale = service
            .update_stock_adjustment(
                &seed.adjustment_a1,
                update_request(&seed.adjustment_a1, adjustment_a.base.version + 1, "过期版本"),
                &actor(COMPANY),
            )
            .await
            .expect_err("过期 header 版本必须失败");
        assert!(matches!(stale, Error::ConflictError(_)));
        assert_eq!(write_facts(fixture.db()).await, baseline);

        let mut drifted_line = update_request(&seed.adjustment_a1, adjustment_a.base.version, "跨单明细漂移");
        drifted_line.lines.as_mut().expect("更新明细")[0].line_id = format!("line-{}", seed.adjustment_b1);
        service
            .update_stock_adjustment(&seed.adjustment_a1, drifted_line, &actor(COMPANY))
            .await
            .expect_err("明细身份漂移必须失败并回滚表头");
        assert_eq!(
            write_facts(fixture.db()).await,
            baseline,
            "跨单明细漂移不得留下先写表头或审计"
        );

        suspend_account(fixture.db(), SINGLE).await;
        let inactive = hidden_message(
            service
                .update_stock_adjustment(
                    &seed.adjustment_a1,
                    update_request(&seed.adjustment_a1, adjustment_a.base.version, "账号状态漂移"),
                    &actor(SINGLE),
                )
                .await
                .expect_err("账号暂停后必须重新验证 active actor"),
        );
        assert_eq!(inactive, guessed, "暂停账号不得泄露对象存在性");
        assert_eq!(
            write_facts(fixture.db()).await,
            baseline,
            "账号状态漂移不得改写 header、lines 或 audit"
        );

        let state_change = fixture
            .db()
            .collection::<Document>("stock_adjustments")
            .update_one(
                doc! { "id": &seed.adjustment_b2 },
                doc! { "$set": { "status": "IN_APPROVAL" } },
            )
            .await
            .expect("把独立调整单置为不可编辑状态");
        assert_eq!(state_change.modified_count, 1);
        let non_editable = fixture
            .db()
            .stock_adjustments()
            .find_by_id(&seed.adjustment_b2, &mut NoTransaction)
            .await
            .expect("读取不可编辑调整单")
            .expect("不可编辑调整单必须存在");
        let non_editable_baseline = write_facts(fixture.db()).await;
        service
            .update_stock_adjustment(
                &seed.adjustment_b2,
                update_request(&seed.adjustment_b2, non_editable.base.version, "审批中不得更新"),
                &actor(COMPANY),
            )
            .await
            .expect_err("非 Draft/Rejected 状态必须拒绝更新");
        assert_eq!(
            write_facts(fixture.db()).await,
            non_editable_baseline,
            "状态失败不得改写 header、lines 或 audit"
        );

        cleanup(fixture).await;
    });
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn inactive_actor_cannot_enumerate_lists_or_distinguish_exact_details() {
    require_mongo!(async {
        let (fixture, service, seed) = fixture("stock_adj_auth_inactive_read").await;
        suspend_account(fixture.db(), SINGLE).await;
        let baseline = write_facts(fixture.db()).await;

        let adjustment_list_error = service
            .stock_adjustment_list(&adjustment_params(None, 1, 100), &actor(SINGLE))
            .await
            .expect_err("暂停账号不得获得调整单 items/total");
        assert!(matches!(adjustment_list_error, Error::Forbidden(_)));
        let balance_list_error = service
            .stock_balance_list(&balance_params(None, 1, 100), &actor(SINGLE))
            .await
            .expect_err("暂停账号不得获得余额 items/total");
        assert!(matches!(balance_list_error, Error::Forbidden(_)));

        let adjustment_exact = hidden_message(
            service
                .stock_adjustment_detail(&seed.adjustment_a1, &actor(SINGLE))
                .await
                .expect_err("暂停账号不得读取已知调整单"),
        );
        let adjustment_missing = hidden_message(
            service
                .stock_adjustment_detail("adjustment-guessed", &actor(SINGLE))
                .await
                .expect_err("暂停账号猜测调整单必须隐藏"),
        );
        assert_eq!(adjustment_exact, adjustment_missing);

        let balance_exact = hidden_message(
            service
                .stock_balance_detail(&seed.balance_a.base.id, &actor(SINGLE))
                .await
                .expect_err("暂停账号不得读取已知余额"),
        );
        let balance_missing = hidden_message(
            service
                .stock_balance_detail("balance-guessed", &actor(SINGLE))
                .await
                .expect_err("暂停账号猜测余额必须隐藏"),
        );
        assert_eq!(balance_exact, balance_missing);
        assert_eq!(
            write_facts(fixture.db()).await,
            baseline,
            "暂停账号的全部读拒绝不得产生业务或审计写入"
        );

        cleanup(fixture).await;
    });
}

/// 断言事务快照 R 与进程授权快照 R+1 不一致时的稳定失败语义。
fn assert_policy_revision_error(error: Error) {
    match error {
        Error::Rbac(message) => assert_eq!(message, "授权策略版本已变化，无法在当前事务中证明授权快照"),
        other => panic!("策略版本漂移必须返回稳定 RBAC 错误，实际为 {other:?}"),
    }
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向启用 enableTestCommands 的 MongoDB 副本集"]
async fn update_policy_revision_drift_is_zero_write() {
    require_mongo!(async {
        const COMMAND_APP: &str = "stock-auth-update-policy-drift";

        let (fixture, _service, seed) = fixture("stock_adj_auth_update_policy").await;
        let adjustment = fixture
            .db()
            .stock_adjustments()
            .find_by_id(&seed.adjustment_a1, &mut NoTransaction)
            .await
            .expect("读取策略漂移更新单")
            .expect("策略漂移更新单必须存在");
        let baseline = write_facts(fixture.db()).await;
        let command_db = app_named_database(&fixture, COMMAND_APP).await;
        let command_service = InventoryService::new(command_db.clone(), iam::shared_rbac_service(command_db));
        let adjustment_id = seed.adjustment_a1.clone();
        let request = update_request(&seed.adjustment_a1, adjustment.base.version, "策略漂移不得更新");
        let failpoint_before = arm_policy_revision_find(fixture.db(), COMMAND_APP).await;
        let command_task = tokio::spawn(async move {
            command_service
                .update_stock_adjustment(&adjustment_id, request, &actor(COMPANY))
                .await
        });
        if !wait_for_policy_revision_find(fixture.db(), failpoint_before).await {
            disable_fail_command(fixture.db()).await;
            let unexpected = command_task.await.expect("等待未挂起的更新命令");
            cleanup(fixture).await;
            panic!("更新命令未命中策略版本门闩: {unexpected:?}");
        }

        let revision = advance_policy_revision(fixture.db(), "update_policy_revision_probe").await;
        disable_fail_command(fixture.db()).await;
        let command_result = command_task.await.expect("等待策略漂移更新命令");
        let (revision_r, revision_r_plus_one) = revision.expect("提交 update policy R+1");
        assert_eq!(revision_r_plus_one, revision_r + 1);
        assert_policy_revision_error(
            command_result.expect_err("update 的事务快照 R 与授权快照 R+1 必须失败关闭"),
        );
        assert_eq!(
            write_facts(fixture.db()).await,
            baseline,
            "update policy revision 漂移必须在 header、lines、audit 首笔写前失败"
        );

        cleanup(fixture).await;
    });
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向启用 enableTestCommands 的 MongoDB 副本集"]
async fn create_policy_revision_drift_is_zero_write() {
    require_mongo!(async {
        const COMMAND_APP: &str = "stock-auth-create-policy-drift";

        let (fixture, _service, seed) = fixture("stock_adj_auth_create_policy").await;
        let baseline = write_facts(fixture.db()).await;
        let command_db = app_named_database(&fixture, COMMAND_APP).await;
        let command_service = InventoryService::new(command_db.clone(), iam::shared_rbac_service(command_db));
        let request = create_request("ADJ-POLICY-DRIFT", WAREHOUSE_A, SKU_A, &seed.balance_a);
        let failpoint_before = arm_policy_revision_find(fixture.db(), COMMAND_APP).await;
        let command_task = tokio::spawn(async move {
            command_service
                .create_stock_adjustment(request, &actor(COMPANY))
                .await
        });
        if !wait_for_policy_revision_find(fixture.db(), failpoint_before).await {
            disable_fail_command(fixture.db()).await;
            let unexpected = command_task.await.expect("等待未挂起的创建命令");
            cleanup(fixture).await;
            panic!("创建命令未命中策略版本门闩: {unexpected:?}");
        }

        let revision = advance_policy_revision(fixture.db(), "create_policy_revision_probe").await;
        disable_fail_command(fixture.db()).await;
        let command_result = command_task.await.expect("等待策略漂移创建命令");
        let (revision_r, revision_r_plus_one) = revision.expect("提交 create policy R+1");
        assert_eq!(revision_r_plus_one, revision_r + 1);
        assert_policy_revision_error(
            command_result.expect_err("create 的事务快照 R 与授权快照 R+1 必须失败关闭"),
        );
        assert_eq!(
            write_facts(fixture.db()).await,
            baseline,
            "create policy revision 漂移必须在调整单、明细、注册、绑定审计与创建审计首写前失败"
        );

        cleanup(fixture).await;
    });
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn balance_scope_actions_and_pending_adjustments_are_independently_authorized() {
    require_mongo!(async {
        let (fixture, service, seed) = fixture("stock_adj_auth_balance").await;

        let company_page_1 = service
            .stock_balance_list(&balance_params(None, 1, 1), &actor(COMPANY))
            .await
            .expect("Company 相同 sku_id 余额第一页");
        let company_page_2 = service
            .stock_balance_list(&balance_params(None, 2, 1), &actor(COMPANY))
            .await
            .expect("Company 相同 sku_id 余额第二页");
        let company_page_3 = service
            .stock_balance_list(&balance_params(None, 3, 1), &actor(COMPANY))
            .await
            .expect("Company 余额第三页");
        for page in [&company_page_1, &company_page_2, &company_page_3] {
            assert_eq!(page.total, 3);
            assert_eq!(page.items.len(), 1);
            assert_create_action(
                &page.items[0].allowed_actions,
                true,
                "Company 范围必须给全部余额签发创建动作",
            );
        }
        assert_eq!(company_page_1.items[0].id, seed.balance_a.base.id);
        assert_eq!(company_page_2.items[0].id, seed.balance_b.base.id);
        assert_eq!(company_page_3.items[0].id, seed.balance_a_second.base.id);

        let company_two_1 = service
            .stock_balance_list(&balance_params(None, 1, 2), &actor(COMPANY))
            .await
            .expect("Company page_size=2 第一页");
        let company_two_2 = service
            .stock_balance_list(&balance_params(None, 2, 2), &actor(COMPANY))
            .await
            .expect("Company page_size=2 第二页");
        assert_eq!(company_two_1.total, 3);
        assert_eq!(company_two_2.total, 3);
        let first_ids = balance_ids(&company_two_1.items);
        let second_ids = balance_ids(&company_two_2.items);
        assert!(first_ids.is_disjoint(&second_ids));
        assert_eq!(
            first_ids.union(&second_ids).copied().collect::<HashSet<_>>(),
            HashSet::from([
                seed.balance_a.base.id.as_str(),
                seed.balance_b.base.id.as_str(),
                seed.balance_a_second.base.id.as_str(),
            ]),
            "相同 sku_id 必须用稳定 id 次排序避免跨页重复或遗漏"
        );

        let single_page_1 = service
            .stock_balance_list(&balance_params(None, 1, 1), &actor(SINGLE))
            .await
            .expect("单仓余额第一页");
        let single_page_2 = service
            .stock_balance_list(&balance_params(None, 2, 1), &actor(SINGLE))
            .await
            .expect("单仓余额第二页");
        let single_page_3 = service
            .stock_balance_list(&balance_params(None, 3, 1), &actor(SINGLE))
            .await
            .expect("单仓余额越界页");
        assert_eq!(single_page_1.total, 2);
        assert_eq!(single_page_2.total, 2);
        assert_eq!(single_page_3.total, 2);
        assert_eq!(single_page_1.items[0].id, seed.balance_a.base.id);
        assert_eq!(single_page_2.items[0].id, seed.balance_a_second.base.id);
        assert!(single_page_3.items.is_empty());

        let user_intersection = service
            .stock_balance_list(&balance_params(None, 1, 100), &actor(BALANCE_INTERSECTION))
            .await
            .expect("用户范围必须与 Company 角色范围取交集");
        assert_eq!(user_intersection.total, 2);
        assert_eq!(
            balance_ids(&user_intersection.items),
            HashSet::from([
                seed.balance_a.base.id.as_str(),
                seed.balance_a_second.base.id.as_str(),
            ])
        );
        assert!(
            user_intersection
                .items
                .iter()
                .all(|item| item.allowed_actions.is_empty()),
            "仅有余额权限的用户范围交集列表不得签发调整动作"
        );

        let multi_role = service
            .stock_balance_list(&balance_params(None, 1, 100), &actor(BALANCE_MULTI))
            .await
            .expect("各自具备 list+scope 的启用角色可并集范围");
        assert_eq!(multi_role.total, 3);
        assert_eq!(
            balance_ids(&multi_role.items),
            HashSet::from([
                seed.balance_a.base.id.as_str(),
                seed.balance_a_second.base.id.as_str(),
                seed.balance_b.base.id.as_str(),
            ])
        );
        assert!(
            multi_role
                .items
                .iter()
                .all(|item| item.allowed_actions.is_empty()),
            "多个纯余额角色的列表不得签发调整动作"
        );

        let balance_only = service
            .stock_balance_list(&balance_params(None, 1, 100), &actor(BALANCE_ONLY))
            .await
            .expect("balance-only 角色必须可见 A 仓余额本体");
        assert_eq!(balance_only.total, 2);
        assert!(
            balance_only
                .items
                .iter()
                .all(|item| item.allowed_actions.is_empty()),
            "调整 detail/create 范围不覆盖行仓库时列表动作必须为空"
        );

        let independently_scoped_list = service
            .stock_balance_list(&balance_params(None, 1, 100), &actor(BALANCE_SEPARATE))
            .await
            .expect("balance list 必须只采用 list 角色的 A 仓范围");
        assert_eq!(independently_scoped_list.total, 2);
        assert_eq!(
            balance_ids(&independently_scoped_list.items),
            HashSet::from([
                seed.balance_a.base.id.as_str(),
                seed.balance_a_second.base.id.as_str(),
            ])
        );
        assert!(
            independently_scoped_list
                .items
                .iter()
                .all(|item| item.allowed_actions.is_empty()),
            "分离的 balance list/detail 角色不得签发调整动作"
        );

        let explicit_wrong_warehouse = service
            .stock_balance_list(&balance_params(Some(WAREHOUSE_B), 1, 100), &actor(SINGLE))
            .await
            .expect("余额请求仓库必须与授权范围取交集");
        assert!(explicit_wrong_warehouse.items.is_empty());
        assert_eq!(explicit_wrong_warehouse.total, 0);

        for denied_actor in [EMPTY, WRONG, DISABLED, SPLIT, LIST_SPLIT] {
            let denied = service
                .stock_balance_list(&balance_params(None, 1, 100), &actor(denied_actor))
                .await
                .expect("无有效 balance list+同角色范围时返回空页");
            assert!(denied.items.is_empty(), "{denied_actor} 不得读取余额列表");
            assert_eq!(denied.total, 0, "{denied_actor} 不得泄露余额 total");
        }

        let mixed_page = service
            .stock_balance_list(&balance_params(None, 1, 100), &actor(BALANCE_SCOPED))
            .await
            .expect("Company 余额读取与单仓调整动作可独立投影");
        assert_eq!(mixed_page.total, 3);
        for balance in &mixed_page.items {
            assert_create_action(
                &balance.allowed_actions,
                balance.warehouse_id == WAREHOUSE_A,
                "动作必须按该行 Warehouse 和同一调整角色签发",
            );
        }

        let company_detail = service
            .stock_balance_detail(&seed.balance_b.base.id, &actor(COMPANY))
            .await
            .expect("Company balance detail 可读任意仓");
        assert_eq!(company_detail.balance.id, seed.balance_b.base.id);

        let single_detail = service
            .stock_balance_detail(&seed.balance_a.base.id, &actor(SINGLE))
            .await
            .expect("单仓 balance detail 可读本仓");
        assert_eq!(single_detail.balance.id, seed.balance_a.base.id);
        let hidden = hidden_message(
            service
                .stock_balance_detail(&seed.balance_b.base.id, &actor(SINGLE))
                .await
                .expect_err("单仓 balance detail 不得读取另一仓"),
        );
        let guessed = hidden_message(
            service
                .stock_balance_detail("balance-guessed", &actor(SINGLE))
                .await
                .expect_err("猜测余额 ID 必须隐藏"),
        );
        assert_eq!(hidden, guessed, "余额越权与不存在不得泄露不同语义");
        for denied_actor in [EMPTY, WRONG, DISABLED, LIST_SPLIT] {
            let denied = hidden_message(
                service
                    .stock_balance_detail(&seed.balance_a.base.id, &actor(denied_actor))
                    .await
                    .expect_err("无有效 balance detail+同角色范围必须隐藏"),
            );
            assert_eq!(
                denied, guessed,
                "{denied_actor} 的余额详情拒绝必须与 guessed missing 文案一致"
            );
        }

        let separated_hidden = hidden_message(
            service
                .stock_balance_detail(&seed.balance_a.base.id, &actor(BALANCE_SEPARATE))
                .await
                .expect_err("detail-only B 仓角色不得借用 list-only A 仓范围"),
        );
        assert_eq!(separated_hidden, guessed);
        let separated_detail = service
            .stock_balance_detail(&seed.balance_b.base.id, &actor(BALANCE_SEPARATE))
            .await
            .expect("detail-only B 仓角色必须可读 B 仓余额");
        assert!(separated_detail.pending_adjustments.is_empty());
        assert_create_action(
            &separated_detail.balance.allowed_actions,
            false,
            "balance list/detail 分离角色不得获得调整创建动作",
        );

        let intersected_hidden = hidden_message(
            service
                .stock_balance_detail(&seed.balance_b.base.id, &actor(BALANCE_INTERSECTION))
                .await
                .expect_err("用户 A 仓范围必须与 balance detail Company 角色范围取交集"),
        );
        assert_eq!(intersected_hidden, guessed);

        let readable = service
            .stock_balance_detail(&seed.balance_a.base.id, &actor(BALANCE_SCOPED))
            .await
            .expect("余额读取 Company 且调整范围覆盖时可读 pending");
        assert_create_action(
            &readable.balance.allowed_actions,
            true,
            "同角色 detail+create+Warehouse 范围必须签发动作",
        );
        assert_eq!(
            readable
                .pending_adjustments
                .iter()
                .map(|item| item.id.as_str())
                .collect::<HashSet<_>>(),
            HashSet::from([seed.adjustment_a1.as_str(), seed.adjustment_a2.as_str()])
        );

        let detail_without_create = service
            .stock_balance_detail(&seed.balance_a.base.id, &actor(DETAIL_ONLY))
            .await
            .expect("库存调整 detail 范围覆盖时可读 pending");
        assert_create_action(
            &detail_without_create.balance.allowed_actions,
            false,
            "缺 create 权限不得签发余额动作",
        );
        assert_eq!(detail_without_create.pending_adjustments.len(), 2);

        let split_detail = service
            .stock_balance_detail(&seed.balance_a.base.id, &actor(SPLIT))
            .await
            .expect("split actor 的启用 detail 角色可读取 exact balance/pending");
        assert_eq!(split_detail.pending_adjustments.len(), 2);
        assert_create_action(
            &split_detail.balance.allowed_actions,
            false,
            "另一角色的 create 权限不得与 detail 角色拼接签发动作",
        );

        for actor_id in [BALANCE_ONLY, BALANCE_INTERSECTION, BALANCE_MULTI] {
            let hidden_pending = service
                .stock_balance_detail(&seed.balance_a.base.id, &actor(actor_id))
                .await
                .expect("独立 stock_balance:detail 范围仍可读取余额本体");
            assert!(
                hidden_pending.pending_adjustments.is_empty(),
                "{actor_id} 的库存调整读取范围不覆盖本仓时不得泄露 pending"
            );
            assert_create_action(
                &hidden_pending.balance.allowed_actions,
                false,
                "{actor_id} 不得由其他角色范围或停用残留策略签发动作",
            );
        }

        cleanup(fixture).await;
    });
}
