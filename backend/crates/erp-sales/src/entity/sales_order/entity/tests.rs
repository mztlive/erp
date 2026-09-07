use std::str::FromStr;

use erp_core::common::state::{ensure_transition, DocumentState};
use erp_core::common::time::Instant;
use erp_core::ids::{
    ContractId, CustomerAccountId, PartyId, SalesOrderId, SalesOrderLineId, SalesOrderRevisionId,
};
use erp_core::money::{Amount, Quantity};

use super::super::types::{BusinessType, OriginSystem};
use super::*;

fn data() -> SalesOrderData {
    SalesOrderData {
        order_no: " SO-2026-0001 ".to_string(),
        business_type: BusinessType::GoodsService,
        origin_system: OriginSystem::Erp,
        source_identity_id: None,
        customer_id: CustomerAccountId::new("cust-1"),
        contract_id: Some(ContractId::new("contract-1")),
        settlement_party_id: PartyId::new("party-1"),
        source_status_code: None,
    }
}

#[test]
fn new_trims_and_initializes_draft_state() {
    let order = SalesOrder::new(SalesOrderId::new("o-1"), data(), "admin-1").unwrap();

    assert_eq!(order.order_no, "SO-2026-0001");
    assert_eq!(order.business_type, BusinessType::GoodsService);
    assert_eq!(order.origin_system, OriginSystem::Erp);
    assert_eq!(order.commercial_status, CommercialStatus::Draft);
    assert_eq!(order.stable.status, order.commercial_status);
    assert_eq!(order.review_status, ReviewStatus::NotSubmitted);
    assert_eq!(order.fulfillment_progress, FulfillmentProgress::NotStarted);
    assert_eq!(order.collection_progress, CollectionProgress::NotCollected);
    assert_eq!(order.invoice_progress, InvoiceProgress::NotInvoiced);
    assert_eq!(order.close_status, CloseStatus::NotSatisfied);
    assert_eq!(order.stable.created_by, "admin-1");
    assert!(order.effective_at.is_none());
    assert!(order.closed_at.is_none());
}

#[test]
fn entity_rules_cover_versions_relations_and_operability() {
    let mut order = SalesOrder::new(SalesOrderId::new("o-1"), data(), "admin-1").unwrap();
    assert!(order.matches_version(1));
    assert!(!order.matches_version(0));
    assert!(order.matches_contract_context(
        &ContractId::new("contract-1"),
        &CustomerAccountId::new("cust-1"),
        &PartyId::new("party-1"),
    ));
    assert_eq!(
        order.sales_change_start_blocker(),
        Some("只有已生效的销售单才能发起变更")
    );
    assert_eq!(
        order.procurement_creation_blocker(Quantity::from_str("1").unwrap()),
        Some("销售单最终生效后才能分配供给")
    );

    order.start_approval_submission("admin-1").unwrap();
    order
        .approve(Instant::from_unix_secs(1_800_000_000), "approver")
        .unwrap();
    order.attach_revision("rev-1", "approver");
    assert!(order.is_fully_formalized());
    assert!(order.current_revision_matches(&SalesOrderRevisionId::new("rev-1")));
    assert!(order.sales_change_start_blocker().is_none());
    assert!(order
        .procurement_creation_blocker(Quantity::from_str("1").unwrap())
        .is_none());
    assert_eq!(
        order.procurement_creation_blocker(Quantity::from_str("0").unwrap()),
        Some("当前销售单待分配供给已全部覆盖")
    );
}

