//! `sales_invoice_allocation` 销项发票分配（数据模型 §6.8）。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::errors::{Error, Result};
use crate::ids::{InvoiceId, ReceivableAccountId, SalesInvoiceAllocationId};
use crate::money::Amount;

use super::receipt_allocation::{validate_action_reference, AllocationAction};

/// 销项发票分配创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SalesInvoiceAllocationData {
    /// 销项发票。
    pub invoice_id: InvoiceId,
    /// 销售单可开票对象（应收往来子账）。
    pub receivable_account_id: ReceivableAccountId,
    /// 发票内追加序号（从 1 开始）。
    pub allocation_seq: u32,
    /// 分配动作。
    pub allocation_action: AllocationAction,
    /// 分配含税金额。
    pub allocated_gross_amount: Amount,
    /// 分配不含税金额。
    pub allocated_net_amount: Amount,
    /// 分配税额。
    pub allocated_tax_amount: Amount,
    /// 红票反向分配引用的原蓝票分配。
    pub reverses_allocation_id: Option<SalesInvoiceAllocationId>,
}

/// 销项发票分配实体（正式事实，数据模型 §6.8）。
///
/// `(invoice_id, allocation_seq)` 唯一；金额三元组满足 gross = net + tax 恒等。
/// 蓝票净分配不得超过发票有效余额与目标子账 `open_invoiceable_total`、红票只
/// 允许反向原蓝票分配且累计不超过原有效分配是跨行约束，由 P3 登记事务双侧
/// 锁定校验（§8.3）。分配行过账后不可更新或删除。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct SalesInvoiceAllocation {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 销项发票。
    pub invoice_id: InvoiceId,
    /// 应收往来子账。
    pub receivable_account_id: ReceivableAccountId,
    /// 发票内追加序号。
    pub allocation_seq: u32,
    /// 分配动作。
    pub allocation_action: AllocationAction,
    /// 分配含税金额。
    pub allocated_gross_amount: Amount,
    /// 分配不含税金额。
    pub allocated_net_amount: Amount,
    /// 分配税额。
    pub allocated_tax_amount: Amount,
    /// 红票引用的原蓝票分配。
    pub reverses_allocation_id: Option<SalesInvoiceAllocationId>,
}

impl SalesInvoiceAllocation {
    /// 创建销项发票分配。
    ///
    /// 完成金额恒等（gross = net + tax）与正数、序号从 1 起与「动作 ↔ 原分配
    /// 引用」一致性校验。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::SalesInvoiceAllocationId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的分配实体。
    ///
    /// # 错误
    /// 当金额恒等不成立/非正、序号为 0 或动作与引用不一致时返回错误。
    pub fn new(id: SalesInvoiceAllocationId, data: SalesInvoiceAllocationData) -> Result<Self> {
        validate_amounts(
            data.allocated_gross_amount,
            data.allocated_net_amount,
            data.allocated_tax_amount,
        )?;
        if data.allocation_seq == 0 {
            return Err(Error::from("分配序号必须从 1 开始"));
        }
        validate_action_reference(data.allocation_action, &data.reverses_allocation_id)?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            invoice_id: data.invoice_id,
            receivable_account_id: data.receivable_account_id,
            allocation_seq: data.allocation_seq,
            allocation_action: data.allocation_action,
            allocated_gross_amount: data.allocated_gross_amount,
            allocated_net_amount: data.allocated_net_amount,
            allocated_tax_amount: data.allocated_tax_amount,
            reverses_allocation_id: data.reverses_allocation_id,
        })
    }

    /// 更新销项发票分配。
    ///
    /// 分配行过账后不可更新或删除（数据模型 §6.9 末尾），任何修改都被拒绝。
    ///
    /// # 参数
    /// * `update` - 更新数据（恒被拒绝）
    /// * `updated_by` - 更新执行人（恒被拒绝）
    ///
    /// # 返回
    /// 恒返回错误。
    pub fn update(
        &mut self,
        update: SalesInvoiceAllocationData,
        updated_by: impl Into<String>,
    ) -> Result<()> {
        let _ = (update, updated_by);
        Err(Error::from("正式事实过账后不可更新或删除"))
    }
}

