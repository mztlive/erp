//! 原结算详情仓储 Mongo 门控库测试；保持 ignore，不引入 supply 的 test-support 回边。

use erp_supply::entity::supplier_settlement::{
    SupplierSettlementDifference, SupplierSettlementDifferenceEvidence, SupplierSettlementStatement,
};
use erp_supply::repository::SupplierSettlementExt;
use std::str::FromStr;

use erp_core::common::time::{BusinessDate, Instant};
use erp_core::ids::{
    SupplierAccountId, SupplierFulfillmentItemId, SupplierFulfillmentOrderId, SupplierSettlementDifferenceId,
    SupplierSettlementItemId, SupplierSettlementStatementId,
};
use erp_core::money::{Amount, Quantity};
use erp_supply::entity::supplier_settlement::{
    SettlementDifferenceStatus, SettlementDifferenceType, SupplierSettlementDifferenceData,
    SupplierSettlementDifferenceEvidenceData, SupplierSettlementItem, SupplierSettlementItemData,
    SupplierSettlementStatementData,
};
use test_support::{require_mongo, TestDb};

use persistence_core::NoTransaction;

/// 构造单条补证实体。
///
/// # 参数
/// * `statement_id` - 所属结算单主键
/// * `id` - 补证主键
/// * `difference_id` - 所属差异主键
/// * `provided_at_secs` - 补证时间（秒级时间戳）
///
/// # 返回
/// 返回可直接归组的不可变补证实体。
fn sample_evidence_in_statement(
    statement_id: &str,
    id: &str,
    difference_id: &str,
    provided_at_secs: i64,
) -> SupplierSettlementDifferenceEvidence {
    SupplierSettlementDifferenceEvidence::new(
        id,
        SupplierSettlementDifferenceEvidenceData {
            request_id: format!("request-{id}"),
            statement_id: SupplierSettlementStatementId::new(statement_id),
            difference_id: SupplierSettlementDifferenceId::new(difference_id),
            evidence_reference_ids: vec![format!("ticket://{id}")],
            opinion_code: None,
            comment: None,
            provided_by: "preparer-1".to_string(),
            provided_at: Instant::from_unix_secs(provided_at_secs),
            command_hash: "a".repeat(64),
        },
    )
    .unwrap()
}

/// 构造单条补证实体（归属 `statement-1`）。
///
/// # 参数
/// * `id` - 补证主键
/// * `difference_id` - 所属差异主键
/// * `provided_at_secs` - 补证时间（秒级时间戳）
///
/// # 返回
/// 返回可直接归组的不可变补证实体。
fn sample_evidence(
    id: &str,
    difference_id: &str,
    provided_at_secs: i64,
) -> SupplierSettlementDifferenceEvidence {
    sample_evidence_in_statement("statement-1", id, difference_id, provided_at_secs)
}

/// 构造结算单夹具。
///
/// # 参数
/// * `id` - 结算单主键
///
/// # 返回
/// 返回草稿状态的结算单实体。
fn statement_fixture(id: &str) -> SupplierSettlementStatement {
    SupplierSettlementStatement::new(
        SupplierSettlementStatementId::new(id),
        SupplierSettlementStatementData {
            statement_no: format!("ST-{id}"),
            supplier_id: SupplierAccountId::new("supplier-1"),
            period_start: BusinessDate::from_ymd(2026, 7, 1).unwrap(),
            period_end: BusinessDate::from_ymd(2026, 7, 31).unwrap(),
            period_policy_id: "calendar-month".to_string(),
            period_policy_version: "1".to_string(),
            period_timezone: "Asia/Shanghai".to_string(),
            external_bill_no: Some(format!("BILL-{id}")),
            external_bill_version: Some("1".to_string()),
            erp_amount: Amount::from_str("115.00").unwrap(),
            supplier_amount: Amount::from_str("115.00").unwrap(),
            subject_hash: "a".repeat(64),
            source_as_of: Instant::from_unix_secs(1_700_000_000),
            source_snapshot_at: Instant::from_unix_secs(1_700_000_000),
            source_snapshot_hash: "b".repeat(64),
            refresh_cutoff_policy_id: "supplier-settlement-review-cutoff".to_string(),
            refresh_cutoff_policy_version: "1".to_string(),
            prepared_by: "preparer-1".to_string(),
        },
    )
    .unwrap()
}

