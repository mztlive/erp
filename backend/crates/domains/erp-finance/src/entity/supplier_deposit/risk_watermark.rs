//! 风险发生时刻独立于账户版本时间，防止后续核对覆盖原风险水位。

use erp_core::common::time::Instant;
use erp_core::{Error, Result};

use super::SupplierDepositAccount;

impl SupplierDepositAccount {
    /// 为既有受限账户保守继承旧更新时间，普通核对不得冲淡缺失的风险证据。
    /// # 参数
    /// 无；仅消费当前账户原有持久化状态。
    /// # 返回
    /// 旧受限记录缺风险水位时以原更新时间建立水位，其余记录保持不变。
    /// # 错误
    /// 原更新时间越界时拒绝，不猜测风险时刻。
    pub fn retain_legacy_reconciliation_risk(&mut self) -> Result<()> {
        if self.reconciliation_required && self.last_reconciliation_risk_at.is_none() {
            let seconds =
                i64::try_from(self.base.updated_at).map_err(|_| Error::from("历史账户资金风险时间越界"))?;
            self.last_reconciliation_risk_at = Some(Instant::try_from_unix_secs(seconds)?);
        }
        Ok(())
    }

    /// 登记新增资金或付款执行风险，持续受限期间发生的新风险也推进水位。
    /// # 参数
    /// `now` 为服务器确认本次风险的时刻，不能使用旧交易日期替代。
    /// # 返回
    /// 账户保持受限，最新风险时刻单调推进。
    /// # 错误
    /// 旧账户风险时刻无法读取时拒绝，不丢弃原水位。
    pub fn flag_reconciliation_risk(&mut self, now: Instant) -> Result<()> {
        self.retain_legacy_reconciliation_risk()?;
        self.last_reconciliation_risk_at =
            Some(self.last_reconciliation_risk_at.map_or(now, |previous| previous.max(now)));
        self.reconciliation_required = true;
        Ok(())
    }

    /// 核对平衡观察已经覆盖最后新增风险，普通账户更新不构成新的风险。
    /// # 参数
    /// `observed_at` 为已持久化平衡核对的真实观察时刻。
    /// # 返回
    /// 平衡依据不早于风险水位时成功。
    /// # 错误
    /// 旧平衡或尚未保留旧风险水位的历史受限记录时拒绝。
    pub fn ensure_reconciliation_risk_observed(&self, observed_at: Instant) -> Result<()> {
        let mut original = self.clone();
        original.retain_legacy_reconciliation_risk()?;
        if original.last_reconciliation_risk_at.is_some_and(|risk| observed_at < risk) {
            return Err(Error::from("平衡核对早于账户最后资金风险，请重新核实后办理结案"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::supplier_deposit::{DepositAccountIdentity, DepositCurrency};

    fn account() -> SupplierDepositAccount {
        SupplierDepositAccount::new(
            "account".into(),
            DepositAccountIdentity {
                payer_party_id: "payer".into(),
                supplier_id: "supplier".into(),
                external_account_ref: "external".into(),
                currency: DepositCurrency::CNY,
            },
        )
        .unwrap()
    }

    #[test]
    fn applied_funds_do_not_let_an_old_balanced_observation_clear_a_later_task_risk() {
        let mut account = account();
        account.flag_reconciliation_risk(Instant::from_unix_secs(30)).unwrap();
        assert!(account.ensure_reconciliation_risk_observed(Instant::from_unix_secs(20)).is_err());
        account.ensure_reconciliation_risk_observed(Instant::from_unix_secs(30)).unwrap();
        account.base.updated_at = 40;
        account.retain_legacy_reconciliation_risk().unwrap();
        assert_eq!(account.last_reconciliation_risk_at, Some(Instant::from_unix_secs(30)));
        account.flag_reconciliation_risk(Instant::from_unix_secs(45)).unwrap();
        assert!(account.ensure_reconciliation_risk_observed(Instant::from_unix_secs(40)).is_err());
    }

    #[test]
    fn legacy_risk_is_retained_before_a_new_balanced_record_updates_the_account() {
        let mut account = account();
        account.reconciliation_required = true;
        account.base.updated_at = 30;
        let mut json = serde_json::to_value(&account).unwrap();
        json.as_object_mut().unwrap().remove("last_reconciliation_risk_at");
        let mut legacy: SupplierDepositAccount = serde_json::from_value(json).unwrap();
        legacy.retain_legacy_reconciliation_risk().unwrap();
        legacy.base.updated_at = 50;
        assert!(legacy.ensure_reconciliation_risk_observed(Instant::from_unix_secs(20)).is_err());
        legacy.ensure_reconciliation_risk_observed(Instant::from_unix_secs(40)).unwrap();
        assert_eq!(legacy.last_reconciliation_risk_at, Some(Instant::from_unix_secs(30)));
    }
}
