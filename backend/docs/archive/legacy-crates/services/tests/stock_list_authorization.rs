//! FUL-S04：库存流水与库存预占列表的真实 MongoDB 授权验收。
//!
//! 用例使用随机独立数据库、真实 Casbin Mongo Adapter 和公开 `InventoryService`；
//! 仅在 `ERP_TEST_MONGO_URI` 指向 MongoDB 7 单节点副本集时通过
//! `--include-ignored` 串行执行。

use std::str::FromStr;

use casbin::Adapter;
use database::{ensure_indexes, AccessControlExt, InventoryExt, MongoCasbinAdapter, NoTransaction};
use entities::access_control::{DataScope, DataScopeData, DataScopeSubjectType, DataScopeType};
use entities::common::source::SourceType;
use entities::common::time::Instant;
use entities::ids::{
    DataScopeId, SalesOrderLineId, SkuId, StockBalanceId, StockMovementId, StockReservationId, WarehouseId,
};
use entities::inventory::{
    MovementDirection, MovementType, ReservationStatus, StockBalance, StockBalanceData, StockMovement,
    StockMovementData, StockReservation, StockReservationData, StockReservationSourceType,
};
use entities::money::Quantity;
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
    InventoryService, StockMovementListParams, StockMovementView, StockReservationListParams,
    StockReservationView,
};
use services::Error;
use test_support::{require_mongo, TestDb};

const COMPANY: &str = "company-user";
const SINGLE: &str = "single-user";
const EMPTY_INTERSECTION: &str = "empty-intersection-user";
const USER_INTERSECTION: &str = "user-intersection-user";
const DISABLED: &str = "disabled-user";
const NO_SCOPE_BORROW: &str = "no-scope-borrow-user";
const MULTI: &str = "multi-user";
const MOVEMENT_ONLY: &str = "movement-only-user";
const RESERVATION_ONLY: &str = "reservation-only-user";
const OTHER_INVENTORY: &str = "other-inventory-user";
const INACTIVE: &str = "inactive-user";

const ROLE_COMPANY: &str = "stock-list-company";
const ROLE_SINGLE: &str = "stock-list-single";
const ROLE_EMPTY_INTERSECTION: &str = "stock-list-empty-intersection";
const ROLE_USER_INTERSECTION: &str = "stock-list-user-intersection";
const ROLE_DISABLED: &str = "stock-list-disabled";
const ROLE_NO_SCOPE: &str = "stock-list-no-scope";
const ROLE_UNRELATED_COMPANY: &str = "stock-list-unrelated-company";
const ROLE_MULTI_A: &str = "stock-list-multi-a";
const ROLE_MULTI_B: &str = "stock-list-multi-b";
const ROLE_MOVEMENT_ONLY: &str = "stock-list-movement-only";
const ROLE_RESERVATION_ONLY: &str = "stock-list-reservation-only";
const ROLE_OTHER_INVENTORY: &str = "stock-list-other-inventory";
const ROLE_INACTIVE: &str = "stock-list-inactive";

const WAREHOUSE_A: &str = "warehouse-a";
const WAREHOUSE_B: &str = "warehouse-b";
const SKU: &str = "sku-shared";
const BALANCE_A: &str = "balance-a";
const MOVEMENT_PERMISSION: &str = "stock_movement:list";
const RESERVATION_PERMISSION: &str = "stock_reservation:list";
const FACT_IDS: [&str; 4] = ["01-A", "02-B", "03-A", "04-B"];

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

/// 构造角色级 Warehouse DataScope。
fn role_scope(id: &str, role_id: &str, scope_type: DataScopeType, targets: &[&str]) -> DataScope {
    DataScope::new(
        DataScopeId::new(id),
        DataScopeData {
            subject_type: DataScopeSubjectType::Role,
            subject_id: role_id.to_string(),
            scope_type,
            scope_targets: targets.iter().map(|target| (*target).to_string()).collect(),
        },
    )
    .expect("角色 DataScope fixture")
}

