use std::str::FromStr;

use application_core::AuditActor;
use erp_core::AccountKind;
use erp_core::common::time::{BusinessDate, Instant};
use erp_core::ids::{
    SupplierAccountId, SupplierFulfillmentItemId, SupplierFulfillmentOrderId, SupplierSettlementDifferenceId,
    SupplierSettlementItemId, SupplierSettlementStatementId, WorkItemId,
};
use erp_core::money::{Amount, Quantity};
use erp_supply::entity::supplier_settlement::{
    SettlementDifferenceStatus, SettlementDifferenceType, SupplierSettlementDifference,
    SupplierSettlementDifferenceData, SupplierSettlementDifferenceEvidence,
    SupplierSettlementDifferenceEvidenceData, SupplierSettlementItem, SupplierSettlementItemData,
    SupplierSettlementStatement, SupplierSettlementStatementData,
};
use erp_supply::repository::SupplierSettlementExt;
use erp_supply::service::supplier_settlement::review::{
    SETTLEMENT_REVIEW_OWNER_ROLE, settlement_review_access,
};
use erp_supply::service::supplier_settlement::shared::{
    REVIEW_CUTOFF_POLICY_ID, REVIEW_CUTOFF_POLICY_VERSION,
};
use erp_workflow::entity::work_item::{
    AssignmentSource, WorkItem, WorkItemData, WorkItemPriority, WorkItemType,
};
use persistence_core::NoTransaction;
use test_support::{TestDb, require_mongo};

use super::*;
use crate::Error;

fn sample_statement() -> SupplierSettlementStatement {
    let mut statement = SupplierSettlementStatement::new(
        SupplierSettlementStatementId::new("statement-1"),
        SupplierSettlementStatementData {
            statement_no: "ST-2026-001".to_string(),
            supplier_id: SupplierAccountId::new("supplier-1"),
            period_start: BusinessDate::from_ymd(2026, 7, 1).unwrap(),
            period_end: BusinessDate::from_ymd(2026, 7, 31).unwrap(),
            period_policy_id: "calendar-month".to_string(),
            period_policy_version: "1".to_string(),
            period_timezone: "Asia/Shanghai".to_string(),
            external_bill_no: Some("BILL-1".to_string()),
            external_bill_version: Some("1".to_string()),
            erp_amount: Amount::from_str("100.00").unwrap(),
            supplier_amount: Amount::from_str("101.00").unwrap(),
            subject_hash: "a".repeat(64),
            source_as_of: Instant::from_unix_secs(1_700_000_000),
            source_snapshot_at: Instant::from_unix_secs(1_700_000_000),
            source_snapshot_hash: "b".repeat(64),
            refresh_cutoff_policy_id: REVIEW_CUTOFF_POLICY_ID.to_string(),
            refresh_cutoff_policy_version: REVIEW_CUTOFF_POLICY_VERSION.to_string(),
            prepared_by: "preparer-1".to_string(),
            business_org_unit_id: "org-finance".to_string(),
            difference_handler_user_id: String::new(),
        },
    )
    .unwrap();
    statement.update_subject_hash(statement.review_subject_hash(&[])).unwrap();
    statement
}

fn sample_work_item(statement: &SupplierSettlementStatement) -> WorkItem {
    WorkItem::new_at(
        WorkItemId::new("work-item-1"),
        WorkItemData {
            work_item_type: WorkItemType::SupplierSettlementReview,
            business_object_type: "supplier_settlement_statement".to_string(),
            business_object_id: statement.base.id.clone(),
            subject_version: statement.subject_hash.clone(),
            owner_role: SETTLEMENT_REVIEW_OWNER_ROLE.to_string(),
            owner_organization_id: statement.business_org_unit_id.clone(),
            owner_user_id: "reviewer-1".to_string(),
            assignment_source: AssignmentSource::SystemRule,
            priority: WorkItemPriority::High,
            due_at: None,
            reason_code: None,
            impact_summary: None,
        },
        Instant::from_unix_secs(1_700_000_000),
    )
    .unwrap()
}

#[test]
fn review_access_is_actor_specific_and_fail_closed() {
    let statement = sample_statement();
    let item = sample_work_item(&statement);
    let (domain_actions, blockers) = settlement_review_access(item.is_owned_by("other-reviewer"), true, true);
    assert!(domain_actions.is_empty());
    assert_eq!(blockers[0].code, "CURRENT_OWNER_MISMATCH");

    let (domain_actions, blockers) = settlement_review_access(item.is_owned_by("reviewer-1"), true, true);
    assert_eq!(domain_actions, vec!["REJECT", "CONFIRM"]);
    assert!(blockers.is_empty());

    let (domain_actions, blockers) = settlement_review_access(item.is_owned_by("reviewer-1"), false, true);
    assert!(domain_actions.is_empty());
    assert_eq!(blockers[0].code, "ASSIGNMENT_NOT_ELIGIBLE");
    let (domain_actions, blockers) = settlement_review_access(item.is_owned_by("reviewer-1"), true, false);
    assert!(domain_actions.is_empty());
    assert_eq!(blockers[0].code, "SEGREGATION_OF_DUTIES");
}

