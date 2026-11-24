//! 京东订单来源事实映射；父单不重复累计，缺失的状态和数量不推导完成。
use std::collections::{BTreeMap, BTreeSet};

use erp_core::money::{Amount, Quantity, UnitPrice, round_to_cent};
use erp_supply::entity::supplier_fulfillment::{CancelStatus, FulfillmentStatus};
use erp_supply::ports::connector::common::{ConnectorResult, Lookup, Snapshot};
use erp_supply::ports::connector::order::{
    ConfirmationState, EncryptedOrderEvidence, OrderAmounts, OrderKey, OrderLine, OrderReference,
    OrderSnapshot,
};
use erp_supply::ports::connector::order_tree::{OrderNode, OrderNodeLine, OrderNodeRole, OrderTree};
use rust_decimal::Decimal;
use serde::Serialize;

use super::delivery::observed;
use super::logistics::LogisticsEvidence;
use super::transport::JdVopReadRequest;
use super::wire::WireText;
use super::wire_order::JdOrder;
use super::{JdVopConnector, mapping, protocol_required};

#[derive(Serialize)]
struct OrderEvidence<'a> {
    orders: &'a BTreeMap<String, JdOrder>,
    logistics: Vec<LogisticsEvidence>,
}

impl JdVopConnector {
    pub(super) async fn query_order_facts(
        &self,
        key: &OrderKey,
        frozen: &[OrderLine],
        verify_frozen_price: bool,
    ) -> ConnectorResult<Lookup<Snapshot<OrderSnapshot>>> {
        let orders = self.read_order_graph(key).await?;
        if orders.is_empty() {
            return Ok(Lookup::NotVisible);
        }
        let mut roots =
            orders.values().filter(|order| order.parent_jd_order_id.as_ref().is_none_or(|id| id.0 == "0"));
        let (Some(root), None) = (roots.next(), roots.next()) else {
            return Err(mapping("京东原根订单关系尚未完整可见"));
        };

        let mut snapshot = order_snapshot(root, &orders, frozen, verify_frozen_price)?;
        let mut shipments = snapshot.shipments.take().unwrap_or_default();
        let mut logistics = Vec::new();
        if verify_frozen_price {
            for leaf in orders.values().filter(|order| order.parent_type.0 == "0") {
                let observed = self.logistics(leaf).await?;
                let line_ids = snapshot
                    .topology
                    .as_ref()
                    .and_then(|tree| {
                        tree.leaves().into_iter().find(|node| node.external_order_no == leaf.jd_order_id.0)
                    })
                    .map(|node| node.lines.iter().map(|line| line.line_id.clone()).collect::<Vec<_>>());
                shipments.extend(observed.shipments.into_iter().map(|mut shipment| {
                    shipment.line_ids = line_ids.clone().unwrap_or_default();
                    shipment
                }));
                logistics.push(observed.evidence);
            }
        }
        snapshot.shipments = Some(shipments);

        let codec = self.evidence_codec.as_ref().ok_or_else(protocol_required)?;
        let evidence = OrderEvidence { orders: &orders, logistics };
        let text = serde_json::to_string(&evidence).map_err(|_| mapping("京东订单证据编码失败"))?;
        snapshot.source_evidence = Some(EncryptedOrderEvidence {
            content_fingerprint: self.hash(&evidence)?,
            encrypted_payload: codec.encrypt(&text).map_err(|_| mapping("京东订单来源证据加密失败"))?,
        });
        Ok(Lookup::Found(observed(snapshot)))
    }

