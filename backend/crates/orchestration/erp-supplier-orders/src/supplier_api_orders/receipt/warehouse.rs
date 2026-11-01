use std::collections::BTreeMap;

use erp_core::common::time::Instant;
use erp_core::money::Quantity;
use erp_fulfillment::entity::fulfillment::PurchaseReceiptState;
use erp_procurement::entity::purchase_order::PurchaseOrderRevisionLine;
use erp_supply::api_order::ApiOrderExecution;

use super::evidence::{WarehouseReceiptFacts, add, received_map, zero};
use crate::{Error, Result};

impl WarehouseReceiptFacts {
    pub(super) fn quantities(&self, execution: &ApiOrderExecution) -> Result<BTreeMap<String, Quantity>> {
        let mut received = received_map();
        for source in &execution.source_lines {
            let original = self
                .purchase_lines
                .iter()
                .find(|line| line.base.id == source.purchase_order_revision_line_id)
                .ok_or_else(|| Error::BusinessLogicError("实际入库缺少原采购冻结行".into()))?;
            let approved = self.approved_quantity(original)?;
            let qualified = self.qualified_quantity(execution, &original.base.id)?;
            // 只有原采购行全部实收才证明该行每个执行组均收到；不对部分入库猜分子单。
            if qualified != approved {
                return Err(Error::BusinessLogicError(
                    "原采购行尚未全部合格入库，请继续办理实际收货；未按SKU分摊".into(),
                ));
            }
            received.insert(source.fulfillment_item_id.clone(), source.quantity);
        }
        Ok(received)
    }

    fn approved_quantity(&self, original: &PurchaseOrderRevisionLine) -> Result<Quantity> {
        let approved =
            original.quantity.ok_or_else(|| Error::BusinessLogicError("原采购行缺少批准数量".into()))?;
        let mut grouped = zero();
        for group in self.executions.iter().filter(|group| group.superseded_by_purchase_revision_id.is_none())
        {
            for row in group
                .source_lines
                .iter()
                .filter(|row| row.purchase_order_revision_line_id == original.base.id)
            {
                add(&mut grouped, row.quantity)?;
            }
        }
        if grouped != approved {
            return Err(Error::BusinessLogicError(
                "原采购行与全部接口执行数量不守恒，不能分配入库收货".into(),
            ));
        }
        Ok(approved)
    }

    fn qualified_quantity(&self, execution: &ApiOrderExecution, original_line: &str) -> Result<Quantity> {
        let mut qualified = zero();
        for line in
            self.lines.iter().filter(|line| line.purchase_order_revision_line_id.as_ref() == original_line)
        {
            let receipt = self
                .receipts
                .iter()
                .find(|receipt| receipt.base.id == line.purchase_receipt_id.as_ref())
                .ok_or_else(|| Error::BusinessLogicError("实际入库行缺少原入库单".into()))?;
            if receipt.purchase_order_id.as_ref() != execution.purchase_order_id
                || receipt.warehouse_id.as_ref() != self.warehouse_id
                || receipt.status != PurchaseReceiptState::Posted
                || receipt.posted_at.is_none_or(|at| at > Instant::now())
                || line.received_quantity != line.qualified_quantity
                || line.rejected_quantity != zero()
            {
                return Err(Error::BusinessLogicError(
                    "原入库仍有拒收、待检或仓库归属未核实，不能通知供应商全部收到".into(),
                ));
            }
            add(&mut qualified, line.qualified_quantity)?;
        }
        Ok(qualified)
    }
}
