//! 首次履约确认消费冻结的采购销售分配，形成实际成本与精确销售归属。

use erp_core::common::time::Instant;
use erp_core::ids::{FileAssetId, SalesOrderId, SalesOrderLineId, SupplierAccountId};
use erp_core::money::{Amount, Quantity, Rate};

use super::{PreparedCostEntry, prepare_cost_entry};
use crate::dto::cost::{CostAllocationLineRequest, CreateCostEntryRequest};
use crate::entity::cost::{CostScope, CostStage, CostType};
use crate::{Error, Result};

/// 组合层已校验采购资格和销售关联的整笔履约成本事实。
pub struct FulfillmentCostFact {
    /// 来源采购单身份。
    pub purchase_order_id: String,
    /// 来源采购销售分配身份；同一分配只能形成一笔实际供货成本。
    pub allocation_id: String,
    /// 冻结分配版本。
    pub allocation_version: u64,
    /// 冻结分配数量。
    pub allocated_quantity: Quantity,
    /// 本次确认的履约数量。
    pub fulfilled_quantity: Quantity,
    /// 冻结分配含税成本。
    pub gross_amount: Amount,
    /// 冻结分配不含税成本。
    pub net_amount: Amount,
    /// 冻结采购行进项税率。
    pub input_tax_rate: Rate,
    /// 成本供应商。
    pub supplier_id: SupplierAccountId,
    /// 经营归属销售单。
    pub sales_order_id: SalesOrderId,
    /// 经营归属销售稳定明细。
    pub sales_order_line_id: SalesOrderLineId,
    /// 实际交付或服务完成时间。
    pub occurred_at: Instant,
    /// 已确认履约的业务凭证。
    pub evidence_attachment_id: Option<FileAssetId>,
}

/// 按冻结分配金额准备实际供货成本；不重算税额、不写入数据库。
///
/// # 参数
/// * `fact` - 已校验外域关联的首次整笔履约事实
///
/// # 返回
/// 返回实际供货成本及唯一的销售明细分配；部分数量或零不含税成本返回 `None`。
///
/// # 错误
/// 履约数量超出冻结分配或成本金额非法时返回错误。
pub fn prepare(fact: FulfillmentCostFact) -> Result<Option<PreparedCostEntry>> {
    if incomplete_or_zero_cost(&fact)? {
        return Ok(None);
    }
    let tax_amount = Amount::try_from(
        fact.gross_amount
            .to_decimal()
            .checked_sub(fact.net_amount.to_decimal())
            .ok_or_else(|| Error::BusinessLogicError("履约采购成本税额超出范围".into()))?,
    )?;
    let prepared = prepare_cost_entry(CreateCostEntryRequest {
        cost_type: CostType::Product,
        cost_stage: CostStage::Actual,
        cost_scope: CostScope::NonVoucherFulfillment,
        cost_basis: None,
        supplier_id: Some(fact.supplier_id),
        gross_amount: fact.gross_amount,
        net_amount: fact.net_amount,
        tax_amount,
        tax_inclusion: true,
        input_tax_rate: fact.input_tax_rate,
        occurred_at: fact.occurred_at,
        source_fact_type: "purchase_fulfillment".into(),
        source_document_id: fact.purchase_order_id,
        source_line_id: fact.allocation_id,
        source_version: fact.allocation_version.to_string(),
        evidence_attachment_id: fact.evidence_attachment_id,
        allocations: vec![CostAllocationLineRequest {
            sales_order_id: fact.sales_order_id,
            sales_order_line_id: Some(fact.sales_order_line_id),
            allocated_gross_amount: fact.gross_amount,
            allocated_net_amount: fact.net_amount,
            rounding_residual_flag: None,
        }],
    })?;
    Ok(Some(prepared))
}