/// 构造会与各角色范围逐角色求交的用户级 Warehouse DataScope。
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

/// 写入账号、启用/停用角色、真实 Casbin 策略和逐角色 Warehouse DataScope。
async fn seed_authorization(db: &Database) {
    for id in [
        COMPANY,
        SINGLE,
        EMPTY_INTERSECTION,
        USER_INTERSECTION,
        DISABLED,
        NO_SCOPE_BORROW,
        MULTI,
        MOVEMENT_ONLY,
        RESERVATION_ONLY,
        OTHER_INVENTORY,
        INACTIVE,
    ] {
        db.accounts()
            .create(&account(id), &mut NoTransaction)
            .await
            .expect("写入库存列表授权账号");
    }
    suspend_account(db, INACTIVE).await;

    for role_id in [
        ROLE_COMPANY,
        ROLE_SINGLE,
        ROLE_EMPTY_INTERSECTION,
        ROLE_USER_INTERSECTION,
        ROLE_DISABLED,
        ROLE_NO_SCOPE,
        ROLE_UNRELATED_COMPANY,
        ROLE_MULTI_A,
        ROLE_MULTI_B,
        ROLE_MOVEMENT_ONLY,
        ROLE_RESERVATION_ONLY,
        ROLE_OTHER_INVENTORY,
        ROLE_INACTIVE,
    ] {
        let mut role = Role::new(
            role_id.to_string(),
            RoleData {
                name: role_id.to_string(),
                description: None,
                system: false,
            },
        )
        .expect("库存列表授权角色 fixture");
        if role_id == ROLE_DISABLED {
            role.update(RoleUpdate {
                disabled: Some(true),
                ..RoleUpdate::default()
            })
            .expect("停用残留权限角色");
        }
        db.roles()
            .create(&role, &mut NoTransaction)
            .await
            .expect("写入库存列表授权角色");
    }

    let bindings = [
        (COMPANY, ROLE_COMPANY),
        (SINGLE, ROLE_SINGLE),
        (EMPTY_INTERSECTION, ROLE_EMPTY_INTERSECTION),
        (USER_INTERSECTION, ROLE_USER_INTERSECTION),
        (DISABLED, ROLE_DISABLED),
        (NO_SCOPE_BORROW, ROLE_NO_SCOPE),
        (NO_SCOPE_BORROW, ROLE_UNRELATED_COMPANY),
        (MULTI, ROLE_MULTI_A),
        (MULTI, ROLE_MULTI_B),
        (MOVEMENT_ONLY, ROLE_MOVEMENT_ONLY),
        (RESERVATION_ONLY, ROLE_RESERVATION_ONLY),
        (OTHER_INVENTORY, ROLE_OTHER_INVENTORY),
        (INACTIVE, ROLE_INACTIVE),
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
        .expect("写入库存列表角色绑定"));

    let mut policies = Vec::new();
    for role_id in [
        ROLE_COMPANY,
        ROLE_SINGLE,
        ROLE_EMPTY_INTERSECTION,
        ROLE_USER_INTERSECTION,
        ROLE_DISABLED,
        ROLE_NO_SCOPE,
        ROLE_MULTI_A,
        ROLE_MULTI_B,
        ROLE_INACTIVE,
    ] {
        grant(&mut policies, role_id, "stock_movement", "list");
        grant(&mut policies, role_id, "stock_reservation", "list");
    }
    grant(&mut policies, ROLE_MOVEMENT_ONLY, "stock_movement", "list");
    grant(&mut policies, ROLE_RESERVATION_ONLY, "stock_reservation", "list");
    grant(&mut policies, ROLE_OTHER_INVENTORY, "stock_balance", "detail");
    grant(&mut policies, ROLE_UNRELATED_COMPANY, "stock_balance", "list");
    assert!(adapter
        .add_policies("p", "p", policies)
        .await
        .expect("写入库存流水与预占列表权限"));

    for data_scope in [
        role_scope("scope-company", ROLE_COMPANY, DataScopeType::Company, &[]),
        role_scope(
            "scope-single-a",
            ROLE_SINGLE,
            DataScopeType::Organization,
            &[WAREHOUSE_A],
        ),
        role_scope(
            "scope-empty-role-a",
            ROLE_EMPTY_INTERSECTION,
            DataScopeType::Organization,
            &[WAREHOUSE_A],
        ),
        user_scope("scope-empty-user-b", EMPTY_INTERSECTION, &[WAREHOUSE_B]),
        role_scope(
            "scope-user-intersection-company",
            ROLE_USER_INTERSECTION,
            DataScopeType::Company,
            &[],
        ),
        user_scope("scope-user-intersection-a", USER_INTERSECTION, &[WAREHOUSE_A]),
        role_scope(
            "scope-disabled-company",
            ROLE_DISABLED,
            DataScopeType::Company,
            &[],
        ),
        // ROLE_NO_SCOPE 故意无范围；不得借用这个无关角色的 Company 范围。
        role_scope(
            "scope-unrelated-company",
            ROLE_UNRELATED_COMPANY,
            DataScopeType::Company,
            &[],
        ),
        role_scope(
            "scope-multi-a",
            ROLE_MULTI_A,
            DataScopeType::Organization,
            &[WAREHOUSE_A],
        ),
        role_scope(
            "scope-multi-b",
            ROLE_MULTI_B,
            DataScopeType::Organization,
            &[WAREHOUSE_B],
        ),
        role_scope(
            "scope-movement-only",
            ROLE_MOVEMENT_ONLY,
            DataScopeType::Organization,
            &[WAREHOUSE_A],
        ),
        role_scope(
            "scope-reservation-only",
            ROLE_RESERVATION_ONLY,
            DataScopeType::Organization,
            &[WAREHOUSE_B],
        ),
        role_scope(
            "scope-other-inventory",
            ROLE_OTHER_INVENTORY,
            DataScopeType::Company,
            &[],
        ),
        role_scope("scope-inactive", ROLE_INACTIVE, DataScopeType::Company, &[]),
    ] {
        db.data_scopes()
            .create(&data_scope, &mut NoTransaction)
            .await
            .expect("写入库存列表 Warehouse DataScope");
    }
}

