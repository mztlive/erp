//! 域 D33 `supplier_settlement` 仓储集成测试（真实 MongoDB 单节点副本集）。
//!
//! 统一 `#[ignore]` + `ERP_TEST_MONGO_URI` 门控（test-support `require_mongo!`）：
//! 无数据库环境 `cargo test` 跳过；CI 与验收执行
//! `cargo test -p database --test supplier_settlement_repository -- --include-ignored`。
//! 每个测试使用独立随机数据库名，结束自动 drop（TestDb）。

use std::str::FromStr;

use database::repository::extensions::SupplierSettlementExt;
use database::{ensure_indexes, NoTransaction, Transactional};
use entities::common::time::BusinessDate;
use entities::ids::{
    PayableAccountId, SupplierAccountId, SupplierFulfillmentItemId, SupplierFulfillmentOrderId,
    SupplierSettlementDifferenceId, SupplierSettlementItemId, SupplierSettlementStatementId,
};
use entities::money::Amount;
use entities::supplier_settlement::{
    SettlementDifferenceStatus, SettlementDifferenceType, SettlementStatus, SupplierSettlementDifference,
    SupplierSettlementDifferenceData, SupplierSettlementItem, SupplierSettlementItemData,
    SupplierSettlementStatement, SupplierSettlementStatementData, SupplierSettlementStatementUpdate,
};
use mongodb::bson::doc;
use mongodb::Database;
use test_support::{assert_indexes, require_mongo, TestDb};

/// 供应商结算单列表筛选条件类型（经 `SupplierSettlementExt` 关联类型跨 crate 可达）。
type SupplierSettlementStatementFilter =
    <Database as SupplierSettlementExt>::SupplierSettlementStatementFilter;
/// 供应商结算明细列表筛选条件类型。
type SupplierSettlementItemFilter = <Database as SupplierSettlementExt>::SupplierSettlementItemFilter;
/// 供应商结算差异列表筛选条件类型。
type SupplierSettlementDifferenceFilter =
    <Database as SupplierSettlementExt>::SupplierSettlementDifferenceFilter;

/// 构造可复用的供应商结算单实体（草稿，差异 = 1023.45 − 1000.00 = 23.45）。
fn sample_statement(no: &str) -> SupplierSettlementStatement {
    SupplierSettlementStatement::new(
        SupplierSettlementStatementId::new(format!("statement-{no}")),
        SupplierSettlementStatementData {
            statement_no: no.to_string(),
            supplier_id: SupplierAccountId::new(format!("supplier-{no}")),
            period_start: BusinessDate::from_ymd(2026, 7, 1).unwrap(),
            period_end: BusinessDate::from_ymd(2026, 7, 31).unwrap(),
            external_bill_no: None,
            external_bill_version: None,
            erp_amount: Amount::from_str("1000.00").unwrap(),
            supplier_amount: Amount::from_str("1023.45").unwrap(),
            status: SettlementStatus::Draft,
            prepared_by: "preparer-a".to_string(),
            reviewed_by: None,
            confirmed_at: None,
            payable_account_id: None,
        },
    )
    .unwrap()
}

/// 构造可复用的结算明细实体（构成恒等：100 + 10 + 5 − 15 = 100）。
fn sample_item(statement_id: &SupplierSettlementStatementId, item_no: &str) -> SupplierSettlementItem {
    SupplierSettlementItem::new(
        SupplierSettlementItemId::new(format!("settlement-item-{item_no}")),
        SupplierSettlementItemData {
            statement_id: statement_id.clone(),
            supplier_fulfillment_order_id: SupplierFulfillmentOrderId::new(format!("order-{item_no}")),
            supplier_fulfillment_item_id: SupplierFulfillmentItemId::new(format!("ff-item-{item_no}")),
            order_amount: Amount::from_str("100.00").unwrap(),
            freight_amount: Amount::from_str("10.00").unwrap(),
            service_fee_amount: Amount::from_str("5.00").unwrap(),
            refund_amount: Amount::from_str("15.00").unwrap(),
            erp_calculated_amount: Amount::from_str("100.00").unwrap(),
            supplier_billed_amount: Amount::from_str("99.50").unwrap(),
        },
    )
    .unwrap()
}

/// 构造可复用的结算差异实体（待处理，无处理结果）。
fn sample_difference(item_id: &SupplierSettlementItemId, diff_no: &str) -> SupplierSettlementDifference {
    SupplierSettlementDifference::new(
        SupplierSettlementDifferenceId::new(format!("difference-{diff_no}")),
        SupplierSettlementDifferenceData {
            statement_item_id: item_id.clone(),
            difference_type: SettlementDifferenceType::Amount,
            difference_amount: Amount::from_str("12.00").unwrap(),
            status: SettlementDifferenceStatus::Pending,
            resolution: None,
            resolved_by: None,
            resolved_at: None,
        },
    )
    .unwrap()
}

