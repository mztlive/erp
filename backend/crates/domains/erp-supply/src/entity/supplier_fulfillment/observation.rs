//! 来源查询的单调状态应用；缺失来源业务时间保持缺失。

use erp_core::common::time::Instant;

use super::{CancelStatus, FulfillmentStatus, SupplierFulfillmentOrder};
use crate::{Error, Result};

impl SupplierFulfillmentOrder {
    /// 应用已核对身份的供应商最新履约观察，不伪造中间状态或业务时间。
    /// # 参数
    /// `observed` 为供应商明确返回的状态，`changed_at` 仅为来源提供的业务更新时间。
    /// # 返回
    /// 状态实际前进时为 true；重复或可证明的低阶段为 false。
    /// # 错误
    /// 拒单与履约事实冲突，或本地处于异常终态时拒绝自动覆盖。
    pub fn apply_observed_fulfillment(
        &mut self,
        observed: FulfillmentStatus,
        changed_at: Option<Instant>,
    ) -> Result<bool> {
        if self.cancel_status == CancelStatus::Canceled
            && matches!(
                observed,
                FulfillmentStatus::Fulfilling | FulfillmentStatus::Shipped | FulfillmentStatus::Completed
            )
        {
            return Err(Error::BusinessLogicError("供应商取消与后续履约结果冲突，请核对原订单".into()));
        }
        let current = self.fulfillment_status;
        if observed == current {
            return Ok(false);
        }
        if current == FulfillmentStatus::Rejected || observed == FulfillmentStatus::Rejected {
            return self.apply_observed_rejection(observed);
        }
        let old =
            stage(current).ok_or_else(|| Error::BusinessLogicError("供应商订单异常须人工核对".into()))?;
        let new =
            stage(observed).ok_or_else(|| Error::BusinessLogicError("来源履约状态不能自动应用".into()))?;
        if new <= old {
            return Ok(false);
        }
        self.fulfillment_status = observed;
        if observed == FulfillmentStatus::Accepted {
            self.accepted_at = self.accepted_at.or(changed_at);
        }
        if observed == FulfillmentStatus::Completed {
            self.completed_at = self.completed_at.or(changed_at);
        }
        Ok(true)
    }

    /// 应用来源明确确认的取消结果，保持取消轨道独立。
    /// # 参数
    /// `observed` 为真实订单查询的取消轨道。
    /// # 返回
    /// 已变化返回 true；重复或来源未提供进展返回 false。
    /// # 错误
    /// 本地已有另一取消终态，或已经完成履约后来源声称取消时拒绝。
    pub fn apply_observed_cancellation(&mut self, observed: CancelStatus) -> Result<bool> {
        if observed == self.cancel_status || observed == CancelStatus::None {
            return Ok(false);
        }
        if self.cancel_status == CancelStatus::Canceled {
            return Ok(false);
        }
        if self.fulfillment_status == FulfillmentStatus::Completed
            || !matches!(self.cancel_status, CancelStatus::None | CancelStatus::CancelPending)
        {
            return Err(Error::BusinessLogicError("供应商取消结果与当前订单事实冲突".into()));
        }
        self.cancel_status = observed;
        Ok(true)
    }

    fn apply_observed_rejection(&mut self, observed: FulfillmentStatus) -> Result<bool> {
        if observed == FulfillmentStatus::Rejected
            && matches!(
                self.fulfillment_status,
                FulfillmentStatus::Received
                    | FulfillmentStatus::Submitting
                    | FulfillmentStatus::ResultUnknown
            )
        {
            self.fulfillment_status = observed;
            return Ok(true);
        }
        Err(Error::BusinessLogicError("供应商拒单结果与已有履约事实冲突".into()))
    }
}

fn stage(status: FulfillmentStatus) -> Option<u8> {
    match status {
        FulfillmentStatus::Received => Some(0),
        FulfillmentStatus::Submitting | FulfillmentStatus::ResultUnknown => Some(1),
        FulfillmentStatus::Accepted => Some(2),
        FulfillmentStatus::Fulfilling => Some(3),
        FulfillmentStatus::Shipped => Some(4),
        FulfillmentStatus::Completed => Some(5),
        FulfillmentStatus::Rejected | FulfillmentStatus::Exception => None,
    }
}

#[cfg(test)]
mod tests {
    use erp_core::ids::{SupplierAccountId, SupplierApiConnectionId, SupplierFulfillmentOrderId};

    use super::*;
    use crate::entity::supplier_fulfillment::SupplierFulfillmentOrderData;

    fn order() -> SupplierFulfillmentOrder {
        SupplierFulfillmentOrder::new(
            SupplierFulfillmentOrderId::new("order"),
            SupplierFulfillmentOrderData::submitting(
                "PO-1",
                SupplierAccountId::new("supplier"),
                SupplierApiConnectionId::new("connection"),
                1,
                Instant::from_unix_secs(1),
                "cipher",
                "fingerprint",
            )
            .with_follow_up("buyer", "procurement"),
        )
        .unwrap()
    }

    #[test]
    fn query_completion_does_not_invent_missing_source_milestones() {
        let mut order = order();
        assert!(order.apply_observed_fulfillment(FulfillmentStatus::Completed, None).unwrap());
        assert_eq!(order.fulfillment_status, FulfillmentStatus::Completed);
        assert_eq!(order.completed_at, None);
        assert_eq!(order.accepted_at, None);
        assert!(!order.apply_observed_fulfillment(FulfillmentStatus::Accepted, None).unwrap());
        assert_eq!(order.fulfillment_status, FulfillmentStatus::Completed);
    }

    #[test]
    fn exact_source_time_and_repeated_state_are_preserved() {
        let mut order = order();
        let time = Instant::from_unix_secs(30);
        order.apply_observed_fulfillment(FulfillmentStatus::Accepted, Some(time)).unwrap();
        assert_eq!(order.accepted_at, Some(time));
        assert!(
            !order
                .apply_observed_fulfillment(FulfillmentStatus::Accepted, Some(Instant::from_unix_secs(90)))
                .unwrap()
        );
        assert_eq!(order.accepted_at, Some(time));
    }

    #[test]
    fn rejection_and_cancellation_conflicts_cannot_erase_fulfillment() {
        let mut order = order();
        order.apply_observed_fulfillment(FulfillmentStatus::Completed, None).unwrap();
        assert!(order.apply_observed_fulfillment(FulfillmentStatus::Rejected, None).is_err());
        assert!(order.apply_observed_cancellation(CancelStatus::Canceled).is_err());
        assert_eq!(order.cancel_status, CancelStatus::None);
    }

    #[test]
    fn external_cancellation_is_not_an_erp_refund() {
        let mut order = order();
        assert!(order.apply_observed_cancellation(CancelStatus::Canceled).unwrap());
        assert!(!order.apply_observed_cancellation(CancelStatus::Canceled).unwrap());
        assert_eq!(order.fulfillment_status, FulfillmentStatus::Submitting);
        assert_eq!(order.refund_status, crate::entity::supplier_fulfillment::RefundStatus::None);
    }

    #[test]
    fn cancelled_order_cannot_resume_shipment_from_a_partial_later_snapshot() {
        let mut order = order();
        order.apply_observed_fulfillment(FulfillmentStatus::Accepted, None).unwrap();
        order.apply_observed_cancellation(CancelStatus::Canceled).unwrap();
        assert!(order.apply_observed_fulfillment(FulfillmentStatus::Shipped, None).is_err());
        assert_eq!(order.fulfillment_status, FulfillmentStatus::Accepted);
        assert_eq!(order.cancel_status, CancelStatus::Canceled);
    }
}
