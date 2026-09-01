//! FUL-R01 采购入库累计合格收货聚合（Repository 下沉）的真实 MongoDB 验收。
//!
//! 覆盖：多入库单同一采购行正确累加；草稿、已删除入库单及已删除行不计入；
//! 无历史记录返回空映射；过账路径同一 session 可见未提交写入；超精度
//! Decimal128 返回错误而非 panic。

use std::str::FromStr;

use database::{ensure_indexes, FulfillmentExt, NoTransaction, Transactional};
use entities::common::time::Instant;
use entities::fulfillment::{
    PurchaseReceipt, PurchaseReceiptData, PurchaseReceiptLine, PurchaseReceiptLineData, QualityResult,
};
use entities::ids::{
    PurchaseOrderId, PurchaseOrderRevisionLineId, PurchaseReceiptId, PurchaseReceiptLineId, WarehouseId,
};
use entities::money::Quantity;
use mongodb::bson::doc;
use mongodb::Database;
use test_support::{require_mongo, TestDb};

/// 已过账入库单夹具（可选软删除）。
fn posted_receipt(
    id: &str,
    receipt_no: &str,
    purchase_order_id: &PurchaseOrderId,
    warehouse_id: &WarehouseId,
    deleted: bool,
) -> PurchaseReceipt {
    let mut receipt = PurchaseReceipt::new(
        PurchaseReceiptId::new(id),
        PurchaseReceiptData {
            receipt_no: receipt_no.to_string(),
            purchase_order_id: purchase_order_id.clone(),
            warehouse_id: warehouse_id.clone(),
        },
    )
    .expect("入库单构造失败");
    receipt
        .mark_posted(Instant::from_unix_secs(1_700_000_000), "tester-1")
        .expect("过账失败");
    if deleted {
        receipt.base.deleted_at = 1_700_000_001;
    }
    receipt
}

/// 草稿入库单夹具（不得计入累计）。
fn draft_receipt(
    id: &str,
    purchase_order_id: &PurchaseOrderId,
    warehouse_id: &WarehouseId,
) -> PurchaseReceipt {
    PurchaseReceipt::new(
        PurchaseReceiptId::new(id),
        PurchaseReceiptData {
            receipt_no: format!("PR-{id}"),
            purchase_order_id: purchase_order_id.clone(),
            warehouse_id: warehouse_id.clone(),
        },
    )
    .expect("入库单构造失败")
}

/// 入库行夹具（可选软删除；合格/不合格数量与质量结果保持守恒）。
fn receipt_line(
    receipt_id: &str,
    line_no: u32,
    revision_line_id: &str,
    received: &str,
    qualified: &str,
    rejected: &str,
    deleted: bool,
) -> PurchaseReceiptLine {
    let qualified_quantity = Quantity::from_str(qualified).expect("合格数量");
    let rejected_quantity = Quantity::from_str(rejected).expect("不合格数量");
    let mut line = PurchaseReceiptLine::new(
        PurchaseReceiptLineId::new(format!("{receipt_id}-line-{line_no}")),
        PurchaseReceiptLineData {
            purchase_receipt_id: PurchaseReceiptId::new(receipt_id),
            line_no,
            purchase_order_revision_line_id: PurchaseOrderRevisionLineId::new(revision_line_id),
            received_quantity: Quantity::from_str(received).expect("到货数量"),
            qualified_quantity,
            rejected_quantity,
            quality_result: QualityResult::from_quantities(qualified_quantity, rejected_quantity),
        },
    )
    .expect("入库行构造失败");
    if deleted {
        line.base.deleted_at = 1_700_000_001;
    }
    line
}

/// 插入入库单表头与行（独立自动提交）。
async fn insert_receipt_and_lines(db: &Database, receipt: &PurchaseReceipt, lines: &[PurchaseReceiptLine]) {
    db.collection::<PurchaseReceipt>(<mongodb::Database as FulfillmentExt>::PURCHASE_RECEIPTS)
        .insert_one(receipt)
        .await
        .expect("入库单插入失败");
    if !lines.is_empty() {
        db.collection::<PurchaseReceiptLine>(<mongodb::Database as FulfillmentExt>::PURCHASE_RECEIPT_LINES)
            .insert_many(lines.to_vec())
            .await
            .expect("入库行插入失败");
    }
}