/// 把既有账号暂停，证明列表在读取资源前重验 active actor。
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
        .expect("持久化暂停账号");
}

/// 写入一条具有固定发生时间的库存流水。
async fn seed_movement(db: &Database, id: &str, warehouse_id: &str) {
    let movement = StockMovement::new(
        StockMovementId::new(id),
        StockMovementData {
            warehouse_id: WarehouseId::new(warehouse_id),
            sku_id: SkuId::new(SKU),
            movement_type: MovementType::Initial,
            direction: MovementDirection::Increase,
            quantity: Quantity::from_str("1").expect("流水数量"),
            source_document_id: format!("source-{id}"),
            source_line_id: Some(format!("source-line-{id}")),
            reversal_of_movement_id: None,
            fact_no: format!("fact-{id}"),
            occurred_at: Instant::from_unix_secs(10),
            recorded_at: Instant::from_unix_secs(10),
            recorded_by: "stock-list-fixture".to_string(),
            source_type: SourceType::Erp,
            source_reference: None,
            reason_code: None,
            reason_text: None,
        },
    )
    .expect("库存流水 fixture");
    db.stock_movements()
        .create(&movement, &mut NoTransaction)
        .await
        .expect("写入库存流水 fixture");
}

/// 写入一条库存预占；稍后统一冻结 `created_at` 主排序键。
async fn seed_reservation(db: &Database, id: &str, warehouse_id: &str) {
    let reservation = StockReservation::new(
        StockReservationId::new(id),
        StockReservationData {
            warehouse_id: WarehouseId::new(warehouse_id),
            sku_id: SkuId::new(SKU),
            sales_order_line_id: SalesOrderLineId::new(format!("sales-line-{id}")),
            source_type: StockReservationSourceType::ExistingStock,
            purchase_line_sales_allocation_id: None,
            source_receipt_line_id: None,
            source_allocation_id: Some(format!("source-allocation-{id}")),
            reserved_quantity: Quantity::from_str("1").expect("预占数量"),
            consumed_quantity: Quantity::from_str("0").expect("消耗数量"),
            released_quantity: Quantity::from_str("0").expect("释放数量"),
            status: ReservationStatus::Active,
        },
    )
    .expect("库存预占 fixture");
    db.stock_reservations()
        .create(&reservation, &mut NoTransaction)
        .await
        .expect("写入库存预占 fixture");
}

