//! 域 D11 `warehouse` 仓储集成测试（真实 MongoDB 单节点副本集）。
//!
//! 统一 `#[ignore]` + `ERP_TEST_MONGO_URI` 门控（test-support `require_mongo!`）：
//! 无数据库环境 `cargo test` 跳过；CI 与验收执行
//! `cargo test -p database --test warehouse_repository -- --include-ignored`。
//! 每个测试使用独立随机数据库名，结束自动 drop（TestDb）。

use std::str::FromStr;

use database::repository::extensions::WarehouseExt;
use database::{ensure_indexes, NoTransaction, Transactional};
use entities::common::time::BusinessDate;
use entities::ids::{SkuId, WarehouseId, WarehouseRevisionId, WarehouseSkuPolicyId};
use entities::money::Quantity;
use entities::warehouse::{EnableStatus, SensitiveText, Warehouse, WarehouseRevision, WarehouseSkuPolicy};
use mongodb::{bson::doc, Database};
use test_support::{assert_indexes, require_mongo, TestDb};

/// 仓库列表筛选条件类型（经 `WarehouseExt` 关联类型跨 crate 可达）。
type WarehouseFilter = <Database as WarehouseExt>::WarehouseFilter;
/// 仓库修订列表筛选条件类型。
type WarehouseRevisionFilter = <Database as WarehouseExt>::WarehouseRevisionFilter;
/// 仓库-SKU 预警策略列表筛选条件类型。
type WarehouseSkuPolicyFilter = <Database as WarehouseExt>::WarehouseSkuPolicyFilter;

/// 生成合法形态的 64 位十六进制 HMAC 指纹（测试用，非真实密钥派生）。
fn fake_fingerprint(seed: u8) -> String {
    format!("{:02x}", seed).repeat(32)
}

/// 构造可复用的加密敏感值。
fn sample_sensitive_text(seed: u8) -> SensitiveText {
    SensitiveText::new(format!("cipher-{seed}"), fake_fingerprint(seed)).unwrap()
}

/// 执行 `ensure_indexes` 并断言本域全部必需索引就位。
async fn assert_domain_indexes(db: &Database) {
    ensure_indexes(db).await.expect("ensure_indexes 应成功");
    assert_indexes(
        db,
        <Database as WarehouseExt>::WAREHOUSES,
        &["uk_warehouses_warehouse_code", "idx_warehouses_status"],
    )
    .await
    .expect("warehouses 索引缺失");
    assert_indexes(
        db,
        <Database as WarehouseExt>::WAREHOUSE_REVISIONS,
        &[
            "uk_warehouse_revisions_revision",
            "uk_warehouse_revisions_effective_from",
        ],
    )
    .await
    .expect("warehouse_revisions 索引缺失");
    assert_indexes(
        db,
        <Database as WarehouseExt>::WAREHOUSE_SKU_POLICIES,
        &[
            "uk_warehouse_sku_policies_start",
            "idx_warehouse_sku_policies_lookup",
        ],
    )
    .await
    .expect("warehouse_sku_policies 索引缺失");
}

/// 构造可复用的仓库实体。
fn sample_warehouse(id: &str, code: &str) -> Warehouse {
    Warehouse::new(
        WarehouseId::new(id),
        entities::warehouse::warehouse_entity::WarehouseData {
            warehouse_code: code.to_string(),
            status: EnableStatus::Active,
        },
        "admin-1",
    )
    .unwrap()
}

/// 构造可复用的仓库修订实体。
fn sample_warehouse_revision(
    id: &str,
    warehouse_id: &str,
    revision_no: u32,
    effective_from: (i32, u32, u32),
) -> WarehouseRevision {
    WarehouseRevision::new(
        WarehouseRevisionId::new(id),
        entities::warehouse::warehouse_revision::WarehouseRevisionData {
            warehouse_id: WarehouseId::new(warehouse_id),
            revision_no,
            name: format!("仓库修订-{revision_no}"),
            address: sample_sensitive_text(1),
            contact: sample_sensitive_text(2),
            effective_from: BusinessDate::from_ymd(effective_from.0, effective_from.1, effective_from.2)
                .unwrap(),
            effective_to: None,
            change_reason: "期初建仓".to_string(),
        },
    )
    .unwrap()
}

