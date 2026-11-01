//! 采购替代前的资金资格；供应商取消状态本身不解除未知资金风险。

use erp_core::money::Amount;
use erp_core::{Error, Result};

use super::{
    DepositExposure, SupplierDepositAccount, SupplierDepositReservation, SupplierDepositUsage, add, subtract,
};

impl SupplierDepositReservation {
    /// 核对原执行资金已经消解，返回允许释放的未使用尾款。
    /// # 参数
    /// * `has_external_write` - 原执行已登记写动作或具有外部订单证据。
    /// * `account` / `usages` - 同一事务读取的原账户及全部实际使用。
    /// # 返回
    /// 仅从未发送或具有独立风险消解证据且净使用/核销均为零时返回尾款。
    /// # 错误
    /// 未知发送、未返款、未反向核销、跨账户或对账风险时拒绝。
    pub fn replacement_release_amount(
        &self,
        has_external_write: bool,
        account: &SupplierDepositAccount,
        usages: &[SupplierDepositUsage],
    ) -> Result<Amount> {
        if account.base.id != self.account_id || account.reconciliation_required {
            return Err(Error::from("原供应商资金仍待核对，不能替代采购执行"));
        }
        let mut net_debit = Amount::zero();
        let mut net_settlement = Amount::zero();
        let mut covered_consumption = Amount::zero();
        for usage in usages {
            if usage.account_id != self.account_id || usage.reservation_id != self.base.id {
                return Err(Error::from("原执行资金使用归属不一致"));
            }
            let debit = subtract(usage.amount, usage.returned_amount)?;
            let settlement = subtract(usage.settled_amount, usage.reversed_settlement_amount)?;
            if debit < Amount::zero() || settlement < Amount::zero() {
                return Err(Error::from("原执行资金使用或核销不守恒"));
            }
            net_debit = add(net_debit, debit)?;
            net_settlement = add(net_settlement, settlement)?;
            covered_consumption = usage
                .batches
                .iter()
                .try_fold(covered_consumption, |total, batch| add(total, batch.amount))?;
        }
        if covered_consumption != self.consumed_amount {
            return Err(Error::from("原执行资金批次使用与占用消耗不一致，不能替代执行"));
        }
        if net_debit != Amount::zero() || net_settlement != Amount::zero() {
            return Err(Error::from("原供应商扣款或核销尚未全部返还冲回，不能替代采购执行"));
        }
        let never_sent = !has_external_write
            && self.exposure == DepositExposure::NotSent
            && self.consumed_amount == Amount::zero()
            && usages.is_empty();
        let cleared = self.exposure == DepositExposure::Cleared
            && self
                .clearance_evidence_reference
                .as_deref()
                .is_some_and(|reference| !reference.trim().is_empty());
        if !never_sent && !cleared {
            return Err(Error::from("原供应商资金仍可能扣款或冻结，请先核实原结果，不能按取消状态释放"));
        }
        self.remaining()
    }
}