/// 多入库单同一采购行正确累加；草稿、已删除入库单及已删除行不计入；
/// 无历史记录返回空映射。
#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn posted_receipt_totals_accumulate_and_exclude_drafts_and_deleted_rows() {
    require_mongo!(async {
        let fixture = TestDb::new("ful_receipt_totals")
            .await
            .expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        let db = fixture.db();
        let po_id = PurchaseOrderId::new("po-totals-1");
        let warehouse_id = WarehouseId::new("warehouse-1");

        insert_receipt_and_lines(
            db,
            &posted_receipt("receipt-1", "PR-001", &po_id, &warehouse_id, false),
            &[
                receipt_line(
                    "receipt-1",
                    1,
                    "po-line-a",
                    "10.000000",
                    "9.500000",
                    "0.500000",
                    false,
                ),
                receipt_line(
                    "receipt-1",
                    2,
                    "po-line-b",
                    "3.500000",
                    "3.500000",
                    "0.000000",
                    false,
                ),
            ],
        )
        .await;
        insert_receipt_and_lines(
            db,
            &posted_receipt("receipt-2", "PR-002", &po_id, &warehouse_id, false),
            &[receipt_line(
                "receipt-2",
                1,
                "po-line-a",
                "2.250000",
                "2.250000",
                "0.000000",
                false,
            )],
        )
        .await;
        // 草稿入库单不计入。
        insert_receipt_and_lines(
            db,
            &draft_receipt("receipt-draft", &po_id, &warehouse_id),
            &[receipt_line(
                "receipt-draft",
                1,
                "po-line-a",
                "99.000000",
                "99.000000",
                "0.000000",
                false,
            )],
        )
        .await;
        // 已删除入库单不计入。
        insert_receipt_and_lines(
            db,
            &posted_receipt("receipt-deleted", "PR-003", &po_id, &warehouse_id, true),
            &[receipt_line(
                "receipt-deleted",
                1,
                "po-line-a",
                "50.000000",
                "50.000000",
                "0.000000",
                false,
            )],
        )
        .await;
        // 已删除入库行不计入。
        insert_receipt_and_lines(
            db,
            &posted_receipt("receipt-3", "PR-004", &po_id, &warehouse_id, false),
            &[receipt_line(
                "receipt-3",
                1,
                "po-line-a",
                "7.000000",
                "7.000000",
                "0.000000",
                true,
            )],
        )
        .await;

        let totals = db
            .fulfillment()
            .qualified_received_totals_by_purchase_revision_line(&po_id, &mut NoTransaction)
            .await
            .expect("聚合查询失败");
        assert_eq!(totals.len(), 2, "只有两个采购版本行有累计");
        assert_eq!(
            totals.get(&PurchaseOrderRevisionLineId::new("po-line-a")),
            Some(&Quantity::from_str("11.750000").expect("合法数量")),
            "同一采购行必须跨入库单累加"
        );
        assert_eq!(
            totals.get(&PurchaseOrderRevisionLineId::new("po-line-b")),
            Some(&Quantity::from_str("3.500000").expect("合法数量"))
        );

        // 无历史记录返回空映射。
        let empty = db
            .fulfillment()
            .qualified_received_totals_by_purchase_revision_line(
                &PurchaseOrderId::new("po-totals-empty"),
                &mut NoTransaction,
            )
            .await
            .expect("空聚合查询失败");
        assert!(empty.is_empty(), "无历史记录必须返回空映射");
    });
}