    async fn read_order_graph(&self, key: &OrderKey) -> ConnectorResult<BTreeMap<String, JdOrder>> {
        let reader = self.reader.as_ref().ok_or_else(protocol_required)?;
        let request = match key {
            OrderKey::Merchant(number) => {
                JdVopReadRequest::Order { merchant_order_no: Some(number.clone()), external_order_no: None }
            },
            OrderKey::External(number) => {
                JdVopReadRequest::Order { merchant_order_no: None, external_order_no: Some(number.clone()) }
            },
        };
        let initial: Vec<JdOrder> = reader.read(request).await?;
        let mut orders = BTreeMap::new();
        append_orders(&mut orders, initial)?;

        loop {
            let missing = missing_nodes(&orders);
            if missing.is_empty() {
                break;
            }
            let previous = orders.len();
            for number in missing {
                let next: Vec<JdOrder> = reader
                    .read(JdVopReadRequest::Order {
                        merchant_order_no: None,
                        external_order_no: Some(number),
                    })
                    .await?;
                append_orders(&mut orders, next)?;
            }
            if orders.len() == previous {
                return Err(mapping("京东子订单或原父订单暂不可见，请继续查询原结果"));
            }
        }

        validate_identity(key, &orders)?;
        Ok(orders)
    }
}

fn order_snapshot(
    root: &JdOrder,
    orders: &BTreeMap<String, JdOrder>,
    frozen: &[OrderLine],
    verify_frozen_price: bool,
) -> ConnectorResult<OrderSnapshot> {
    let mut snapshot = summary(root)?;
    if !frozen.is_empty() {
        let tree = build_tree_facts(root, orders, frozen, verify_frozen_price)?;
        snapshot.confirmation = aggregate_confirmation(&tree);
        snapshot.fulfillment = aggregate_fulfillment(&tree);
        let targets = tree.action_targets();
        snapshot.cancellation = (!targets.is_empty()
            && targets.iter().all(|node| node.cancellation == Some(CancelStatus::Canceled)))
        .then_some(CancelStatus::Canceled);
        if verify_frozen_price {
            snapshot.shipments = Some(super::order_dispatch::shipments(&tree, orders)?);
        }
        snapshot.topology = Some(tree);
    }
    Ok(snapshot)
}

pub(super) fn summary(order: &JdOrder) -> ConnectorResult<OrderSnapshot> {
    Ok(OrderSnapshot {
        source_evidence: None,
        reference: OrderReference {
            merchant_order_no: order.third_order_id.clone(),
            external_order_no: order.jd_order_id.0.clone(),
        },
        amounts: amounts(order)?,
        fulfillment: leaf_fulfillment(order),
        cancellation: cancellation(order)?,
        refund: None,
        payment: None,
        confirmation: confirmation(order)?,
        topology: None,
        complete_line_coverage: false,
        shipments: None,
    })
}

fn append_orders(orders: &mut BTreeMap<String, JdOrder>, incoming: Vec<JdOrder>) -> ConnectorResult<()> {
    for order in incoming {
        if order.requires_special_fulfillment() {
            return Err(mapping("京东原订单包含尚未支持的专门履约流程"));
        }
        if let Some(previous) = orders.get(&order.jd_order_id.0) {
            if previous != &order {
                return Err(mapping("京东同一订单返回冲突事实"));
            }
        } else {
            orders.insert(order.jd_order_id.0.clone(), order);
        }
    }
    if orders.len() > 200 {
        return Err(mapping("京东订单树超过本次有界查询范围"));
    }
    Ok(())
}

fn missing_nodes(orders: &BTreeMap<String, JdOrder>) -> BTreeSet<String> {
    let mut missing = BTreeSet::new();
    for order in orders.values() {
        for id in order.child_order_ids().iter().chain(order.parent_jd_order_id.iter()) {
            if id.0 != "0" && !orders.contains_key(&id.0) {
                missing.insert(id.0.clone());
            }
        }
    }
    missing
}

fn validate_identity(key: &OrderKey, orders: &BTreeMap<String, JdOrder>) -> ConnectorResult<()> {
    let merchant = orders.values().next().map(|order| order.third_order_id.as_str());
    if orders.values().any(|order| Some(order.third_order_id.as_str()) != merchant) {
        return Err(mapping("京东订单树混入其他第三方订单"));
    }
    let valid = match key {
        OrderKey::Merchant(number) => merchant == Some(number.as_str()),
        OrderKey::External(number) => orders.contains_key(number),
    };
    if !valid {
        return Err(mapping("京东查询结果与原订单身份不符"));
    }
    Ok(())
}