/// 构造结算明细夹具（订单 100 + 运费 10 + 服务费 5 − 退款 0 = ERP 115）。
///
/// # 参数
/// * `id` - 明细主键
/// * `statement_id` - 所属结算单主键
/// * `created_at` - 冻结创建时间（覆盖时钟，便于排序断言）
///
/// # 返回
/// 返回满足金额恒等的冻结明细实体。
fn item_fixture(id: &str, statement_id: &str, created_at: u64) -> SupplierSettlementItem {
    let mut item = SupplierSettlementItem::new(
        SupplierSettlementItemId::new(id),
        SupplierSettlementItemData {
            statement_id: SupplierSettlementStatementId::new(statement_id),
            supplier_fulfillment_order_id: SupplierFulfillmentOrderId::new(format!("order-{id}")),
            supplier_fulfillment_item_id: SupplierFulfillmentItemId::new(format!("fulfillment-{id}")),
            quantity: Quantity::from_str("1").unwrap(),
            order_amount: Amount::from_str("100.00").unwrap(),
            freight_amount: Amount::from_str("10.00").unwrap(),
            service_fee_amount: Amount::from_str("5.00").unwrap(),
            refund_amount: Amount::from_str("0.00").unwrap(),
            erp_calculated_amount: Amount::from_str("115.00").unwrap(),
            erp_calculated_net_amount: Amount::from_str("100.00").unwrap(),
            erp_calculated_tax_amount: Amount::from_str("15.00").unwrap(),
            supplier_billed_amount: Amount::from_str("115.00").unwrap(),
            supplier_billed_net_amount: Amount::from_str("100.00").unwrap(),
            supplier_billed_tax_amount: Amount::from_str("15.00").unwrap(),
        },
    )
    .unwrap();
    item.base.created_at = created_at;
    item
}

/// 构造结算差异夹具。
///
/// # 参数
/// * `id` - 差异主键
/// * `item_id` - 所属明细主键
/// * `status` - 差异状态（待处理或已认可不带处理三元组）
/// * `created_at` - 创建时间（覆盖时钟，便于排序断言）
///
/// # 返回
/// 返回新建的结算差异实体。
fn difference_fixture(
    id: &str,
    item_id: &str,
    status: SettlementDifferenceStatus,
    created_at: u64,
) -> SupplierSettlementDifference {
    let mut difference = SupplierSettlementDifference::new(
        SupplierSettlementDifferenceId::new(id),
        SupplierSettlementDifferenceData {
            statement_item_id: SupplierSettlementItemId::new(item_id),
            difference_type: SettlementDifferenceType::Amount,
            difference_amount: Amount::from_str("1.00").unwrap(),
            status,
            resolution: None,
            resolved_by: None,
            resolved_at: None,
        },
    )
    .unwrap();
    difference.base.created_at = created_at;
    difference
}

/// 标记实体为软删除。
///
/// # 参数
/// * `entity` - 待标记的实体可变引用
fn mark_deleted(entity: &mut entity_core::BaseModel) {
    entity.deleted_at = 1_700_000_001;
}

/// 缺失的结算单返回 `None`（调用方映射为 `NotFound`）。
#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn detail_snapshot_missing_statement_returns_none() {
    require_mongo!(async {
        let fixture = TestDb::new("ful_r07_detail_missing")
            .await
            .expect("测试数据库创建失败");
        let snapshot = fixture
            .db()
            .supplier_settlement()
            .statement_detail_snapshot("statement-missing", &mut NoTransaction)
            .await
            .expect("快照查询失败");
        assert!(snapshot.is_none(), "缺失结算单必须返回 None");
    });
}

