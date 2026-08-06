//! 域 D30 `mall_after_sales` 仓储集成测试（真实 MongoDB 单节点副本集）。
//!
//! 统一 `#[ignore]` + `ERP_TEST_MONGO_URI` 门控（test-support `require_mongo!`）：
//! 无数据库环境 `cargo test` 跳过；CI 与验收执行
//! `cargo test -p database --test mall_after_sales_repository -- --include-ignored`。
//! 每个测试使用独立随机数据库名，结束自动 drop（TestDb）。
//!
//! 已知限制（P1 冻结实体缺陷，待地基修订，本域测试矩阵不覆盖头表持久化）：
//! - `MallAfterSalesRequest` 同时声明扁平化的 `BaseModel.created_at` 与同名域字段
//!   `created_at: Instant`（商城申请时间），serde 无法往返（序列化覆盖/反序列化
//!   报「duplicate/missing field created_at」），`mall_after_sales_request` 头表
//!   暂不可持久化；
//! - `MallBalanceRestoration` 同时声明扁平化的 `BaseModel.version` 与同名域字段
//!   `version: String`（恢复身份版本），serde 无法往返（反序列化报
//!   「invalid type: integer 1, expected a string」），`mall_balance_restoration`
//!   头表暂不可持久化。
//!
//! 本域测试矩阵由 `mall_after_sales_request_line`（非事实，完整 CRUD/乐观锁/
//! 软删除/唯一冲突/列表）、退款头/行/分配（唯一冲突、事务参与、多步骤）与
//! 余额恢复分配（唯一冲突、列表）覆盖；两个头表待实体修订（域字段改名）后补充。

use database::repository::extensions::MallAfterSalesExt;
use database::{ensure_indexes, NoTransaction, Transactional};
use entities::common::time::Instant;
use entities::ids::{
    MallAfterSalesRequestLineId, MallBalanceRestorationAllocationId, MallBalanceRestorationId,
    MallCardInstanceId, MallConsumptionEntryId, MallOrderFactId, MallOrderId, MallOrderItemId,
    MallPaymentSourceId, MallRefundAllocationId, MallRefundId, MallRefundLineId, SupplierFulfillmentItemId,
};
use entities::mall_after_sales::{
    AfterSalesLineStatus, AllocationAction, MallAfterSalesRequestLine, MallAfterSalesRequestLineData,
    MallBalanceRestorationAllocation, MallBalanceRestorationAllocationData, MallRefund, MallRefundAllocation,
    MallRefundAllocationData, MallRefundData, MallRefundLine, MallRefundLineData,
};
use entities::money::{Amount, Quantity};
use mongodb::Database;
use std::str::FromStr;
use test_support::{assert_indexes, require_mongo, TestDb};

/// 构造可复用的售后明细行实体。
fn sample_request_line(id: &str, request_id: &str, line_no: u32) -> MallAfterSalesRequestLine {
    MallAfterSalesRequestLine::new(
        MallAfterSalesRequestLineId::new(id),
        MallAfterSalesRequestLineData {
            after_sales_request_id: entities::ids::MallAfterSalesRequestId::new(request_id),
            line_no,
            mall_order_item_id: MallOrderItemId::new(format!("item-{line_no}")),
            supplier_fulfillment_item_id: Some(SupplierFulfillmentItemId::new("sfi-1")),
            requested_quantity: Quantity::from_str("1.000000").unwrap(),
            requested_amount: Amount::from_str("49.00").unwrap(),
            line_status: AfterSalesLineStatus::Pending,
        },
    )
    .unwrap()
}

/// 构造可复用的退款头实体。
fn sample_refund(id: &str, fact_id: &str) -> MallRefund {
    MallRefund::new(
        MallRefundId::new(id),
        MallRefundData {
            mall_order_fact_id: MallOrderFactId::new(fact_id),
            after_sales_request_id: entities::ids::MallAfterSalesRequestId::new("asr-1"),
            mall_id: " mall-a ".to_string(),
            external_refund_no: format!(" rn-{id} "),
            external_refund_version: " v1 ".to_string(),
            mall_order_id: MallOrderId::new("order-1"),
            refund_amount: Amount::from_str("49.00").unwrap(),
            refunded_at: Instant::from_unix_secs(1_700_000_200),
        },
    )
    .unwrap()
}