/// 校验分配金额三元组恒等且为正。
///
/// # 参数
/// * `gross` - 分配含税金额
/// * `net` - 分配不含税金额
/// * `tax` - 分配税额
///
/// # 返回
/// 恒等成立返回 `Ok(())`。
///
/// # 错误
/// 金额非正或恒等不成立时返回错误。
fn validate_amounts(gross: Amount, net: Amount, tax: Amount) -> Result<()> {
    if gross.to_decimal().is_sign_negative()
        || gross.to_decimal().is_zero()
        || net.to_decimal().is_sign_negative()
        || tax.to_decimal().is_sign_negative()
    {
        return Err(Error::from("分配金额必须为正数"));
    }
    if gross != net.checked_add(tax) {
        return Err(Error::from("分配含税金额必须等于不含税金额加税额"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::money::{line_amounts, Quantity, Rate, UnitPrice};
    use std::str::FromStr;

    fn data() -> SalesInvoiceAllocationData {
        let (gross, net, tax) = line_amounts(
            UnitPrice::from_str("9.9900").unwrap(),
            Quantity::from_str("3.000000").unwrap(),
            Rate::from_str("0.130000").unwrap(),
        );
        SalesInvoiceAllocationData {
            invoice_id: InvoiceId::new("inv-1"),
            receivable_account_id: ReceivableAccountId::new("ra-1"),
            allocation_seq: 1,
            allocation_action: AllocationAction::Apply,
            allocated_gross_amount: gross,
            allocated_net_amount: net,
            allocated_tax_amount: tax,
            reverses_allocation_id: None,
        }
    }

    #[test]
    fn new_keeps_line_amount_identity() {
        let allocation = SalesInvoiceAllocation::new(SalesInvoiceAllocationId::new("si-1"), data()).unwrap();

        assert_eq!(
            allocation.allocated_gross_amount,
            allocation
                .allocated_net_amount
                .checked_add(allocation.allocated_tax_amount),
            "gross = net + tax 必须精确成立"
        );
        assert_eq!(
            allocation.allocated_gross_amount,
            crate::money::Amount::from_str("29.97").unwrap()
        );
    }

    #[test]
    fn new_rejects_amount_mismatch_and_zero_seq() {
        let mismatch = SalesInvoiceAllocationData {
            allocated_tax_amount: crate::money::Amount::from_str("3.89").unwrap(),
            ..data()
        };
        assert!(SalesInvoiceAllocation::new(SalesInvoiceAllocationId::new("si-2"), mismatch).is_err());

        let zero_seq = SalesInvoiceAllocationData {
            allocation_seq: 0,
            ..data()
        };
        assert!(SalesInvoiceAllocation::new(SalesInvoiceAllocationId::new("si-3"), zero_seq).is_err());
    }

    #[test]
    fn new_enforces_action_reference_consistency() {
        let reverse_without_ref = SalesInvoiceAllocationData {
            allocation_action: AllocationAction::Reverse,
            reverses_allocation_id: None,
            ..data()
        };
        assert!(
            SalesInvoiceAllocation::new(SalesInvoiceAllocationId::new("si-4"), reverse_without_ref).is_err()
        );

        let reverse_valid = SalesInvoiceAllocationData {
            allocation_action: AllocationAction::Reverse,
            reverses_allocation_id: Some(SalesInvoiceAllocationId::new("si-1")),
            ..data()
        };
        assert!(SalesInvoiceAllocation::new(SalesInvoiceAllocationId::new("si-5"), reverse_valid).is_ok());
    }

    #[test]
    fn formal_fact_rejects_update() {
        let mut allocation =
            SalesInvoiceAllocation::new(SalesInvoiceAllocationId::new("si-1"), data()).unwrap();
        assert!(allocation.update(data(), "admin-2").is_err());
    }
}
