//! 供应商留抵余额及使用记录；不形成新的银行付款或现金退款。
use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::money::Amount;
use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Entity)]
pub struct SupplierCredit {
    #[serde(flatten)]
    pub base: BaseModel,
    pub source_refund_id: String,
    pub source_purchase_order_id: String,
    pub original_payment_id: String,
    pub supplier_id: String,
    pub payer_party_id: String,
    pub amount: Amount,
    pub used_amount: Amount,
}
impl SupplierCredit {
    /// 计算尚未抵扣余额。
    /// # 参数
    /// 无。
    /// # 返回
    /// 精确可用金额。
    /// # 错误
    /// 无。
    pub fn available(&self) -> Amount {
        self.amount.checked_sub(self.used_amount)
    }
    /// 核对主体、供应商并消费留抵；实体 CAS 与财务核销须在同一事务。
    /// # 参数
    /// * `supplier` / `payer` - 本次已批准付款申请主体。
    /// * `amount` - 本次使用金额。
    /// # 返回
    /// 成功时余额减少。
    /// # 错误
    /// 跨供应商、跨付款主体、非正数或超额时拒绝且不改变余额。
    pub fn consume(&mut self, supplier: &str, payer: &str, amount: Amount) -> Result<()> {
        if supplier != self.supplier_id || payer != self.payer_party_id {
            return Err(Error::from("留抵只能用于同一供应商和同一付款主体"));
        }
        if amount <= Amount::zero() || amount > self.available() {
            return Err(Error::from("抵扣金额须大于零且不得超过可用留抵余额"));
        }
        self.used_amount = self.used_amount.checked_add(amount);
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Entity)]
pub struct SupplierCreditUse {
    #[serde(flatten)]
    pub base: BaseModel,
    pub credit_id: String,
    pub payment_request_id: String,
    pub work_item_id: String,
    pub task_version: String,
    pub request_version: u64,
    pub payable_account_id: String,
    pub payable_entry_id: String,
    pub amount: Amount,
    pub actor_id: String,
    pub idempotency_key: String,
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;
    fn amount(value: &str) -> Amount {
        Amount::from_str(value).unwrap()
    }
    fn credit() -> SupplierCredit {
        SupplierCredit {
            base: BaseModel::new("credit".into()),
            source_refund_id: "refund".into(),
            source_purchase_order_id: "po".into(),
            original_payment_id: "pay".into(),
            supplier_id: "supplier".into(),
            payer_party_id: "payer".into(),
            amount: amount("175"),
            used_amount: Amount::zero(),
        }
    }
    #[test]
    fn partial_use_preserves_balance_and_rejects_overdraft() {
        let mut value = credit();
        value.consume("supplier", "payer", amount("100")).unwrap();
        assert_eq!(value.available(), amount("75"));
        assert!(value.consume("supplier", "payer", amount("76")).is_err());
        assert_eq!(value.available(), amount("75"));
        value.consume("supplier", "payer", amount("75")).unwrap();
        assert_eq!(value.available(), Amount::zero());
    }
    #[test]
    fn parties_and_positive_amount_are_mandatory() {
        let mut value = credit();
        assert!(value.consume("other", "payer", amount("1")).is_err());
        assert!(value.consume("supplier", "other", amount("1")).is_err());
        assert!(value.consume("supplier", "payer", Amount::zero()).is_err());
        assert_eq!(value.available(), amount("175"));
    }
}