#[test]
fn progress_derivation_and_refresh_are_entity_owned() {
    let zero = Amount::from_str("0").unwrap();
    let fifty = Amount::from_str("50").unwrap();
    let hundred = Amount::from_str("100").unwrap();
    assert_eq!(
        CollectionProgress::from_receivable_balances([(hundred, zero)]),
        CollectionProgress::NotCollected
    );
    assert_eq!(
        CollectionProgress::from_receivable_balances([(fifty, fifty)]),
        CollectionProgress::PartiallyCollected
    );
    assert_eq!(
        CollectionProgress::from_receivable_balances([(zero, hundred)]),
        CollectionProgress::Settled
    );
    assert_eq!(
        InvoiceProgress::from_receivable_balances([(fifty, fifty)]),
        InvoiceProgress::PartiallyInvoiced
    );
    assert_eq!(
        InvoiceProgress::from_receivable_balances([(zero, hundred)]),
        InvoiceProgress::Completed
    );

    let mut order = SalesOrder::new(SalesOrderId::new("o-1"), data(), "admin-1").unwrap();
    let closed_at = Instant::from_unix_secs(1_800_000_000);
    assert!(order.refresh_progress(
        Some(FulfillmentProgress::Completed),
        CollectionProgress::Settled,
        InvoiceProgress::Completed,
        closed_at,
        "system",
    ));
    assert_eq!(order.close_status, CloseStatus::Closed);
    assert_eq!(order.closed_at, Some(closed_at));
    assert!(!order.refresh_progress(
        None,
        CollectionProgress::Settled,
        InvoiceProgress::Completed,
        Instant::from_unix_secs(1_900_000_000),
        "system",
    ));
    assert_eq!(order.closed_at, Some(closed_at));
}

#[test]
fn review_status_exposes_active_task_rule() {
    assert!(ReviewStatus::InApproval.has_active_review_task());
    assert!(ReviewStatus::PendingOperations.has_active_review_task());
    assert!(!ReviewStatus::NotSubmitted.has_active_review_task());
    assert!(!ReviewStatus::Approved.has_active_review_task());
    assert!(!ReviewStatus::Rejected.has_active_review_task());
}

#[test]
fn procurement_guard_advances_monotonically() {
    let mut order = SalesOrder::new(SalesOrderId::new("o-1"), data(), "admin-1").unwrap();

    assert_eq!(order.advance_procurement_guard("buyer-1").unwrap(), 1);
    assert_eq!(order.advance_procurement_guard("buyer-2").unwrap(), 2);
    assert_eq!(order.procurement_guard_version, 2);
    assert_eq!(order.stable.updated_by, "buyer-2");
}

#[test]
fn new_rejects_blank_and_overlong_order_no() {
    let blank = SalesOrderData {
        order_no: "   ".to_string(),
        ..data()
    };
    assert!(SalesOrder::new(SalesOrderId::new("o-1"), blank, "admin-1").is_err());

    let overlong = SalesOrderData {
        order_no: "x".repeat(65),
        ..data()
    };
    assert!(SalesOrder::new(SalesOrderId::new("o-1"), overlong, "admin-1").is_err());
}

#[test]
fn new_rejects_overlong_source_fields() {
    let overlong_identity = SalesOrderData {
        source_identity_id: Some("x".repeat(257)),
        ..data()
    };
    assert!(SalesOrder::new(SalesOrderId::new("o-1"), overlong_identity, "admin-1").is_err());

    let overlong_code = SalesOrderData {
        source_status_code: Some("x".repeat(65)),
        ..data()
    };
    assert!(SalesOrder::new(SalesOrderId::new("o-1"), overlong_code, "admin-1").is_err());
}

#[test]
fn update_keeps_identity_fields_and_touches_auditor() {
    let mut order = SalesOrder::new(SalesOrderId::new("o-1"), data(), "admin-1").unwrap();
    order
        .update(
            SalesOrderUpdate {
                customer_id: Some(CustomerAccountId::new("cust-2")),
                source_status_code: Some("  PAID ".to_string()),
                ..Default::default()
            },
            "admin-2",
        )
        .unwrap();

    assert_eq!(order.customer_id, CustomerAccountId::new("cust-2"));
    assert_eq!(order.source_status_code.as_deref(), Some("PAID"));
    assert_eq!(order.order_no, "SO-2026-0001", "单号不可修改");
    assert_eq!(
        order.business_type,
        BusinessType::GoodsService,
        "业务性质不可修改"
    );
    assert_eq!(order.origin_system, OriginSystem::Erp, "创建入口不可修改");
    assert_eq!(order.stable.updated_by, "admin-2");
}

