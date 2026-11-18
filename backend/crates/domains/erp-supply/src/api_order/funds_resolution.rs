//! 资金风险释放只消费已经核验的供应商终态，独立于ERP发货数量应用。

use erp_core::common::time::Instant;

use super::{ApiOrderExecution, ApiOrderStage, CancellationActionState, ConfirmationActionState};
use crate::entity::supplier_fulfillment::{CancelStatus, FulfillmentStatus, SupplierFulfillmentOrder};
use crate::order_tracking::{OrderObservation, OrderTracking};
use crate::{Error, Result};

/// 供应商完整终态；不表示余额没有冻结或后续扣款，财务仍须单独核实。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionFundsFinality {
    NeverSubmitted,
    Completed,
    Cancelled,
}

impl ApiOrderExecution {
    /// 核对原执行尚未提交或已有完整供应商终态，未知动作严格阻断。
    /// 返回已经核验的供应商终态及最后外部观察时刻。
    /// 缺原单、身份不一致、仍有未知写动作、未核验拓扑或叶子未全部终止时拒绝。
    /// 预期终态、原履约单及同事务读取的最新来源跟踪和观察；未提交时后二者可以缺失。
    pub fn ensure_funds_finality(
        &self,
        expected: ExecutionFundsFinality,
        order: &SupplierFulfillmentOrder,
        tracking: Option<&OrderTracking>,
        observation: Option<&OrderObservation>,
    ) -> Result<Instant> {
        let (actual, observed_at) = self.funds_finality(order, tracking, observation)?;
        if expected != actual {
            return Err(Error::BusinessLogicError("释放原因与已经核实的原供应商执行终态不一致".into()));
        }
        Ok(observed_at)
    }

    fn funds_finality(
        &self,
        order: &SupplierFulfillmentOrder,
        tracking: Option<&OrderTracking>,
        observation: Option<&OrderObservation>,
    ) -> Result<(ExecutionFundsFinality, Instant)> {
        if order.base.id != self.base.id
            || order.connection_id.as_ref() != self.connection_id
            || order.supplier_id.as_ref() != self.supplier_id
        {
            return Err(Error::BusinessLogicError("资金风险核实的原执行与供应商订单归属不一致".into()));
        }

        if self.ensure_unsent_for_purchase_change().is_ok() && order.external_order_no.is_none() {
            let time = i64::try_from(self.base.created_at)
                .map_err(|_| Error::ValidationError("原执行时间越界".into()))?;
            return Ok((ExecutionFundsFinality::NeverSubmitted, Instant::try_from_unix_secs(time)?));
        }

        self.ensure_no_pending_funds_action()?;
        let (Some(tracking), Some(observation)) = (tracking, observation) else {
            return Err(Error::BusinessLogicError("原供应商订单终态尚未取得可靠查询证据".into()));
        };
        self.ensure_funds_observation(order, tracking, observation)?;

        if observation.cancellation == Some(CancelStatus::Canceled) {
            observation.ensure_safe_cancellation(order)?;
            return Ok((ExecutionFundsFinality::Cancelled, observation.observed_at));
        }

        let tree = self
            .order_tree
            .as_ref()
            .ok_or_else(|| Error::BusinessLogicError("原执行缺少完整叶子终态".into()))?;
        let leaves = tree.leaves();
        if observation.fulfillment != Some(FulfillmentStatus::Completed)
            || order.fulfillment_status != FulfillmentStatus::Completed
            || leaves.is_empty()
            || leaves.iter().any(|leaf| leaf.fulfillment != Some(FulfillmentStatus::Completed))
        {
            return Err(Error::BusinessLogicError("原供应商全部有效子单尚未核实完成，不能释放尾款".into()));
        }
        Ok((ExecutionFundsFinality::Completed, observation.observed_at))
    }