fn build_tree_facts(
    root: &JdOrder,
    orders: &BTreeMap<String, JdOrder>,
    frozen: &[OrderLine],
    verify_frozen_price: bool,
) -> ConnectorResult<OrderTree> {
    let mut by_sku = BTreeMap::new();
    for line in frozen {
        if by_sku.insert(line.sku.spec_id.as_str(), line).is_some() {
            return Err(mapping("京东重复订货编号的采购行缺少可证明的数量分配"));
        }
    }

    let mut nodes = Vec::new();
    for order in orders.values() {
        let role = match order.parent_type.0.as_str() {
            "1" if !order.child_order_ids().is_empty() => OrderNodeRole::Parent,
            "1" if order.is_reservation_root() => OrderNodeRole::ReservationRoot,
            "0" if order.child_order_ids().is_empty() => OrderNodeRole::Leaf,
            _ => OrderNodeRole::Unresolved,
        };

        let mut lines = Vec::new();
        for source in &order.sku_info_list {
            if source.line_type.as_ref().is_some_and(|kind| kind.0 == "1") {
                continue;
            }
            if source.line_type.as_ref().is_some_and(|kind| !matches!(kind.0.as_str(), "0" | "1")) {
                return Err(mapping("京东订单含系统尚不支持的附加履约明细"));
            }
            let frozen = by_sku
                .get(source.sku_id.0.as_str())
                .ok_or_else(|| mapping("京东订单明细不属于原冻结采购行"))?;
            if verify_frozen_price && matches!(role, OrderNodeRole::Leaf | OrderNodeRole::ReservationRoot) {
                validate_line_price(source.sku_price.as_ref(), frozen)?;
            }
            let quantity: Quantity = source.sku_num.0.parse().map_err(|_| mapping("京东子单数量格式无效"))?;
            lines.push(OrderNodeLine { line_id: frozen.line_id.clone(), sku: frozen.sku.clone(), quantity });
        }
        nodes.push(OrderNode {
            external_order_no: order.jd_order_id.0.clone(),
            parent_order_no: order
                .parent_jd_order_id
                .as_ref()
                .filter(|id| id.0 != "0")
                .map(|id| id.0.clone()),
            role,
            lines,
            confirmation: confirmation(order)?,
            amounts: amounts(order)?,
            fulfillment: leaf_fulfillment(order),
            cancellation: cancellation(order)?,
            refund: None,
        });
    }

    let tree = OrderTree { root_order_no: root.jd_order_id.0.clone(), version: 1, nodes };
    tree.validate(frozen).map_err(|_| mapping("京东拆单关系或逐行数量尚未守恒，请继续核实原订单"))?;
    Ok(tree)
}

fn validate_line_price(price: Option<&WireText>, frozen: &OrderLine) -> ConnectorResult<()> {
    let price: UnitPrice = price
        .ok_or_else(|| mapping("京东订单明细缺少可核实单价，已阻断确认"))?
        .0
        .parse()
        .map_err(|_| mapping("京东订单明细单价格式无效"))?;
    if price != frozen.approved_unit_price {
        return Err(mapping("京东订单明细单价与原批准单价不一致，已阻断确认"));
    }
    Ok(())
}