/// 已软删除的结算单返回 `None`，与主键读取的软删除语义一致。
#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn detail_snapshot_soft_deleted_statement_returns_none() {
    require_mongo!(async {
        let fixture = TestDb::new("ful_r07_detail_deleted_statement")
            .await
            .expect("测试数据库创建失败");
        let db = fixture.db();
        let mut statement = statement_fixture("statement-1");
        mark_deleted(&mut statement.base);
        db.collection::<SupplierSettlementStatement>(
            <mongodb::Database as SupplierSettlementExt>::SUPPLIER_SETTLEMENT_STATEMENTS,
        )
        .insert_one(statement)
        .await
        .expect("结算单插入失败");
        let snapshot = db
            .supplier_settlement()
            .statement_detail_snapshot("statement-1", &mut NoTransaction)
            .await
            .expect("快照查询失败");
        assert!(snapshot.is_none(), "已软删除结算单必须返回 None");
    });
}

/// 无明细、无差异、无补证时稳定返回空集合（空明细/空差异不发空 `$in`）。
#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn detail_snapshot_empty_detail_returns_empty_sets() {
    require_mongo!(async {
        let fixture = TestDb::new("ful_r07_detail_empty")
            .await
            .expect("测试数据库创建失败");
        let db = fixture.db();
        db.collection::<SupplierSettlementStatement>(
            <mongodb::Database as SupplierSettlementExt>::SUPPLIER_SETTLEMENT_STATEMENTS,
        )
        .insert_one(statement_fixture("statement-1"))
        .await
        .expect("结算单插入失败");
        let snapshot = db
            .supplier_settlement()
            .statement_detail_snapshot("statement-1", &mut NoTransaction)
            .await
            .expect("快照查询失败")
            .expect("结算单必须存在");
        assert!(snapshot.items.is_empty(), "无明细时明细集合为空");
        assert!(snapshot.differences.is_empty(), "无差异时差异集合为空");
        assert!(snapshot.evidence_by_difference.is_empty(), "无补证时归组映射为空");
    });
}

/// 已软删除的明细、差异、补证一律排除，只返回未删除事实。
#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn detail_snapshot_excludes_soft_deleted_facts() {
    require_mongo!(async {
        let fixture = TestDb::new("ful_r07_detail_soft_deleted")
            .await
            .expect("测试数据库创建失败");
        let db = fixture.db();
        db.collection::<SupplierSettlementStatement>(
            <mongodb::Database as SupplierSettlementExt>::SUPPLIER_SETTLEMENT_STATEMENTS,
        )
        .insert_one(statement_fixture("statement-1"))
        .await
        .expect("结算单插入失败");
        let mut deleted_item = item_fixture("item-deleted", "statement-1", 100);
        mark_deleted(&mut deleted_item.base);
        db.collection::<SupplierSettlementItem>(
            <mongodb::Database as SupplierSettlementExt>::SUPPLIER_SETTLEMENT_ITEMS,
        )
        .insert_many(vec![item_fixture("item-1", "statement-1", 100), deleted_item])
        .await
        .expect("明细插入失败");
        let mut deleted_difference = difference_fixture(
            "difference-deleted",
            "item-1",
            SettlementDifferenceStatus::Pending,
            100,
        );
        mark_deleted(&mut deleted_difference.base);
        db.collection::<SupplierSettlementDifference>(
            <mongodb::Database as SupplierSettlementExt>::SUPPLIER_SETTLEMENT_DIFFERENCES,
        )
        .insert_many(vec![
            difference_fixture("difference-1", "item-1", SettlementDifferenceStatus::Pending, 100),
            deleted_difference,
        ])
        .await
        .expect("差异插入失败");
        let mut deleted_evidence = sample_evidence("evidence-deleted", "difference-1", 100);
        mark_deleted(&mut deleted_evidence.base);
        db.collection::<SupplierSettlementDifferenceEvidence>(
            <mongodb::Database as SupplierSettlementExt>::SUPPLIER_SETTLEMENT_DIFFERENCE_EVIDENCE,
        )
        .insert_many(vec![
            sample_evidence("evidence-1", "difference-1", 101),
            deleted_evidence,
        ])
        .await
        .expect("补证插入失败");
        let snapshot = db
            .supplier_settlement()
            .statement_detail_snapshot("statement-1", &mut NoTransaction)
            .await
            .expect("快照查询失败")
            .expect("结算单必须存在");
        assert_eq!(snapshot.items.len(), 1, "已软删除明细必须排除");
        assert_eq!(snapshot.items[0].base.id, "item-1");
        assert_eq!(snapshot.differences.len(), 1, "已软删除差异必须排除");
        assert_eq!(snapshot.differences[0].base.id, "difference-1");
        let group = snapshot
            .evidence_by_difference
            .get("difference-1")
            .expect("存活补证必须归组");
        assert_eq!(group.len(), 1, "已软删除补证必须排除");
        assert_eq!(group[0].base.id, "evidence-1");
    });
}