/// 过账路径证明使用同一 session：事务内未提交的入库写入对聚合可见。
#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn posted_receipt_totals_use_caller_transaction_session() {
    require_mongo!(async {
        let fixture = TestDb::new("ful_receipt_totals_session")
            .await
            .expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        let db = fixture.db();
        let po_id = PurchaseOrderId::new("po-totals-session");
        let warehouse_id = WarehouseId::new("warehouse-1");

        insert_receipt_and_lines(
            db,
            &posted_receipt("receipt-1", "PR-101", &po_id, &warehouse_id, false),
            &[receipt_line(
                "receipt-1",
                1,
                "po-line-a",
                "10.000000",
                "10.000000",
                "0.000000",
                false,
            )],
        )
        .await;

        let client = db.client().clone();
        let db_in_tx = db.clone();
        let po_in_tx = po_id.clone();
        let receipt_in_tx = posted_receipt("receipt-tx", "PR-102", &po_id, &warehouse_id, false);
        let line_in_tx = receipt_line(
            "receipt-tx",
            1,
            "po-line-a",
            "1.500000",
            "1.500000",
            "0.000000",
            false,
        );
        let totals_in_tx = client
            .with_transaction(move |session| {
                let receipt = receipt_in_tx.clone();
                let line = line_in_tx.clone();
                Box::pin(async move {
                    db_in_tx
                        .collection::<PurchaseReceipt>(
                            <mongodb::Database as FulfillmentExt>::PURCHASE_RECEIPTS,
                        )
                        .insert_one(&receipt)
                        .session(&mut *session)
                        .await?;
                    db_in_tx
                        .collection::<PurchaseReceiptLine>(
                            <mongodb::Database as FulfillmentExt>::PURCHASE_RECEIPT_LINES,
                        )
                        .insert_one(&line)
                        .session(&mut *session)
                        .await?;
                    db_in_tx
                        .fulfillment()
                        .qualified_received_totals_by_purchase_revision_line(&po_in_tx, session)
                        .await
                })
            })
            .await
            .expect("事务查询失败");
        assert_eq!(
            totals_in_tx.get(&PurchaseOrderRevisionLineId::new("po-line-a")),
            Some(&Quantity::from_str("11.500000").expect("合法数量")),
            "事务内聚合必须看到同一 session 的未提交写入"
        );

        let totals_after_commit = db
            .fulfillment()
            .qualified_received_totals_by_purchase_revision_line(&po_id, &mut NoTransaction)
            .await;
        assert_eq!(
            totals_after_commit
                .expect("提交后聚合查询失败")
                .get(&PurchaseOrderRevisionLineId::new("po-line-a")),
            Some(&Quantity::from_str("11.500000").expect("合法数量"))
        );
    });
}

/// 超精度 Decimal128 必须返回错误而非 panic。
#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn posted_receipt_totals_reject_precision_overflow() {
    require_mongo!(async {
        let fixture = TestDb::new("ful_receipt_totals_overflow")
            .await
            .expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        let db = fixture.db();
        let po_id = PurchaseOrderId::new("po-totals-overflow");
        let warehouse_id = WarehouseId::new("warehouse-1");

        insert_receipt_and_lines(
            db,
            &posted_receipt("receipt-overflow", "PR-201", &po_id, &warehouse_id, false),
            &[],
        )
        .await;
        // 绕过实体校验直接写入超精度合格数量，验证聚合反序列化返回错误而非 panic。
        let overflow_line = doc! {
            "id": "overflow-line-1",
            "version": 1i64,
            "created_at": 1_700_000_000i64,
            "updated_at": 1_700_000_000i64,
            "deleted_at": 0i64,
            "purchase_receipt_id": "receipt-overflow",
            "line_no": 1i64,
            "purchase_order_revision_line_id": "po-line-overflow",
            "received_quantity": { "$numberDecimal": "1.1234567" },
            "qualified_quantity": { "$numberDecimal": "1.1234567" },
            "rejected_quantity": { "$numberDecimal": "0.0000000" },
            "quality_result": "PARTIAL",
        };
        db.collection::<mongodb::bson::Document>(
            <mongodb::Database as FulfillmentExt>::PURCHASE_RECEIPT_LINES,
        )
        .insert_one(overflow_line)
        .await
        .expect("超精度行插入失败");

        let result = db
            .fulfillment()
            .qualified_received_totals_by_purchase_revision_line(&po_id, &mut NoTransaction)
            .await;
        assert!(result.is_err(), "超精度 Decimal128 必须返回错误而非 panic");
    });
}
