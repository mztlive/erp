//! 最终子单发货声明与冻结数量；运单独立保存，不分摊包裹数量。

use std::collections::BTreeMap;

use erp_supply::entity::supplier_fulfillment::FulfillmentStatus;
use erp_supply::ports::connector::common::ConnectorResult;
use erp_supply::ports::connector::order::{Shipment, ShipmentLine};
use erp_supply::ports::connector::order_tree::OrderTree;

use super::mapping;
use super::wire_order::JdOrder;

// 用已通过完整拓扑、冻结行和价格校验的订单树及同次查询详情，把最终子单的发货声明转成一次性行数量事实。
// 事实以子单为稳定身份，不附运单号；拒收、退回、状态冲突或未支持的发货类型必须先核实。
pub(super) fn shipments(
    tree: &OrderTree,
    orders: &BTreeMap<String, JdOrder>,
) -> ConnectorResult<Vec<Shipment>> {
    let mut shipments = Vec::new();
    for leaf in tree.leaves() {
        let order = orders.get(&leaf.external_order_no).ok_or_else(|| mapping("京东子单详情缺失"))?;
        validate_dispatch_state(order)?;
        if !matches!(leaf.fulfillment, Some(FulfillmentStatus::Shipped | FulfillmentStatus::Completed)) {
            continue;
        }
        if !matches!(order.order_type.as_ref().map(|value| value.0.as_str()), Some("1" | "5")) {
            return Err(mapping("京东发货订单类型尚未核实为普通实物或厂家直送，请核对原订单"));
        }

        shipments.push(Shipment {
            external_shipment_id: Some(format!("jd-order-dispatch:{}", leaf.external_order_no)),
            source_order_no: Some(leaf.external_order_no.clone()),
            line_ids: leaf.lines.iter().map(|line| line.line_id.clone()).collect(),
            quantity_lines: Some(
                leaf.lines
                    .iter()
                    .map(|line| ShipmentLine { line_id: line.line_id.clone(), quantity: line.quantity })
                    .collect(),
            ),
            complete_line_coverage: false,
            carrier: None,
            tracking_no: None,
            courier_phone: None,
            events: vec![],
        });
    }
    Ok(shipments)
}

fn validate_dispatch_state(order: &JdOrder) -> ConnectorResult<()> {
    let Some(state) = &order.order_state else { return Ok(()) };
    let progress = state.jd_order_state.as_ref().map(|value| value.0.as_str());
    let delivery = state.delivery_state.as_ref().map(|value| value.0.as_str());
    let cancelled = state.cancel_order_state.as_ref().is_some_and(|value| value.0 != "0");
    if delivery.is_some_and(|value| !matches!(value, "0" | "1"))
        || progress == Some("17")
        || cancelled && (matches!(progress, Some("16" | "19")) || delivery == Some("1"))
        || delivery == Some("1") && !matches!(progress, None | Some("16" | "19"))
    {
        return Err(mapping("京东子单存在拒收、退回或发货状态冲突，请核对原履约记录"));
    }
    Ok(())
}
