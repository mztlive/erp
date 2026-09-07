//! FUL-E12 草稿创建与刷新同源等价（真实 MongoDB 验收）。
//!
//! `create_statement` 与 `refresh_statement` 共用 `SupplierSettlementDraftSnapshot::
//! from_source` 领域工厂（主键由 Service 注入）。本测试以真实 Mongo 驱动两条
//! Service 路径：对同一业务来源批次（仅身份字段不同、业务行完全相同的 v1/v2
//! 来源），refresh 重算出的明细/差异业务快照必须与 create 完全一致。

use std::str::FromStr;

use database::{ensure_indexes, NoTransaction, SupplierSettlementExt};
use entities::common::time::{BusinessDate, Instant};
use entities::ids::{
    SupplierAccountId, SupplierFulfillmentItemId, SupplierFulfillmentOrderId, SupplierSettlementItemId,
};
use entities::money::{Amount, Quantity};
use entities::supplier_settlement::{
    SettlementSourceFactType, SupplierSettlementDifference, SupplierSettlementItem,
    SupplierSettlementSourceEvidence, SupplierSettlementSourceEvidenceData,
    SupplierSettlementSourceEvidenceLine, SETTLEMENT_TIMEZONE,
};
use services::audit::AuditActor;
use services::supplier_settlement::{
    CreateSettlementStatementRequest, RefreshSettlementStatementRequest, SettlementDraftAction,
    SupplierSettlementService,
};
use test_support::{require_mongo, TestDb};

fn amount(value: &str) -> Amount {
    Amount::from_str(value).unwrap()
}

/// 来源证据行夹具（零运费/服务费/退款，订单金额即 ERP 金额）。
fn source_line(item_id: &str, erp_gross: &str, supplier_gross: &str) -> SupplierSettlementSourceEvidenceLine {
    SupplierSettlementSourceEvidenceLine {
        supplier_fulfillment_order_id: SupplierFulfillmentOrderId::new(format!("order-{item_id}")),
        supplier_fulfillment_item_id: SupplierFulfillmentItemId::new(item_id),
        quantity: Quantity::from_str("1").unwrap(),
        source_fact_types: vec![SettlementSourceFactType::FulfillmentCompleted],
        evidence_reference_ids: vec![format!("fulfillment://{item_id}")],
        order_gross: amount(erp_gross),
        order_net: amount(erp_gross),
        order_tax: amount("0.00"),
        freight_gross: amount("0.00"),
        freight_net: amount("0.00"),
        freight_tax: amount("0.00"),
        service_fee_gross: amount("0.00"),
        service_fee_net: amount("0.00"),
        service_fee_tax: amount("0.00"),
        refund_gross: amount("0.00"),
        refund_net: amount("0.00"),
        refund_tax: amount("0.00"),
        erp_gross: amount(erp_gross),
        erp_net: amount(erp_gross),
        erp_tax: amount("0.00"),
        supplier_billed_gross: amount(supplier_gross),
        supplier_billed_net: amount(supplier_gross),
        supplier_billed_tax: amount("0.00"),
    }
}

/// 来源证据批次夹具（身份字段可注入；来源摘要按业务事实计算）。
fn source(
    id: &str,
    request_id: &str,
    bill_no: &str,
    version: u64,
    lines: Vec<SupplierSettlementSourceEvidenceLine>,
) -> SupplierSettlementSourceEvidence {
    let mut data = SupplierSettlementSourceEvidenceData {
        request_id: request_id.to_string(),
        supplier_id: SupplierAccountId::new("supplier-1"),
        period_start: BusinessDate::from_str("2026-07-01").unwrap(),
        period_end: BusinessDate::from_str("2026-07-31").unwrap(),
        period_policy_id: "monthly".to_string(),
        period_policy_version: "1".to_string(),
        timezone: SETTLEMENT_TIMEZONE.to_string(),
        source_version: version,
        external_bill_no: bill_no.to_string(),
        external_bill_version: "1".to_string(),
        external_bill_evidence_reference_id: "bill://1".to_string(),
        lines,
        source_as_of: Instant::from_unix_secs(1_700_000_000),
        recorded_by: "finance-1".to_string(),
        source_hash: String::new(),
        request_hash: "b".repeat(64),
    };
    data.source_hash = data.canonical_source_hash();
    SupplierSettlementSourceEvidence::new(id, data).expect("来源证据构造失败")
}