/// 构造可复用的仓库-SKU 预警策略实体。
fn sample_policy(
    id: &str,
    warehouse_id: &str,
    sku_id: &str,
    effective_from: (i32, u32, u32),
) -> WarehouseSkuPolicy {
    WarehouseSkuPolicy::new(
        WarehouseSkuPolicyId::new(id),
        entities::warehouse::warehouse_sku_policy::WarehouseSkuPolicyData {
            warehouse_id: WarehouseId::new(warehouse_id),
            sku_id: SkuId::new(sku_id),
            minimum_available_quantity: Quantity::from_str("10.000000").unwrap(),
            status: EnableStatus::Active,
            effective_from: BusinessDate::from_ymd(effective_from.0, effective_from.1, effective_from.2)
                .unwrap(),
            effective_to: None,
        },
    )
    .unwrap()
}

#[tokio::test]
#[ignore]
async fn warehouse_crud_soft_delete_restore_roundtrip() {
    require_mongo!(async {
        let test_db = TestDb::new("wh_crud").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let mut warehouse = sample_warehouse("wh-1", "WH-BJ-001");
        db.warehouses()
            .create(&warehouse, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(warehouse.base.version, 1);

        let found = db
            .warehouses()
            .find_by_id(&warehouse.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .expect("创建后应可读回");
        assert_eq!(found.warehouse_code, "WH-BJ-001");
        assert_eq!(found.stable.created_by, "admin-1");

        warehouse
            .update(
                entities::warehouse::warehouse_entity::WarehouseUpdate {
                    status: Some(EnableStatus::Disabled),
                },
                "admin-2",
            )
            .unwrap();
        db.warehouses()
            .update(&mut warehouse, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(warehouse.base.version, 2, "乐观锁成功后 version 递增");
        assert!(!warehouse.is_active());

        let mut stale = warehouse.clone();
        warehouse
            .update(
                entities::warehouse::warehouse_entity::WarehouseUpdate {
                    status: Some(EnableStatus::Active),
                },
                "admin-3",
            )
            .unwrap();
        db.warehouses()
            .update(&mut warehouse, &mut NoTransaction)
            .await
            .unwrap();

        stale
            .update(
                entities::warehouse::warehouse_entity::WarehouseUpdate {
                    status: Some(EnableStatus::Active),
                },
                "admin-4",
            )
            .unwrap();
        let error = db
            .warehouses()
            .update(&mut stale, &mut NoTransaction)
            .await
            .expect_err("陈旧 version 更新必须被 CAS 拒绝");
        assert!(
            matches!(error, database::Error::OptimisticLockingError),
            "期望 OptimisticLockingError，实际为 {error:?}"
        );

        db.warehouses()
            .soft_delete(&mut warehouse, &mut NoTransaction)
            .await
            .unwrap();
        let after_delete = db
            .warehouses()
            .find_by_id(&warehouse.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(after_delete.is_none(), "软删除后按 ID 不可见");

        db.warehouses()
            .restore(&mut warehouse, &mut NoTransaction)
            .await
            .unwrap();
        let after_restore = db
            .warehouses()
            .find_by_id(&warehouse.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(after_restore.is_some(), "恢复后按 ID 重新可见");
    })
}

#[tokio::test]
#[ignore]
async fn warehouse_code_unique_conflict_surfaces_duplicate_key() {
    require_mongo!(async {
        let test_db = TestDb::new("wh_dup").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let warehouse = sample_warehouse("wh-1", "WH-BJ-001");
        db.warehouses()
            .create(&warehouse, &mut NoTransaction)
            .await
            .unwrap();

        let duplicate = sample_warehouse("wh-2", "WH-BJ-001");
        let error = db
            .warehouses()
            .create(&duplicate, &mut NoTransaction)
            .await
            .expect_err("重复仓库代码必须被唯一索引拒绝");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );
    })
}

#[tokio::test]
#[ignore]
async fn warehouse_revision_append_only_unique_revision_no() {
    require_mongo!(async {
        let test_db = TestDb::new("wh_rev").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        db.warehouses()
            .create(&sample_warehouse("wh-1", "WH-BJ-001"), &mut NoTransaction)
            .await
            .unwrap();

        let revision = sample_warehouse_revision("rev-1", "wh-1", 1, (2026, 1, 1));
        db.warehouse_revisions()
            .create(&revision, &mut NoTransaction)
            .await
            .unwrap();

        let found = db
            .warehouse_revisions()
            .find_by_id(&revision.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .expect("创建后应可读回");
        assert_eq!(found.revision.revision_no, 1);
        assert_eq!(found.change_reason, "期初建仓");
        assert!(found.is_effective_on(BusinessDate::from_ymd(2026, 6, 1).unwrap()));

        let second = sample_warehouse_revision("rev-2", "wh-1", 2, (2026, 7, 1));
        db.warehouse_revisions()
            .create(&second, &mut NoTransaction)
            .await
            .expect("同仓库递增修订序号应允许");

        let duplicate = sample_warehouse_revision("rev-3", "wh-1", 1, (2026, 8, 1));
        let error = db
            .warehouse_revisions()
            .create(&duplicate, &mut NoTransaction)
            .await
            .expect_err("同仓库同修订序号必须被唯一索引拒绝");
        assert!(matches!(error, database::Error::DuplicateKey(_)));
    })
}

#[tokio::test]
#[ignore]
async fn warehouse_revision_effective_window_overlap_surface_rejected() {
    require_mongo!(async {
        let test_db = TestDb::new("wh_eff").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        db.warehouses()
            .create(&sample_warehouse("wh-1", "WH-BJ-001"), &mut NoTransaction)
            .await
            .unwrap();

        let revision = sample_warehouse_revision("rev-1", "wh-1", 1, (2026, 1, 1));
        db.warehouse_revisions()
            .create(&revision, &mut NoTransaction)
            .await
            .unwrap();

        let overlapping = sample_warehouse_revision("rev-2", "wh-1", 2, (2026, 1, 1));
        let error = db
            .warehouse_revisions()
            .create(&overlapping, &mut NoTransaction)
            .await
            .expect_err("同仓库同生效开始日的重叠窗口必须被唯一索引拒绝");
        assert!(matches!(error, database::Error::DuplicateKey(_)));

        let shifted = sample_warehouse_revision("rev-3", "wh-1", 2, (2026, 7, 1));
        db.warehouse_revisions()
            .create(&shifted, &mut NoTransaction)
            .await
            .expect("不同生效开始日的修订应允许");
    })
}

#[tokio::test]
#[ignore]
async fn warehouse_sku_policy_crud_and_unique_start_day() {
    require_mongo!(async {
        let test_db = TestDb::new("wh_policy").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let mut policy = sample_policy("policy-1", "wh-1", "sku-1", (2026, 1, 1));
        db.warehouse_sku_policies()
            .create(&policy, &mut NoTransaction)
            .await
            .unwrap();

        policy
            .update(
                entities::warehouse::warehouse_sku_policy::WarehouseSkuPolicyUpdate {
                    minimum_available_quantity: Some(Quantity::from_str("5.000000").unwrap()),
                    status: Some(EnableStatus::Disabled),
                },
            )
            .unwrap();
        db.warehouse_sku_policies()
            .update(&mut policy, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(policy.base.version, 2);

        let duplicate = sample_policy("policy-2", "wh-1", "sku-1", (2026, 1, 1));
        let error = db
            .warehouse_sku_policies()
            .create(&duplicate, &mut NoTransaction)
            .await
            .expect_err("同仓库同 SKU 同生效开始日的策略必须被唯一索引拒绝");
        assert!(matches!(error, database::Error::DuplicateKey(_)));

        let other_sku = sample_policy("policy-3", "wh-1", "sku-2", (2026, 1, 1));
        db.warehouse_sku_policies()
            .create(&other_sku, &mut NoTransaction)
            .await
            .expect("不同 SKU 相同生效开始日应允许");
    })
}

#[tokio::test]
#[ignore]
async fn create_warehouse_with_revision_commits_atomically_in_transaction() {
    require_mongo!(async {
        let test_db = TestDb::new("wh_tx_commit").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let warehouse = sample_warehouse("wh-1", "WH-BJ-001");
        let revision = sample_warehouse_revision("rev-1", "wh-1", 1, (2026, 1, 1));

        let db_clone = db.clone();
        let mut warehouse_for_tx = warehouse.clone();
        let revision_for_tx = revision.clone();
        test_db
            .client()
            .with_transaction::<_, (), database::Error>(move |session| {
                Box::pin(async move {
                    db_clone
                        .warehouse()
                        .create_warehouse_with_revision(&mut warehouse_for_tx, &revision_for_tx, session)
                        .await?;
                    Ok(())
                })
            })
            .await
            .expect("事务提交应成功");

        let warehouse_found = db
            .warehouses()
            .find_by_id(&warehouse.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .expect("事务提交后仓库必须可见");
        assert_eq!(
            warehouse_found.stable.current_revision_id.as_deref(),
            Some("rev-1"),
            "当前修订指针必须链入首个修订"
        );
        let revision_found = db
            .warehouse_revisions()
            .find_by_id(&revision.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(revision_found.is_some(), "事务提交后修订必须可见");
    })
}

#[tokio::test]
#[ignore]
async fn create_warehouse_with_revision_rolls_back_whole_on_conflict() {
    require_mongo!(async {
        let test_db = TestDb::new("wh_tx_abort").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let warehouse = sample_warehouse("wh-1", "WH-BJ-001");
        let revision = sample_warehouse_revision("rev-1", "wh-1", 1, (2026, 1, 1));

        let db_clone = db.clone();
        let mut warehouse_for_tx = warehouse.clone();
        let revision_for_tx = revision.clone();
        let warehouse_again = warehouse.clone();
        let result: std::result::Result<(), database::Error> = test_db
            .client()
            .with_transaction(move |session| {
                Box::pin(async move {
                    db_clone
                        .warehouse()
                        .create_warehouse_with_revision(&mut warehouse_for_tx, &revision_for_tx, session)
                        .await?;
                    let _ = db_clone
                        .warehouses()
                        .create(&warehouse_again, session)
                        .await
                        .expect_err("重复仓库代码必须在事务内触发唯一冲突");
                    Ok(())
                })
            })
            .await;
        assert!(result.is_err(), "闭包返回错误必须整体回滚");

        let warehouse_found = db
            .warehouses()
            .find_by_id(&warehouse.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(warehouse_found.is_none(), "回滚后仓库不得残留");
        let revision_found = db
            .warehouse_revisions()
            .find_by_id(&revision.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(revision_found.is_none(), "回滚后修订不得残留");
    })
}

#[tokio::test]
#[ignore]
async fn multi_step_with_no_transaction_commits_parts_independently() {
    require_mongo!(async {
        let test_db = TestDb::new("wh_ntx").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let mut warehouse = sample_warehouse("wh-1", "WH-BJ-001");
        let revision = sample_warehouse_revision("rev-1", "wh-1", 1, (2026, 1, 1));
        db.warehouse()
            .create_warehouse_with_revision(&mut warehouse, &revision, &mut NoTransaction)
            .await
            .expect("NoTransaction 下多步写入各自自动提交");

        let warehouse_found = db
            .warehouses()
            .find_by_id(&warehouse.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(warehouse_found.is_some(), "非事务执行器下第一步写入已提交");
        let revision_found = db
            .warehouse_revisions()
            .find_by_id(&revision.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(revision_found.is_some(), "非事务执行器下第二步写入已提交");
    })
}

#[tokio::test]
#[ignore]
async fn warehouse_list_pagination_projection_and_sort_whitelist() {
    require_mongo!(async {
        let test_db = TestDb::new("wh_list").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let mut disabled = sample_warehouse("wh-2", "WH-SH-002");
        disabled.stable.status = EnableStatus::Disabled;
        db.warehouses()
            .create(&sample_warehouse("wh-1", "WH-BJ-001"), &mut NoTransaction)
            .await
            .unwrap();
        db.warehouses()
            .create(&disabled, &mut NoTransaction)
            .await
            .unwrap();
        db.warehouses()
            .create(&sample_warehouse("wh-3", "WH-GZ-003"), &mut NoTransaction)
            .await
            .unwrap();

        let filter = WarehouseFilter {
            warehouse_code: None,
            status: Some(EnableStatus::Active),
            page: 1,
            page_size: 1,
            sort_by: Some("warehouse_code".to_string()),
            sort_ascending: true,
        };
        let page = db
            .warehouses()
            .search_warehouses(&filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page.total, 2, "启用仓库共两条");
        assert_eq!(page.items.len(), 1);
        let row = &page.items[0];
        assert_eq!(row.warehouse_code, "WH-BJ-001");
        assert_eq!(row.status, EnableStatus::Active);
        assert!(row.version >= 1);
        assert!(row.created_at > 0);

        let page_two = WarehouseFilter { page: 2, ..filter };
        let second = db
            .warehouses()
            .search_warehouses(&page_two, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(second.items.len(), 1, "分页边界：第二页 1 条");
        assert_eq!(second.items[0].warehouse_code, "WH-GZ-003");

        let exact = WarehouseFilter {
            warehouse_code: Some("WH-SH-002".to_string()),
            status: None,
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };
        let hit = db
            .warehouses()
            .search_warehouses(&exact, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(hit.total, 1);
        assert_eq!(hit.items[0].status, EnableStatus::Disabled);
    })
}

#[tokio::test]
#[ignore]
async fn warehouse_revision_list_excludes_sensitive_fields() {
    require_mongo!(async {
        let test_db = TestDb::new("wh_proj").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let mut warehouse = sample_warehouse("wh-1", "WH-BJ-001");
        let revision = sample_warehouse_revision("rev-1", "wh-1", 1, (2026, 1, 1));
        db.warehouse()
            .create_warehouse_with_revision(&mut warehouse, &revision, &mut NoTransaction)
            .await
            .unwrap();

        let filter = WarehouseRevisionFilter {
            warehouse_id: Some("wh-1".to_string()),
            name: None,
            page: 1,
            page_size: 20,
            sort_by: Some("revision_no".to_string()),
            sort_ascending: false,
        };
        let page = db
            .warehouse_revisions()
            .search_warehouse_revisions(&filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page.total, 1);
        let row = &page.items[0];
        assert_eq!(row.name, "仓库修订-1");
        assert_eq!(row.revision_no, 1);
        assert_eq!(row.change_reason, "期初建仓");

        let raw = db
            .collection::<mongodb::bson::Document>(<Database as WarehouseExt>::WAREHOUSE_REVISIONS)
            .find_one(doc! { "id": "rev-1" })
            .await
            .unwrap()
            .expect("原始文档应存在");
        assert!(!raw.contains_key("encrypted"), "列表投影不得携带加密列密文");
        assert!(!raw.contains_key("fingerprint"), "列表投影不得携带查询指纹");
    })
}

#[tokio::test]
#[ignore]
async fn warehouse_sku_policy_list_filters_and_pagination() {
    require_mongo!(async {
        let test_db = TestDb::new("wh_policy_list").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        db.warehouse_sku_policies()
            .create(
                &sample_policy("policy-1", "wh-1", "sku-1", (2026, 1, 1)),
                &mut NoTransaction,
            )
            .await
            .unwrap();
        db.warehouse_sku_policies()
            .create(
                &sample_policy("policy-2", "wh-1", "sku-2", (2026, 3, 1)),
                &mut NoTransaction,
            )
            .await
            .unwrap();
        db.warehouse_sku_policies()
            .create(
                &sample_policy("policy-3", "wh-2", "sku-1", (2026, 5, 1)),
                &mut NoTransaction,
            )
            .await
            .unwrap();

        let filter = WarehouseSkuPolicyFilter {
            warehouse_id: Some("wh-1".to_string()),
            sku_id: None,
            status: None,
            page: 1,
            page_size: 1,
            sort_by: Some("effective_from".to_string()),
            sort_ascending: true,
        };
        let page = db
            .warehouse_sku_policies()
            .search_warehouse_sku_policies(&filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page.total, 2, "wh-1 下共两条策略");
        assert_eq!(page.items.len(), 1);
        let row = &page.items[0];
        assert_eq!(row.sku_id, "sku-1");
        assert_eq!(
            row.minimum_available_quantity,
            Quantity::from_str("10.000000").unwrap(),
            "Decimal128 预警阈值往返一致"
        );

        let sku_filter = WarehouseSkuPolicyFilter {
            warehouse_id: None,
            sku_id: Some("sku-1".to_string()),
            status: Some(EnableStatus::Active),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };
        let sku_page = db
            .warehouse_sku_policies()
            .search_warehouse_sku_policies(&sku_filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(sku_page.total, 2, "sku-1 下共两条策略");
    })
}

#[tokio::test]
#[ignore]
async fn transaction_abort_rolls_back_policy_and_warehouse_writes() {
    require_mongo!(async {
        let test_db = TestDb::new("wh_tx_rollback").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let warehouse = sample_warehouse("wh-1", "WH-BJ-001");
        let policy = sample_policy("policy-1", "wh-1", "sku-1", (2026, 1, 1));

        let db_clone = db.clone();
        let warehouse_for_tx = warehouse.clone();
        let policy_for_tx = policy.clone();
        let result: std::result::Result<(), database::Error> = test_db
            .client()
            .with_transaction(move |session| {
                Box::pin(async move {
                    db_clone.warehouses().create(&warehouse_for_tx, session).await?;
                    db_clone
                        .warehouse_sku_policies()
                        .create(&policy_for_tx, session)
                        .await?;
                    Err(database::Error::OptimisticLockingError)
                })
            })
            .await;
        assert!(result.is_err(), "闭包返回错误必须整体回滚");

        let warehouse_found = db
            .warehouses()
            .find_by_id(&warehouse.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(warehouse_found.is_none(), "回滚后仓库不得残留");
        let policy_found = db
            .warehouse_sku_policies()
            .find_by_id(&policy.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(policy_found.is_none(), "回滚后策略不得残留");
    })
}
