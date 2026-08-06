//! 域 D20 `cost` 仓储集成测试（真实 MongoDB 单节点副本集）。
//!
//! 统一 `#[ignore]` + `ERP_TEST_MONGO_URI` 门控（test-support `require_mongo!`）：
//! 无数据库环境 `cargo test` 跳过；CI 与验收执行
//! `cargo test -p database --test cost_repository -- --include-ignored`。
//! 每个测试使用独立随机数据库名，结束自动 drop（TestDb）。

use database::repository::extensions::CostExt;
use database::{ensure_indexes, NoTransaction, Transactional};
use entities::common::time::Instant;
use entities::cost::{
    CostAllocation, CostAllocationData, CostEntry, CostEntryData, CostScope, CostStage, CostType,
};
use entities::ids::{
    CostAllocationId, CostEntryId, MallConsumptionEntryId, SalesOrderId, SalesOrderLineId, SupplierAccountId,
};
use entities::money::{Amount, Rate};
use mongodb::Database;
use std::str::FromStr;
use test_support::{assert_indexes, require_mongo, TestDb};

/// 成本事实列表筛选条件类型（经 `CostExt` 关联类型跨 crate 可达）。
type CostEntryFilter = <Database as CostExt>::CostEntryFilter;
/// 成本分配列表筛选条件类型。
type CostAllocationFilter = <Database as CostExt>::CostAllocationFilter;

/// 构造可复用的成本事实（gross = net + tax 恒等，税额固定 13.00）。
fn sample_entry(source_no: &str, gross: &str, stage: CostStage) -> CostEntry {
    let gross_amount = Amount::from_str(gross).unwrap();
    let tax_amount = Amount::from_str("13.00").unwrap();
    let net_amount = gross_amount.checked_sub(tax_amount);
    CostEntry::new(
        CostEntryId::new(format!("ce-{source_no}")),
        CostEntryData {
            cost_type: CostType::Product,
            cost_stage: stage,
            cost_scope: CostScope::NonVoucherFulfillment,
            cost_basis: None,
            supplier_id: Some(SupplierAccountId::new("sup-1")),
            gross_amount,
            net_amount,
            tax_amount,
            tax_inclusion: true,
            input_tax_rate: Rate::from_str("0.130000").unwrap(),
            occurred_at: Instant::from_unix_secs(1_700_000_000),
            source_fact_type: "PURCHASE_RECEIPT".to_string(),
            source_document_id: source_no.to_string(),
            source_line_id: format!("{source_no}-L1"),
            source_version: "v1".to_string(),
            adjusts_cost_entry_id: None,
            evidence_attachment_id: None,
        },
    )
    .unwrap()
}

/// 构造可复用的成本分配行（销售单归属）。
fn sample_allocation(entry_id: &CostEntryId, order_no: &str, gross: &str) -> CostAllocation {
    CostAllocation::new(
        CostAllocationId::new(format!("ca-{order_no}")),
        CostAllocationData {
            cost_entry_id: entry_id.clone(),
            sales_order_id: Some(SalesOrderId::new(order_no)),
            sales_order_line_id: Some(SalesOrderLineId::new(format!("{order_no}-l1"))),
            mall_consumption_entry_id: None,
            mall_payment_source_id: None,
            allocated_gross_amount: Amount::from_str(gross).unwrap(),
            allocated_net_amount: Amount::from_str("100.00").unwrap(),
            rounding_residual_flag: false,
        },
    )
    .unwrap()
}

/// 执行 `ensure_indexes` 并断言本域全部必需索引就位。
async fn assert_domain_indexes(db: &Database) {
    ensure_indexes(db).await.expect("ensure_indexes 应成功");
    assert_indexes(
        db,
        <Database as CostExt>::COST_ENTRIES,
        &[
            "uk_cost_entries_identity",
            "idx_cost_entries_profit",
            "idx_cost_entries_stage_time",
            "idx_cost_entries_supplier",
        ],
    )
    .await
    .expect("cost_entries 索引缺失");
    assert_indexes(
        db,
        <Database as CostExt>::COST_ALLOCATIONS,
        &[
            "idx_cost_allocations_entry",
            "idx_cost_allocations_sales_order",
            "idx_cost_allocations_consumption",
        ],
    )
    .await
    .expect("cost_allocations 索引缺失");
}