/// 构造可复用的退款行实体。
fn sample_refund_line(id: &str, refund_id: &str, line_no: u32) -> MallRefundLine {
    MallRefundLine::new(
        MallRefundLineId::new(id),
        MallRefundLineData {
            mall_refund_id: MallRefundId::new(refund_id),
            line_no,
            mall_order_item_id: MallOrderItemId::new("item-1"),
            refunded_quantity: Quantity::from_str("1.000000").unwrap(),
            line_refund_amount: Amount::from_str("49.00").unwrap(),
        },
    )
    .unwrap()
}

/// 构造可复用的退款分配实体（`APPLY`）。
fn sample_apply_allocation(id: &str, line_id: &str, allocation_no: u32) -> MallRefundAllocation {
    MallRefundAllocation::new(
        MallRefundAllocationId::new(id),
        MallRefundAllocationData {
            mall_refund_line_id: MallRefundLineId::new(line_id),
            allocation_no,
            original_consumption_entry_id: MallConsumptionEntryId::new("ce-1"),
            original_payment_source_id: MallPaymentSourceId::new("ps-1"),
            allocated_refund_amount: Amount::from_str("49.00").unwrap(),
            allocation_action: AllocationAction::Apply,
            reverses_allocation_id: None,
            reversal_consumption_entry_id: None,
        },
    )
    .unwrap()
}

/// 构造可复用的余额恢复分配实体。
fn sample_restoration_allocation(id: &str, restoration_id: &str) -> MallBalanceRestorationAllocation {
    MallBalanceRestorationAllocation::new(
        MallBalanceRestorationAllocationId::new(id),
        MallBalanceRestorationAllocationData {
            mall_balance_restoration_id: MallBalanceRestorationId::new(restoration_id),
            allocation_no: 1,
            mall_refund_allocation_id: MallRefundAllocationId::new(format!("ra-{id}")),
            mall_card_instance_id: MallCardInstanceId::new("card-1"),
            restored_amount: Amount::from_str("49.00").unwrap(),
        },
    )
    .unwrap()
}

/// 执行 `ensure_indexes` 并断言本域全部必需索引就位。
async fn assert_domain_indexes(db: &Database) {
    ensure_indexes(db).await.expect("ensure_indexes 应成功");
    assert_indexes(
        db,
        <Database as MallAfterSalesExt>::MALL_AFTER_SALES_REQUESTS,
        &[
            "uk_mall_after_sales_requests_identity",
            "idx_mall_after_sales_requests_order_status",
        ],
    )
    .await
    .expect("mall_after_sales_requests 索引缺失");
    assert_indexes(
        db,
        <Database as MallAfterSalesExt>::MALL_AFTER_SALES_REQUEST_LINES,
        &[
            "uk_mall_after_sales_request_lines_no",
            "uk_mall_after_sales_request_lines_item",
            "idx_mall_after_sales_request_lines_item_status",
        ],
    )
    .await
    .expect("mall_after_sales_request_lines 索引缺失");
    assert_indexes(
        db,
        <Database as MallAfterSalesExt>::MALL_REFUNDS,
        &[
            "uk_mall_refunds_fact",
            "uk_mall_refunds_identity",
            "idx_mall_refunds_after_sales_request",
        ],
    )
    .await
    .expect("mall_refunds 索引缺失");
    assert_indexes(
        db,
        <Database as MallAfterSalesExt>::MALL_REFUND_LINES,
        &["uk_mall_refund_lines_no", "uk_mall_refund_lines_item"],
    )
    .await
    .expect("mall_refund_lines 索引缺失");
    assert_indexes(
        db,
        <Database as MallAfterSalesExt>::MALL_REFUND_ALLOCATIONS,
        &[
            "uk_mall_refund_allocations_no",
            "uk_mall_refund_allocations_reverses",
            "idx_mall_refund_allocations_consumption",
        ],
    )
    .await
    .expect("mall_refund_allocations 索引缺失");
    assert_indexes(
        db,
        <Database as MallAfterSalesExt>::MALL_BALANCE_RESTORATIONS,
        &[
            "uk_mall_balance_restorations_fact",
            "uk_mall_balance_restorations_identity",
            "idx_mall_balance_restorations_after_sales_request",
        ],
    )
    .await
    .expect("mall_balance_restorations 索引缺失");
    assert_indexes(
        db,
        <Database as MallAfterSalesExt>::MALL_BALANCE_RESTORATION_ALLOCATIONS,
        &[
            "uk_mall_balance_restoration_allocations_no",
            "idx_mall_balance_restoration_allocations_refund",
        ],
    )
    .await
    .expect("mall_balance_restoration_allocations 索引缺失");
}

