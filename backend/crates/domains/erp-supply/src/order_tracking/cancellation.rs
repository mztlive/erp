//! 订单树取消必须逐个叶子核实，根单状态不代替子单与资金进度。

use super::OrderObservation;
use crate::entity::supplier_fulfillment::{CancelStatus, FulfillmentStatus, RefundStatus};
use crate::{Error, Result};

impl OrderObservation {
    /// 核对完整当前子单均已取消且没有发货及未决返款。
    /// # 参数
    /// 无；订单树已在来源观察构建与应用时验证数量守恒。
    /// # 返回
    /// 无订单树的旧协议沿用原取消校验；有树时所有叶子必须具备明确事实。
    /// # 错误
    /// 缺失任一取消、发货或返款事实，或仍有处理中子单时拒绝。
    pub fn ensure_leaf_cancellations(&self) -> Result<()> {
        let Some(tree) = &self.topology else {
            return Ok(());
        };
        let leaves = tree.action_targets();
        if leaves.is_empty()
            || leaves.iter().any(|node| {
                node.cancellation != Some(CancelStatus::Canceled)
                    || !matches!(
                        node.fulfillment,
                        Some(
                            FulfillmentStatus::Received
                                | FulfillmentStatus::Submitting
                                | FulfillmentStatus::Accepted
                                | FulfillmentStatus::Rejected
                        )
                    )
                    || !matches!(node.refund, Some(RefundStatus::None | RefundStatus::Refunded))
            })
        {
            return Err(Error::BusinessLogicError(
                "相关子单尚未全部核实取消，或存在发货、返款待核实，请先处理原单".into(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use entity_core::BaseModel;
    use erp_core::common::time::Instant;

    use super::*;
    use crate::ports::connector::common::{SourceRevision, SupplierSku};
    use crate::ports::connector::order_tree::{OrderNode, OrderNodeLine, OrderNodeRole, OrderTree};

    fn node(id: &str, cancel: CancelStatus) -> OrderNode {
        OrderNode {
            external_order_no: id.into(),
            parent_order_no: Some("root".into()),
            role: OrderNodeRole::Leaf,
            lines: vec![OrderNodeLine {
                line_id: "line".into(),
                sku: SupplierSku { product_id: None, spec_id: "sku".into() },
                quantity: "1".parse().unwrap(),
            }],
            confirmation: None,
            amounts: None,
            fulfillment: Some(FulfillmentStatus::Accepted),
            cancellation: Some(cancel),
            refund: Some(RefundStatus::None),
        }
    }
    fn observation() -> OrderObservation {
        OrderObservation {
            base: BaseModel::new("observation".into()),
            order_id: "order".into(),
            connection_id: "connection".into(),
            merchant_order_no: "merchant".into(),
            external_order_no: "root".into(),
            source_revision: SourceRevision::Unversioned,
            source_updated_at: None,
            observed_at: Instant::now(),
            fulfillment: None,
            cancellation: Some(CancelStatus::Canceled),
            refund: None,
            payment_confirmed: None,
            shipments: None,
            complete_line_coverage: false,
            source_evidence: None,
            topology: Some(OrderTree {
                root_order_no: "root".into(),
                version: 1,
                nodes: vec![node("a", CancelStatus::Canceled), node("b", CancelStatus::Canceled)],
            }),
        }
    }
    #[test]
    fn root_cancelled_does_not_hide_partial_child_or_pending_refund() {
        let original = observation();
        original.ensure_leaf_cancellations().unwrap();

        let mut partial = original.clone();
        partial.topology.as_mut().unwrap().nodes[1].cancellation = None;
        assert!(partial.ensure_leaf_cancellations().is_err());

        let mut pending = original.clone();
        pending.topology.as_mut().unwrap().nodes[1].refund = Some(RefundStatus::RefundPending);
        assert!(pending.ensure_leaf_cancellations().is_err());

        let mut shipped = original.clone();
        shipped.topology.as_mut().unwrap().nodes[0].fulfillment = Some(FulfillmentStatus::Shipped);
        assert!(shipped.ensure_leaf_cancellations().is_err());
    }
}