/// 写入余额详情内嵌流水与预占所需的 A 仓库存维度。
async fn seed_balance(db: &Database) {
    let balance = StockBalance::new(
        StockBalanceId::new(BALANCE_A),
        StockBalanceData {
            warehouse_id: WarehouseId::new(WAREHOUSE_A),
            sku_id: SkuId::new(SKU),
            on_hand_quantity: Quantity::from_str("10").expect("账面数量"),
            reserved_quantity: Quantity::from_str("2").expect("预占数量"),
            available_quantity: Quantity::from_str("8").expect("可用数量"),
            last_movement_id: None,
        },
    )
    .expect("库存余额 fixture");
    db.stock_balances()
        .create(&balance, &mut NoTransaction)
        .await
        .expect("写入库存余额 fixture");
}

/// 建立两仓各两条、ID 交错且主排序键相同的真实 Mongo 授权 fixture。
async fn fixture(prefix: &str) -> (TestDb, InventoryService) {
    let fixture = TestDb::new(prefix).await.expect("测试数据库创建失败");
    ensure_indexes(fixture.db()).await.expect("索引创建失败");
    seed_authorization(fixture.db()).await;
    seed_balance(fixture.db()).await;
    for (id, warehouse_id) in [
        (FACT_IDS[0], WAREHOUSE_A),
        (FACT_IDS[1], WAREHOUSE_B),
        (FACT_IDS[2], WAREHOUSE_A),
        (FACT_IDS[3], WAREHOUSE_B),
    ] {
        seed_movement(fixture.db(), id, warehouse_id).await;
        seed_reservation(fixture.db(), id, warehouse_id).await;
    }
    let frozen = fixture
        .db()
        .collection::<Document>("stock_reservations")
        .update_many(doc! {}, doc! { "$set": { "created_at": 10_i64 } })
        .await
        .expect("冻结库存预占相同主排序键");
    assert_eq!(frozen.modified_count, 4);

    let service = InventoryService::new(
        fixture.db().clone(),
        iam::shared_rbac_service(fixture.db().clone()),
    );
    (fixture, service)
}

/// 构造库存流水分页参数。
fn movement_params(warehouse_id: Option<&str>, page: u64, page_size: u32) -> StockMovementListParams {
    StockMovementListParams {
        warehouse_id: warehouse_id.map(WarehouseId::new),
        sku_id: None,
        movement_type: None,
        direction: None,
        occurred_from: None,
        occurred_to: None,
        page: Some(page),
        page_size: Some(page_size),
        sort_by: Some("occurred_at".to_string()),
        sort_dir: Some("asc".to_string()),
    }
}

/// 构造库存预占分页参数。
fn reservation_params(warehouse_id: Option<&str>, page: u64, page_size: u32) -> StockReservationListParams {
    StockReservationListParams {
        warehouse_id: warehouse_id.map(WarehouseId::new),
        sku_id: None,
        status: None,
        sales_order_line_id: None,
        page: Some(page),
        page_size: Some(page_size),
        sort_by: Some("created_at".to_string()),
        sort_dir: Some("asc".to_string()),
    }
}