/// 断言金额 Decimal128 往返保真（原值、小数位逐字一致）。
fn assert_amount_fidelity(actual: Amount, expected: &str) {
    assert_eq!(
        actual.to_decimal(),
        Amount::from_str(expected).unwrap().to_decimal()
    );
    assert_eq!(actual.to_decimal().to_string(), expected);
}

#[tokio::test]
#[ignore]
async fn entry_create_read_roundtrip_preserves_decimal128_and_time() {
    require_mongo!(async {
        let test_db = TestDb::new("cost_crud").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let entry = sample_entry("PR-1", "113.00", CostStage::Actual);
        db.cost_entries()
            .create(&entry, &mut NoTransaction)
            .await
            .unwrap();

        let found = db
            .cost_entries()
            .find_by_id(&entry.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .expect("创建后应可读回");
        assert_amount_fidelity(found.gross_amount, "113.00");
        assert_amount_fidelity(found.net_amount, "100.00");
        assert_amount_fidelity(found.tax_amount, "13.00");
        assert_eq!(found.cost_type, CostType::Product);
        assert_eq!(found.cost_stage, CostStage::Actual);
        assert_eq!(found.cost_scope, CostScope::NonVoucherFulfillment);
        assert_eq!(found.supplier_id, Some(SupplierAccountId::new("sup-1")));
        assert_eq!(found.source_document_id, "PR-1");
        assert_eq!(
            found.occurred_at,
            Instant::from_unix_secs(1_700_000_000),
            "成本发生时间必须往返一致"
        );
    })
}

#[tokio::test]
#[ignore]
async fn entry_identity_conflict_surfaces_duplicate_key() {
    require_mongo!(async {
        let test_db = TestDb::new("cost_dup").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let entry = sample_entry("PR-1", "113.00", CostStage::Actual);
        db.cost_entries()
            .create(&entry, &mut NoTransaction)
            .await
            .unwrap();

        let duplicate = sample_entry("PR-1", "226.00", CostStage::Actual);
        let error = db
            .cost_entries()
            .create(&duplicate, &mut NoTransaction)
            .await
            .expect_err("同一业务幂等键重复写入必须被唯一索引拒绝");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );

        let different_stage = sample_entry("PR-1", "226.00", CostStage::Confirmed);
        db.cost_entries()
            .create(&different_stage, &mut NoTransaction)
            .await
            .expect("同来源不同阶段是独立阶段事实，不得被误判重复");
    })
}

