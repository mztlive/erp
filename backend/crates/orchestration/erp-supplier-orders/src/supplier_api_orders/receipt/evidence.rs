//! 从原单据的净有效数量生成实收证明，不从京东订单或物流反推收到数量。

use std::collections::BTreeMap;

use erp_core::money::Quantity;
use erp_fulfillment::entity::fulfillment::{
    AcceptanceFulfillmentAllocation, CustomerAcceptance, CustomerAcceptanceLine, Delivery, DeliveryLine,
    PurchaseReceipt, PurchaseReceiptLine,
};
use erp_procurement::entity::purchase_order::{PurchaseLineSalesAllocation, PurchaseOrderRevisionLine};
use erp_supply::api_order::ApiOrderExecution;
use erp_supply::order_tracking::OrderTracking;
use erp_supply::ports::connector::order::OrderLine;
use erp_supply::ports::connector::receipt::ReceiptLine;
use rust_decimal::Decimal;
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::{Error, Result};

#[derive(Default)]
pub(super) struct DirectReceiptFacts {
    pub deliveries: Vec<Delivery>,
    pub delivery_lines: Vec<DeliveryLine>,
    pub allocations: Vec<AcceptanceFulfillmentAllocation>,
    pub acceptance_lines: Vec<CustomerAcceptanceLine>,
    pub acceptances: Vec<CustomerAcceptance>,
    pub purchase_allocations: Vec<PurchaseLineSalesAllocation>,
}

pub(super) struct WarehouseReceiptFacts {
    pub receipts: Vec<PurchaseReceipt>,
    pub lines: Vec<PurchaseReceiptLine>,
    pub purchase_lines: Vec<PurchaseOrderRevisionLine>,
    pub executions: Vec<ApiOrderExecution>,
    pub warehouse_id: String,
}

pub(super) enum ActualReceiptFacts {
    Direct(DirectReceiptFacts),
    Warehouse(WarehouseReceiptFacts),
}

/// 证明覆盖整个执行组后才逐叶子通知；不会把一笔部分验收猜分给多个子单。
pub(super) struct ReceiptEvidence {
    pub lines: Vec<ReceiptLine>,
    pub fingerprint: String,
}

impl ReceiptEvidence {
    pub(super) fn from_facts(
        execution: &ApiOrderExecution,
        tracking: &OrderTracking,
        frozen: &[OrderLine],
        facts: &ActualReceiptFacts,
    ) -> Result<Self> {
        let quantities = match facts {
            ActualReceiptFacts::Direct(facts) => facts.quantities(execution, tracking)?,
            ActualReceiptFacts::Warehouse(facts) => facts.quantities(execution)?,
        };
        let lines = frozen
            .iter()
            .map(|line| ReceiptLine {
                line_id: line.line_id.clone(),
                sku: line.sku.clone(),
                quantity: quantities.get(&line.line_id).copied().unwrap_or_else(zero),
            })
            .collect::<Vec<_>>();
        execution.ensure_receipt_coverage(&lines)?;

        let fingerprint = match facts {
            ActualReceiptFacts::Direct(facts) => fingerprint(&(
                &lines,
                &facts.deliveries,
                &facts.delivery_lines,
                &facts.allocations,
                &facts.acceptance_lines,
                &facts.acceptances,
                &facts.purchase_allocations,
            ))?,
            ActualReceiptFacts::Warehouse(facts) => {
                fingerprint(&(&lines, &facts.receipts, &facts.lines, &facts.purchase_lines))?
            },
        };
        Ok(Self { lines, fingerprint })
    }
}

pub(super) fn add(total: &mut Quantity, quantity: Quantity) -> Result<()> {
    *total = Quantity::try_from(
        total
            .to_decimal()
            .checked_add(quantity.to_decimal())
            .ok_or_else(|| Error::BusinessLogicError("实际收到数量汇总超限".into()))?,
    )?;
    Ok(())
}

pub(super) fn fingerprint(value: &impl Serialize) -> Result<String> {
    let bytes = serde_json::to_vec(value).map_err(|_| Error::Internal("实际收到依据无法保存摘要".into()))?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

pub(super) fn received_map() -> BTreeMap<String, Quantity> {
    BTreeMap::new()
}

pub(super) fn zero() -> Quantity {
    Quantity::try_from(Decimal::ZERO).expect("固定零数量合法")
}
