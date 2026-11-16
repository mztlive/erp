//! 供应商执行进度的最小授权视图；不返回金额、地址或内部明细身份。

use std::collections::BTreeMap;

use erp_catalog::repository::CatalogExt;
use erp_procurement::repository::PurchaseOrderExt;
use erp_supply::api_order::ApiOrderExecution;
use erp_supply::entity::supplier_fulfillment::{CancelStatus, FulfillmentStatus, RefundStatus};
use erp_supply::order_tracking::OrderShipment;
use erp_supply::order_tracking::repository::OrderTrackingExt;
use erp_supply::ports::connector::order::{ConfirmationState, ShipmentLine};
use erp_supply::ports::connector::order_tree::{OrderNodeRole, OrderTree};
use mongodb::Database;
use persistence_core::Executor;
use serde::Serialize;

use crate::{Error, Result};

#[derive(Clone, Serialize)]
pub struct ExecutionProgressLine {
    pub product_name: String,
    pub unit: Option<String>,
    pub sku_code: String,
    pub quantity: String,
}

#[derive(Serialize)]
pub struct ExecutionOrderNodeView {
    pub external_order_no: String,
    pub parent_order_no: Option<String>,
    pub role: OrderNodeRole,
    pub confirmation: Option<ConfirmationState>,
    pub fulfillment: Option<FulfillmentStatus>,
    pub cancellation: Option<CancelStatus>,
    pub refund: Option<RefundStatus>,
    pub lines: Vec<ExecutionProgressLine>,
}

#[derive(Serialize)]
pub struct ExecutionOrderTreeView {
    pub root_order_no: String,
    pub version: u64,
    pub pending: bool,
    pub nodes: Vec<ExecutionOrderNodeView>,
}

#[derive(Serialize)]
pub struct ExecutionShipmentView {
    pub external_shipment_id: Option<String>,
    pub source_order_no: Option<String>,
    pub carrier: Option<String>,
    pub tracking_no: Option<String>,
    pub quantity_proven: bool,
    pub lines: Vec<ExecutionProgressLine>,
}

type LineLabels = BTreeMap<String, (String, Option<String>, String)>;

/// 已授权原供应商执行的跨域进度查询；调用方先核验订单对象范围。
#[derive(Clone)]
pub struct SupplierExecutionProgressReadModel {
    db: Database,
}

impl SupplierExecutionProgressReadModel {
    /// 装配只读执行进度查询。
    /// # 参数
    /// `db` 为应用数据库。
    /// # 返回
    /// 不执行查询的读模型。
    /// # 错误
    /// 不返回错误。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 读取原采购商品标签及供应商订单树和多运单。
    /// # 参数
    /// `execution` 已经服务端授权；`external_skus` 为同执行加密快照中的外部身份映射。
    /// `executor` 为原授权读取阶段的执行器，所有关联事实共用它。
    /// # 返回
    /// 去除成本、内部明细身份及个人资料的业务视图。
    /// # 错误
    /// 原采购明细缺失、观察归属不符或查询失败时拒绝。
    pub async fn for_execution(
        &self,
        execution: &ApiOrderExecution,
        external_skus: &BTreeMap<String, String>,
        executor: &mut dyn Executor,
    ) -> Result<(Option<ExecutionOrderTreeView>, Vec<ExecutionShipmentView>)> {
        let labels = self.execution_line_labels(execution, external_skus, executor).await?;
        let pending = execution.pending_order_tree.is_some();
        let tree = execution
            .pending_order_tree
            .as_ref()
            .or(execution.order_tree.as_ref())
            .map(|tree| ExecutionOrderTreeView::from_tree(tree, &labels, pending));
        let Some(tracking) =
            self.db.supplier_order_tracking().find_by_id(&execution.base.id, executor).await?
        else {
            return Ok((tree, vec![]));
        };
        let Some(id) = tracking.latest_observation_id else {
            return Ok((tree, vec![]));
        };
        let Some(observation) = self.db.supplier_order_observations().find_by_id(&id, executor).await? else {
            return Ok((tree, vec![]));
        };
        if observation.order_id != execution.base.id || observation.connection_id != execution.connection_id {
            return Err(Error::ConflictError("供应商物流观察与当前订单不一致".into()));
        }
        let shipments = observation
            .shipments
            .unwrap_or_default()
            .iter()
            .map(|shipment| ExecutionShipmentView::from_shipment(shipment, execution, &labels))
            .collect();
        Ok((tree, shipments))
    }

    async fn execution_line_labels(
        &self,
        execution: &ApiOrderExecution,
        external_skus: &BTreeMap<String, String>,
        executor: &mut dyn Executor,
    ) -> Result<LineLabels> {
        let mut labels = BTreeMap::new();
        for source in &execution.source_lines {
            let line = self
                .db
                .purchase_order_revision_lines()
                .find_by_id(&source.purchase_order_revision_line_id, executor)
                .await?
                .filter(|line| {
                    line.purchase_order_revision_id.as_ref() == execution.purchase_order_revision_id
                })
                .ok_or_else(|| Error::NotFound("原执行采购明细不存在".into()))?;
            let sku = external_skus.get(&source.fulfillment_item_id).cloned().unwrap_or_default();
            let unit = self.unit_label(line.base_unit_code.as_deref(), executor).await?;
            labels.insert(
                source.fulfillment_item_id.clone(),
                (line.product_name_snapshot.unwrap_or_else(|| "来源商品待核实".into()), unit, sku),
            );
        }
        Ok(labels)
    }

    async fn unit_label(&self, code: Option<&str>, executor: &mut dyn Executor) -> Result<Option<String>> {
        let Some(code) = code else {
            return Ok(None);
        };
        let value = self.db.catalog().unit_by_code(code, executor).await?;
        Ok(Some(value.map(|unit| unit.name).unwrap_or_else(|| code.into())))
    }
}

