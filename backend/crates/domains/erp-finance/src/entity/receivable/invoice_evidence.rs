//! 附件开票按批准额度登记；不虚构票面号码、日期及税额。
use entity_core::BaseModel;
use erp_core::common::stable::StableBase;
use erp_core::ids::{InvoiceId, PartyId, ReceivableAccountId, SalesInvoiceAllocationId};
use erp_core::money::Amount;
use erp_core::{Error, Result};

use super::{
    AccountingDirection, AllocationAction, Invoice, InvoiceDirection, InvoiceKind, InvoiceStatus,
    SalesInvoiceAllocation,
};

impl Invoice {
    /// 按批准额度创建附件登记记录。
    /// # 参数
    /// 身份、批准申请主体、剩余批准金额和登记人。
    /// # 返回
    /// 票面字段为空的销项蓝票草稿；规范化键仅用于内部唯一性。
    /// # 错误
    /// 金额非正时拒绝。
    pub fn from_evidence(id: InvoiceId, party_id: PartyId, amount: Amount, actor: &str) -> Result<Self> {
        if amount <= Amount::zero() {
            return Err(Error::from("本次申请没有剩余可开票金额"));
        }
        Ok(Self {
            normalized_no: format!("attachment:{id}"),
            base: BaseModel::new(id.to_string()),
            stable: StableBase::new(InvoiceStatus::Draft, actor),
            invoice_direction: InvoiceDirection::Sales,
            invoice_kind: InvoiceKind::Blue,
            accounting_direction: AccountingDirection::Increase,
            party_id,
            invoice_code: None,
            invoice_no: None,
            normalized_code: None,
            invoice_date: None,
            gross_amount: amount,
            net_amount: None,
            tax_amount: None,
            rounding_adjustment_amount: Amount::zero(),
            rounding_reason: None,
            original_invoice_id: None,
            sales_invoice_request_id: None,
        })
    }

    /// 读取真实票面税额拆分，禁止将附件登记的未知税额当成零。
    /// # 参数
    /// 当前发票。
    /// # 返回
    /// 已采集的不含税金额及税额。
    /// # 错误
    /// 附件登记缺少税额时拒绝需要票面税额的操作。
    pub fn tax_breakdown(&self) -> Result<(Amount, Amount)> {
        self.net_amount
            .zip(self.tax_amount)
            .ok_or_else(|| Error::from("附件登记未采集票面税额，不支持该操作"))
    }

    /// 创建只包含批准含税金额的应收分配。
    /// # 参数
    /// 新分配身份和申请绑定的应收。
    /// # 返回
    /// 单笔完整额度分配；不含税金额及税额留空。
    /// # 错误
    /// 非附件销项蓝票或金额非正时拒绝。
    pub fn evidence_allocation(
        &self,
        id: SalesInvoiceAllocationId,
        account: ReceivableAccountId,
    ) -> Result<SalesInvoiceAllocation> {
        if self.invoice_direction != InvoiceDirection::Sales
            || self.invoice_kind != InvoiceKind::Blue
            || self.invoice_no.is_some()
            || self.net_amount.is_some()
            || self.tax_amount.is_some()
            || self.gross_amount <= Amount::zero()
        {
            return Err(Error::from("只有附件销项登记可以按批准金额分配"));
        }
        Ok(SalesInvoiceAllocation {
            base: BaseModel::new(id.to_string()),
            invoice_id: self.base.id.clone().into(),
            receivable_account_id: account,
            allocation_seq: 1,
            allocation_action: AllocationAction::Apply,
            allocated_gross_amount: self.gross_amount,
            allocated_net_amount: None,
            allocated_tax_amount: None,
            reverses_allocation_id: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn evidence_preserves_amount_without_inventing_invoice_facts() {
        let invoice = Invoice::from_evidence(
            InvoiceId::new("inv"),
            PartyId::new("party"),
            "165".parse().unwrap(),
            "actor",
        )
        .unwrap();
        let json = serde_json::to_value(&invoice).unwrap();
        for field in ["invoice_no", "invoice_date", "net_amount", "tax_amount"] {
            assert!(json[field].is_null(), "{field} 不得伪造");
        }
        let restored: Invoice = serde_json::from_value(json).unwrap();
        assert_eq!(restored, invoice);
        assert!(invoice.invoice_no.is_none());
        assert!(invoice.invoice_date.is_none());
        assert!(invoice.tax_breakdown().is_err());
        let allocation = invoice
            .evidence_allocation(
                SalesInvoiceAllocationId::new("allocation"),
                ReceivableAccountId::new("account"),
            )
            .unwrap();
        assert_eq!(allocation.allocated_gross_amount, invoice.gross_amount);
        assert!(allocation.allocated_net_amount.is_none());
        assert!(allocation.allocated_tax_amount.is_none());
        assert!(
            Invoice::from_evidence(InvoiceId::new("inv"), PartyId::new("party"), Amount::zero(), "actor")
                .is_err()
        );
        assert!(
            Invoice::from_evidence(
                InvoiceId::new("inv"),
                PartyId::new("party"),
                "-1".parse().unwrap(),
                "actor"
            )
            .is_err()
        );
    }
}