    fn ensure_no_pending_funds_action(&self) -> Result<()> {
        if !matches!(self.stage, ApiOrderStage::Observe | ApiOrderStage::Stopped)
            || self.pending_order_tree.is_some()
            || self.confirmations.values().any(|action| action.state == ConfirmationActionState::SentUnknown)
            || self.cancellation_actions.values().any(|action| {
                matches!(
                    action.state,
                    CancellationActionState::Ready
                        | CancellationActionState::SentUnknown
                        | CancellationActionState::Accepted
                )
            })
        {
            return Err(Error::BusinessLogicError("原执行仍有未发送或结果未知的动作，请先查询原结果".into()));
        }
        Ok(())
    }

    fn ensure_funds_observation(
        &self,
        order: &SupplierFulfillmentOrder,
        tracking: &OrderTracking,
        observation: &OrderObservation,
    ) -> Result<()> {
        if tracking.base.id != self.base.id
            || tracking.connection_id != self.connection_id
            || tracking.purchase_order_id != self.purchase_order_id
            || tracking.latest_observation_id.as_deref() != Some(observation.base.id.as_str())
            || tracking.last_verified_at.is_none_or(|time| time < observation.observed_at)
            || observation.order_id != self.base.id
            || observation.connection_id != self.connection_id
            || observation.merchant_order_no != self.base.id
            || order.external_order_no.as_deref() != Some(observation.external_order_no.as_str())
            || observation.source_evidence.is_none()
            || self.order_tree.as_ref().zip(observation.topology.as_ref()).is_none_or(
                |(current, observed)| {
                    current.root_order_no != observed.root_order_no || current.nodes != observed.nodes
                },
            )
        {
            return Err(Error::BusinessLogicError("原供应商最新完整观察与执行身份或拓扑不一致".into()));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use erp_core::ids::{SupplierAccountId, SupplierApiConnectionId, SupplierFulfillmentOrderId};

    use super::*;
    use crate::entity::supplier_fulfillment::{RefundStatus, SupplierFulfillmentOrderData};
    use crate::order_tracking::TrackingSourceLine;
    use crate::ports::connector::common::{Snapshot, SourceRevision, SourceStamp, SupplierSku};
    use crate::ports::connector::order::{EncryptedOrderEvidence, OrderReference, OrderSnapshot};
    use crate::ports::connector::order_tree::{OrderNode, OrderNodeLine, OrderNodeRole, OrderTree};

    fn fixture() -> (ApiOrderExecution, SupplierFulfillmentOrder, OrderTracking, OrderObservation) {
        let mut execution = ApiOrderExecution::queued(
            "execution".into(),
            "purchase".into(),
            "revision".into(),
            "connection".into(),
            "supplier".into(),
            vec![],
        );
        execution.stage = ApiOrderStage::Observe;
        let tree = OrderTree {
            root_order_no: "external".into(),
            version: 1,
            nodes: vec![OrderNode {
                external_order_no: "external".into(),
                parent_order_no: None,
                role: OrderNodeRole::Leaf,
                lines: vec![OrderNodeLine {
                    line_id: "line".into(),
                    sku: SupplierSku { product_id: None, spec_id: "sku".into() },
                    quantity: "1".parse().unwrap(),
                }],
                confirmation: None,
                amounts: None,
                fulfillment: Some(FulfillmentStatus::Completed),
                cancellation: None,
                refund: None,
            }],
        };
        execution.order_tree = Some(tree.clone());
        let order = SupplierFulfillmentOrder::new(
            SupplierFulfillmentOrderId::new("execution"),
            SupplierFulfillmentOrderData {
                fulfillment_order_no: "execution".into(),
                supplier_id: SupplierAccountId::new("supplier"),
                connection_id: SupplierApiConnectionId::new("connection"),
                split_no: 1,
                fulfillment_status: FulfillmentStatus::Completed,
                cancel_status: CancelStatus::None,
                refund_status: RefundStatus::None,
                external_order_no: Some("external".into()),
                submitted_at: Some(Instant::from_unix_secs(80)),
                accepted_at: Some(Instant::from_unix_secs(90)),
                completed_at: Some(Instant::from_unix_secs(100)),
                address_snapshot_encrypted: "encrypted".into(),
                address_snapshot_fingerprint: "fingerprint".into(),
                follow_up_user_id: "buyer".into(),
                business_org_unit_id: "org".into(),
            },
        )
        .unwrap();
        let mut tracking = OrderTracking::new(
            "execution".into(),
            "connection".into(),
            "purchase".into(),
            vec![TrackingSourceLine {
                fulfillment_item_id: "line".into(),
                purchase_order_revision_line_id: "purchase-line".into(),
                sales_order_line_id: None,
                delivery_id: None,
                quantity: "1".parse().unwrap(),
            }],
        )
        .unwrap();
        let observation = OrderObservation::from_snapshot(
            "observation".into(),
            &tracking,
            Snapshot {
                stamp: SourceStamp {
                    revision: SourceRevision::Unversioned,
                    changed_at: None,
                    observed_at: Instant::from_unix_secs(100),
                },
                value: OrderSnapshot {
                    reference: OrderReference {
                        merchant_order_no: "execution".into(),
                        external_order_no: "external".into(),
                    },
                    source_evidence: Some(EncryptedOrderEvidence {
                        content_fingerprint: "digest".into(),
                        encrypted_payload: "cipher".into(),
                    }),
                    amounts: None,
                    fulfillment: Some(FulfillmentStatus::Completed),
                    cancellation: None,
                    refund: None,
                    payment: None,
                    topology: Some(tree),
                    confirmation: None,
                    complete_line_coverage: false,
                    shipments: None,
                },
            },
        )
        .unwrap();
        tracking.latest_observation_id = Some(observation.base.id.clone());
        tracking.last_verified_at = Some(observation.observed_at);
        (execution, order, tracking, observation)
    }

    #[test]
    fn complete_source_order_is_independent_of_unapplied_erp_shipping() {
        let (execution, order, tracking, observation) = fixture();
        assert!(tracking.applied_observation_id.is_none());
        assert!(tracking.applied_shipments.is_empty());
        assert_eq!(
            execution
                .ensure_funds_finality(
                    ExecutionFundsFinality::Completed,
                    &order,
                    Some(&tracking),
                    Some(&observation)
                )
                .unwrap(),
            observation.observed_at
        );
    }

    #[test]
    fn unknown_actions_unresolved_or_nonterminal_leaves_never_clear_funds() {
        let (mut execution, order, tracking, observation) = fixture();
        execution.stage = ApiOrderStage::ConfirmationUnknown;
        assert!(
            execution
                .ensure_funds_finality(
                    ExecutionFundsFinality::Completed,
                    &order,
                    Some(&tracking),
                    Some(&observation)
                )
                .is_err()
        );

        execution.stage = ApiOrderStage::Observe;
        execution.pending_order_tree = execution.order_tree.clone();
        assert!(
            execution
                .ensure_funds_finality(
                    ExecutionFundsFinality::Completed,
                    &order,
                    Some(&tracking),
                    Some(&observation)
                )
                .is_err()
        );

        execution.pending_order_tree = None;
        let mut nonterminal = observation.clone();
        nonterminal.topology.as_mut().unwrap().nodes[0].fulfillment = Some(FulfillmentStatus::Shipped);
        assert!(
            execution
                .ensure_funds_finality(
                    ExecutionFundsFinality::Completed,
                    &order,
                    Some(&tracking),
                    Some(&nonterminal)
                )
                .is_err()
        );

        assert!(
            execution
                .ensure_funds_finality(
                    ExecutionFundsFinality::Cancelled,
                    &order,
                    Some(&tracking),
                    Some(&observation)
                )
                .is_err()
        );
    }
}