#[test]
fn effective_order_rejects_direct_update() {
    let mut order = SalesOrder::new(SalesOrderId::new("o-1"), data(), "admin-1").unwrap();
    order
        .submit_for_review("admin-1")
        .and_then(|()| order.transition_review(ReviewStatus::Approved, "reviewer"))
        .and_then(|()| order.approve(Instant::from_unix_secs(1_800_000_000), "reviewer"))
        .unwrap();

    assert_eq!(order.commercial_status, CommercialStatus::Effective);
    assert_eq!(order.stable.status, order.commercial_status);
    assert!(order.update(SalesOrderUpdate::default(), "admin-2").is_err());
}

#[test]
fn commercial_status_machine_edges_are_directed() {
    // §7.1 主状态逐边定向断言（含不可逆终态，不适用对称闭包辅助）。
    assert!(ensure_transition(CommercialStatus::Draft, CommercialStatus::Draft).is_ok());
    assert!(ensure_transition(CommercialStatus::Draft, CommercialStatus::PendingReview).is_ok());
    assert!(ensure_transition(CommercialStatus::Draft, CommercialStatus::Voided).is_ok());
    assert!(ensure_transition(CommercialStatus::PendingReview, CommercialStatus::Draft).is_ok());
    assert!(ensure_transition(CommercialStatus::PendingReview, CommercialStatus::Effective).is_ok());
    assert!(ensure_transition(CommercialStatus::Draft, CommercialStatus::Effective).is_err());
    assert!(ensure_transition(CommercialStatus::Effective, CommercialStatus::Draft).is_err());
    assert!(ensure_transition(CommercialStatus::Effective, CommercialStatus::Voided).is_err());
    assert!(ensure_transition(CommercialStatus::Voided, CommercialStatus::Draft).is_err());
    assert!(CommercialStatus::Effective.allowed_next().is_empty());
    assert!(CommercialStatus::Voided.allowed_next().is_empty());
}

#[test]
fn full_approval_flow_and_rejection_flow() {
    let mut order = SalesOrder::new(SalesOrderId::new("o-1"), data(), "admin-1").unwrap();

    // 实物及服务：提交直接进入统一审批，最终通过生效；撤回回到草稿。
    order.start_approval_submission("admin-1").unwrap();
    assert_eq!(order.commercial_status, CommercialStatus::PendingReview);
    assert_eq!(order.review_status, ReviewStatus::InApproval);
    assert!(order
        .transition_review(ReviewStatus::Rejected, "approver")
        .is_err());
    assert!(order
        .transition_review(ReviewStatus::PendingProcurementConfirmation, "approver")
        .is_err());
    order
        .transition_review(ReviewStatus::Approved, "approver")
        .and_then(|()| order.approve(Instant::from_unix_secs(1_800_000_000), "approver"))
        .unwrap();
    assert_eq!(order.commercial_status, CommercialStatus::Effective);
    assert_eq!(order.review_status, ReviewStatus::Approved);
    assert_eq!(order.effective_at.unwrap().unix_secs(), 1_800_000_000);

    let mut withdrawn = SalesOrder::new(SalesOrderId::new("o-2"), data(), "admin-1").unwrap();
    withdrawn.start_approval_submission("admin-1").unwrap();
    withdrawn.cancel_approval_submission("admin-1").unwrap();
    assert_eq!(withdrawn.commercial_status, CommercialStatus::Draft);
    assert_eq!(withdrawn.stable.status, withdrawn.commercial_status);
    assert_eq!(withdrawn.review_status, ReviewStatus::NotSubmitted);
    assert!(withdrawn.cancel_approval_submission("admin-1").is_err());
}

