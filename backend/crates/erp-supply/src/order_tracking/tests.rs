use erp_core::common::time::Instant;

use super::*;
use crate::entity::supplier_fulfillment::FulfillmentStatus;
use crate::ports::connector::common::{Snapshot, SourceRevision, SourceStamp};
use crate::ports::connector::order::{OrderReference, OrderSnapshot, Shipment};

fn tracking() -> OrderTracking {
    OrderTracking::new(
        "order".into(),
        "connection".into(),
        "purchase".into(),
        vec![TrackingSourceLine {
            fulfillment_item_id: "item".into(),
            purchase_order_revision_line_id: "purchase-line".into(),
            sales_order_line_id: Some("sales-line".into()),
            delivery_id: Some("delivery".into()),
            quantity: "2".parse().unwrap(),
        }],
    )
    .unwrap()
}

fn snapshot() -> Snapshot<OrderSnapshot> {
    Snapshot {
        stamp: SourceStamp {
            revision: SourceRevision::Unversioned,
            changed_at: None,
            observed_at: Instant::from_unix_secs(30),
        },
        value: OrderSnapshot {
            reference: OrderReference {
                merchant_order_no: "merchant".into(),
                external_order_no: "external".into(),
            },
            amounts: None,
            fulfillment: Some(FulfillmentStatus::Accepted),
            cancellation: None,
            refund: None,
            payment: None,
            shipments: None,
        },
    }
}

#[test]
fn duplicate_source_lines_and_nonpositive_quantity_are_rejected() {
    let original = tracking();
    let line = original.source_lines[0].clone();
    assert!(
        OrderTracking::new("o".into(), "c".into(), "p".into(), vec![line.clone(), line.clone()]).is_err()
    );
    let mut zero = line;
    zero.quantity = "0".parse().unwrap();
    assert!(OrderTracking::new("o".into(), "c".into(), "p".into(), vec![zero]).is_err());
}

#[test]
fn unknown_source_time_remains_unknown_and_acceptance_does_not_prove_dispatch() {
    let observation = OrderObservation::from_snapshot("obs".into(), &tracking(), snapshot()).unwrap();
    assert_eq!(observation.source_updated_at, None);
    assert_eq!(observation.observed_at, Instant::from_unix_secs(30));
    assert!(!observation.proves_dispatch());
}

#[test]
fn repeated_queries_are_equal_without_read_time_or_local_event_identity() {
    let first = OrderObservation::from_snapshot("one".into(), &tracking(), snapshot()).unwrap();
    let mut next = snapshot();
    next.stamp.observed_at = Instant::from_unix_secs(90);
    let second = OrderObservation::from_snapshot("two".into(), &tracking(), next).unwrap();
    assert!(first.same_content(&second));
}

#[test]
fn courier_contact_is_not_serialized_and_cross_order_packages_are_rejected() {
    let mut source = snapshot();
    source.value.shipments = Some(vec![Shipment {
        external_shipment_id: None,
        line_ids: vec!["item".into()],
        carrier: Some("同城配送".into()),
        tracking_no: None,
        courier_phone: Some("13800009999".into()),
    }]);
    let observation = OrderObservation::from_snapshot("obs".into(), &tracking(), source.clone()).unwrap();
    assert!(observation.shipments.as_ref().unwrap()[0].has_courier_contact);
    assert!(!serde_json::to_string(&observation).unwrap().contains("13800009999"));
    source.value.shipments.as_mut().unwrap()[0].line_ids = vec!["another-order-item".into()];
    assert!(OrderObservation::from_snapshot("bad".into(), &tracking(), source).is_err());
}

#[test]
fn purchase_change_retirement_prevents_callback_reactivation_and_preserves_evidence() {
    let mut tracking = tracking();
    let now = Instant::from_unix_secs(30);
    tracking.latest_observation_id = Some("old-observation".into());
    tracking.applied_observation_id = Some("old-applied".into());
    tracking.lease_id = Some("in-flight-query".into());
    tracking.lease_until = Some(Instant::from_unix_secs(60));
    assert!(!tracking.can_refresh(false, now));
    tracking.stop_for_purchase_change();
    assert_eq!(tracking.state, TrackingState::Complete);
    assert_eq!(tracking.lease_id, None);
    assert_eq!(tracking.lease_until, None);
    assert!(!tracking.can_refresh(true, now));
    assert_eq!(tracking.latest_observation_id.as_deref(), Some("old-observation"));
    assert_eq!(tracking.applied_observation_id.as_deref(), Some("old-applied"));
    // 正常完成且未被采购变更替代的订单仍须允许后续退款/取消刷新。
    assert!(tracking.can_refresh(false, now));
}
