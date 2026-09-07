use super::investigate::apply_replay_outcome;
use super::SupplierFulfillmentService;
use crate::dto::supplier_fulfillment::SupplierOrderInvestigationOutcome;
use crate::entity::failure::SupplierFailureClass;
use crate::entity::supplier_fulfillment::*;
use crate::ports::supplier_gateway::DispatchOutcome;
use erp_core::common::time::Instant;
use erp_core::ids::{SupplierAccountId, SupplierApiConnectionId};
fn sample_order() -> SupplierFulfillmentOrder {
    SupplierFulfillmentOrder::new(
        SupplierFulfillmentOrderId::new("order-1"),
        SupplierFulfillmentOrderData {
            fulfillment_order_no: "FO-2026-001".to_string(),
            supplier_id: SupplierAccountId::new("supplier-1"),
            connection_id: SupplierApiConnectionId::new("connection-1"),
            split_no: 1,
            fulfillment_status: FulfillmentStatus::Submitting,
            cancel_status: CancelStatus::None,
            refund_status: RefundStatus::None,
            external_order_no: None,
            submitted_at: Some(Instant::from_unix_secs(1_700_000_000)),
            accepted_at: None,
            completed_at: None,
            address_snapshot_encrypted: "encrypted".to_string(),
            address_snapshot_fingerprint: "fingerprint".to_string(),
        },
    )
    .unwrap()
}
fn sample_action(action_type: SupplierOrderActionType) -> SupplierOrderAction {
    SupplierOrderAction::new(
        SupplierOrderActionId::new("action-1"),
        SupplierOrderActionData {
            supplier_fulfillment_order_id: SupplierFulfillmentOrderId::new("order-1"),
            action_type,
            idempotency_key: "FO-2026-001".to_string(),
            status: SupplierOrderActionStatus::Pending,
            external_request_id: None,
            request_summary: None,
            response_summary: None,
            attempt_count: 0,
            next_attempt_at: None,
        },
    )
    .unwrap()
}
#[test]
fn missing_external_order_number_keeps_ordinary_and_replay_rules_distinct() {
    let mut order = sample_order();
    let mut action = sample_action(SupplierOrderActionType::Place);
    SupplierFulfillmentService::apply_dispatch_outcome(
        &mut order,
        &mut action,
        DispatchOutcome::Succeeded {
            external_request_id: "req".into(),
            external_order_no: None,
        },
        false,
    )
    .unwrap();
    assert_eq!(order.fulfillment_status, FulfillmentStatus::Accepted);
    assert_eq!(action.status, SupplierOrderActionStatus::Succeeded);
    assert!(order.external_order_no.is_none());
    let mut order = sample_order();
    let before = order.clone();
    let mut action = sample_action(SupplierOrderActionType::Place);
    let finding = apply_replay_outcome(
        &mut order,
        &mut action,
        DispatchOutcome::Succeeded {
            external_request_id: "req".into(),
            external_order_no: None,
        },
    )
    .unwrap();
    assert_eq!(order, before);
    assert_eq!(action.status, SupplierOrderActionStatus::ResultUnknown);
    assert_eq!(finding.outcome, SupplierOrderInvestigationOutcome::ResultUnknown);
    assert_eq!(finding.resolution, None);
}
#[test]
fn ordinary_retry_fact_preserves_pending_and_no_retry_fact_marks_exception() {
    let mut order = sample_order();
    let before = order.clone();
    let mut action = sample_action(SupplierOrderActionType::Place);
    SupplierFulfillmentService::apply_dispatch_outcome(
        &mut order,
        &mut action,
        DispatchOutcome::Failed {
            error_class: SupplierFailureClass::TransientFailure,
            summary: "temporary".into(),
        },
        true,
    )
    .unwrap();
    assert_eq!(order, before);
    assert_eq!(action.status, SupplierOrderActionStatus::Pending);
    assert_eq!(action.attempt_count, 1);
    assert!(action.next_attempt_at.is_some());
    assert_eq!(action.response_summary, None);
    let mut order = sample_order();
    let mut action = sample_action(SupplierOrderActionType::Place);
    SupplierFulfillmentService::apply_dispatch_outcome(
        &mut order,
        &mut action,
        DispatchOutcome::Failed {
            error_class: SupplierFailureClass::BusinessRejected,
            summary: "rejected".into(),
        },
        false,
    )
    .unwrap();
    assert_eq!(order.fulfillment_status, FulfillmentStatus::Exception);
    assert_eq!(action.status, SupplierOrderActionStatus::Failed);
    assert_eq!(action.attempt_count, 0);
    assert_eq!(action.response_summary.as_deref(), Some("rejected"));
}
#[test]
fn replay_failure_ignores_class_and_never_reuses_ordinary_retry_policy() {
    for class in [
        SupplierFailureClass::TransientFailure,
        SupplierFailureClass::BusinessRejected,
        SupplierFailureClass::RateLimited,
    ] {
        let mut order = sample_order();
        let before = order.clone();
        let mut action = sample_action(SupplierOrderActionType::Place);
        let finding = apply_replay_outcome(
            &mut order,
            &mut action,
            DispatchOutcome::Failed {
                error_class: class,
                summary: "retry unknown".into(),
            },
        )
        .unwrap();
        assert_eq!(order, before);
        assert_eq!(action.status, SupplierOrderActionStatus::ResultUnknown);
        assert_eq!(action.attempt_count, 1);
        assert_eq!(action.next_attempt_at, None);
        assert_eq!(finding.resolution, None);
        assert_eq!(finding.outcome, SupplierOrderInvestigationOutcome::ResultUnknown);
    }
}