fn actor() -> AuditActor {
    AuditActor::new(
        "finance-1".to_string(),
        "finance-1".to_string(),
        entities::AccountKind::Admin,
    )
}

/// 只比较业务快照字段（排除注入的系统主键与乐观锁/时间）。
fn assert_business_items_equal(created: &[SupplierSettlementItem], refreshed: &[SupplierSettlementItem]) {
    assert_eq!(created.len(), refreshed.len(), "明细条数必须一致");
    let created_by_line = created
        .iter()
        .map(|item| (item.supplier_fulfillment_item_id.clone(), item))
        .collect::<std::collections::HashMap<_, _>>();
    let refreshed_by_line = refreshed
        .iter()
        .map(|item| (item.supplier_fulfillment_item_id.clone(), item))
        .collect::<std::collections::HashMap<_, _>>();
    for (line_id, left) in &created_by_line {
        let right = refreshed_by_line
            .get(line_id)
            .expect("刷新后同一履约行明细必须存在");
        assert_eq!(
            left.supplier_fulfillment_order_id,
            right.supplier_fulfillment_order_id
        );
        assert_eq!(
            left.supplier_fulfillment_item_id,
            right.supplier_fulfillment_item_id
        );
        assert_eq!(left.quantity, right.quantity);
        assert_eq!(left.order_amount, right.order_amount);
        assert_eq!(left.freight_amount, right.freight_amount);
        assert_eq!(left.service_fee_amount, right.service_fee_amount);
        assert_eq!(left.refund_amount, right.refund_amount);
        assert_eq!(left.erp_calculated_amount, right.erp_calculated_amount);
        assert_eq!(left.erp_calculated_net_amount, right.erp_calculated_net_amount);
        assert_eq!(left.erp_calculated_tax_amount, right.erp_calculated_tax_amount);
        assert_eq!(left.supplier_billed_amount, right.supplier_billed_amount);
        assert_eq!(left.supplier_billed_net_amount, right.supplier_billed_net_amount);
        assert_eq!(left.supplier_billed_tax_amount, right.supplier_billed_tax_amount);
    }
}

/// 差异按「明细 → 履约行」映射后比较业务字段（差异主键由 Service 注入，不比较）。
fn differences_by_statement_item(
    differences: &[SupplierSettlementDifference],
) -> std::collections::HashMap<String, &SupplierSettlementDifference> {
    differences
        .iter()
        .map(|difference| (format!("{}", difference.statement_item_id), difference))
        .collect()
}