fn leaf_fulfillment(order: &JdOrder) -> Option<FulfillmentStatus> {
    if order.parent_type.0 != "0" || !order.child_order_ids().is_empty() {
        return None;
    }
    let state = order.order_state.as_ref()?;
    if state.cancel_order_state.as_ref().is_some_and(|value| value.0 != "0")
        || state.delivery_state.as_ref().is_some_and(|value| !matches!(value.0.as_str(), "0" | "1"))
    {
        return None;
    }
    // 最终子单的16/19是发货声明；数量须另经完整拓扑和冻结行校验。
    match state.jd_order_state.as_ref().map(|value| value.0.as_str()) {
        Some("19") => Some(FulfillmentStatus::Completed),
        Some("16") => Some(FulfillmentStatus::Shipped),
        Some("6" | "7" | "8" | "9" | "10" | "29" | "30" | "31")
            if !state.delivery_state.as_ref().is_some_and(|value| value.0 == "1") =>
        {
            Some(FulfillmentStatus::Fulfilling)
        },
        None if state.delivery_state.as_ref().is_some_and(|value| value.0 == "1") => {
            Some(FulfillmentStatus::Shipped)
        },
        _ => None,
    }
}

fn aggregate_fulfillment(tree: &OrderTree) -> Option<FulfillmentStatus> {
    let leaves = tree.leaves();
    if leaves.is_empty() || leaves.iter().any(|leaf| leaf.fulfillment.is_none()) {
        return None;
    }
    if leaves.iter().all(|leaf| leaf.fulfillment == Some(FulfillmentStatus::Completed)) {
        Some(FulfillmentStatus::Completed)
    } else if leaves.iter().all(|leaf| {
        matches!(leaf.fulfillment, Some(FulfillmentStatus::Shipped | FulfillmentStatus::Completed))
    }) {
        Some(FulfillmentStatus::Shipped)
    } else {
        Some(FulfillmentStatus::Fulfilling)
    }
}

pub(super) fn confirmation(order: &JdOrder) -> ConnectorResult<Option<ConfirmationState>> {
    match order
        .order_state
        .as_ref()
        .and_then(|state| state.confirm_state.as_ref())
        .map(|value| value.0.as_str())
    {
        Some("0") => Ok(Some(ConfirmationState::Pending)),
        Some("1") => Ok(Some(ConfirmationState::Confirmed)),
        None => Ok(None),
        _ => Err(mapping("京东订单确认状态未识别")),
    }
}

fn cancellation(order: &JdOrder) -> ConnectorResult<Option<CancelStatus>> {
    match order
        .order_state
        .as_ref()
        .and_then(|state| state.cancel_order_state.as_ref())
        .map(|value| value.0.as_str())
    {
        Some("1") => Ok(Some(CancelStatus::Canceled)),
        Some("0") | None => Ok(None),
        _ => Err(mapping("京东订单取消状态未识别")),
    }
}

fn amounts(order: &JdOrder) -> ConnectorResult<Option<OrderAmounts>> {
    let Some(price) = &order.order_price else { return Ok(None) };
    let payable: Amount =
        price.order_total_price.0.parse().map_err(|_| mapping("京东订单实际金额格式无效"))?;
    let shipping = price
        .order_total_freight
        .as_ref()
        .map(|value| value.0.parse::<Amount>())
        .transpose()
        .map_err(|_| mapping("京东订单实际运费格式无效"))?;
    if payable < Amount::zero() || shipping.is_some_and(|value| value < Amount::zero()) {
        return Err(mapping("京东订单金额不能为负数"));
    }

    let goods = order.sku_info_list.iter().try_fold(Decimal::ZERO, |total, line| {
        let price: Decimal = line.sku_price.as_ref()?.0.parse().ok()?;
        let quantity: Decimal = line.sku_num.0.parse().ok()?;
        if price < Decimal::ZERO || quantity <= Decimal::ZERO {
            return None;
        }
        price.checked_mul(quantity).map(round_to_cent).and_then(|value| total.checked_add(value))
    });
    let reconciled = goods.zip(shipping).is_some_and(|(goods, freight)| {
        goods.checked_add(freight.to_decimal()) == Some(payable.to_decimal())
    });
    let goods = if reconciled {
        goods
            .map(|value| value.to_string().parse::<Amount>().map_err(|_| mapping("京东商品成本精度无效")))
            .transpose()?
    } else {
        None
    };
    Ok(Some(OrderAmounts {
        payable,
        goods_cost: goods,
        shipping_cost: shipping,
        service_cost: reconciled.then_some(Amount::zero()),
    }))
}

