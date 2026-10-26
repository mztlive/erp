//! 采购变更只采用已正式应用的供应商取消证据，不接受人工取消声明。

use super::{ApiOrderExecution, ApiOrderStage};
use crate::entity::supplier_fulfillment::{
    CancelStatus, FulfillmentStatus, RefundStatus, SupplierFulfillmentOrder,
};
use crate::order_tracking::{OrderObservation, OrderTracking};
use crate::{Error, Result};

impl ApiOrderExecution {
    /// 以真实查询且已应用的取消结果停止原执行，保留外部身份和动作历史。
    /// # 参数
    /// 替代版本、当前正式订单、同身份跟踪检查点及其已应用的不可变观察。
    /// # 返回
    /// 取消已确认、没有已知发货或未决退款时，标记原执行由新版本替代。
    /// # 错误
    /// 身份不符、取消未应用、创建/通知未知、已履约或退款未决时拒绝。
    pub fn stop_cancelled_for_purchase_change(
        &mut self,
        replacement_revision: &str,
        order: &SupplierFulfillmentOrder,
        tracking: &OrderTracking,
        observation: &OrderObservation,
    ) -> Result<()> {
        let known = matches!(
            self.stage,
            ApiOrderStage::Observe | ApiOrderStage::ConfirmPayment | ApiOrderStage::Stopped
        );
        if !known
            || !cancellation_identity_matches(self, order, tracking, observation)
            || order.cancel_status != CancelStatus::Canceled
            || observation.cancellation != Some(CancelStatus::Canceled)
            || !before_dispatch(order.fulfillment_status)
            || observation.fulfillment.is_some_and(|status| !before_dispatch(status))
            || observation.shipments.as_ref().is_some_and(|shipments| !shipments.is_empty())
            || !refund_resolved(order.refund_status, observation.refund)
        {
            return Err(Error::BusinessLogicError(
                "请先在供应商侧处理原单并等待系统核实取消；已发货、结果未知或退款未决时不能重新发单".into(),
            ));
        }
        self.mark_superseded(replacement_revision)
    }
}

fn cancellation_identity_matches(
    execution: &ApiOrderExecution,
    order: &SupplierFulfillmentOrder,
    tracking: &OrderTracking,
    observation: &OrderObservation,
) -> bool {
    order.base.id == execution.base.id
        && order.connection_id.as_ref() == execution.connection_id
        && order.supplier_id.as_ref() == execution.supplier_id
        && order.external_order_no.as_deref() == Some(observation.external_order_no.as_str())
        && tracking.base.id == execution.base.id
        && tracking.connection_id == execution.connection_id
        && tracking.purchase_order_id == execution.purchase_order_id
        && tracking.last_error.is_none()
        && tracking.latest_observation_id.as_deref() == Some(observation.base.id.as_str())
        && tracking.applied_observation_id == tracking.latest_observation_id
        && observation.order_id == execution.base.id
        && observation.connection_id == execution.connection_id
        && observation.merchant_order_no == execution.base.id
        && !observation.external_order_no.is_empty()
}

fn before_dispatch(status: FulfillmentStatus) -> bool {
    matches!(
        status,
        FulfillmentStatus::Received
            | FulfillmentStatus::Submitting
            | FulfillmentStatus::Accepted
            | FulfillmentStatus::Rejected
    )
}

fn refund_resolved(current: RefundStatus, observed: Option<RefundStatus>) -> bool {
    matches!(
        (current, observed),
        (RefundStatus::None, None | Some(RefundStatus::None))
            | (RefundStatus::Refunded, Some(RefundStatus::Refunded))
    )
}

#[cfg(test)]
mod tests {
    use entity_core::BaseModel;
    use erp_core::common::time::Instant;
    use erp_core::ids::{SupplierAccountId, SupplierApiConnectionId, SupplierFulfillmentOrderId};

    use super::*;
    use crate::entity::supplier_fulfillment::SupplierFulfillmentOrderData;
    use crate::order_tracking::TrackingState;
    use crate::ports::connector::common::SourceRevision;

