//! 供应商发货数量与已应用交付的守恒证据；支持子单声明及明确包裹数量。

use std::collections::{BTreeMap, HashSet};

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use super::{OrderObservation, OrderShipment, OrderTracking};
use crate::entity::supplier_fulfillment::FulfillmentStatus;
use crate::ports::connector::order::ShipmentLine;
use crate::{Error, Result};

/// 已在同一事务中形成 ERP 发货的稳定来源事实及数量。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppliedShipment {
    pub source_order_no: String,
    pub shipment_id: String,
    pub lines: Vec<ShipmentLine>,
    pub delivery_ids: Vec<String>,
}

/// 仅由适配器核验数量生成的待应用发货；不含个人地址或联系方式。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingShipment {
    pub source_order_no: String,
    pub shipment_id: String,
    pub lines: Vec<ShipmentLine>,
    pub carrier: Option<String>,
    pub tracking_no: Option<String>,
}

impl OrderTracking {
    /// 核对全部包裹、排除已应用身份并验证累计数量不超过冻结行。
    /// # 参数
    /// `observation` 为当前来源观察；调用方须在同一事务保存应用凭据。
    /// # 返回
    /// 仅返回可证明数量且尚未应用的包裹；缺少数量的包裹只用于轨迹。
    /// # 错误
    /// 同一包裹异载荷、来源行不明、数量非正、重复或累计超发时拒绝。
    pub fn pending_shipments(&self, observation: &OrderObservation) -> Result<Vec<PendingShipment>> {
        if let Some(tree) = &observation.topology {
            tree.validate_tracking(&self.source_lines)?;
        }
        let mut quantities = self.applied_quantities()?;
        let mut pending = Vec::new();
        let mut seen = HashSet::new();
        let mut per_order = BTreeMap::new();
        for applied in &self.applied_shipments {
            add_order_lines(&mut per_order, &applied.source_order_no, &applied.lines)?;
        }
        for shipment in observation.shipments.iter().flatten() {
            let Some(value) = shipment.quantity_evidence(&observation.external_order_no)? else {
                continue;
            };
            if !seen.insert((value.source_order_no.clone(), value.shipment_id.clone())) {
                return Err(Error::ConflictError("来源包裹身份重复，请核对物流资料".into()));
            }
            if self.applied_shipment(&value)?.is_some() {
                continue;
            }
            add_order_lines(&mut per_order, &value.source_order_no, &value.lines)?;
            ensure_leaf_quantities(observation, &value, &per_order)?;
            add_lines(&mut quantities, &value.lines)?;
            pending.push(value);
        }
        self.ensure_shipment_totals(&quantities)?;
        Ok(pending)
    }

    /// 保存已经完成真实交付的包裹身份；不允许来源重放重复累计。
    /// # 参数
    /// `shipment` 为此前校验的待应用包裹；`delivery_ids` 为同事务生成的发货身份。
    /// # 返回
    /// 追加稳定应用凭据；相同包裹及相同交付重放不产生变更。
    /// # 错误
    /// 交付身份为空、同包裹异载荷或已应用交付冲突时拒绝。
    pub fn record_shipment(&mut self, shipment: &PendingShipment, delivery_ids: Vec<String>) -> Result<()> {
        if delivery_ids.is_empty() || delivery_ids.iter().any(|id| id.trim().is_empty()) {
            return Err(Error::ValidationError("包裹缺少真实发货记录".into()));
        }
        if let Some(original) = self.applied_shipment(shipment)? {
            if original.delivery_ids != delivery_ids {
                return Err(Error::ConflictError("包裹对应的原发货记录不一致".into()));
            }
            return Ok(());
        }
        let mut quantities = self.applied_quantities()?;
        add_lines(&mut quantities, &shipment.lines)?;
        self.ensure_shipment_totals(&quantities)?;
        self.applied_shipments.push(AppliedShipment {
            source_order_no: shipment.source_order_no.clone(),
            shipment_id: shipment.shipment_id.clone(),
            lines: shipment.lines.clone(),
            delivery_ids,
        });
        Ok(())
    }

