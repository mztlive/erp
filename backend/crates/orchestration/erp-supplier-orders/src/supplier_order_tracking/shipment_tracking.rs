//! 已发货后的运单补充；按原子单及销售行关联，不再次过账数量。

use std::collections::BTreeMap;

use erp_core::ids::{DeliveryId, SalesOrderLineId};
use erp_fulfillment::entity::fulfillment::{DeliveryTrackingEntry, DeliveryType};
use erp_fulfillment::repository::FulfillmentExt;
use erp_supply::order_tracking::{OrderObservation, OrderTracking};
use mongodb::Database;
use persistence_core::Executor;

use crate::{Error, Result};

/// 补充本执行已应用交付的运单，和数量凭据共用事务。
/// # 参数
/// 数据库、含真实交付身份的跟踪记录、当前观察及调用方执行器。
/// # 返回
/// 仅合并真实运单；重复观察不重复登记或再次发货。
/// # 错误
/// 原交付或销售行不符、交付已冲正、物流资料无效及存储失败时拒绝。
pub(super) async fn merge_tracking(
    db: &Database,
    tracking: &OrderTracking,
    observation: &OrderObservation,
    executor: &mut dyn Executor,
) -> Result<()> {
    for (id, entries) in tracking_updates(tracking, observation) {
        let mut delivery = db
            .deliveries()
            .find_by_id(&id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("来源发货对应的真实交付不存在".into()))?;
        if delivery.delivery_type != DeliveryType::SupplierDirect {
            return Err(Error::BusinessLogicError("供应商运单不能改写仓库发货单".into()));
        }

        let lines =
            db.fulfillment().delivery_lines_by_delivery_ids(&[DeliveryId::new(&id)], executor).await?;
        let entries = entries
            .into_iter()
            .filter(|entry| lines.iter().any(|line| line.sales_order_line_id == entry.sales_order_line_id))
            .collect::<Vec<_>>();

        let previous = delivery.tracking_entries.clone();
        delivery.merge_observed_tracking(entries)?;
        delivery.ensure_tracking_lines(&lines)?;
        if delivery.tracking_entries != previous {
            db.deliveries().update(&mut delivery, executor).await?;
        }
    }
    Ok(())
}

fn tracking_updates(
    tracking: &OrderTracking,
    observation: &OrderObservation,
) -> BTreeMap<String, Vec<DeliveryTrackingEntry>> {
    let mut updates = BTreeMap::<String, Vec<DeliveryTrackingEntry>>::new();
    for applied in &tracking.applied_shipments {
        for shipment in observation.shipments.iter().flatten() {
            let source = shipment.source_order_no.as_deref().unwrap_or(&observation.external_order_no);
            let Some(number) = &shipment.tracking_no else { continue };
            if source != applied.source_order_no {
                continue;
            }
            let same_shipment = shipment.external_shipment_id.as_ref().or(shipment.tracking_no.as_ref())
                == Some(&applied.shipment_id);
            for line in &applied.lines {
                if !same_shipment && !shipment.line_ids.contains(&line.line_id) {
                    continue;
                }
                let sales = tracking
                    .source_lines
                    .iter()
                    .find(|source| source.fulfillment_item_id == line.line_id)
                    .and_then(|source| source.sales_order_line_id.as_ref());
                let Some(sales) = sales else { continue };
                let entry = DeliveryTrackingEntry {
                    sales_order_line_id: SalesOrderLineId::new(sales),
                    tracking_no: number.clone(),
                    carrier: shipment.carrier.clone(),
                };
                for id in &applied.delivery_ids {
                    let entries = updates.entry(id.clone()).or_default();
                    if !entries.contains(&entry) {
                        entries.push(entry.clone());
                    }
                }
            }
        }
    }
    updates
}

#[cfg(test)]
mod tests {
    use erp_core::common::time::Instant;
    use erp_supply::order_tracking::{AppliedShipment, TrackingSourceLine};
    use erp_supply::ports::connector::common::{Snapshot, SourceRevision, SourceStamp};
    use erp_supply::ports::connector::order::{OrderReference, OrderSnapshot, Shipment, ShipmentLine};

    use super::*;

    #[test]
    fn late_waybills_follow_original_child_and_sales_lines_without_quantity_allocation() {
        let sources = ["one", "two"].map(|id| TrackingSourceLine {
            fulfillment_item_id: id.into(),
            purchase_order_revision_line_id: format!("purchase-{id}"),
            sales_order_line_id: Some(format!("sale-{id}")),
            delivery_id: Some(format!("draft-{id}")),
            quantity: "2".parse().unwrap(),
        });
        let mut tracking =
            OrderTracking::new("execution".into(), "connection".into(), "purchase".into(), sources.into())
                .unwrap();
        tracking.applied_shipments = ["101", "102"]
            .map(|id| AppliedShipment {
                source_order_no: id.into(),
                shipment_id: format!("jd-order-dispatch:{id}"),
                lines: ["one", "two"]
                    .map(|line| ShipmentLine { line_id: line.into(), quantity: "1".parse().unwrap() })
                    .into(),
                delivery_ids: vec![format!("delivery-{id}")],
            })
            .into();

        let shipments = [("101", "A"), ("101", "B"), ("102", "C"), ("103", "foreign"), ("101", "A")].map(
            |(child, waybill)| Shipment {
                external_shipment_id: Some(waybill.into()),
                source_order_no: Some(child.into()),
                line_ids: vec!["one".into(), "two".into()],
                quantity_lines: None,
                complete_line_coverage: false,
                carrier: Some("京东".into()),
                tracking_no: Some(waybill.into()),
                courier_phone: None,
                events: vec![],
            },
        );
        let snapshot = OrderSnapshot {
            source_evidence: None,
            reference: OrderReference {
                merchant_order_no: "merchant".into(),
                external_order_no: "100".into(),
            },
            amounts: None,
            fulfillment: None,
            cancellation: None,
            refund: None,
            payment: None,
            confirmation: None,
            topology: None,
            complete_line_coverage: false,
            shipments: Some(shipments.into()),
        };
        let observation = OrderObservation::from_snapshot(
            "observation".into(),
            &tracking,
            Snapshot {
                stamp: SourceStamp {
                    revision: SourceRevision::Unversioned,
                    changed_at: None,
                    observed_at: Instant::now(),
                },
                value: snapshot,
            },
        )
        .unwrap();

        let updates = tracking_updates(&tracking, &observation);
        assert_eq!(updates.len(), 2);
        assert_eq!(updates["delivery-101"].len(), 4);
        assert_eq!(updates["delivery-102"].len(), 2);
        assert!(updates["delivery-101"].iter().all(|entry| ["A", "B"].contains(&entry.tracking_no.as_str())));
        assert!(updates["delivery-102"].iter().all(|entry| entry.tracking_no == "C"));
        assert_eq!(updates["delivery-101"][0].sales_order_line_id.as_ref(), "sale-one");

        assert_eq!(tracking_updates(&tracking, &observation), updates);
        assert!(tracking.pending_shipments(&observation).unwrap().is_empty());
        assert!(tracking.quantities_fully_applied().unwrap());
    }
}
