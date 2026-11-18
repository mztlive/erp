use std::collections::BTreeMap;

use erp_core::common::time::Instant;
use erp_core::money::Quantity;
use erp_fulfillment::entity::fulfillment::{
    AcceptanceFulfillmentAllocation, AllocationAction, CustomerAcceptanceState, DeliveryLine, DeliveryState,
    DeliveryType, FulfillmentFactType,
};
use erp_supply::api_order::{ApiOrderExecution, ApiOrderSourceLine};
use erp_supply::order_tracking::OrderTracking;

use super::evidence::{DirectReceiptFacts, add, received_map, zero};
use crate::{Error, Result};

impl DirectReceiptFacts {
    pub(super) fn quantities(
        &self,
        execution: &ApiOrderExecution,
        tracking: &OrderTracking,
    ) -> Result<BTreeMap<String, Quantity>> {
        let mut quantities = received_map();
        for line in &self.delivery_lines {
            let delivery = self
                .deliveries
                .iter()
                .find(|row| row.base.id == line.delivery_id.as_ref())
                .ok_or_else(|| Error::BusinessLogicError("实际验收缺少原发货单".into()))?;
            if !matches!(delivery.status, DeliveryState::Shipped | DeliveryState::Signed) {
                continue;
            }
            if delivery.delivery_type != DeliveryType::SupplierDirect
                || delivery.purchase_order_id.as_ref().map(|id| id.as_ref())
                    != Some(execution.purchase_order_id.as_str())
                || delivery.shipped_at.is_none_or(|at| at > Instant::now())
            {
                return Err(Error::BusinessLogicError("实际验收的发货单不属于原采购直发执行".into()));
            }

            let source = self.source(execution, tracking, line)?;
            self.ensure_acceptance_lineage(line, delivery.sales_order_id.as_ref())?;

            let net =
                AcceptanceFulfillmentAllocation::net_quantity_for_fact(&self.allocations, &line.base.id)?;
            if net > line.quantity {
                return Err(Error::BusinessLogicError("原发货行净验收超过实际交付数量".into()));
            }
            add(quantities.entry(source.fulfillment_item_id.clone()).or_insert_with(zero), net)?;
        }
        Ok(quantities)
    }