fn aggregate_confirmation(tree: &OrderTree) -> Option<ConfirmationState> {
    let leaves = tree.action_targets();
    if leaves.is_empty() || leaves.iter().any(|leaf| leaf.confirmation.is_none()) {
        None
    } else if leaves.iter().all(|leaf| leaf.confirmation == Some(ConfirmationState::Confirmed)) {
        Some(ConfirmationState::Confirmed)
    } else {
        Some(ConfirmationState::Pending)
    }
}

#[cfg(test)]
mod tests {
    use erp_supply::ports::connector::common::SupplierSku;
    use serde_json::json;

    use super::super::wire::WireId;
    use super::*;

    fn frozen(sku: &str, price: &str, quantity: &str) -> OrderLine {
        OrderLine {
            source_purchase_terms: None,
            line_id: format!("line-{sku}"),
            sku: SupplierSku { product_id: None, spec_id: sku.into() },
            quantity: quantity.parse().unwrap(),
            approved_unit_price: price.parse().unwrap(),
            attributes: vec![],
        }
    }

    fn leaf(id: &str, state: &str) -> JdOrder {
        serde_json::from_value(json!({
            "jdOrderId": id, "thirdOrderId":"merchant", "parentType":"0", "orderType":"1",
            "childJdOrderIdList":[],
            "skuInfoList":[{"skuId":"123", "skuNum":"1", "skuPrice":"10", "type":"0"}],
            "orderState":{"confirmState":"1", "cancelOrderState":"0", "jdOrderState":state}
        }))
        .unwrap()
    }

