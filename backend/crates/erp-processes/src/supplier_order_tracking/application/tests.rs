use std::sync::Mutex;

use erp_core::ids::{SupplierAccountId, SupplierApiConnectionId, SupplierFulfillmentOrderId};
use erp_supply::api_order::{ApiOrderExecution, ApiOrderStage};
use erp_supply::entity::supplier_fulfillment::{
    FulfillmentStatus, RefundStatus, SupplierFulfillmentOrderData,
};
use erp_supply::order_tracking::{TrackingSourceLine, TrackingState};
use erp_supply::ports::connector::common::{Snapshot, SourceRevision, SourceStamp};
use erp_supply::ports::connector::order::{OrderReference, OrderSnapshot};
use persistence_core::NoTransaction;

use super::*;

struct Application {
    order: SupplierFulfillmentOrder,
    steps: Mutex<Vec<&'static str>>,
    saved: Mutex<Option<OrderTracking>>,
}

#[async_trait]
impl ObservationApplication for Application {
    async fn cancelled_order(
        &self,
        _id: &str,
        _executor: &mut dyn Executor,
    ) -> Result<SupplierFulfillmentOrder> {
        self.steps.lock().unwrap().push("load_order");
        Ok(self.order.clone())
    }
    async fn deliveries(
        &self,
        _tracking: &OrderTracking,
        _observation: &OrderObservation,
        _executor: &mut dyn Executor,
    ) -> Result<bool> {
        self.steps.lock().unwrap().push("deliveries");
        Ok(false)
    }
    async fn checkpoint(&self, tracking: &mut OrderTracking, _executor: &mut dyn Executor) -> Result<()> {
        self.steps.lock().unwrap().push("checkpoint");
        *self.saved.lock().unwrap() = Some(tracking.clone());
        Ok(())
    }
}

fn cancellation() -> (Application, OrderTracking, OrderObservation, ApiOrderExecution) {
    let mut tracking = OrderTracking::new(
        "order".into(),
        "connection".into(),
        "purchase".into(),
        vec![TrackingSourceLine {
            fulfillment_item_id: "item".into(),
            purchase_order_revision_line_id: "line".into(),
            sales_order_line_id: Some("sales".into()),
            delivery_id: Some("delivery".into()),
            quantity: "1".parse().unwrap(),
        }],
    )
    .unwrap();
    let observation = OrderObservation::from_snapshot(
        "cancel-observation".into(),
        &tracking,
        Snapshot {
            stamp: SourceStamp {
                revision: SourceRevision::Unversioned,
                changed_at: None,
                observed_at: Instant::from_unix_secs(20),
            },
            value: OrderSnapshot {
                reference: OrderReference {
                    merchant_order_no: "order".into(),
                    external_order_no: "external".into(),
                },
                amounts: None,
                fulfillment: None,
                cancellation: Some(CancelStatus::Canceled),
                refund: None,
                payment: None,
                shipments: None,
            },
        },
    )
    .unwrap();
    let mut data = SupplierFulfillmentOrderData::submitting(
        "order",
        SupplierAccountId::new("supplier"),
        SupplierApiConnectionId::new("connection"),
        1,
        Instant::from_unix_secs(1),
        "cipher",
        "fingerprint",
    )
    .with_follow_up("buyer", "procurement");
    data.external_order_no = Some("external".into());
    let mut order = SupplierFulfillmentOrder::new(SupplierFulfillmentOrderId::new("order"), data).unwrap();
    order.apply_observed_fulfillment(FulfillmentStatus::Accepted, None).unwrap();
    order.apply_observed_cancellation(observation.cancellation.unwrap()).unwrap();
    tracking.latest_observation_id = Some(observation.base.id.clone());
    tracking.last_error = Some("等待取消结果应用".into());
    tracking.lease_id = Some("query-lease".into());
    let mut execution = ApiOrderExecution::queued(
        "order".into(),
        "purchase".into(),
        "revision".into(),
        "connection".into(),
        "supplier".into(),
        vec![],
    );
    execution.stage = ApiOrderStage::Observe;
    let application = Application { order, steps: Mutex::new(Vec::new()), saved: Mutex::new(None) };
    (application, tracking, observation, execution)
}

#[tokio::test]
async fn real_cancellation_application_finishes_checkpoint_and_enables_controlled_purchase_change() {
    let (application, mut tracking, observation, mut execution) = cancellation();
    assert_eq!(tracking.applied_observation_id, None);
    apply_current_observation(
        &application,
        &mut tracking,
        &observation,
        Instant::from_unix_secs(30),
        &mut NoTransaction,
    )
    .await
    .unwrap();
    assert_eq!(*application.steps.lock().unwrap(), vec!["load_order", "checkpoint"]);
    let saved = application.saved.lock().unwrap().clone().unwrap();
    assert_eq!(saved.applied_observation_id, saved.latest_observation_id);
    assert_eq!(saved.last_error, None);
    assert_eq!(saved.state, TrackingState::Complete);
    assert_eq!(saved.lease_id, None);
    execution
        .stop_cancelled_for_purchase_change("replacement", &application.order, &saved, &observation)
        .unwrap();
    assert_eq!(execution.superseded_by_purchase_revision_id.as_deref(), Some("replacement"));
}

#[tokio::test]
async fn shipped_or_unresolved_refund_cancellation_never_records_success_or_delivery() {
    let (mut application, mut tracking, observation, _) = cancellation();
    application.order.cancel_status = CancelStatus::None;
    application.order.apply_observed_fulfillment(FulfillmentStatus::Shipped, None).unwrap();
    application.order.apply_observed_cancellation(CancelStatus::Canceled).unwrap();
    assert!(
        apply_current_observation(
            &application,
            &mut tracking,
            &observation,
            Instant::now(),
            &mut NoTransaction
        )
        .await
        .is_err()
    );
    assert_eq!(*application.steps.lock().unwrap(), vec!["load_order"]);
    assert!(application.saved.lock().unwrap().is_none());
    assert_eq!(tracking.applied_observation_id, None);

    let (mut application, mut tracking, mut observation, _) = cancellation();
    application.order.refund_status = RefundStatus::RefundPending;
    observation.refund = Some(RefundStatus::RefundPending);
    assert!(
        apply_current_observation(
            &application,
            &mut tracking,
            &observation,
            Instant::now(),
            &mut NoTransaction
        )
        .await
        .is_err()
    );
    assert_eq!(*application.steps.lock().unwrap(), vec!["load_order"]);
    assert!(application.saved.lock().unwrap().is_none());
}