/// 他单事实不泄漏；明细/差异按 `created_at` 与主键稳定排序，
/// 补证组内按 `provided_at` 与主键稳定排序。
#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn detail_snapshot_isolates_statement_and_keeps_stable_order() {
    require_mongo!(async {
        let fixture = TestDb::new("ful_r07_detail_isolation")
            .await
            .expect("测试数据库创建失败");
        let db = fixture.db();
        db.collection::<SupplierSettlementStatement>(
            <mongodb::Database as SupplierSettlementExt>::SUPPLIER_SETTLEMENT_STATEMENTS,
        )
        .insert_many(vec![
            statement_fixture("statement-1"),
            statement_fixture("statement-2"),
        ])
        .await
        .expect("结算单插入失败");
        db.collection::<SupplierSettlementItem>(
            <mongodb::Database as SupplierSettlementExt>::SUPPLIER_SETTLEMENT_ITEMS,
        )
        .insert_many(vec![
            item_fixture("item-b", "statement-1", 200),
            item_fixture("item-a", "statement-1", 100),
            item_fixture("item-9", "statement-2", 100),
        ])
        .await
        .expect("明细插入失败");
        db.collection::<SupplierSettlementDifference>(
            <mongodb::Database as SupplierSettlementExt>::SUPPLIER_SETTLEMENT_DIFFERENCES,
        )
        .insert_many(vec![
            difference_fixture("difference-b", "item-b", SettlementDifferenceStatus::Pending, 200),
            difference_fixture(
                "difference-a",
                "item-a",
                SettlementDifferenceStatus::SupplierAcknowledged,
                100,
            ),
            difference_fixture("difference-9", "item-9", SettlementDifferenceStatus::Pending, 100),
        ])
        .await
        .expect("差异插入失败");
        db.collection::<SupplierSettlementDifferenceEvidence>(
            <mongodb::Database as SupplierSettlementExt>::SUPPLIER_SETTLEMENT_DIFFERENCE_EVIDENCE,
        )
        .insert_many(vec![
            sample_evidence_in_statement("statement-1", "evidence-2", "difference-a", 102),
            sample_evidence_in_statement("statement-1", "evidence-1", "difference-a", 101),
            // 孤立补证：差异主键指向本单差异但归属他单，必须被结算单过滤排除。
            sample_evidence_in_statement("statement-2", "evidence-orphan", "difference-a", 100),
            sample_evidence_in_statement("statement-2", "evidence-9", "difference-9", 100),
        ])
        .await
        .expect("补证插入失败");
        let snapshot = db
            .supplier_settlement()
            .statement_detail_snapshot("statement-1", &mut NoTransaction)
            .await
            .expect("快照查询失败")
            .expect("结算单必须存在");
        let item_ids = snapshot
            .items
            .iter()
            .map(|item| item.base.id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            item_ids,
            vec!["item-a", "item-b"],
            "明细按创建时间稳定排序，他单明细不得混入"
        );
        let difference_ids = snapshot
            .differences
            .iter()
            .map(|difference| difference.base.id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            difference_ids,
            vec!["difference-a", "difference-b"],
            "差异按创建时间稳定排序，他单差异不得混入"
        );
        assert!(
            !snapshot.evidence_by_difference.contains_key("difference-9"),
            "他单差异的补证不得泄漏"
        );
        assert!(
            !snapshot.evidence_by_difference.contains_key("difference-b"),
            "无补证的差异不在映射中出现"
        );
        let group = snapshot
            .evidence_by_difference
            .get("difference-a")
            .expect("本单补证必须归组");
        let evidence_ids = group
            .iter()
            .map(|evidence| evidence.base.id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            evidence_ids,
            vec!["evidence-1", "evidence-2"],
            "补证组内按提供时间稳定排序，孤立补证不得混入"
        );
    });
}