impl ExecutionOrderTreeView {
    fn from_tree(tree: &OrderTree, labels: &LineLabels, pending: bool) -> Self {
        Self {
            root_order_no: tree.root_order_no.clone(),
            version: tree.version,
            pending,
            nodes: tree
                .nodes
                .iter()
                .map(|node| ExecutionOrderNodeView {
                    external_order_no: node.external_order_no.clone(),
                    parent_order_no: node.parent_order_no.clone(),
                    role: node.role,
                    confirmation: node.confirmation,
                    fulfillment: node.fulfillment,
                    cancellation: node.cancellation,
                    refund: node.refund,
                    lines: node
                        .lines
                        .iter()
                        .map(|line| {
                            ExecutionProgressLine::from_source(
                                &line.line_id,
                                &line.sku.spec_id,
                                line.quantity.to_string(),
                                labels,
                            )
                        })
                        .collect(),
                })
                .collect(),
        }
    }
}

impl ExecutionProgressLine {
    fn from_source(id: &str, sku: &str, quantity: String, labels: &LineLabels) -> Self {
        let label = labels.get(id);
        Self {
            product_name: label.map(|label| label.0.clone()).unwrap_or_else(|| "来源商品待核实".into()),
            unit: label.and_then(|label| label.1.clone()),
            sku_code: sku.into(),
            quantity,
        }
    }
}

impl ExecutionShipmentView {
    fn from_shipment(shipment: &OrderShipment, execution: &ApiOrderExecution, labels: &LineLabels) -> Self {
        let quantity_lines = shipment.quantity_lines.clone().unwrap_or_else(|| {
            if shipment.complete_line_coverage {
                execution
                    .source_lines
                    .iter()
                    .filter(|line| {
                        shipment.line_ids.is_empty() || shipment.line_ids.contains(&line.fulfillment_item_id)
                    })
                    .map(|line| ShipmentLine {
                        line_id: line.fulfillment_item_id.clone(),
                        quantity: line.quantity,
                    })
                    .collect()
            } else {
                vec![]
            }
        });
        let lines = quantity_lines
            .iter()
            .map(|line| {
                let sku = labels.get(&line.line_id).map(|label| label.2.as_str()).unwrap_or_default();
                ExecutionProgressLine::from_source(&line.line_id, sku, line.quantity.to_string(), labels)
            })
            .collect();
        Self {
            external_shipment_id: shipment.external_shipment_id.clone(),
            source_order_no: shipment.source_order_no.clone(),
            carrier: shipment.carrier.clone(),
            tracking_no: shipment.tracking_no.clone(),
            quantity_proven: shipment.quantity_lines.is_some() || shipment.complete_line_coverage,
            lines,
        }
    }
}

#[cfg(test)]
mod tests {
    use erp_supply::ports::connector::common::SupplierSku;
    use erp_supply::ports::connector::order::{OrderAmounts, ShipmentLine};
    use erp_supply::ports::connector::order_tree::{OrderNode, OrderNodeLine};

    use super::*;

    #[test]
    fn progress_view_contains_business_labels_without_cost_or_internal_line_identity() {
        let labels =
            [("private-line-id".into(), ("商品名称".into(), Some("件".into()), "external-sku".into()))]
                .into();
        let tree = OrderTree {
            root_order_no: "external-root".into(),
            version: 1,
            nodes: vec![OrderNode {
                external_order_no: "external-root".into(),
                parent_order_no: None,
                role: OrderNodeRole::Leaf,
                lines: vec![OrderNodeLine {
                    line_id: "private-line-id".into(),
                    sku: SupplierSku { product_id: None, spec_id: "external-sku".into() },
                    quantity: "2".parse().unwrap(),
                }],
                confirmation: Some(ConfirmationState::Pending),
                fulfillment: None,
                cancellation: None,
                refund: None,
                amounts: Some(OrderAmounts {
                    payable: "19.98".parse().unwrap(),
                    goods_cost: None,
                    shipping_cost: None,
                    service_cost: None,
                }),
            }],
        };
        let value = serde_json::to_value(ExecutionOrderTreeView::from_tree(&tree, &labels, false)).unwrap();
        assert_eq!(value["nodes"][0]["lines"][0]["product_name"], "商品名称");
        assert_eq!(value["nodes"][0]["lines"][0]["quantity"], "2");
        assert!(value["nodes"][0].get("amounts").is_none());
        assert!(value["nodes"][0]["lines"][0].get("line_id").is_none());
    }

    #[test]
    fn unproven_multi_waybill_preserves_tracking_without_guessing_quantity() {
        let execution =
            ApiOrderExecution::queued("e".into(), "p".into(), "r".into(), "c".into(), "s".into(), vec![]);
        let mut shipment = OrderShipment {
            events: Vec::new(),
            external_shipment_id: Some("waybill".into()),
            line_ids: vec![],
            source_order_no: Some("child".into()),
            quantity_lines: None,
            complete_line_coverage: false,
            carrier: Some("快递".into()),
            tracking_no: Some("no".into()),
            has_courier_contact: false,
        };
        let view = ExecutionShipmentView::from_shipment(&shipment, &execution, &BTreeMap::new());
        assert!(!view.quantity_proven);
        assert!(view.lines.is_empty());
        assert_eq!(view.tracking_no.as_deref(), Some("no"));
        shipment.quantity_lines =
            Some(vec![ShipmentLine { line_id: "line".into(), quantity: "1".parse().unwrap() }]);
        let view = ExecutionShipmentView::from_shipment(&shipment, &execution, &BTreeMap::new());
        assert!(view.quantity_proven);
        assert_eq!(view.lines[0].quantity, "1");
    }
}