#[tokio::test]
#[ignore]
async fn request_lines_crud_optimistic_lock_soft_delete_restore_roundtrip() {
    require_mongo!(async {
        let test_db = TestDb::new("mas_lines_crud").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let mut line = sample_request_line("asrl-1", "asr-1", 1);
        db.mall_after_sales_request_lines()
            .create(&line, &mut NoTransaction)
            .await
            .unwrap();

        let found = db
            .mall_after_sales_request_lines()
            .find_by_id(&line.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .expect("创建后应可读回");
        assert_eq!(found.line_no, 1);
        assert_eq!(found.requested_amount, Amount::from_str("49.00").unwrap());
        assert_eq!(found.line_status, AfterSalesLineStatus::Pending);

        let mut stale = line.clone();
        line.line_status = AfterSalesLineStatus::SupplierAccepted;
        db.mall_after_sales_request_lines()
            .update(&mut line, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(line.base.version, 2, "乐观锁成功后 version 递增");

        stale.line_status = AfterSalesLineStatus::Completed;
        let error = db
            .mall_after_sales_request_lines()
            .update(&mut stale, &mut NoTransaction)
            .await
            .expect_err("陈旧 version 更新必须被 CAS 拒绝");
        assert!(
            matches!(error, database::Error::OptimisticLockingError),
            "期望 OptimisticLockingError，实际为 {error:?}"
        );
        assert_eq!(stale.base.version, 1, "CAS 失败不得改动内存版本");

        db.mall_after_sales_request_lines()
            .soft_delete(&mut line, &mut NoTransaction)
            .await
            .unwrap();
        assert!(
            db.mall_after_sales_request_lines()
                .find_by_id(&line.base.id, &mut NoTransaction)
                .await
                .unwrap()
                .is_none(),
            "软删除后按 ID 不可见"
        );

        db.mall_after_sales_request_lines()
            .restore(&mut line, &mut NoTransaction)
            .await
            .unwrap();
        assert!(
            db.mall_after_sales_request_lines()
                .find_by_id(&line.base.id, &mut NoTransaction)
                .await
                .unwrap()
                .is_some(),
            "恢复后按 ID 重新可见"
        );
    })
}

#[tokio::test]
#[ignore]
async fn request_lines_list_and_uniqueness() {
    require_mongo!(async {
        let test_db = TestDb::new("mas_lines_list").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let line_1 = sample_request_line("asrl-1", "asr-1", 1);
        db.mall_after_sales_request_lines()
            .create(&line_1, &mut NoTransaction)
            .await
            .unwrap();
        let line_2 = sample_request_line("asrl-2", "asr-1", 2);
        db.mall_after_sales_request_lines()
            .create(&line_2, &mut NoTransaction)
            .await
            .unwrap();

        let lines = db
            .mall_after_sales_request_lines()
            .list_by_request(
                &entities::ids::MallAfterSalesRequestId::new("asr-1"),
                &mut NoTransaction,
            )
            .await
            .unwrap();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].line_no, 1);
        assert_eq!(lines[1].line_no, 2);
        assert_eq!(lines[0].requested_amount, Amount::from_str("49.00").unwrap());

        let duplicate_no = sample_request_line("asrl-3", "asr-1", 2);
        let error = db
            .mall_after_sales_request_lines()
            .create(&duplicate_no, &mut NoTransaction)
            .await
            .expect_err("同申请重复行号必须被唯一索引拒绝");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );

        let duplicate_item = MallAfterSalesRequestLine::new(
            MallAfterSalesRequestLineId::new("asrl-4"),
            MallAfterSalesRequestLineData {
                line_no: 5,
                ..MallAfterSalesRequestLineData {
                    after_sales_request_id: entities::ids::MallAfterSalesRequestId::new("asr-1"),
                    line_no: 5,
                    mall_order_item_id: MallOrderItemId::new("item-1"),
                    supplier_fulfillment_item_id: None,
                    requested_quantity: Quantity::from_str("1.000000").unwrap(),
                    requested_amount: Amount::from_str("49.00").unwrap(),
                    line_status: AfterSalesLineStatus::Pending,
                }
            },
        )
        .unwrap();
        let error = db
            .mall_after_sales_request_lines()
            .create(&duplicate_item, &mut NoTransaction)
            .await
            .expect_err("同申请重复商品明细必须被唯一索引拒绝");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );
    })
}