    /// 判断全部冻结数量是否已由真实 ERP 交付覆盖。
    /// # 参数
    /// 无。
    /// # 返回
    /// 每条冻结行累计应用数量恰好等于批准数量时返回 true。
    /// # 错误
    /// 数量溢出或凭据超发时返回错误。
    pub fn quantities_fully_applied(&self) -> Result<bool> {
        let quantities = self.applied_quantities()?;
        self.ensure_shipment_totals(&quantities)?;
        Ok(self.source_lines.iter().all(|line| {
            quantities.get(&line.fulfillment_item_id).copied().unwrap_or(Decimal::ZERO)
                == line.quantity.to_decimal()
        }))
    }

    /// 核对已发货节点均有对应的已应用数量，通用流程不得自行从状态补造交付。
    /// # 参数
    /// `observation` 为已验证当前拓扑与身份的来源观察。
    /// # 返回
    /// 当前已报告发货节点全部被真实数量覆盖，或尚无发货事实时成功。
    /// # 错误
    /// 已发货节点仍缺商品数量证据时要求核对原发货凭证，未知节点不推导发货。
    pub fn ensure_dispatch_quantities(&self, observation: &OrderObservation) -> Result<()> {
        let missing = if let Some(tree) = &observation.topology {
            let mut applied = BTreeMap::new();
            for shipment in &self.applied_shipments {
                add_order_lines(&mut applied, &shipment.source_order_no, &shipment.lines)?;
            }
            tree.leaves().iter().any(|leaf| {
                matches!(leaf.fulfillment, Some(FulfillmentStatus::Shipped | FulfillmentStatus::Completed))
                    && leaf.lines.iter().any(|line| {
                        applied
                            .get(&(leaf.external_order_no.clone(), line.line_id.clone()))
                            .copied()
                            .unwrap_or(Decimal::ZERO)
                            < line.quantity.to_decimal()
                    })
            })
        } else {
            observation.proves_dispatch() && !self.quantities_fully_applied()?
        };
        if missing {
            return Err(Error::BusinessLogicError(
                "供应商已报告发货进度，但商品发货数量待核实；请核对子单和发货凭证，通过原发货入口办理".into(),
            ));
        }
        Ok(())
    }

    fn applied_shipment(&self, pending: &PendingShipment) -> Result<Option<&AppliedShipment>> {
        let Some(previous) = self.applied_shipments.iter().find(|value| {
            value.source_order_no == pending.source_order_no && value.shipment_id == pending.shipment_id
        }) else {
            return Ok(None);
        };
        if previous.lines != pending.lines {
            return Err(Error::ConflictError("同一来源包裹数量发生变化，请核对原发货记录".into()));
        }
        Ok(Some(previous))
    }

    fn applied_quantities(&self) -> Result<BTreeMap<String, Decimal>> {
        let mut quantities = BTreeMap::new();
        for applied in &self.applied_shipments {
            add_lines(&mut quantities, &applied.lines)?;
        }
        Ok(quantities)
    }

    fn ensure_shipment_totals(&self, quantities: &BTreeMap<String, Decimal>) -> Result<()> {
        for (id, quantity) in quantities {
            let source = self
                .source_lines
                .iter()
                .find(|line| &line.fulfillment_item_id == id)
                .ok_or_else(|| Error::ValidationError("来源包裹包含非原订单明细".into()))?;
            if *quantity > source.quantity.to_decimal() {
                return Err(Error::BusinessLogicError("来源包裹累计数量超过批准数量，请核对分批发货".into()));
            }
        }
        Ok(())
    }
}

impl OrderShipment {
    /// 将包裹的明确数量转换为可去重的来源证据。
    /// # 参数
    /// `root_order_no` 为查询绑定的原始外部单号，用于无子单的旧协议。
    /// # 返回
    /// 缺少数量返回 None；子单数量必须先由适配器根据其专属合同核验。
    /// # 错误
    /// 稳定包裹身份、来源订单或正数量不满足时拒绝自动应用。
    pub fn quantity_evidence(&self, root_order_no: &str) -> Result<Option<PendingShipment>> {
        let Some(lines) = &self.quantity_lines else {
            return Ok(None);
        };
        let source = self.source_order_no.as_deref().unwrap_or(root_order_no);
        let id = self
            .external_shipment_id
            .as_ref()
            .or(self.tracking_no.as_ref())
            .filter(|id| !id.trim().is_empty())
            .ok_or_else(|| Error::ValidationError("按量发货缺少稳定包裹身份".into()))?;
        if source.trim().is_empty() || lines.is_empty() {
            return Err(Error::ValidationError("按量发货缺少来源订单或数量明细".into()));
        }
        let mut checked = BTreeMap::new();
        add_lines(&mut checked, lines)?;
        let mut lines = lines.clone();
        lines.sort_by(|left, right| left.line_id.cmp(&right.line_id));
        Ok(Some(PendingShipment {
            source_order_no: source.into(),
            shipment_id: id.clone(),
            lines,
            carrier: self.carrier.clone(),
            tracking_no: self.tracking_no.clone(),
        }))
    }
}