/// 详情明细夹具（订单 100 + 运费 10 + 服务费 5 − 退款 0 = ERP 115）。
fn detail_item(id: &str) -> SupplierSettlementItem {
    SupplierSettlementItem::new(
        SupplierSettlementItemId::new(id),
        SupplierSettlementItemData {
            statement_id: SupplierSettlementStatementId::new("statement-1"),
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
    .unwrap()
}

/// 详情差异夹具（待处理或已认可，不带处理三元组）。
fn detail_difference(
    id: &str,
    item_id: &str,
    status: SettlementDifferenceStatus,
) -> SupplierSettlementDifference {
    SupplierSettlementDifference::new(
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
    .unwrap()
}

/// 缺失的结算单映射为 `NotFound`（快照 `None` 的服务语义）。
#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn detail_missing_statement_maps_to_not_found() {
    require_mongo!(async {
        let fixture = TestDb::new("ful_r07_service_detail_missing").await.expect("测试数据库创建失败");
        let service = SupplierSettlementReadService::new(fixture.db().clone());
        let actor = AuditActor::new("viewer-1".to_string(), "viewer".to_string(), AccountKind::Admin);
        let error = service
            .supplier_settlement_statement_detail("statement-missing", &actor)
            .await
            .expect_err("缺失结算单必须失败");
        assert!(matches!(error, Error::NotFound(_)), "缺失结算单必须映射为 NotFound");
    });
}

/// 详情正确挂载补证并上报已举证/待处理计数。
#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn detail_attaches_evidence_and_reports_counts() {
    require_mongo!(async {
        let fixture = TestDb::new("ful_r07_service_detail_counts").await.expect("测试数据库创建失败");
        let db = fixture.db();
        let statement = sample_statement();
        let items = vec![detail_item("item-1"), detail_item("item-2")];
        let differences = vec![
            detail_difference("difference-1", "item-1", SettlementDifferenceStatus::Pending),
            detail_difference("difference-2", "item-2", SettlementDifferenceStatus::SupplierAcknowledged),
        ];
        db.supplier_settlement()
            .create_statement_with_items(&statement, &items, &differences, &mut NoTransaction)
            .await
            .expect("结算单及明细差异插入失败");
        let evidence = SupplierSettlementDifferenceEvidence::new(
            "evidence-1",
            SupplierSettlementDifferenceEvidenceData {
                request_id: "request-1".to_string(),
                statement_id: SupplierSettlementStatementId::new("statement-1"),
                difference_id: SupplierSettlementDifferenceId::new("difference-1"),
                evidence_reference_ids: vec!["ticket://1".to_string()],
                opinion_code: None,
                comment: None,
                provided_by: "preparer-1".to_string(),
                provided_at: Instant::from_unix_secs(1_700_000_100),
                command_hash: "a".repeat(64),
            },
        )
        .unwrap();
        db.supplier_settlement_difference_evidence()
            .create(&evidence, &mut NoTransaction)
            .await
            .expect("补证插入失败");
        let service = SupplierSettlementReadService::new(db.clone());
        let actor = AuditActor::new("viewer-1".to_string(), "viewer".to_string(), AccountKind::Admin);
        let view =
            service.supplier_settlement_statement_detail("statement-1", &actor).await.expect("详情查询失败");
        assert_eq!(view.stats.item_count, 2, "明细计数必须为 2");
        assert_eq!(view.stats.difference_count, 2, "差异计数必须为 2");
        assert_eq!(view.stats.pending_difference_count, 1, "待处理计数必须为 1");
        assert_eq!(view.stats.evidenced_difference_count, 1, "已举证计数必须为 1");
        let evidenced = view
            .differences
            .iter()
            .find(|difference| difference.id == "difference-1")
            .expect("差异 difference-1 必须存在");
        assert_eq!(evidenced.evidence.len(), 1, "补证只能归入所属差异");
        let bare = view
            .differences
            .iter()
            .find(|difference| difference.id == "difference-2")
            .expect("差异 difference-2 必须存在");
        assert!(bare.evidence.is_empty(), "无补证差异的证据集合为空");
    });
}