#[tokio::test]
#[ignore]
async fn multi_step_create_commits_and_rolls_back_atomically() {
    require_mongo!(async {
        let test_db = TestDb::new("cost_tx").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let entry = sample_entry("PR-1", "226.00", CostStage::Actual);
        let entry_id: CostEntryId = entry.base.id.clone().into();
        let allocations = vec![
            sample_allocation(&entry_id, "so-1", "113.00"),
            sample_allocation(&entry_id, "so-2", "113.00"),
        ];
        let db_clone = db.clone();
        let entry_for_tx = entry.clone();
        test_db
            .client()
            .with_transaction::<_, (), database::Error>(move |session| {
                Box::pin(async move {
                    db_clone
                        .cost()
                        .create_cost_entry_with_allocations(&entry_for_tx, allocations, session)
                        .await?;
                    Ok(())
                })
            })
            .await
            .expect("事务提交应成功");
        assert!(db
            .cost_entries()
            .find_by_id(&entry.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .is_some());
        assert_eq!(
            db.cost_allocations()
                .find_allocations_by_entries(std::slice::from_ref(&entry_id), &mut NoTransaction)
                .await
                .unwrap()
                .len(),
            2,
            "事务提交后分配行必须全部可见"
        );

        let entry2 = sample_entry("PR-2", "113.00", CostStage::Actual);
        let entry2_id: CostEntryId = entry2.base.id.clone().into();
        let entry2_id_for_tx = entry2_id.clone();
        let db_clone = db.clone();
        let entry_for_tx = entry2.clone();
        let result: std::result::Result<(), database::Error> = test_db
            .client()
            .with_transaction(move |session| {
                Box::pin(async move {
                    db_clone
                        .cost()
                        .create_cost_entry_with_allocations(
                            &entry_for_tx,
                            vec![sample_allocation(&entry2_id_for_tx, "so-3", "113.00")],
                            session,
                        )
                        .await?;
                    Err(database::Error::OptimisticLockingError)
                })
            })
            .await;
        assert!(result.is_err(), "闭包返回错误必须整体回滚");
        assert!(
            db.cost_entries()
                .find_by_id(&entry2.base.id, &mut NoTransaction)
                .await
                .unwrap()
                .is_none(),
            "回滚后成本事实不得残留"
        );
        assert!(
            db.cost_allocations()
                .find_allocations_by_entries(&[entry2_id], &mut NoTransaction)
                .await
                .unwrap()
                .is_empty(),
            "回滚后分配行不得残留"
        );
    })
}

#[tokio::test]
#[ignore]
async fn multi_step_without_transaction_commits_each_write_independently() {
    require_mongo!(async {
        let test_db = TestDb::new("cost_notx").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let entry = sample_entry("PR-1", "226.00", CostStage::Actual);
        let entry_id: CostEntryId = entry.base.id.clone().into();
        let allocations = vec![
            sample_allocation(&entry_id, "so-1", "113.00"),
            sample_allocation(&entry_id, "so-2", "113.00"),
        ];
        db.cost()
            .create_cost_entry_with_allocations(&entry, allocations, &mut NoTransaction)
            .await
            .expect("NoTransaction 下每笔写入各自自动提交");
        assert!(
            db.cost_entries()
                .find_by_id(&entry.base.id, &mut NoTransaction)
                .await
                .unwrap()
                .is_some(),
            "无事务时成本事实自动提交可见（Service 必须传事务保证原子性）"
        );
        assert_eq!(
            db.cost_allocations()
                .find_allocations_by_entries(&[entry_id], &mut NoTransaction)
                .await
                .unwrap()
                .len(),
            2
        );

        let duplicate = sample_entry("PR-1", "226.00", CostStage::Actual);
        let error = db
            .cost()
            .create_cost_entry_with_allocations(
                &duplicate,
                vec![sample_allocation(
                    &duplicate.base.id.clone().into(),
                    "so-9",
                    "113.00",
                )],
                &mut NoTransaction,
            )
            .await
            .expect_err("首笔写入违反业务幂等唯一索引时透出 DuplicateKey");
        assert!(matches!(error, database::Error::DuplicateKey(_)));
    })
}

#[tokio::test]
#[ignore]
async fn list_queries_respect_pagination_sort_whitelist_and_projection() {
    require_mongo!(async {
        let test_db = TestDb::new("cost_list").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        for seq in 1..=3 {
            let mut entry = sample_entry(&format!("PR-2026-{seq}"), "113.00", CostStage::Actual);
            entry.occurred_at = Instant::from_unix_secs(1_700_000_000 + seq);
            db.cost_entries()
                .create(&entry, &mut NoTransaction)
                .await
                .unwrap();
        }
        let expected = sample_entry("PR-2026-9", "113.00", CostStage::Expected);
        db.cost_entries()
            .create(&expected, &mut NoTransaction)
            .await
            .unwrap();

        let filter = CostEntryFilter {
            cost_type: Some(CostType::Product),
            cost_stage: Some(CostStage::Actual),
            cost_scope: None,
            supplier_id: Some(SupplierAccountId::new("sup-1")),
            source_document_id: Some("PR-2026".to_string()),
            page: 2,
            page_size: 2,
            sort_by: Some("occurred_at".to_string()),
            sort_ascending: true,
        };
        let page = db
            .cost_entries()
            .search_cost_entries(&filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page.total, 3, "仅 ACTUAL 阶段 3 条命中");
        assert_eq!(page.items.len(), 1, "第二页只应剩一条");
        assert_eq!(page.items[0].source_document_id, "PR-2026-3");

        let whitelist_fallback = CostEntryFilter {
            cost_type: None,
            cost_stage: None,
            cost_scope: None,
            supplier_id: None,
            source_document_id: None,
            page: 1,
            page_size: 20,
            sort_by: Some("$where".to_string()),
            sort_ascending: false,
        };
        let page = db
            .cost_entries()
            .search_cost_entries(&whitelist_fallback, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page.total, 4, "非白名单排序回退 created_at 降序");
        let row = &page.items[0];
        assert_eq!(row.cost_stage, CostStage::Actual);
        assert_amount_fidelity(row.gross_amount, "113.00");
        assert_eq!(row.cost_type, CostType::Product);
        assert!(row.version >= 1);
        assert!(row.created_at > 0);
    })
}

#[tokio::test]
#[ignore]
async fn allocation_list_filters_by_ownership_and_preserves_amounts() {
    require_mongo!(async {
        let test_db = TestDb::new("cost_alloc_list").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let entry = sample_entry("PR-1", "226.00", CostStage::Actual);
        let entry_id: CostEntryId = entry.base.id.clone().into();
        db.cost_entries()
            .create(&entry, &mut NoTransaction)
            .await
            .unwrap();
        let allocation = sample_allocation(&entry_id, "so-1", "226.00");
        let allocation_id = allocation.base.id.clone();
        db.cost_allocations()
            .create(&allocation, &mut NoTransaction)
            .await
            .unwrap();

        let mall_allocation = CostAllocation::new(
            CostAllocationId::new("ca-mall"),
            CostAllocationData {
                cost_entry_id: entry_id.clone(),
                sales_order_id: None,
                sales_order_line_id: None,
                mall_consumption_entry_id: Some(MallConsumptionEntryId::new("mce-1")),
                mall_payment_source_id: None,
                allocated_gross_amount: Amount::from_str("113.00").unwrap(),
                allocated_net_amount: Amount::from_str("100.00").unwrap(),
                rounding_residual_flag: true,
            },
        )
        .unwrap();
        db.cost_allocations()
            .create(&mall_allocation, &mut NoTransaction)
            .await
            .unwrap();

        let filter = CostAllocationFilter {
            cost_entry_id: Some(entry_id.clone()),
            sales_order_id: None,
            mall_consumption_entry_id: None,
            page: 1,
            page_size: 20,
            sort_by: Some("allocated_gross_amount".to_string()),
            sort_ascending: false,
        };
        let page = db
            .cost_allocations()
            .search_cost_allocations(&filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page.total, 2);
        assert_eq!(page.items.len(), 2);
        let row = &page.items[0];
        assert_eq!(row.cost_entry_id, entry_id.to_string());
        assert_amount_fidelity(row.allocated_gross_amount, "226.00");
        assert_amount_fidelity(row.allocated_net_amount, "100.00");

        let order_filter = CostAllocationFilter {
            cost_entry_id: None,
            sales_order_id: Some(SalesOrderId::new("so-1")),
            mall_consumption_entry_id: None,
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };
        let page = db
            .cost_allocations()
            .search_cost_allocations(&order_filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.items[0].id, allocation_id);

        let mall_filter = CostAllocationFilter {
            cost_entry_id: None,
            sales_order_id: None,
            mall_consumption_entry_id: Some(MallConsumptionEntryId::new("mce-1")),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };
        let page = db
            .cost_allocations()
            .search_cost_allocations(&mall_filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page.total, 1);
        assert!(page.items[0].rounding_residual_flag);
    })
}

#[tokio::test]
#[ignore]
async fn batch_queries_avoid_n_plus_one() {
    require_mongo!(async {
        let test_db = TestDb::new("cost_batch").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let entry1 = sample_entry("PR-1", "113.00", CostStage::Actual);
        let entry1_id: CostEntryId = entry1.base.id.clone().into();
        let entry2 = sample_entry("PR-2", "113.00", CostStage::Actual);
        let entry2_id: CostEntryId = entry2.base.id.clone().into();
        db.cost_entries()
            .create(&entry1, &mut NoTransaction)
            .await
            .unwrap();
        db.cost_entries()
            .create(&entry2, &mut NoTransaction)
            .await
            .unwrap();
        db.cost_allocations()
            .create(
                &sample_allocation(&entry1_id, "so-1", "113.00"),
                &mut NoTransaction,
            )
            .await
            .unwrap();
        db.cost_allocations()
            .create(
                &sample_allocation(&entry2_id, "so-1", "113.00"),
                &mut NoTransaction,
            )
            .await
            .unwrap();

        let by_ids = db
            .cost_entries()
            .find_entries_by_ids(&[entry1_id.clone(), entry2_id.clone()], &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(by_ids.len(), 2, "$in 一次取回，不得 N+1");

        let by_source = db
            .cost_entries()
            .find_entries_by_source("PURCHASE_RECEIPT", "PR-1", &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(by_source.len(), 1);
        assert_eq!(by_source[0].cost_stage, CostStage::Actual);

        let by_entries = db
            .cost_allocations()
            .find_allocations_by_entries(&[entry1_id, entry2_id], &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(by_entries.len(), 2);

        let by_orders = db
            .cost_allocations()
            .find_allocations_by_orders(&[SalesOrderId::new("so-1")], &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(by_orders.len(), 2);
        assert_amount_fidelity(by_orders[0].allocated_gross_amount, "113.00");
    })
}