fn add_lines(quantities: &mut BTreeMap<String, Decimal>, lines: &[ShipmentLine]) -> Result<()> {
    let mut seen = HashSet::new();
    for line in lines {
        if line.line_id.trim().is_empty()
            || line.quantity.to_decimal() <= Decimal::ZERO
            || !seen.insert(&line.line_id)
        {
            return Err(Error::ValidationError("包裹明细重复或数量无效".into()));
        }
        let total = quantities.entry(line.line_id.clone()).or_insert(Decimal::ZERO);
        *total = total
            .checked_add(line.quantity.to_decimal())
            .ok_or_else(|| Error::ValidationError("来源发货累计数量超限".into()))?;
    }
    Ok(())
}

fn add_order_lines(
    totals: &mut BTreeMap<(String, String), Decimal>,
    order: &str,
    lines: &[ShipmentLine],
) -> Result<()> {
    for line in lines {
        let current = totals.entry((order.into(), line.line_id.clone())).or_insert(Decimal::ZERO);
        *current = current
            .checked_add(line.quantity.to_decimal())
            .ok_or_else(|| Error::ValidationError("子单发货数量超限".into()))?;
    }
    Ok(())
}

fn ensure_leaf_quantities(
    observation: &OrderObservation,
    shipment: &PendingShipment,
    totals: &BTreeMap<(String, String), Decimal>,
) -> Result<()> {
    let Some(tree) = &observation.topology else {
        return if shipment.source_order_no == observation.external_order_no {
            Ok(())
        } else {
            Err(Error::BusinessLogicError("包裹所属子单尚未完整核实，请继续查询原订单".into()))
        };
    };
    let node = tree
        .leaves()
        .into_iter()
        .find(|node| node.external_order_no == shipment.source_order_no)
        .ok_or_else(|| Error::BusinessLogicError("包裹不属于当前已核实子单，请核对再次拆单".into()))?;
    for line in &shipment.lines {
        let allocation = node
            .lines
            .iter()
            .find(|value| value.line_id == line.line_id)
            .ok_or_else(|| Error::ValidationError("包裹明细不属于该来源子单".into()))?;
        let total = totals
            .get(&(shipment.source_order_no.clone(), line.line_id.clone()))
            .copied()
            .unwrap_or(Decimal::ZERO);
        if total > allocation.quantity.to_decimal() {
            return Err(Error::BusinessLogicError("包裹累计数量超过所属子单的核实数量".into()));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use erp_core::common::time::Instant;

    use super::*;
    use crate::order_tracking::TrackingSourceLine;
    use crate::ports::connector::common::{Snapshot, SourceRevision, SourceStamp, SupplierSku};
    use crate::ports::connector::order::{OrderReference, OrderSnapshot};
    use crate::ports::connector::order_tree::{OrderNode, OrderNodeLine, OrderNodeRole, OrderTree};

    fn tracking() -> OrderTracking {
        OrderTracking::new(
            "order".into(),
            "connection".into(),
            "purchase".into(),
            vec![
                TrackingSourceLine {
                    fulfillment_item_id: "first".into(),
                    purchase_order_revision_line_id: "p1".into(),
                    sales_order_line_id: Some("sale1".into()),
                    delivery_id: Some("delivery".into()),
                    quantity: "3".parse().unwrap(),
                },
                TrackingSourceLine {
                    fulfillment_item_id: "second".into(),
                    purchase_order_revision_line_id: "p2".into(),
                    sales_order_line_id: Some("sale2".into()),
                    delivery_id: Some("delivery".into()),
                    quantity: "2".parse().unwrap(),
                },
            ],
        )
        .unwrap()
    }
    fn pending(id: &str, line: &str, quantity: &str) -> PendingShipment {
        PendingShipment {
            source_order_no: "child".into(),
            shipment_id: id.into(),
            lines: vec![ShipmentLine { line_id: line.into(), quantity: quantity.parse().unwrap() }],
            carrier: None,
            tracking_no: Some(id.into()),
        }
    }

    fn observation(tracking: &OrderTracking) -> OrderObservation {
        OrderObservation::from_snapshot(
            "observed".into(),
            tracking,
            Snapshot {
                stamp: SourceStamp {
                    revision: SourceRevision::Unversioned,
                    changed_at: None,
                    observed_at: Instant::from_unix_secs(1),
                },
                value: OrderSnapshot {
                    source_evidence: None,
                    reference: OrderReference {
                        merchant_order_no: "order".into(),
                        external_order_no: "root".into(),
                    },
                    amounts: None,
                    fulfillment: Some(FulfillmentStatus::Completed),
                    cancellation: None,
                    refund: None,
                    payment: None,
                    shipments: None,
                    topology: None,
                    confirmation: None,
                    complete_line_coverage: false,
                },
            },
        )
        .unwrap()
    }

    fn node(id: &str, line: &str, quantity: &str) -> OrderNode {
        OrderNode {
            external_order_no: id.into(),
            parent_order_no: Some("root".into()),
            role: OrderNodeRole::Leaf,
            lines: vec![OrderNodeLine {
                line_id: line.into(),
                sku: SupplierSku { product_id: None, spec_id: line.into() },
                quantity: quantity.parse().unwrap(),
            }],
            confirmation: None,
            amounts: None,
            fulfillment: None,
            cancellation: None,
            refund: None,
        }
    }

    #[test]
    fn dispatch_without_topology_requires_actual_applied_quantities() {
        let mut tracking = tracking();
        let observation = observation(&tracking);
        assert!(tracking.ensure_dispatch_quantities(&observation).is_err());
        tracking.record_shipment(&pending("first", "first", "3"), vec!["d1".into()]).unwrap();
        assert!(tracking.ensure_dispatch_quantities(&observation).is_err());
        tracking.record_shipment(&pending("second", "second", "2"), vec!["d2".into()]).unwrap();
        tracking.ensure_dispatch_quantities(&observation).unwrap();
    }

    #[test]
    fn only_dispatched_current_leaves_require_their_own_quantity_evidence() {
        let mut tracking = tracking();
        let mut observation = observation(&tracking);
        let mut parent = node("root", "first", "3");
        parent.parent_order_no = None;
        parent.role = OrderNodeRole::Parent;
        parent.lines.extend(node("second", "second", "2").lines);
        parent.fulfillment = Some(FulfillmentStatus::Completed);
        observation.topology = Some(OrderTree {
            root_order_no: "root".into(),
            version: 1,
            nodes: vec![parent, node("child-a", "first", "3"), node("child-b", "second", "2")],
        });
        tracking.ensure_dispatch_quantities(&observation).unwrap();
        observation.topology.as_mut().unwrap().nodes[1].fulfillment = Some(FulfillmentStatus::Shipped);
        assert!(tracking.ensure_dispatch_quantities(&observation).is_err());
        let mut first = pending("first", "first", "3");
        first.source_order_no = "child-a".into();
        tracking.record_shipment(&first, vec!["d1".into()]).unwrap();
        tracking.ensure_dispatch_quantities(&observation).unwrap();
        observation.topology.as_mut().unwrap().nodes[2].fulfillment = Some(FulfillmentStatus::Completed);
        assert!(tracking.ensure_dispatch_quantities(&observation).is_err());
        let mut second = pending("second", "second", "2");
        second.source_order_no = "child-b".into();
        tracking.record_shipment(&second, vec!["d2".into()]).unwrap();
        tracking.ensure_dispatch_quantities(&observation).unwrap();
    }
    #[test]
    fn partial_dispatch_replay_and_complete_quantities_are_conserved() {
        let mut tracking = tracking();
        let first = pending("SF1", "first", "1");
        tracking.record_shipment(&first, vec!["d1".into()]).unwrap();
        tracking.record_shipment(&first, vec!["d1".into()]).unwrap();
        assert_eq!(tracking.applied_shipments.len(), 1);
        assert!(!tracking.quantities_fully_applied().unwrap());
        tracking.record_shipment(&pending("SF2", "first", "2"), vec!["d2".into()]).unwrap();
        tracking.record_shipment(&pending("SF3", "second", "2"), vec!["d3".into()]).unwrap();
        assert!(tracking.quantities_fully_applied().unwrap());
        assert!(tracking.record_shipment(&pending("SF4", "first", "1"), vec!["d4".into()]).is_err());
        assert_eq!(tracking.applied_shipments.len(), 3);
    }
    #[test]
    fn changed_package_foreign_line_and_empty_delivery_are_rejected() {
        let mut tracking = tracking();
        tracking.record_shipment(&pending("SF1", "first", "1"), vec!["d1".into()]).unwrap();
        assert!(tracking.record_shipment(&pending("SF1", "first", "2"), vec!["d1".into()]).is_err());
        assert!(tracking.record_shipment(&pending("SF2", "foreign", "1"), vec!["d2".into()]).is_err());
        assert!(tracking.record_shipment(&pending("SF2", "second", "1"), vec![]).is_err());
    }

    #[test]
    fn package_replay_preserves_original_record_and_conflict_priority() {
        let mut tracking = tracking();
        let shipment = pending("SF1", "first", "1");
        tracking.record_shipment(&shipment, vec!["d1".into()]).unwrap();
        let original = tracking.applied_shipments.clone();

        tracking.record_shipment(&shipment, vec!["d1".into()]).unwrap();
        assert_eq!(tracking.applied_shipments, original);
        assert!(matches!(
            tracking.record_shipment(&shipment, vec!["d2".into()]),
            Err(Error::ConflictError(message)) if message == "包裹对应的原发货记录不一致"
        ));
        assert_eq!(tracking.applied_shipments, original);

        let changed = pending("SF1", "first", "2");
        assert!(matches!(
            tracking.record_shipment(&changed, vec!["d2".into()]),
            Err(Error::ConflictError(message)) if message == "同一来源包裹数量发生变化，请核对原发货记录"
        ));
        assert_eq!(tracking.applied_shipments, original);
        assert!(matches!(
            tracking.record_shipment(&changed, vec![]),
            Err(Error::ValidationError(message)) if message == "包裹缺少真实发货记录"
        ));
        assert_eq!(tracking.applied_shipments, original);
    }

    #[test]
    fn observed_package_replay_is_skipped_but_changed_quantities_are_rejected() {
        let mut tracking = tracking();
        let mut shipment = pending("SF1", "first", "1");
        shipment.source_order_no = "root".into();
        tracking.record_shipment(&shipment, vec!["d1".into()]).unwrap();
        let mut observation = observation(&tracking);
        observation.shipments = Some(vec![OrderShipment {
            external_shipment_id: Some(shipment.shipment_id.clone()),
            line_ids: vec!["first".into()],
            source_order_no: Some(shipment.source_order_no.clone()),
            quantity_lines: Some(shipment.lines.clone()),
            complete_line_coverage: false,
            carrier: None,
            tracking_no: shipment.tracking_no,
            has_courier_contact: false,
            events: Vec::new(),
        }]);
        assert!(tracking.pending_shipments(&observation).unwrap().is_empty());
        observation.shipments.as_mut().unwrap()[0].quantity_lines.as_mut().unwrap()[0].quantity =
            "2".parse().unwrap();
        assert!(matches!(
            tracking.pending_shipments(&observation),
            Err(Error::ConflictError(message)) if message == "同一来源包裹数量发生变化，请核对原发货记录"
        ));
    }

    #[test]
    fn missing_quantities_only_prove_trajectory_and_legacy_records_read_empty() {
        let shipment = OrderShipment {
            external_shipment_id: Some("p1".into()),
            line_ids: vec!["first".into()],
            source_order_no: Some("child".into()),
            quantity_lines: None,
            complete_line_coverage: false,
            carrier: None,
            tracking_no: Some("SF1".into()),
            has_courier_contact: false,
            events: Vec::new(),
        };
        assert_eq!(shipment.quantity_evidence("root").unwrap(), None);
        let mut json = serde_json::to_value(tracking()).unwrap();
        json.as_object_mut().unwrap().remove("applied_shipments");
        let old: OrderTracking = serde_json::from_value(json).unwrap();
        assert!(old.applied_shipments.is_empty());
    }
}