#[test]
fn voucher_order_enters_unified_in_approval() {
    let voucher_data = SalesOrderData {
        business_type: BusinessType::Voucher,
        ..data()
    };
    let mut order = SalesOrder::new(SalesOrderId::new("o-3"), voucher_data, "admin-1").unwrap();
    order.start_approval_submission("admin-1").unwrap();
    assert_eq!(order.commercial_status, CommercialStatus::PendingReview);
    assert_eq!(order.review_status, ReviewStatus::InApproval);
    assert!(order
        .transition_review(ReviewStatus::Rejected, "approver")
        .is_err());
    assert!(order
        .transition_review(ReviewStatus::PendingSalesLeader, "approver")
        .is_err());
    assert!(order
        .transition_review(ReviewStatus::PendingOperations, "approver")
        .is_err());
    order
        .transition_review(ReviewStatus::Approved, "approver")
        .and_then(|()| order.approve(Instant::from_unix_secs(1_800_000_000), "approver"))
        .unwrap();
    assert_eq!(order.commercial_status, CommercialStatus::Effective);
    assert_eq!(order.review_status, ReviewStatus::Approved);

    let mut withdrawn = SalesOrder::new(
        SalesOrderId::new("o-4"),
        SalesOrderData {
            business_type: BusinessType::Voucher,
            ..data()
        },
        "admin-1",
    )
    .unwrap();
    withdrawn.start_approval_submission("admin-1").unwrap();
    withdrawn.cancel_approval_submission("admin-1").unwrap();
    assert_eq!(withdrawn.commercial_status, CommercialStatus::Draft);
    assert_eq!(withdrawn.review_status, ReviewStatus::NotSubmitted);
}

#[test]
fn void_rejects_from_non_draft() {
    let mut order = SalesOrder::new(SalesOrderId::new("o-1"), data(), "admin-1").unwrap();
    order.submit_for_review("admin-1").unwrap();
    assert!(order.void("admin-2").is_err(), "审核中不可作废");
    order.return_to_draft("admin-2").unwrap();
    order.void("admin-2").unwrap();
    assert_eq!(order.commercial_status, CommercialStatus::Voided);
    assert_eq!(order.stable.status, order.commercial_status);
}

#[test]
fn line_new_and_remove() {
    let line = SalesOrderLine::new(
        SalesOrderLineId::new("l-1"),
        SalesOrderId::new("o-1"),
        SalesOrderLineData { line_no: 1 },
    )
    .unwrap();
    assert_eq!(line.line_status, LineStatus::Active);

    let mut line = line;
    line.remove().unwrap();
    assert_eq!(line.line_status, LineStatus::Removed);
    assert!(line.remove().is_ok(), "同态重复移除幂等通过");
    assert!(ensure_transition(LineStatus::Removed, LineStatus::Active).is_err());
}

#[test]
fn line_rejects_zero_line_no() {
    assert!(SalesOrderLine::new(
        SalesOrderLineId::new("l-1"),
        SalesOrderId::new("o-1"),
        SalesOrderLineData { line_no: 0 },
    )
    .is_err());
}

#[test]
fn progress_enums_expose_labels() {
    assert_eq!(FulfillmentProgress::PartiallyFulfilled.label(), "部分履约");
    assert_eq!(CollectionProgress::Settled.label(), "已结清");
    assert_eq!(InvoiceProgress::PartiallyInvoiced.label(), "部分开票");
    assert_eq!(CloseStatus::Closeable.label(), "可关闭");
    assert_eq!(ReviewStatus::PendingLowMarginSuperior.label(), "待低毛利上级确认");
    assert_eq!(CommercialStatus::Effective.label(), "已生效");
    assert_eq!(
        serde_json::to_string(&CommercialStatus::PendingReview).unwrap(),
        "\"PENDING_REVIEW\""
    );
}