/// 部分履约与零不含税金额不能生成整笔成本，保留履约事实并保持成本未覆盖。
fn incomplete_or_zero_cost(fact: &FulfillmentCostFact) -> Result<bool> {
    if fact.fulfilled_quantity > fact.allocated_quantity {
        return Err(Error::BusinessLogicError("履约数量不得超出冻结采购销售分配数量".into()));
    }
    if fact.gross_amount < fact.net_amount {
        return Err(Error::BusinessLogicError("履约采购成本不含税金额不得超过含税金额".into()));
    }
    Ok(fact.fulfilled_quantity < fact.allocated_quantity || fact.net_amount.to_decimal().is_zero())
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use erp_core::common::time::Instant;
    use erp_core::ids::{FileAssetId, SalesOrderId, SalesOrderLineId, SupplierAccountId};
    use erp_core::money::{Amount, Quantity, Rate};

    use super::{FulfillmentCostFact, prepare};
    use crate::entity::cost::{CostStage, CostType};

    /// 构造有尾差的冻结成本；准备过程必须逐分保持原分配金额。
    fn fact() -> FulfillmentCostFact {
        FulfillmentCostFact {
            purchase_order_id: "po-1".into(),
            allocation_id: "allocation-1".into(),
            allocation_version: 1,
            allocated_quantity: Quantity::from_str("3").unwrap(),
            fulfilled_quantity: Quantity::from_str("3").unwrap(),
            gross_amount: Amount::from_str("10.01").unwrap(),
            net_amount: Amount::from_str("8.70").unwrap(),
            input_tax_rate: Rate::from_str("0.13").unwrap(),
            supplier_id: SupplierAccountId::new("supplier-1"),
            sales_order_id: SalesOrderId::new("so-1"),
            sales_order_line_id: SalesOrderLineId::new("line-1"),
            occurred_at: Instant::from_unix_secs(1_700_000_000),
            evidence_attachment_id: Some(FileAssetId::new("file-1")),
        }
    }

    /// 实际成本采用冻结分配，不按最新报价或税率重新计算。
    #[test]
    fn exact_frozen_cost_and_sales_line_are_preserved() {
        let prepared = prepare(fact()).unwrap().unwrap();
        assert_eq!(prepared.entry.cost_stage, CostStage::Actual);
        assert_eq!(prepared.entry.cost_type, CostType::Product);
        assert_eq!(prepared.entry.gross_amount.to_string(), "10.01");
        assert_eq!(prepared.entry.net_amount.to_string(), "8.70");
        assert_eq!(prepared.entry.tax_amount.to_string(), "1.31");
        assert_eq!(prepared.entry.source_document_id, "po-1");
        assert_eq!(prepared.entry.source_line_id, "allocation-1");
        assert_eq!(prepared.entry.source_version, "1");
        assert_eq!(prepared.allocations.len(), 1);
        assert_eq!(prepared.allocations[0].allocated_net_amount, prepared.entry.net_amount);
        assert_eq!(prepared.allocations[0].sales_order_line_id.as_ref().unwrap().as_ref(), "line-1");
    }

    /// 首次确认不能把部分履约伪装成整笔成本，也不能产生负税额成本。
    #[test]
    fn inconsistent_quantity_and_amounts_are_rejected() {
        let mut partial = fact();
        partial.fulfilled_quantity = Quantity::from_str("1").unwrap();
        assert!(prepare(partial).unwrap().is_none());
        let mut excess = fact();
        excess.fulfilled_quantity = Quantity::from_str("4").unwrap();
        assert!(prepare(excess).is_err());
        let mut invalid = fact();
        invalid.net_amount = Amount::from_str("10.02").unwrap();
        assert!(prepare(invalid).is_err());
    }

    /// 没有实际金额时不创建非法的零金额成本分配。
    #[test]
    fn zero_cost_does_not_fail_fulfillment_confirmation() {
        let mut zero = fact();
        zero.gross_amount = Amount::from_str("0").unwrap();
        zero.net_amount = Amount::from_str("0").unwrap();
        assert!(prepare(zero).unwrap().is_none());
        let mut zero_net = fact();
        zero_net.net_amount = Amount::from_str("0").unwrap();
        assert!(prepare(zero_net).unwrap().is_none());
    }
}