    fn cancelled() -> (ApiOrderExecution, SupplierFulfillmentOrder, OrderTracking, OrderObservation) {
        let mut execution =
            ApiOrderExecution::queued("e".into(), "p".into(), "r".into(), "c".into(), "s".into(), vec![]);
        execution.stage = ApiOrderStage::Observe;
        execution.action_hashes.insert("create".into(), "original-hash".into());
        let order = SupplierFulfillmentOrder::new(
            SupplierFulfillmentOrderId::new("e"),
            SupplierFulfillmentOrderData {
                fulfillment_order_no: "API-1".into(),
                supplier_id: SupplierAccountId::new("s"),
                connection_id: SupplierApiConnectionId::new("c"),
                split_no: 1,
                fulfillment_status: FulfillmentStatus::Accepted,
                cancel_status: CancelStatus::Canceled,
                refund_status: RefundStatus::None,
                external_order_no: Some("external-1".into()),
                submitted_at: Some(Instant::now()),
                accepted_at: Some(Instant::now()),
                completed_at: None,
                address_snapshot_encrypted: "encrypted".into(),
                address_snapshot_fingerprint: "fingerprint".into(),
                follow_up_user_id: String::new(),
                business_org_unit_id: String::new(),
            },
        )
        .unwrap();
        let tracking = OrderTracking {
            base: BaseModel::new("e".into()),
            connection_id: "c".into(),
            purchase_order_id: "p".into(),
            source_lines: vec![],
            state: TrackingState::Polling,
            poll_after: Instant::now(),
            lease_id: None,
            lease_until: None,
            latest_observation_id: Some("obs".into()),
            applied_observation_id: Some("obs".into()),
            last_error: None,
            last_verified_at: Some(Instant::now()),
        };
        let observation = OrderObservation {
            base: BaseModel::new("obs".into()),
            order_id: "e".into(),
            connection_id: "c".into(),
            merchant_order_no: "e".into(),
            external_order_no: "external-1".into(),
            source_revision: SourceRevision::Unversioned,
            source_updated_at: None,
            observed_at: Instant::now(),
            fulfillment: None,
            cancellation: Some(CancelStatus::Canceled),
            refund: None,
            payment_confirmed: Some(true),
            shipments: None,
        };
        (execution, order, tracking, observation)
    }

    #[test]
    fn applied_cancellation_can_retire_without_erasing_external_identity() {
        let (mut execution, order, tracking, observation) = cancelled();
        execution
            .stop_cancelled_for_purchase_change("new-revision", &order, &tracking, &observation)
            .unwrap();
        assert_eq!(execution.stage, ApiOrderStage::Stopped);
        assert_eq!(execution.superseded_by_purchase_revision_id.as_deref(), Some("new-revision"));
        assert_eq!(execution.action_hashes["create"], "original-hash");
        assert_eq!(order.external_order_no.as_deref(), Some("external-1"));
        execution
            .stop_cancelled_for_purchase_change("new-revision", &order, &tracking, &observation)
            .unwrap();
        assert!(
            execution
                .stop_cancelled_for_purchase_change("other-revision", &order, &tracking, &observation)
                .is_err()
        );
    }

    #[test]
    fn manual_or_unapplied_cancellation_and_unknown_execution_still_block() {
        let (execution, order, tracking, observation) = cancelled();
        let mut unapplied = tracking.clone();
        unapplied.applied_observation_id = None;
        assert!(
            execution
                .clone()
                .stop_cancelled_for_purchase_change("new", &order, &unapplied, &observation)
                .is_err()
        );
        let mut no_cancellation = observation.clone();
        no_cancellation.cancellation = None;
        assert!(
            execution
                .clone()
                .stop_cancelled_for_purchase_change("new", &order, &tracking, &no_cancellation)
                .is_err()
        );
        for stage in [ApiOrderStage::CreationUnknown, ApiOrderStage::PaymentUnknown] {
            let mut unknown = execution.clone();
            unknown.stage = stage;
            assert!(
                unknown.stop_cancelled_for_purchase_change("new", &order, &tracking, &observation).is_err()
            );
        }
    }

    #[test]
    fn dispatch_and_unresolved_refund_or_wrong_identity_block_replacement() {
        let (execution, order, tracking, observation) = cancelled();
        let mut shipped = order.clone();
        shipped.fulfillment_status = FulfillmentStatus::Shipped;
        assert!(
            execution
                .clone()
                .stop_cancelled_for_purchase_change("new", &shipped, &tracking, &observation)
                .is_err()
        );
        for refund in [
            RefundStatus::RefundPending,
            RefundStatus::Partial,
            RefundStatus::RefundFailed,
            RefundStatus::Manual,
        ] {
            let mut unresolved = order.clone();
            unresolved.refund_status = refund;
            assert!(
                execution
                    .clone()
                    .stop_cancelled_for_purchase_change("new", &unresolved, &tracking, &observation)
                    .is_err()
            );
        }
        let mut wrong = observation.clone();
        wrong.external_order_no = "other".into();
        assert!(
            execution.clone().stop_cancelled_for_purchase_change("new", &order, &tracking, &wrong).is_err()
        );
    }
}