/// 返回流水页的稳定主键顺序。
fn movement_ids(items: &[StockMovementView]) -> Vec<&str> {
    items.iter().map(|item| item.id.as_str()).collect()
}

/// 返回预占页的稳定主键顺序。
fn reservation_ids(items: &[StockReservationView]) -> Vec<&str> {
    items.iter().map(|item| item.id.as_str()).collect()
}

/// 断言无授权的流水列表为空且 total 不泄露。
async fn assert_movement_empty(service: &InventoryService, actor_id: &str, label: &str) {
    let page = service
        .stock_movement_list(&movement_params(None, 1, 100), &actor(actor_id))
        .await
        .unwrap_or_else(|error| panic!("{label} 应返回授权空页，实际错误为 {error:?}"));
    assert!(page.items.is_empty(), "{label} 不得返回流水");
    assert_eq!(page.total, 0, "{label} 不得泄露流水 total");
}

/// 断言无授权的预占列表为空且 total 不泄露。
async fn assert_reservation_empty(service: &InventoryService, actor_id: &str, label: &str) {
    let page = service
        .stock_reservation_list(&reservation_params(None, 1, 100), &actor(actor_id))
        .await
        .unwrap_or_else(|error| panic!("{label} 应返回授权空页，实际错误为 {error:?}"));
    assert!(page.items.is_empty(), "{label} 不得返回预占");
    assert_eq!(page.total, 0, "{label} 不得泄露预占 total");
}

/// 断言列表因 inactive actor 失败关闭。
fn assert_inactive_forbidden(error: Error) {
    match error {
        Error::Forbidden(_) => {}
        other => panic!("inactive actor 必须返回 Forbidden，实际为 {other:?}"),
    }
}

/// 为策略漂移命令创建带独立 application name 的数据库句柄。
async fn app_named_database(fixture: &TestDb, app_name: &str) -> Database {
    let uri = std::env::var("ERP_TEST_MONGO_URI").expect("真实 Mongo 测试连接串");
    let mut options = ClientOptions::parse(uri).await.expect("解析 Mongo 测试连接串");
    options.app_name = Some(app_name.to_string());
    Client::with_options(options)
        .expect("创建带 appName 的 Mongo client")
        .database(fixture.name())
}

/// 读取 failCommand 的累计进入次数。
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

/// 断言事务快照 R 与进程授权快照 R+1 不一致时的稳定失败语义。
fn assert_policy_revision_error(error: Error) {
    match error {
        Error::Rbac(message) => assert_eq!(message, "授权策略版本已变化，无法在当前事务中证明授权快照"),
        other => panic!("策略版本漂移必须返回稳定 RBAC 错误，实际为 {other:?}"),
    }
}

