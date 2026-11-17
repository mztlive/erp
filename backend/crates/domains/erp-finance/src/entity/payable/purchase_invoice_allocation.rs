//! `purchase_invoice_allocation` 进项发票分配（数据模型 §6.9）。

use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::ids::{InvoiceId, PayableAccountId, PurchaseInvoiceAllocationId};
use erp_core::money::Amount;
use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};

use super::payment_allocation::AllocationAction;
use crate::entity::receivable::allocation_amount::validate_invoice_amounts;
use crate::entity::receivable::receipt_allocation::validate_action_reference;

/// 进项发票分配创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PurchaseInvoiceAllocationData {
    /// 进项发票。
    pub invoice_id: InvoiceId,
    /// 采购单或供应商结算单应付子账。
    pub payable_account_id: PayableAccountId,
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
    /// 进项红票反向原蓝票分配。
    pub reverses_allocation_id: Option<PurchaseInvoiceAllocationId>,
}

/// 进项发票分配实体（正式事实，数据模型 §6.9）。
///
/// `(invoice_id, allocation_seq)` 唯一；金额三元组满足 gross = net + tax 恒等。
/// 进项发票只能分配同一供应商的应付、蓝票受发票有效余额与目标
/// `open_invoiceable_total` 双侧上限、红票只反向原蓝票分配且累计不超过原有效
/// 分配是跨行约束，由 P3 登记事务双侧锁定校验（§8.3）。与收付款核销完全独立。
/// 分配行过账后不可更新或删除。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct PurchaseInvoiceAllocation {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 进项发票。
    pub invoice_id: InvoiceId,
    /// 应付往来子账。
    pub payable_account_id: PayableAccountId,
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
    pub reverses_allocation_id: Option<PurchaseInvoiceAllocationId>,
}

impl PurchaseInvoiceAllocation {
    /// 创建进项发票分配。
    ///
    /// 完成金额恒等（gross = net + tax）与正数、序号从 1 起与「动作 ↔ 原分配
    /// 引用」一致性校验。
    ///
    /// # 参数
    /// * `id` - 实体主键（`erp_core::ids::PurchaseInvoiceAllocationId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的分配实体。
    ///
    /// # 错误
    /// 当金额恒等不成立/非正、序号为 0 或动作与引用不一致时返回错误。
    pub fn new(id: PurchaseInvoiceAllocationId, data: PurchaseInvoiceAllocationData) -> Result<Self> {
        validate_invoice_amounts(
            data.allocated_gross_amount,
            data.allocated_net_amount,
            data.allocated_tax_amount,
        )?;
        if data.allocation_seq == 0 {
            return Err(Error::from("分配序号必须从 1 开始"));
        }
        validate_action_reference(data.allocation_action.into(), data.reverses_allocation_id.is_some())?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            invoice_id: data.invoice_id,
            payable_account_id: data.payable_account_id,
            allocation_seq: data.allocation_seq,
            allocation_action: data.allocation_action,
            allocated_gross_amount: data.allocated_gross_amount,
            allocated_net_amount: data.allocated_net_amount,
            allocated_tax_amount: data.allocated_tax_amount,
            reverses_allocation_id: data.reverses_allocation_id,
        })
    }

    /// 更新进项发票分配。
    ///
    /// 分配行过账后不可更新或删除（数据模型 §6.9 末尾），任何修改都被拒绝。
    ///
    /// # 参数
    /// * `update` - 更新数据（恒被拒绝）
    /// * `updated_by` - 更新执行人（恒被拒绝）
    ///
    /// # 返回
    /// 恒返回错误。
    ///
    /// # 错误
    /// 任何调用都返回错误：正式事实过账后不可更新或删除。
    pub fn update(
        &mut self,
        update: PurchaseInvoiceAllocationData,
        updated_by: impl Into<String>,
    ) -> Result<()> {
        let _ = (update, updated_by);
        Err(Error::from("正式事实过账后不可更新或删除"))
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use erp_core::money::{Quantity, Rate, UnitPrice, line_amounts};

    use super::*;

    fn data() -> PurchaseInvoiceAllocationData {
        let (gross, net, tax) = line_amounts(
            UnitPrice::from_str("100.0000").unwrap(),
            Quantity::from_str("2.500000").unwrap(),
            Rate::from_str("0.060000").unwrap(),
        );
        PurchaseInvoiceAllocationData {
            invoice_id: InvoiceId::new("inv-9"),
            payable_account_id: PayableAccountId::new("pa-1"),
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
        let allocation =
            PurchaseInvoiceAllocation::new(PurchaseInvoiceAllocationId::new("pi-1"), data()).unwrap();

        assert_eq!(
            allocation.allocated_gross_amount,
            allocation.allocated_net_amount.checked_add(allocation.allocated_tax_amount),
            "gross = net + tax 必须精确成立"
        );
        assert_eq!(allocation.allocated_gross_amount, erp_core::money::Amount::from_str("250.00").unwrap());
    }

    #[test]
    fn new_rejects_amount_mismatch_and_zero_seq() {
        let zero_seq = PurchaseInvoiceAllocationData {
            allocation_seq: 0,
            reverses_allocation_id: Some(PurchaseInvoiceAllocationId::new("original")),
            ..data()
        };
        let mismatch = PurchaseInvoiceAllocationData {
            allocated_tax_amount: erp_core::money::Amount::from_str("14.00").unwrap(),
            ..zero_seq.clone()
        };
        let error =
            PurchaseInvoiceAllocation::new(PurchaseInvoiceAllocationId::new("pi-2"), mismatch).unwrap_err();
        assert!(
            matches!(error, Error::LogicError(message) if message == "分配含税金额必须等于不含税金额加税额")
        );
        let error =
            PurchaseInvoiceAllocation::new(PurchaseInvoiceAllocationId::new("pi-3"), zero_seq).unwrap_err();
        assert!(matches!(error, Error::LogicError(message) if message == "分配序号必须从 1 开始"));
    }

    #[test]
    fn new_enforces_action_reference_consistency() {
        for (action, reference, error) in [
            (AllocationAction::Apply, false, None),
            (AllocationAction::Apply, true, Some("APPLY 分配不得引用原分配")),
            (AllocationAction::Reverse, false, Some("REVERSE 分配必须引用原 APPLY 分配")),
            (AllocationAction::Reverse, true, None),
        ] {
            let result = PurchaseInvoiceAllocation::new(
                PurchaseInvoiceAllocationId::new("allocation"),
                PurchaseInvoiceAllocationData {
                    allocation_action: action,
                    reverses_allocation_id: reference.then(|| PurchaseInvoiceAllocationId::new("original")),
                    ..data()
                },
            );
            match error {
                Some(message) => assert!(
                    matches!(result, Err(Error::LogicError(actual)) if actual == message),
                    "{action:?} / {reference}"
                ),
                None => {
                    let allocation = result.unwrap();
                    assert_eq!(allocation.allocation_action, action);
                    assert_eq!(allocation.reverses_allocation_id.is_some(), reference);
                },
            }
        }
    }

    #[test]
    fn formal_fact_rejects_update() {
        let mut allocation =
            PurchaseInvoiceAllocation::new(PurchaseInvoiceAllocationId::new("pi-1"), data()).unwrap();
        assert!(allocation.update(data(), "admin-2").is_err());
    }
}