    fn source<'a>(
        &self,
        execution: &'a ApiOrderExecution,
        tracking: &OrderTracking,
        line: &DeliveryLine,
    ) -> Result<&'a ApiOrderSourceLine> {
        let allocation = line
            .purchase_line_sales_allocation_id
            .as_ref()
            .and_then(|id| self.purchase_allocations.iter().find(|row| row.base.id == id.as_ref()))
            .ok_or_else(|| Error::BusinessLogicError("原实际发货缺少可核实的采购销售分配".into()))?;

        let candidates = execution
            .source_lines
            .iter()
            .filter(|source| {
                source.purchase_order_revision_line_id == allocation.purchase_order_revision_line_id.as_ref()
                    && source.sales_order_line_id.as_deref() == Some(line.sales_order_line_id.as_ref())
                    && tracking.source_lines.iter().any(|tracked| {
                        tracked.fulfillment_item_id == source.fulfillment_item_id
                            && (tracked
                                .delivery_id
                                .as_ref()
                                .is_some_and(|id| id == line.delivery_id.as_ref())
                                || tracking.applied_shipments.iter().any(|shipment| {
                                    shipment.delivery_ids.iter().any(|id| id == line.delivery_id.as_ref())
                                        && shipment
                                            .lines
                                            .iter()
                                            .any(|part| part.line_id == source.fulfillment_item_id)
                                }))
                    })
            })
            .collect::<Vec<_>>();
        if candidates.len() != 1 {
            return Err(Error::BusinessLogicError(
                "实际发货与原执行行数量归属不唯一，请核对原交付分配；未按SKU分摊".into(),
            ));
        }
        Ok(candidates[0])
    }

    fn ensure_acceptance_lineage(&self, line: &DeliveryLine, sales_order: &str) -> Result<()> {
        for allocation in self.allocations.iter().filter(|row| row.fulfillment_line_id == line.base.id) {
            if allocation.fulfillment_fact_type != FulfillmentFactType::Delivery {
                return Err(Error::BusinessLogicError("实际验收分配不是原发货事实，不能通知收到商品".into()));
            }
            let acceptance_line = self
                .acceptance_lines
                .iter()
                .find(|row| row.base.id == allocation.customer_acceptance_line_id.as_ref())
                .ok_or_else(|| Error::BusinessLogicError("净验收分配缺少原验收行".into()))?;
            let acceptance = self
                .acceptances
                .iter()
                .find(|row| row.base.id == acceptance_line.customer_acceptance_id.as_ref())
                .ok_or_else(|| Error::BusinessLogicError("净验收分配缺少原验收单".into()))?;

            if acceptance.status == CustomerAcceptanceState::Draft
                || acceptance.sales_order_id.as_ref() != sales_order
                || acceptance_line.sales_order_line_id != line.sales_order_line_id
                || acceptance.evidence_attachment_id.is_none()
                || acceptance.accepted_at > Instant::now()
            {
                return Err(Error::BusinessLogicError("原验收状态、签收依据或销售行与实际交付不一致".into()));
            }

            if acceptance.status == CustomerAcceptanceState::Reversed
                && allocation.allocation_action == AllocationAction::Apply
                && !self.allocations.iter().any(|reverse| {
                    reverse.allocation_action == AllocationAction::Reverse
                        && reverse.reverses_allocation_id.as_ref().map(|id| id.as_ref())
                            == Some(allocation.base.id.as_str())
                })
            {
                return Err(Error::BusinessLogicError(
                    "原验收已冲正但反向数量依据不完整，请核对原记录".into(),
                ));
            }
            // 历史短少/拒收不等于当前仍缺货；后续正式验收可补齐，是否收全只看净通过数量。
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use entity_core::BaseModel;
    use erp_core::common::time::Instant;
    use erp_core::ids::{
        AcceptanceFulfillmentAllocationId, CustomerAcceptanceId, CustomerAcceptanceLineId, DeliveryId,
        DeliveryLineId, FileAssetId, PurchaseLineSalesAllocationId, PurchaseOrderId,
        PurchaseOrderRevisionLineId, SalesOrderId, SalesOrderLineId, SalesOrderRevisionLineId,
    };
    use erp_core::money::Amount;
    use erp_fulfillment::entity::fulfillment::{
        AcceptanceResult, AllocationAction, CustomerAcceptance, CustomerAcceptanceData,
        CustomerAcceptanceLine, CustomerAcceptanceLineData, Delivery, DeliveryData, DeliveryLineData,
        FulfillmentFactType,
    };
    use erp_procurement::entity::purchase_order::PurchaseLineSalesAllocation;
    use erp_supply::order_tracking::TrackingSourceLine;

    use super::*;
    use crate::supplier_api_orders::receipt::tests::fixture;

    fn qty(value: &str) -> Quantity {
        value.parse().unwrap()
    }
    fn facts() -> (ApiOrderExecution, OrderTracking, DirectReceiptFacts) {
        let (execution, _) = fixture();
        let tracking = OrderTracking::new(
            "execution".into(),
            "connection".into(),
            "purchase".into(),
            vec![TrackingSourceLine {
                fulfillment_item_id: "item".into(),
                purchase_order_revision_line_id: "purchase-line".into(),
                sales_order_line_id: Some("sales-line".into()),
                delivery_id: Some("delivery".into()),
                quantity: qty("2"),
            }],
        )
        .unwrap();

        let mut delivery = Delivery::new(
            DeliveryId::new("delivery"),
            DeliveryData {
                delivery_no: "FH-1".into(),
                delivery_type: DeliveryType::SupplierDirect,
                sales_order_id: SalesOrderId::new("sale"),
                purchase_order_id: Some(PurchaseOrderId::new("purchase")),
                warehouse_id: None,
                tracking_entries: vec![],
                address_snapshot_encrypted: None,
                address_snapshot_fingerprint: None,
            },
        )
        .unwrap();
        delivery.status = DeliveryState::Shipped;
        delivery.shipped_at = Some(Instant::now());
        let line = DeliveryLine::new(
            DeliveryLineId::new("delivery-line"),
            DeliveryLineData {
                delivery_id: DeliveryId::new("delivery"),
                line_no: 1,
                sales_order_line_id: SalesOrderLineId::new("sales-line"),
                quantity: qty("2"),
                stock_reservation_id: None,
                purchase_line_sales_allocation_id: Some(PurchaseLineSalesAllocationId::new("allocation")),
            },
            DeliveryType::SupplierDirect,
        )
        .unwrap();

        let mut acceptance = CustomerAcceptance::new(
            CustomerAcceptanceId::new("acceptance"),
            CustomerAcceptanceData {
                acceptance_no: "QS-1".into(),
                sales_order_id: SalesOrderId::new("sale"),
                accepted_at: Instant::now(),
                result: AcceptanceResult::Passed,
                evidence_attachment_id: Some(FileAssetId::new("evidence")),
            },
        )
        .unwrap();
        acceptance.status = CustomerAcceptanceState::Posted;
        let acceptance_line = CustomerAcceptanceLine::new(
            CustomerAcceptanceLineId::new("acceptance-line"),
            CustomerAcceptanceLineData {
                customer_acceptance_id: CustomerAcceptanceId::new("acceptance"),
                line_no: 1,
                sales_order_line_id: SalesOrderLineId::new("sales-line"),
                accepted_quantity: qty("2"),
                short_quantity: qty("0"),
                rejected_quantity: qty("0"),
                reason: None,
                evidence_attachment_id: None,
            },
        )
        .unwrap();
        let allocation = AcceptanceFulfillmentAllocation {
            base: BaseModel { id: "accepted".into(), ..BaseModel::fake() },
            customer_acceptance_line_id: CustomerAcceptanceLineId::new("acceptance-line"),
            fulfillment_fact_type: FulfillmentFactType::Delivery,
            fulfillment_line_id: "delivery-line".into(),
            allocation_action: AllocationAction::Apply,
            allocated_quantity: qty("2"),
            reverses_allocation_id: None,
        };

        let purchase_allocation = PurchaseLineSalesAllocation {
            base: BaseModel { id: "allocation".into(), ..BaseModel::fake() },
            purchase_order_revision_line_id: PurchaseOrderRevisionLineId::new("purchase-line"),
            sales_order_revision_line_id: SalesOrderRevisionLineId::new("sales-revision-line"),
            allocated_quantity: qty("2"),
            allocated_cost_gross: Amount::zero(),
            allocated_cost_net: Amount::zero(),
        };
        (
            execution,
            tracking,
            DirectReceiptFacts {
                deliveries: vec![delivery],
                delivery_lines: vec![line],
                allocations: vec![allocation],
                acceptance_lines: vec![acceptance_line],
                acceptances: vec![acceptance],
                purchase_allocations: vec![purchase_allocation],
            },
        )
    }

    #[test]
    fn actual_direct_acceptance_keeps_source_line_identity_and_refuses_ambiguous_or_foreign_proof() {
        let (execution, tracking, facts) = facts();
        assert_eq!(facts.quantities(&execution, &tracking).unwrap()["item"], qty("2"));

        for invalid in [
            "allocation",
            "tracking",
            "sales",
            "evidence",
            "draft",
            "wrong-kind",
            "missing-reverse",
            "ambiguous",
        ] {
            let (mut execution, mut tracking, mut facts) = self::facts();
            match invalid {
                "allocation" => facts.purchase_allocations.clear(),
                "tracking" => tracking.source_lines[0].delivery_id = None,
                "sales" => facts.acceptance_lines[0].sales_order_line_id = SalesOrderLineId::new("foreign"),
                "evidence" => facts.acceptances[0].evidence_attachment_id = None,
                "draft" => facts.acceptances[0].status = CustomerAcceptanceState::Draft,
                "wrong-kind" => {
                    facts.allocations[0].fulfillment_fact_type = FulfillmentFactType::ElectronicDelivery
                },
                "missing-reverse" => facts.acceptances[0].status = CustomerAcceptanceState::Reversed,
                _ => {
                    let mut duplicate = execution.source_lines[0].clone();
                    duplicate.fulfillment_item_id = "duplicate".into();
                    execution.source_lines.push(duplicate);
                    let mut tracked = tracking.source_lines[0].clone();
                    tracked.fulfillment_item_id = "duplicate".into();
                    tracking.source_lines.push(tracked);
                },
            }
            assert!(facts.quantities(&execution, &tracking).is_err(), "{invalid}");
        }
    }

    #[test]
    fn later_actual_acceptance_can_complete_shortage_and_reverse_removes_receipt_coverage() {
        let (execution, tracking, mut facts) = facts();
        facts.allocations[0].allocated_quantity = qty("1");
        facts.acceptance_lines[0].accepted_quantity = qty("1");
        facts.acceptance_lines[0].short_quantity = qty("1");
        assert_eq!(facts.quantities(&execution, &tracking).unwrap()["item"], qty("1"));

        let mut later = facts.acceptance_lines[0].clone();
        later.base.id = "later-line".into();
        later.short_quantity = qty("0");
        facts.acceptance_lines.push(later);
        let mut later = facts.allocations[0].clone();
        later.base.id = "later-accepted".into();
        later.customer_acceptance_line_id = CustomerAcceptanceLineId::new("later-line");
        facts.allocations.push(later);
        assert_eq!(facts.quantities(&execution, &tracking).unwrap()["item"], qty("2"));

        let mut reverse = facts.allocations[0].clone();
        reverse.base.id = "reversed".into();
        reverse.allocation_action = AllocationAction::Reverse;
        reverse.reverses_allocation_id = Some(AcceptanceFulfillmentAllocationId::new("accepted"));
        facts.allocations.push(reverse);
        assert_eq!(facts.quantities(&execution, &tracking).unwrap()["item"], qty("1"));
    }
}