#[tokio::test]
#[ignore]
async fn refund_head_line_and_allocation_roundtrip() {
    require_mongo!(async {
        let test_db = TestDb::new("mas_refund_crud").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let refund = sample_refund("refund-1", "fact-2");
        db.mall_refunds()
            .create(&refund, &mut NoTransaction)
            .await
            .unwrap();

        let found = db
            .mall_refunds()
            .find_by_id(&refund.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .expect("创建后应可读回");
        assert_eq!(found.external_refund_no, "rn-refund-1");
        assert_eq!(found.refund_amount, Amount::from_str("49.00").unwrap());
        assert_eq!(found.mall_order_fact_id, MallOrderFactId::new("fact-2"));
        assert_eq!(
            found.after_sales_request_id,
            entities::ids::MallAfterSalesRequestId::new("asr-1")
        );

        let by_fact = db
            .mall_refunds()
            .find_by_fact_id(&MallOrderFactId::new("fact-2"), &mut NoTransaction)
            .await
            .unwrap()
            .expect("按事实应命中");
        assert_eq!(by_fact.base.id, "refund-1");

        let by_identity = db
            .mall_refunds()
            .find_by_identity("mall-a", "rn-refund-1", "v1", &mut NoTransaction)
            .await
            .unwrap()
            .expect("按商城退款身份应命中");
        assert_eq!(by_identity.base.id, "refund-1");

        let line = sample_refund_line("rl-1", "refund-1", 1);
        db.mall_refund_lines()
            .create(&line, &mut NoTransaction)
            .await
            .unwrap();
        let allocation = sample_apply_allocation("ra-1", "rl-1", 1);
        db.mall_refund_allocations()
            .create(&allocation, &mut NoTransaction)
            .await
            .unwrap();

        let lines = db
            .mall_refund_lines()
            .list_by_refund(&MallRefundId::new("refund-1"), &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].line_refund_amount, Amount::from_str("49.00").unwrap());

        let allocations = db
            .mall_refund_allocations()
            .list_by_lines(&[MallRefundLineId::new("rl-1")], &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(allocations.len(), 1);
        assert_eq!(allocations[0].allocation_action, AllocationAction::Apply);
        assert_eq!(
            allocations[0].original_consumption_entry_id,
            MallConsumptionEntryId::new("ce-1")
        );
        assert!(
            db.mall_refund_allocations()
                .list_by_consumption(&MallConsumptionEntryId::new("ce-1"), &mut NoTransaction)
                .await
                .unwrap()
                .len()
                == 1,
            "按原消费可追溯分配"
        );
    })
}

#[tokio::test]
#[ignore]
async fn refund_unique_conflicts_surface_duplicate_key() {
    require_mongo!(async {
        let test_db = TestDb::new("mas_refund_dup").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let refund = sample_refund("refund-1", "fact-2");
        db.mall_refunds()
            .create(&refund, &mut NoTransaction)
            .await
            .unwrap();

        let duplicate_fact = sample_refund("refund-2", "fact-2");
        let error = db
            .mall_refunds()
            .create(&duplicate_fact, &mut NoTransaction)
            .await
            .expect_err("同一事实的第二份退款头必须被唯一索引拒绝");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );

        let mut duplicate_identity = sample_refund("refund-3", "fact-3");
        duplicate_identity.external_refund_no = "rn-refund-1".to_string();
        let error = db
            .mall_refunds()
            .create(&duplicate_identity, &mut NoTransaction)
            .await
            .expect_err("同一商城退款身份重复必须被唯一索引拒绝");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );

        let line = sample_refund_line("rl-1", "refund-1", 1);
        db.mall_refund_lines()
            .create(&line, &mut NoTransaction)
            .await
            .unwrap();
        let duplicate_line_no = sample_refund_line("rl-2", "refund-1", 1);
        let error = db
            .mall_refund_lines()
            .create(&duplicate_line_no, &mut NoTransaction)
            .await
            .expect_err("同退款重复行号必须被唯一索引拒绝");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );

        let allocation = sample_apply_allocation("ra-1", "rl-1", 1);
        db.mall_refund_allocations()
            .create(&allocation, &mut NoTransaction)
            .await
            .unwrap();
        let duplicate_allocation_no = sample_apply_allocation("ra-2", "rl-1", 1);
        let error = db
            .mall_refund_allocations()
            .create(&duplicate_allocation_no, &mut NoTransaction)
            .await
            .expect_err("同退款行重复分配序号必须被唯一索引拒绝");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );
    })
}