/// 构造已确认状态的结算单（同一供应商同一结算范围的重复确认由唯一索引拒绝，§6.20）。
fn sample_confirmed_statement(no: &str) -> SupplierSettlementStatement {
    let mut statement = sample_statement(no);
    statement
        .update(SupplierSettlementStatementUpdate {
            status: Some(SettlementStatus::Confirmed),
            payable_account_id: Some(PayableAccountId::new("payable-account-1")),
            ..Default::default()
        })
        .unwrap();
    statement
}

/// 执行 `ensure_indexes` 并断言本域全部必需索引就位。
async fn assert_domain_indexes(db: &Database) {
    ensure_indexes(db).await.expect("ensure_indexes 应成功");
    assert_indexes(
        db,
        <Database as SupplierSettlementExt>::SUPPLIER_SETTLEMENT_STATEMENTS,
        &[
            "uk_supplier_settlement_statements_statement_no",
            "uk_supplier_settlement_statements_supplier_external_bill",
            "uk_supplier_settlement_statements_supplier_period_confirmed",
            "idx_supplier_settlement_statements_supplier_period_status",
        ],
    )
    .await
    .expect("supplier_settlement_statements 索引缺失");
    assert_indexes(
        db,
        <Database as SupplierSettlementExt>::SUPPLIER_SETTLEMENT_ITEMS,
        &["uk_supplier_settlement_items_statement_fulfillment_item"],
    )
    .await
    .expect("supplier_settlement_items 索引缺失");
    assert_indexes(
        db,
        <Database as SupplierSettlementExt>::SUPPLIER_SETTLEMENT_DIFFERENCES,
        &[
            "idx_supplier_settlement_differences_statement_item",
            "idx_supplier_settlement_differences_status",
        ],
    )
    .await
    .expect("supplier_settlement_differences 索引缺失");
}

#[tokio::test]
#[ignore]
async fn create_statement_with_items_roundtrip_preserves_money() {
    require_mongo!(async {
        let test_db = TestDb::new("sup_set_crud").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let statement = sample_statement("ST-1");
        let statement_id: SupplierSettlementStatementId = statement.base.id.clone().into();
        let item = sample_item(&statement_id, "1");

        let db_clone = db.clone();
        let statement_for_tx = statement.clone();
        let item_for_tx = item.clone();
        test_db
            .client()
            .with_transaction::<_, (), database::Error>(move |session| {
                Box::pin(async move {
                    db_clone
                        .supplier_settlement()
                        .create_statement_with_items(&statement_for_tx, &[item_for_tx], session)
                        .await?;
                    Ok(())
                })
            })
            .await
            .expect("事务提交应成功");

        let found = db
            .supplier_settlement_statements()
            .find_by_statement_no("ST-1", &mut NoTransaction)
            .await
            .unwrap()
            .expect("创建后应可读回");
        assert_eq!(found.statement_no, "ST-1");
        assert_eq!(found.status, SettlementStatus::Draft);
        assert_eq!(found.erp_amount, Amount::from_str("1000.00").unwrap());
        assert_eq!(found.supplier_amount, Amount::from_str("1023.45").unwrap());
        assert_eq!(found.difference_amount, Amount::from_str("23.45").unwrap());

        let filter = SupplierSettlementItemFilter {
            statement_id: Some(statement_id.clone()),
            page: 1,
            page_size: 20,
            sort_by: Some("created_at".to_string()),
            sort_ascending: true,
        };
        let page = db
            .supplier_settlement_items()
            .search_supplier_settlement_items(&filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page.total, 1, "结算单下应只有一条明细");
        let row = &page.items[0];
        assert_eq!(row.order_amount, Amount::from_str("100.00").unwrap());
        assert_eq!(row.erp_calculated_amount, Amount::from_str("100.00").unwrap());
        assert_eq!(row.supplier_billed_amount, Amount::from_str("99.50").unwrap());
        assert_eq!(row.statement_id, statement_id);
    })
}

