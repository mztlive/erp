use std::str::FromStr;

use application_core::AuditActor;
use erp_core::AccountKind;
use erp_core::common::time::{BusinessDate, Instant};
use erp_core::ids::{SupplierAccountId, SupplierSettlementStatementId, WorkItemId};
use erp_core::money::Amount;
use erp_supply::entity::supplier_settlement::{SupplierSettlementStatement, SupplierSettlementStatementData};
use erp_workflow::entity::work_item::{
    AssignmentSource, WorkItem, WorkItemData, WorkItemPriority, WorkItemType,
};

use super::difference::*;
use super::review::*;
use super::*;

pub(super) fn sample_statement() -> SupplierSettlementStatement {
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
        },
    )
    .unwrap();
    statement.update_subject_hash(statement.review_subject_hash(&[])).unwrap();
    statement
}

pub(super) fn sample_work_item(statement: &SupplierSettlementStatement) -> WorkItem {
    WorkItem::new_at(
        WorkItemId::new("work-item-1"),
        WorkItemData {
            work_item_type: WorkItemType::SupplierSettlementReview,
            business_object_type: "supplier_settlement_statement".to_string(),
            business_object_id: statement.base.id.clone(),
            subject_version: statement.subject_hash.clone(),
            owner_role: SETTLEMENT_REVIEW_OWNER_ROLE.to_string(),
            owner_organization_id: SETTLEMENT_REVIEW_OWNER_ORGANIZATION_ID.to_string(),
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
fn command_receipts_roundtrip_and_reject_fingerprint_reuse() {
    let fingerprint = "f".repeat(64);
    let submission = ReviewSubmissionReceipt {
        operation_id: "op-submit".to_string(),
        statement_version: 2,
        work_item_id: "work-item-1".to_string(),
        task_version: 1,
    };
    let message = review_submission_receipt_message(&fingerprint, &submission);
    assert_eq!(parse_review_submission_receipt(&message, &fingerprint).unwrap(), submission);
    assert!(parse_review_submission_receipt(&message, &"0".repeat(64)).is_err());

    let decision = ReviewDecisionReceipt {
        operation_id: "op-review".to_string(),
        result_status: dto::SettlementReviewDecisionStatus::Confirmed,
        statement_version: 3,
        task_version: 2,
        payable_account_id: Some("payable-1".to_string()),
        cost_delta: Some(Amount::from_str("1.00").unwrap()),
    };
    let message = review_decision_receipt_message(&fingerprint, &decision);
    assert_eq!(parse_review_decision_receipt(&message, &fingerprint).unwrap(), decision);

    let difference = DifferenceDecisionReceipt {
        operation_id: "op-difference".to_string(),
        statement_id: "statement-1".to_string(),
        statement_version: 2,
        difference_version: 2,
    };
    let message = difference_decision_receipt_message(&fingerprint, &difference);
    assert_eq!(parse_difference_decision_receipt(&message, &fingerprint).unwrap(), difference);
}

#[test]
fn work_item_validation_requires_exact_three_versions_and_current_owner() {
    let mut statement = sample_statement();
    statement.submit_review().unwrap();
    let mut item = sample_work_item(&statement);
    let actor = AuditActor::new("reviewer-1".to_string(), "reviewer".to_string(), AccountKind::Admin);

    assert!(
        validate_settlement_review_work_item(
            &item,
            &statement,
            item.base.version,
            &statement.subject_hash,
            &actor,
        )
        .is_ok()
    );
    assert!(
        validate_settlement_review_work_item(
            &item,
            &statement,
            item.base.version + 1,
            &statement.subject_hash,
            &actor,
        )
        .is_err()
    );
    assert!(
        validate_settlement_review_work_item(&item, &statement, item.base.version, &"0".repeat(64), &actor,)
            .is_err()
    );
    item.owner_user_id = Some("other-reviewer".to_string());
    assert!(
        validate_settlement_review_work_item(
            &item,
            &statement,
            item.base.version,
            &statement.subject_hash,
            &actor,
        )
        .is_err()
    );
}

#[test]
fn command_ids_are_bounded() {
    let audit_id = command_audit_id(
        "actor-1",
        "supplier_settlement.review_confirm",
        "statement-1",
        "raw-idempotency-secret",
    );
    assert!(!audit_id.contains("raw-idempotency-secret"));
    assert!(audit_id.len() <= 128);
}