/// 差异按「明细 → 履约行」映射后比较业务字段（差异主键由 Service 注入，不比较）。
fn assert_business_differences_equal(
    created_items: &[SupplierSettlementItem],
    created: &[SupplierSettlementDifference],
    refreshed_items: &[SupplierSettlementItem],
    refreshed: &[SupplierSettlementDifference],
) {
    let created_by_item = differences_by_statement_item(created);
    let refreshed_by_item = differences_by_statement_item(refreshed);
    assert_eq!(created_by_item.len(), refreshed_by_item.len(), "差异条数必须一致");
    for left_item in created_items {
        let item_key = left_item.base.id.clone();
        let right_item = refreshed_items
            .iter()
            .find(|item| item.supplier_fulfillment_item_id == left_item.supplier_fulfillment_item_id)
            .expect("刷新后明细必须存在");
        let left_difference = created_by_item.get(&item_key);
        let right_difference = refreshed_by_item.get(&right_item.base.id);
        match (left_difference, right_difference) {
            (None, None) => {}
            (Some(left), Some(right)) => {
                assert_eq!(left.difference_type, right.difference_type);
                assert_eq!(left.difference_amount, right.difference_amount);
                assert_eq!(left.status, right.status);
            }
            _ => panic!("创建与刷新的差异归属不一致"),
        }
    }
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn create_and_refresh_derive_identical_business_snapshot() {
    require_mongo!(async {
        let fixture = TestDb::new("ful_settlement_draft_equivalence")
            .await
            .expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        let db = fixture.db();
        let service = SupplierSettlementService::new(db.clone());

        let lines = vec![
            source_line("item-1", "113.00", "114.00"),
            source_line("item-2", "105.00", "100.00"),
        ];
        let source_v1 = source("source-v1", "source-request-1", "BILL-1", 1, lines.clone());
        db.supplier_settlement_source_evidence()
            .create(&source_v1, &mut NoTransaction)
            .await
            .expect("来源证据 v1 写入失败");

        let create = service
            .create_statement(
                CreateSettlementStatementRequest {
                    action: SettlementDraftAction::Create,
                    supplier_id: SupplierAccountId::new("supplier-1"),
                    period_start: "2026-07-01".to_string(),
                    period_end: "2026-07-31".to_string(),
                    request_id: "create-request-1".to_string(),
                    idempotency_key: "create-key-1".to_string(),
                },
                &actor(),
            )
            .await
            .expect("创建草稿失败");
        assert_eq!(create.result_status, "CREATED");
        let statement_id = create.statement.id.clone();
        let created_items = db
            .supplier_settlement_items()
            .list_by_statement(&statement_id, &mut NoTransaction)
            .await
            .expect("明细读取失败");
        assert_eq!(created_items.len(), 2, "创建必须生成两条明细");
        let created_differences = db
            .supplier_settlement_differences()
            .list_by_statement_item_ids(
                &created_items
                    .iter()
                    .map(|item| SupplierSettlementItemId::new(item.base.id.clone()))
                    .collect::<Vec<_>>(),
                &mut NoTransaction,
            )
            .await
            .expect("差异读取失败");
        assert_eq!(created_differences.len(), 2, "正负差异各一条");

        // 业务行完全相同、仅身份字段不同的 v2 来源（版本单调递增）：
        // refresh 必须重算，且业务快照与 create 完全一致。
        let source_v2 = source("source-v2", "source-request-2", "BILL-1B", 2, lines);
        db.supplier_settlement_source_evidence()
            .create(&source_v2, &mut NoTransaction)
            .await
            .expect("来源证据 v2 写入失败");

        let refresh = service
            .refresh_statement(
                &statement_id,
                RefreshSettlementStatementRequest {
                    action: SettlementDraftAction::Refresh,
                    statement_id: statement_id.clone(),
                    expected_lock_version: create.statement.version,
                    expected_source_snapshot_hash: source_v1.source_hash.clone(),
                    request_id: "refresh-request-1".to_string(),
                    idempotency_key: "refresh-key-1".to_string(),
                },
                &actor(),
            )
            .await
            .expect("刷新草稿失败");
        assert_eq!(refresh.result_status, "REFRESHED");

        let refreshed_items = db
            .supplier_settlement_items()
            .list_by_statement(&statement_id, &mut NoTransaction)
            .await
            .expect("明细读取失败");
        assert_eq!(refreshed_items.len(), 2, "刷新必须替换为两条明细");

        assert_business_items_equal(&created_items, &refreshed_items);
        assert_eq!(
            refresh.statement.erp_amount, create.statement.erp_amount,
            "ERP 总额必须与创建一致"
        );
        assert_eq!(
            refresh.statement.supplier_amount, create.statement.supplier_amount,
            "供应商总额必须与创建一致"
        );

        let refreshed_differences = db
            .supplier_settlement_differences()
            .list_by_statement_item_ids(
                &refreshed_items
                    .iter()
                    .map(|item| SupplierSettlementItemId::new(item.base.id.clone()))
                    .collect::<Vec<_>>(),
                &mut NoTransaction,
            )
            .await
            .expect("差异读取失败");
        assert_eq!(refreshed_differences.len(), 2, "刷新后正负差异各一条");
        assert_business_differences_equal(
            &created_items,
            &created_differences,
            &refreshed_items,
            &refreshed_differences,
        );
    });
}