#[tokio::test]
#[ignore]
async fn balance_restoration_allocation_roundtrip_and_uniqueness() {
    require_mongo!(async {
        let test_db = TestDb::new("mas_restoration").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let allocation = sample_restoration_allocation("bra-1", "br-1");
        db.mall_balance_restoration_allocations()
            .create(&allocation, &mut NoTransaction)
            .await
            .unwrap();

        let found = db
            .mall_balance_restoration_allocations()
            .find_by_id(&allocation.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .expect("创建后应可读回");
        assert_eq!(found.mall_card_instance_id, MallCardInstanceId::new("card-1"));
        assert_eq!(
            found.mall_refund_allocation_id,
            MallRefundAllocationId::new("ra-bra-1")
        );
        assert_eq!(found.restored_amount, Amount::from_str("49.00").unwrap());

        let duplicate_allocation_no = sample_restoration_allocation("bra-2", "br-1");
        let error = db
            .mall_balance_restoration_allocations()
            .create(&duplicate_allocation_no, &mut NoTransaction)
            .await
            .expect_err("同恢复头重复分配序号必须被唯一索引拒绝");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );

        let other_restoration = sample_restoration_allocation("bra-3", "br-2");
        db.mall_balance_restoration_allocations()
            .create(&other_restoration, &mut NoTransaction)
            .await
            .unwrap();

        let allocations = db
            .mall_balance_restoration_allocations()
            .list_by_restoration(&MallBalanceRestorationId::new("br-1"), &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(allocations.len(), 1, "按恢复头取回分配");
        assert!(
            db.mall_balance_restoration_allocations()
                .list_by_refund_allocation(&MallRefundAllocationId::new("ra-bra-1"), &mut NoTransaction)
                .await
                .unwrap()
                .len()
                == 1,
            "按原 CARD 退款分配可追溯恢复分配"
        );
    })
}

#[tokio::test]
#[ignore]
async fn refund_aggregate_commits_atomically_inside_transaction() {
    require_mongo!(async {
        let test_db = TestDb::new("mas_tx_commit").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let refund = sample_refund("refund-1", "fact-2");
        let line = sample_refund_line("rl-1", "refund-1", 1);
        let allocation = sample_apply_allocation("ra-1", "rl-1", 1);

        let db_clone = db.clone();
        let refund_for_tx = refund.clone();
        let line_for_tx = line.clone();
        let allocation_for_tx = allocation.clone();
        test_db
            .client()
            .with_transaction::<_, (), database::Error>(move |session| {
                Box::pin(async move {
                    db_clone
                        .mall_after_sales()
                        .create_refund_with_lines_and_allocations(
                            &refund_for_tx,
                            &[line_for_tx],
                            &[allocation_for_tx],
                            session,
                        )
                        .await?;
                    Ok(())
                })
            })
            .await
            .expect("事务提交应成功");

        assert!(
            db.mall_refunds()
                .find_by_id(&refund.base.id, &mut NoTransaction)
                .await
                .unwrap()
                .is_some(),
            "事务提交后退款头必须可见"
        );
        assert!(
            db.mall_refund_lines()
                .find_by_id(&line.base.id, &mut NoTransaction)
                .await
                .unwrap()
                .is_some(),
            "事务提交后退款行必须可见"
        );
        assert!(
            db.mall_refund_allocations()
                .find_by_id(&allocation.base.id, &mut NoTransaction)
                .await
                .unwrap()
                .is_some(),
            "事务提交后分配必须可见"
        );
    })
}

#[tokio::test]
#[ignore]
async fn transaction_abort_rolls_back_all_collections() {
    require_mongo!(async {
        let test_db = TestDb::new("mas_tx_abort").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let refund = sample_refund("refund-1", "fact-2");
        let line = sample_refund_line("rl-1", "refund-1", 1);
        let allocation = sample_apply_allocation("ra-1", "rl-1", 1);

        let db_clone = db.clone();
        let refund_for_tx = refund.clone();
        let line_for_tx = line.clone();
        let allocation_for_tx = allocation.clone();
        let result: std::result::Result<(), database::Error> = test_db
            .client()
            .with_transaction(move |session| {
                Box::pin(async move {
                    db_clone
                        .mall_after_sales()
                        .create_refund_with_lines_and_allocations(
                            &refund_for_tx,
                            &[line_for_tx],
                            &[allocation_for_tx],
                            session,
                        )
                        .await?;
                    Err(database::Error::OptimisticLockingError)
                })
            })
            .await;
        assert!(result.is_err(), "闭包返回错误必须整体回滚");

        assert!(
            db.mall_refunds()
                .find_by_id(&refund.base.id, &mut NoTransaction)
                .await
                .unwrap()
                .is_none(),
            "回滚后退款头不得残留"
        );
        assert!(
            db.mall_refund_lines()
                .find_by_id(&line.base.id, &mut NoTransaction)
                .await
                .unwrap()
                .is_none(),
            "回滚后退款行不得残留"
        );
        assert!(
            db.mall_refund_allocations()
                .find_by_id(&allocation.base.id, &mut NoTransaction)
                .await
                .unwrap()
                .is_none(),
            "回滚后分配不得残留"
        );
    })
}

#[tokio::test]
#[ignore]
async fn multi_step_conflict_rolls_back_whole_write() {
    require_mongo!(async {
        let test_db = TestDb::new("mas_tx_conflict").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let existing = sample_refund("refund-1", "fact-2");
        db.mall_refunds()
            .create(&existing, &mut NoTransaction)
            .await
            .unwrap();

        let duplicate = sample_refund("refund-9", "fact-2");
        let line = sample_refund_line("rl-9", "refund-9", 1);
        let allocation = sample_apply_allocation("ra-9", "rl-9", 1);

        let db_clone = db.clone();
        let duplicate_for_tx = duplicate.clone();
        let line_for_tx = line.clone();
        let allocation_for_tx = allocation.clone();
        let result: std::result::Result<(), database::Error> = test_db
            .client()
            .with_transaction(move |session| {
                Box::pin(async move {
                    db_clone
                        .mall_after_sales()
                        .create_refund_with_lines_and_allocations(
                            &duplicate_for_tx,
                            &[line_for_tx],
                            &[allocation_for_tx],
                            session,
                        )
                        .await?;
                    Ok(())
                })
            })
            .await;
        assert!(
            matches!(result, Err(database::Error::DuplicateKey(_))),
            "唯一冲突必须透出 DuplicateKey，实际为 {result:?}"
        );

        assert!(
            db.mall_refund_lines()
                .find_by_id(&line.base.id, &mut NoTransaction)
                .await
                .unwrap()
                .is_none(),
            "冲突回滚后退款行不得残留"
        );
        assert!(
            db.mall_refund_allocations()
                .find_by_id(&allocation.base.id, &mut NoTransaction)
                .await
                .unwrap()
                .is_none(),
            "冲突回滚后分配不得残留"
        );
    })
}