/// 显式删除随机测试库，避免异步 Drop 未收敛。
async fn cleanup(fixture: TestDb) {
    fixture.db().drop().await.expect("清理随机测试数据库");
    std::mem::forget(fixture);
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn movement_scope_is_applied_before_page_and_total() {
    require_mongo!(async {
        let (fixture, service) = fixture("stock_movement_list_auth").await;

        let company_page_1 = service
            .stock_movement_list(&movement_params(None, 1, 2), &actor(COMPANY))
            .await
            .expect("Company 流水第一页");
        let company_page_2 = service
            .stock_movement_list(&movement_params(None, 2, 2), &actor(COMPANY))
            .await
            .expect("Company 流水第二页");
        assert_eq!(company_page_1.total, 4);
        assert_eq!(company_page_2.total, 4);
        assert_eq!(movement_ids(&company_page_1.items), [FACT_IDS[0], FACT_IDS[1]]);
        assert_eq!(movement_ids(&company_page_2.items), [FACT_IDS[2], FACT_IDS[3]]);

        let single_page_1 = service
            .stock_movement_list(&movement_params(None, 1, 1), &actor(SINGLE))
            .await
            .expect("A 仓流水第一页");
        let single_page_2 = service
            .stock_movement_list(&movement_params(None, 2, 1), &actor(SINGLE))
            .await
            .expect("A 仓流水第二页");
        let single_page_3 = service
            .stock_movement_list(&movement_params(None, 3, 1), &actor(SINGLE))
            .await
            .expect("A 仓流水越界页");
        assert_eq!(single_page_1.total, 2);
        assert_eq!(single_page_2.total, 2);
        assert_eq!(single_page_3.total, 2);
        assert_eq!(movement_ids(&single_page_1.items), [FACT_IDS[0]]);
        assert_eq!(movement_ids(&single_page_2.items), [FACT_IDS[2]]);
        assert!(single_page_3.items.is_empty());

        let explicit_wrong = service
            .stock_movement_list(&movement_params(Some(WAREHOUSE_B), 1, 100), &actor(SINGLE))
            .await
            .expect("显式错误仓库应与授权范围取空交集");
        assert!(explicit_wrong.items.is_empty());
        assert_eq!(explicit_wrong.total, 0);

        let intersection = service
            .stock_movement_list(&movement_params(None, 1, 100), &actor(USER_INTERSECTION))
            .await
            .expect("用户范围与 Company 角色范围取交集");
        assert_eq!(intersection.total, 2);
        assert_eq!(movement_ids(&intersection.items), [FACT_IDS[0], FACT_IDS[2]]);

        let multi = service
            .stock_movement_list(&movement_params(None, 1, 100), &actor(MULTI))
            .await
            .expect("同资源多角色 A+B 范围合并");
        assert_eq!(multi.total, 4);
        assert_eq!(movement_ids(&multi.items), FACT_IDS);

        let movement_only = service
            .stock_movement_list(&movement_params(None, 1, 100), &actor(MOVEMENT_ONLY))
            .await
            .expect("movement-only 角色可读取流水");
        assert_eq!(movement_only.total, 2);
        assert_eq!(movement_ids(&movement_only.items), [FACT_IDS[0], FACT_IDS[2]]);

        let rbac = iam::shared_rbac_service(fixture.db().clone());
        assert!(
            rbac.enforce(
                &subject(AccountKind::Admin, DISABLED),
                &Permission::parse(MOVEMENT_PERMISSION).expect("流水权限"),
            )
            .await
            .expect("读取停用角色原始 Casbin 前提"),
            "停用角色的原始 Casbin 授权必须仍为 true"
        );
        assert_movement_empty(&service, EMPTY_INTERSECTION, "用户与角色范围空交集").await;
        assert_movement_empty(&service, DISABLED, "停用角色残留策略").await;
        assert_movement_empty(
            &service,
            NO_SCOPE_BORROW,
            "权限角色无范围不得借无关 Company 角色范围",
        )
        .await;
        assert_movement_empty(&service, RESERVATION_ONLY, "预占资源权限不得读取流水").await;
        assert_movement_empty(&service, OTHER_INVENTORY, "只有其他库存权限").await;

        let balance_detail = service
            .stock_balance_detail(BALANCE_A, &actor(OTHER_INVENTORY))
            .await
            .expect("余额详情无需两类 list 权限即可读取本余额内嵌事实");
        let mut embedded_movement_ids = balance_detail
            .recent_movements
            .iter()
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>();
        embedded_movement_ids.sort_unstable();
        let mut embedded_reservation_ids = balance_detail
            .active_reservations
            .iter()
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>();
        embedded_reservation_ids.sort_unstable();
        assert_eq!(embedded_movement_ids, [FACT_IDS[0], FACT_IDS[2]]);
        assert_eq!(embedded_reservation_ids, [FACT_IDS[0], FACT_IDS[2]]);

        assert_inactive_forbidden(
            service
                .stock_movement_list(&movement_params(None, 1, 100), &actor(INACTIVE))
                .await
                .expect_err("inactive actor 必须失败关闭"),
        );

        cleanup(fixture).await;
    });
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn reservation_scope_is_applied_before_page_and_total() {
    require_mongo!(async {
        let (fixture, service) = fixture("stock_reservation_list_auth").await;

        let company_page_1 = service
            .stock_reservation_list(&reservation_params(None, 1, 2), &actor(COMPANY))
            .await
            .expect("Company 预占第一页");
        let company_page_2 = service
            .stock_reservation_list(&reservation_params(None, 2, 2), &actor(COMPANY))
            .await
            .expect("Company 预占第二页");
        assert_eq!(company_page_1.total, 4);
        assert_eq!(company_page_2.total, 4);
        assert_eq!(reservation_ids(&company_page_1.items), [FACT_IDS[0], FACT_IDS[1]]);
        assert_eq!(reservation_ids(&company_page_2.items), [FACT_IDS[2], FACT_IDS[3]]);

        let single_page_1 = service
            .stock_reservation_list(&reservation_params(None, 1, 1), &actor(SINGLE))
            .await
            .expect("A 仓预占第一页");
        let single_page_2 = service
            .stock_reservation_list(&reservation_params(None, 2, 1), &actor(SINGLE))
            .await
            .expect("A 仓预占第二页");
        let single_page_3 = service
            .stock_reservation_list(&reservation_params(None, 3, 1), &actor(SINGLE))
            .await
            .expect("A 仓预占越界页");
        assert_eq!(single_page_1.total, 2);
        assert_eq!(single_page_2.total, 2);
        assert_eq!(single_page_3.total, 2);
        assert_eq!(reservation_ids(&single_page_1.items), [FACT_IDS[0]]);
        assert_eq!(reservation_ids(&single_page_2.items), [FACT_IDS[2]]);
        assert!(single_page_3.items.is_empty());

        let explicit_wrong = service
            .stock_reservation_list(&reservation_params(Some(WAREHOUSE_B), 1, 100), &actor(SINGLE))
            .await
            .expect("显式错误仓库应与授权范围取空交集");
        assert!(explicit_wrong.items.is_empty());
        assert_eq!(explicit_wrong.total, 0);

        let intersection = service
            .stock_reservation_list(&reservation_params(None, 1, 100), &actor(USER_INTERSECTION))
            .await
            .expect("用户范围与 Company 角色范围取交集");
        assert_eq!(intersection.total, 2);
        assert_eq!(reservation_ids(&intersection.items), [FACT_IDS[0], FACT_IDS[2]]);

        let multi = service
            .stock_reservation_list(&reservation_params(None, 1, 100), &actor(MULTI))
            .await
            .expect("同资源多角色 A+B 范围合并");
        assert_eq!(multi.total, 4);
        assert_eq!(reservation_ids(&multi.items), FACT_IDS);

        let reservation_only = service
            .stock_reservation_list(&reservation_params(None, 1, 100), &actor(RESERVATION_ONLY))
            .await
            .expect("reservation-only 角色可读取预占");
        assert_eq!(reservation_only.total, 2);
        assert_eq!(
            reservation_ids(&reservation_only.items),
            [FACT_IDS[1], FACT_IDS[3]]
        );

        let rbac = iam::shared_rbac_service(fixture.db().clone());
        assert!(
            rbac.enforce(
                &subject(AccountKind::Admin, DISABLED),
                &Permission::parse(RESERVATION_PERMISSION).expect("预占权限"),
            )
            .await
            .expect("读取停用角色原始 Casbin 前提"),
            "停用角色的原始 Casbin 授权必须仍为 true"
        );
        assert_reservation_empty(&service, EMPTY_INTERSECTION, "用户与角色范围空交集").await;
        assert_reservation_empty(&service, DISABLED, "停用角色残留策略").await;
        assert_reservation_empty(
            &service,
            NO_SCOPE_BORROW,
            "权限角色无范围不得借无关 Company 角色范围",
        )
        .await;
        assert_reservation_empty(&service, MOVEMENT_ONLY, "流水资源权限不得读取预占").await;
        assert_reservation_empty(&service, OTHER_INVENTORY, "只有其他库存权限").await;

        assert_inactive_forbidden(
            service
                .stock_reservation_list(&reservation_params(None, 1, 100), &actor(INACTIVE))
                .await
                .expect_err("inactive actor 必须失败关闭"),
        );

        cleanup(fixture).await;
    });
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向启用 enableTestCommands 的 MongoDB 副本集"]
async fn movement_policy_revision_drift_fails_closed() {
    require_mongo!(async {
        const COMMAND_APP: &str = "stock-movement-list-policy-drift";

        let (fixture, _service) = fixture("stock_movement_list_policy").await;
        let command_db = app_named_database(&fixture, COMMAND_APP).await;
        let command_service = InventoryService::new(command_db.clone(), iam::shared_rbac_service(command_db));
        let failpoint_before = arm_policy_revision_find(fixture.db(), COMMAND_APP).await;
        let command_task = tokio::spawn(async move {
            command_service
                .stock_movement_list(&movement_params(None, 1, 100), &actor(COMPANY))
                .await
        });
        if !wait_for_policy_revision_find(fixture.db(), failpoint_before).await {
            disable_fail_command(fixture.db()).await;
            let unexpected = command_task.await.expect("等待未挂起的流水命令");
            cleanup(fixture).await;
            panic!("流水列表未命中策略版本门闩: {unexpected:?}");
        }

        let revision = advance_policy_revision(fixture.db(), "movement_list_policy_revision_probe").await;
        disable_fail_command(fixture.db()).await;
        let command_result = command_task.await.expect("等待策略漂移流水命令");
        let (revision_r, revision_r_plus_one) = revision.expect("提交 movement policy R+1");
        assert_eq!(revision_r_plus_one, revision_r + 1);
        assert_policy_revision_error(
            command_result.expect_err("流水列表事务快照 R 与授权快照 R+1 必须失败关闭"),
        );

        cleanup(fixture).await;
    });
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向启用 enableTestCommands 的 MongoDB 副本集"]
async fn reservation_policy_revision_drift_fails_closed() {
    require_mongo!(async {
        const COMMAND_APP: &str = "stock-reservation-list-policy-drift";

        let (fixture, _service) = fixture("stock_reservation_list_policy").await;
        let command_db = app_named_database(&fixture, COMMAND_APP).await;
        let command_service = InventoryService::new(command_db.clone(), iam::shared_rbac_service(command_db));
        let failpoint_before = arm_policy_revision_find(fixture.db(), COMMAND_APP).await;
        let command_task = tokio::spawn(async move {
            command_service
                .stock_reservation_list(&reservation_params(None, 1, 100), &actor(COMPANY))
                .await
        });
        if !wait_for_policy_revision_find(fixture.db(), failpoint_before).await {
            disable_fail_command(fixture.db()).await;
            let unexpected = command_task.await.expect("等待未挂起的预占命令");
            cleanup(fixture).await;
            panic!("预占列表未命中策略版本门闩: {unexpected:?}");
        }

        let revision = advance_policy_revision(fixture.db(), "reservation_list_policy_revision_probe").await;
        disable_fail_command(fixture.db()).await;
        let command_result = command_task.await.expect("等待策略漂移预占命令");
        let (revision_r, revision_r_plus_one) = revision.expect("提交 reservation policy R+1");
        assert_eq!(revision_r_plus_one, revision_r + 1);
        assert_policy_revision_error(
            command_result.expect_err("预占列表事务快照 R 与授权快照 R+1 必须失败关闭"),
        );

        cleanup(fixture).await;
    });
}