#[tokio::test]
#[ignore]
async fn update_optimistic_lock_success_and_stale_conflict() {
    require_mongo!(async {
        let test_db = TestDb::new("sup_set_optlock").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let mut statement = sample_statement("ST-2");
        db.supplier_settlement_statements()
            .create(&statement, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(statement.base.version, 1);

        statement
            .update(SupplierSettlementStatementUpdate {
                erp_amount: Some(Amount::from_str("1010.00").unwrap()),
                supplier_amount: Some(Amount::from_str("1030.00").unwrap()),
                status: Some(SettlementStatus::Confirmed),
                reviewed_by: Some("reviewer-b".to_string()),
                payable_account_id: Some(PayableAccountId::new("payable-account-1")),
            })
            .unwrap();
        db.supplier_settlement_statements()
            .update(&mut statement, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(statement.base.version, 2, "乐观锁成功后 version 递增");
        assert_eq!(statement.difference_amount, Amount::from_str("20.00").unwrap());
        assert_eq!(statement.status, SettlementStatus::Confirmed);

        let mut stale = statement.clone();
        db.supplier_settlement_statements()
            .soft_delete(&mut statement, &mut NoTransaction)
            .await
            .unwrap();
        let error = db
            .supplier_settlement_statements()
            .update(&mut stale, &mut NoTransaction)
            .await
            .expect_err("已删除或陈旧 version 更新必须被 CAS 拒绝");
        assert!(
            matches!(error, database::Error::OptimisticLockingError),
            "期望 OptimisticLockingError，实际为 {error:?}"
        );
        assert_eq!(stale.base.version, 2, "CAS 失败不得改动内存版本");
    })
}

#[tokio::test]
#[ignore]
async fn soft_delete_and_restore_statement() {
    require_mongo!(async {
        let test_db = TestDb::new("sup_set_softdel").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let mut statement = sample_statement("ST-3");
        db.supplier_settlement_statements()
            .create(&statement, &mut NoTransaction)
            .await
            .unwrap();

        db.supplier_settlement_statements()
            .soft_delete(&mut statement, &mut NoTransaction)
            .await
            .unwrap();
        let after_delete = db
            .supplier_settlement_statements()
            .find_by_id(&statement.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(after_delete.is_none(), "软删除后按 ID 不可见");

        db.supplier_settlement_statements()
            .restore(&mut statement, &mut NoTransaction)
            .await
            .unwrap();
        let after_restore = db
            .supplier_settlement_statements()
            .find_by_id(&statement.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(after_restore.is_some(), "恢复后按 ID 重新可见");
    })
}

#[tokio::test]
#[ignore]
async fn unique_identities_surface_duplicate_key() {
    require_mongo!(async {
        let test_db = TestDb::new("sup_set_dup").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let statement = sample_statement("ST-4");
        db.supplier_settlement_statements()
            .create(&statement, &mut NoTransaction)
            .await
            .unwrap();

        let duplicate_statement = sample_statement("ST-4");
        let statement_error = db
            .supplier_settlement_statements()
            .create(&duplicate_statement, &mut NoTransaction)
            .await
            .expect_err("重复 statement_no 必须被唯一索引拒绝");
        assert!(
            matches!(statement_error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {statement_error:?}"
        );

        let statement_id: SupplierSettlementStatementId = statement.base.id.clone().into();
        let item = sample_item(&statement_id, "4");
        db.supplier_settlement_items()
            .create(&item, &mut NoTransaction)
            .await
            .unwrap();
        let duplicate_item = sample_item(&statement_id, "4");
        let item_error = db
            .supplier_settlement_items()
            .create(&duplicate_item, &mut NoTransaction)
            .await
            .expect_err("重复 (statement_id, supplier_fulfillment_item_id) 必须被唯一索引拒绝");
        assert!(
            matches!(item_error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {item_error:?}"
        );

        let confirmed = sample_confirmed_statement("ST-5");
        db.supplier_settlement_statements()
            .create(&confirmed, &mut NoTransaction)
            .await
            .unwrap();
        let mut overlapping = sample_confirmed_statement("ST-6");
        overlapping.supplier_id = confirmed.supplier_id.clone();
        let overlap_error = db
            .supplier_settlement_statements()
            .create(&overlapping, &mut NoTransaction)
            .await
            .expect_err("同一供应商同一结算范围的重复确认必须被唯一索引拒绝");
        assert!(
            matches!(overlap_error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {overlap_error:?}"
        );
    })
}

#[tokio::test]
#[ignore]
async fn list_search_respects_filters_pagination_and_projection() {
    require_mongo!(async {
        let test_db = TestDb::new("sup_set_list").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let mut pending = sample_statement("ST-7");
        pending
            .update(SupplierSettlementStatementUpdate {
                status: Some(SettlementStatus::PendingReconciliation),
                ..Default::default()
            })
            .unwrap();
        let mut difference_statement = sample_statement("ST-8");
        difference_statement
            .update(SupplierSettlementStatementUpdate {
                status: Some(SettlementStatus::HasDifference),
                ..Default::default()
            })
            .unwrap();
        let draft = sample_statement("ST-9");
        db.supplier_settlement_statements()
            .create(&pending, &mut NoTransaction)
            .await
            .unwrap();
        db.supplier_settlement_statements()
            .create(&difference_statement, &mut NoTransaction)
            .await
            .unwrap();
        db.supplier_settlement_statements()
            .create(&draft, &mut NoTransaction)
            .await
            .unwrap();

        let statement_id: SupplierSettlementStatementId = draft.base.id.clone().into();
        let item = sample_item(&statement_id, "9");
        db.supplier_settlement_items()
            .create(&item, &mut NoTransaction)
            .await
            .unwrap();
        let item_id: SupplierSettlementItemId = item.base.id.clone().into();
        let difference = sample_difference(&item_id, "9");
        db.supplier_settlement_differences()
            .create(&difference, &mut NoTransaction)
            .await
            .unwrap();

        let statement_filter = SupplierSettlementStatementFilter {
            statement_no: None,
            supplier_id: Some(SupplierAccountId::new("supplier-ST-9")),
            status: Some(SettlementStatus::Draft),
            page: 1,
            page_size: 1,
            sort_by: Some("created_at".to_string()),
            sort_ascending: true,
        };
        let statement_page = db
            .supplier_settlement_statements()
            .search_supplier_settlement_statements(&statement_filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(statement_page.total, 1, "supplier-ST-9 且草稿只有一条");
        assert_eq!(statement_page.items.len(), 1);
        let statement_row = &statement_page.items[0];
        assert_eq!(statement_row.statement_no, "ST-9");
        assert_eq!(statement_row.status, SettlementStatus::Draft);
        assert_eq!(statement_row.erp_amount, Amount::from_str("1000.00").unwrap());
        assert_eq!(
            statement_row.difference_amount,
            Amount::from_str("23.45").unwrap()
        );
        assert_eq!(
            statement_row.period_start,
            BusinessDate::from_ymd(2026, 7, 1).unwrap()
        );
        assert!(statement_row.version >= 1);

        let boundary = SupplierSettlementStatementFilter {
            statement_no: None,
            supplier_id: None,
            status: None,
            page: 2,
            page_size: 2,
            sort_by: Some("period_start".to_string()),
            sort_ascending: true,
        };
        let boundary_page = db
            .supplier_settlement_statements()
            .search_supplier_settlement_statements(&boundary, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(boundary_page.total, 3);
        assert_eq!(boundary_page.items.len(), 1, "第二页（每页 2 条）只剩 1 条");

        let whitelist_sort = SupplierSettlementStatementFilter {
            statement_no: None,
            supplier_id: None,
            status: None,
            page: 1,
            page_size: 20,
            sort_by: Some("prepared_by".to_string()),
            sort_ascending: false,
        };
        let whitelist_page = db
            .supplier_settlement_statements()
            .search_supplier_settlement_statements(&whitelist_sort, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(
            whitelist_page.total, 3,
            "白名单外的排序字段必须回退默认排序而不是报错"
        );

        let difference_filter = SupplierSettlementDifferenceFilter {
            statement_item_id: Some(item_id.clone()),
            status: Some(SettlementDifferenceStatus::Pending),
            page: 1,
            page_size: 20,
            sort_by: Some("difference_amount".to_string()),
            sort_ascending: false,
        };
        let difference_page = db
            .supplier_settlement_differences()
            .search_supplier_settlement_differences(&difference_filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(difference_page.total, 1);
        let difference_row = &difference_page.items[0];
        assert_eq!(difference_row.statement_item_id, item_id);
        assert_eq!(difference_row.difference_type, SettlementDifferenceType::Amount);
        assert_eq!(
            difference_row.difference_amount,
            Amount::from_str("12.00").unwrap()
        );
        assert_eq!(difference_row.status, SettlementDifferenceStatus::Pending);
        assert!(difference_row.resolution.is_none());
    })
}

#[tokio::test]
#[ignore]
async fn multi_step_creation_commits_atomically_inside_transaction() {
    require_mongo!(async {
        let test_db = TestDb::new("sup_set_tx_commit").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let statement = sample_statement("ST-10");
        let statement_id: SupplierSettlementStatementId = statement.base.id.clone().into();
        let item = sample_item(&statement_id, "10");

        let db_clone = db.clone();
        let statement_for_tx = statement.clone();
        let item_for_tx = item.clone();
        test_db
            .client()
            .with_transaction::<_, (), database::Error>(move |session| {
                Box::pin(async move {
                    db_clone
                        .supplier_settlement()
                        .create_statement_with_items(&statement_for_tx, &[item_for_tx], session)
                        .await?;
                    Ok(())
                })
            })
            .await
            .expect("事务提交应成功");

        let statement_found = db
            .supplier_settlement_statements()
            .find_by_statement_no("ST-10", &mut NoTransaction)
            .await
            .unwrap();
        assert!(statement_found.is_some(), "事务提交后结算单必须可见");
        let items = db
            .supplier_settlement_items()
            .find_many(
                doc! { "statement_id": statement_id.to_string() },
                &mut NoTransaction,
            )
            .await
            .unwrap();
        assert_eq!(items.len(), 1, "事务提交后明细必须可见");
    })
}

#[tokio::test]
#[ignore]
async fn multi_step_creation_conflict_rolls_back_whole_creation() {
    require_mongo!(async {
        let test_db = TestDb::new("sup_set_tx_abort").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let statement = sample_statement("ST-11");
        let statement_id: SupplierSettlementStatementId = statement.base.id.clone().into();
        let item = sample_item(&statement_id, "11");
        let db_clone = db.clone();
        let statement_for_tx = statement.clone();
        let item_for_tx = item.clone();
        test_db
            .client()
            .with_transaction::<_, (), database::Error>(move |session| {
                Box::pin(async move {
                    db_clone
                        .supplier_settlement()
                        .create_statement_with_items(&statement_for_tx, &[item_for_tx], session)
                        .await?;
                    Ok(())
                })
            })
            .await
            .expect("首批写入应成功");

        let conflicting = {
            let mut statement = sample_statement("ST-11");
            statement.base.id = "statement-ST-11-conflict".to_string();
            statement
        };
        let conflicting_id: SupplierSettlementStatementId = conflicting.base.id.clone().into();
        let conflicting_item = sample_item(&conflicting_id, "11-conflict");
        let db_clone = db.clone();
        let conflicting_for_tx = conflicting.clone();
        let conflicting_item_for_tx = conflicting_item.clone();
        let result: std::result::Result<(), database::Error> = test_db
            .client()
            .with_transaction(move |session| {
                Box::pin(async move {
                    db_clone
                        .supplier_settlement()
                        .create_statement_with_items(&conflicting_for_tx, &[conflicting_item_for_tx], session)
                        .await?;
                    Ok(())
                })
            })
            .await;
        let error = result.expect_err("重复 statement_no 必须使事务失败");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );

        let statement_after = db
            .supplier_settlement_statements()
            .find_by_statement_no("ST-11", &mut NoTransaction)
            .await
            .unwrap();
        assert!(statement_after.is_some(), "首次提交的结算单不受回滚影响");

        let conflicting_filter = SupplierSettlementItemFilter {
            statement_id: Some(conflicting_id.clone()),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };
        let items_after = db
            .supplier_settlement_items()
            .search_supplier_settlement_items(&conflicting_filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(items_after.total, 0, "回滚后冲突结算单的明细不得残留");
    })
}

#[tokio::test]
#[ignore]
async fn transaction_abort_rolls_back_statement_and_difference() {
    require_mongo!(async {
        let test_db = TestDb::new("sup_set_tx_diff").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let statement = sample_statement("ST-12");
        let statement_id: SupplierSettlementStatementId = statement.base.id.clone().into();
        let item = sample_item(&statement_id, "12");
        db.supplier_settlement_items()
            .create(&item, &mut NoTransaction)
            .await
            .unwrap();
        let item_id: SupplierSettlementItemId = item.base.id.clone().into();
        let difference = sample_difference(&item_id, "12");

        let db_clone = db.clone();
        let statement_for_tx = statement.clone();
        let difference_for_tx = difference.clone();
        let result: std::result::Result<(), database::Error> = test_db
            .client()
            .with_transaction(move |session| {
                Box::pin(async move {
                    db_clone
                        .supplier_settlement_statements()
                        .create(&statement_for_tx, session)
                        .await?;
                    db_clone
                        .supplier_settlement_differences()
                        .create(&difference_for_tx, session)
                        .await?;
                    Err(database::Error::OptimisticLockingError)
                })
            })
            .await;
        assert!(result.is_err(), "闭包返回错误必须整体回滚");

        let statement_found = db
            .supplier_settlement_statements()
            .find_by_statement_no("ST-12", &mut NoTransaction)
            .await
            .unwrap();
        assert!(statement_found.is_none(), "回滚后结算单不得残留");
        let difference_found = db
            .supplier_settlement_differences()
            .find_by_id(&difference.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(difference_found.is_none(), "回滚后差异不得残留");
    })
}