    #[test]
    fn shipped_children_apply_once_independently_of_waybills_and_parent_state() {
        use erp_supply::order_tracking::{OrderObservation, OrderTracking, TrackingSourceLine};
        use erp_supply::ports::connector::order::Shipment;

        let frozen = [frozen("123", "10", "2")];
        let mut root = leaf("100", "19");
        root.parent_type = WireText("1".into());
        root.child_jd_order_id_list = Some(["101", "102"].map(|id| WireId(id.into())).into());
        root.sku_info_list[0].sku_num = WireText("2".into());
        let mut orders = BTreeMap::from([(root.jd_order_id.0.clone(), root.clone())]);
        for (id, state) in [("101", "16"), ("102", "8")] {
            let mut child = leaf(id, state);
            child.parent_jd_order_id = Some(root.jd_order_id.clone());
            orders.insert(id.into(), child);
        }
        let mut tracking = OrderTracking::new(
            "execution".into(),
            "connection".into(),
            "purchase".into(),
            vec![TrackingSourceLine {
                fulfillment_item_id: frozen[0].line_id.clone(),
                purchase_order_revision_line_id: "purchase-line".into(),
                sales_order_line_id: Some("sale".into()),
                delivery_id: Some("draft".into()),
                quantity: frozen[0].quantity,
            }],
        )
        .unwrap();

        let first = order_snapshot(&root, &orders, &frozen, true).unwrap();
        assert_eq!(first.fulfillment, Some(FulfillmentStatus::Fulfilling));
        let first = OrderObservation::from_snapshot("first".into(), &tracking, observed(first)).unwrap();
        let pending = tracking.pending_shipments(&first).unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].source_order_no, "101");
        assert_eq!(pending[0].lines[0].quantity, "1".parse().unwrap());
        assert!(pending[0].tracking_no.is_none());
        tracking.record_shipment(&pending[0], vec!["delivery-101".into()]).unwrap();
        assert!(!tracking.quantities_fully_applied().unwrap());

        for id in ["101", "102"] {
            orders.get_mut(id).unwrap().order_state.as_mut().unwrap().jd_order_state =
                Some(WireText("19".into()));
        }

        let mut next = order_snapshot(&root, &orders, &frozen, true).unwrap();
        for number in ["waybill-a", "waybill-b"] {
            next.shipments.as_mut().unwrap().push(Shipment {
                external_shipment_id: Some(number.into()),
                source_order_no: Some("101".into()),
                line_ids: vec![frozen[0].line_id.clone()],
                quantity_lines: None,
                complete_line_coverage: false,
                carrier: None,
                tracking_no: Some(number.into()),
                courier_phone: None,
                events: vec![],
            });
        }
        let next = OrderObservation::from_snapshot("next".into(), &tracking, observed(next)).unwrap();
        let pending = tracking.pending_shipments(&next).unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].source_order_no, "102");
        tracking.record_shipment(&pending[0], vec!["delivery-102".into()]).unwrap();
        assert!(tracking.quantities_fully_applied().unwrap());
        assert!(tracking.pending_shipments(&next).unwrap().is_empty());
        tracking.ensure_dispatch_quantities(&next).unwrap();
    }

    #[test]
    fn dispatch_requires_supported_leaf_state_and_consistent_details() {
        let frozen = [frozen("123", "10", "1")];
        for state in ["6", "7", "8", "9", "10", "29", "30", "31", "999"] {
            let order = leaf("100", state);
            let orders = BTreeMap::from([("100".into(), order.clone())]);
            let value = order_snapshot(&order, &orders, &frozen, true).unwrap();
            assert!(value.shipments.unwrap().is_empty(), "{state}");
        }
        for (state, delivery, cancel) in
            [("16", "2", "0"), ("17", "0", "0"), ("8", "1", "0"), ("19", "0", "1")]
        {
            let mut order = leaf("100", state);
            order.order_state.as_mut().unwrap().delivery_state = Some(WireText(delivery.into()));
            order.order_state.as_mut().unwrap().cancel_order_state = Some(WireText(cancel.into()));
            let orders = BTreeMap::from([("100".into(), order.clone())]);
            assert!(order_snapshot(&order, &orders, &frozen, true).is_err());
        }
        for kind in [None, Some("2"), Some("999")] {
            let mut order = leaf("100", "16");
            order.order_type = kind.map(|value| WireText(value.into()));
            let orders = BTreeMap::from([("100".into(), order.clone())]);
            assert!(order_snapshot(&order, &orders, &frozen, true).is_err());
        }

        let mut delivered = leaf("100", "16");
        delivered.order_state.as_mut().unwrap().jd_order_state = None;
        delivered.order_state.as_mut().unwrap().delivery_state = Some(WireText("1".into()));
        let orders = BTreeMap::from([("100".into(), delivered.clone())]);
        assert_eq!(order_snapshot(&delivered, &orders, &frozen, true).unwrap().shipments.unwrap().len(), 1);

        delivered.sku_info_list[0].sku_num = WireText("2".into());
        let orders = BTreeMap::from([("100".into(), delivered.clone())]);
        assert!(order_snapshot(&delivered, &orders, &frozen, true).is_err());
    }

    #[test]
    fn only_current_leaf_progress_is_mapped_and_never_proves_quantity() {
        for (state, expected) in [
            ("8", Some(FulfillmentStatus::Fulfilling)),
            ("10", Some(FulfillmentStatus::Fulfilling)),
            ("16", Some(FulfillmentStatus::Shipped)),
            ("19", Some(FulfillmentStatus::Completed)),
            ("31", Some(FulfillmentStatus::Fulfilling)),
            ("2", None),
            ("5", None),
            ("17", None),
            ("999", None),
        ] {
            let snapshot = summary(&leaf("100", state)).unwrap();
            assert_eq!(snapshot.fulfillment, expected, "{state}");
            assert!(!snapshot.complete_line_coverage);
            assert!(snapshot.shipments.is_none());
        }

        let mut parent = leaf("100", "19");
        parent.parent_type = WireText("1".into());
        parent.child_jd_order_id_list = Some(vec![]);
        assert_eq!(summary(&parent).unwrap().fulfillment, None);
        parent.parent_type = WireText("0".into());
        parent.child_jd_order_id_list = Some(vec![WireId("101".into())]);
        assert_eq!(summary(&parent).unwrap().fulfillment, None);

        let mut rejected = leaf("100", "19");
        rejected.order_state.as_mut().unwrap().delivery_state = Some(WireText("2".into()));
        assert_eq!(summary(&rejected).unwrap().fulfillment, None);
    }

    #[test]
    fn parent_completion_and_one_completed_child_cannot_complete_the_group() {
        let first = leaf("101", "19");
        let second = leaf("102", "8");
        let mut root = leaf("100", "19");
        root.parent_type = WireText("1".into());
        root.child_jd_order_id_list = Some(["101", "102"].map(|id| WireId(id.into())).into());
        root.sku_info_list[0].sku_num = WireText("2".into());
        root.sku_info_list[0].sku_price = None;
        let mut orders = BTreeMap::new();
        orders.insert(root.jd_order_id.0.clone(), root.clone());
        for mut order in [first, second] {
            order.parent_jd_order_id = Some(root.jd_order_id.clone());
            orders.insert(order.jd_order_id.0.clone(), order);
        }

        let frozen = [frozen("123", "10", "2")];
        let tree = build_tree_facts(&root, &orders, &frozen, true).unwrap();
        assert_eq!(aggregate_fulfillment(&tree), Some(FulfillmentStatus::Fulfilling));
        orders.get_mut("102").unwrap().order_state.as_mut().unwrap().jd_order_state =
            Some(WireText("16".into()));
        assert_eq!(
            aggregate_fulfillment(&build_tree_facts(&root, &orders, &frozen, true).unwrap()),
            Some(FulfillmentStatus::Shipped)
        );
        orders.get_mut("102").unwrap().order_state.as_mut().unwrap().jd_order_state =
            Some(WireText("19".into()));
        assert_eq!(
            aggregate_fulfillment(&build_tree_facts(&root, &orders, &frozen, true).unwrap()),
            Some(FulfillmentStatus::Completed)
        );
        orders.get_mut("102").unwrap().order_state.as_mut().unwrap().jd_order_state =
            Some(WireText("999".into()));
        assert_eq!(aggregate_fulfillment(&build_tree_facts(&root, &orders, &frozen, true).unwrap()), None);
    }

    #[test]
    fn every_order_line_requires_its_original_price_even_when_total_is_unchanged() {
        let mut order = leaf("100", "16");
        let mut second = order.sku_info_list[0].clone();
        second.sku_id = WireId("456".into());
        second.sku_price = Some(WireText("20".into()));
        order.sku_info_list.push(second);
        order.order_price =
            Some(serde_json::from_value(json!({"orderTotalPrice":"30", "orderTotalFreight":"0"})).unwrap());
        let frozen = [frozen("123", "10", "1"), frozen("456", "20", "1")];
        let tree = |order: &JdOrder| {
            build_tree_facts(
                order,
                &BTreeMap::from([(order.jd_order_id.0.clone(), order.clone())]),
                &frozen,
                true,
            )
        };
        tree(&order).unwrap();

        order.sku_info_list[0].sku_price = Some(WireText("11".into()));
        order.sku_info_list[1].sku_price = Some(WireText("19".into()));
        assert_eq!(amounts(&order).unwrap().unwrap().goods_cost, Some("30".parse().unwrap()));
        assert!(tree(&order).unwrap_err().summary.contains("单价与原批准"));

        order.sku_info_list[0].sku_price = None;
        assert!(tree(&order).unwrap_err().summary.contains("缺少可核实单价"));
    }
}
