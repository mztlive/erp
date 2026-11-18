//! 已核验差异及外部资金风险的不可变结案依据。

use erp_core::common::time::Instant;
use erp_core::money::Amount;
use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};

use super::{
    FundsEvidenceInbox, FundsEvidenceResult, SupplierDepositLedgerEntry, SupplierDepositReconciliation,
    SupplierDepositReservation, SupplierDepositUsage, add, required, subtract,
};

/// 释放原因必须与原订单终态及原资金事实共同核验。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DepositReleaseReason {
    ChargesSettled,
    FailedWithoutCharge,
    CancelledAndReturned,
}

/// 结案命令同时绑定原差异、后续平衡核对及核验附件。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DepositReconciliationResolution {
    pub operation_key: String,
    pub payload_fingerprint: String,
    pub balanced_reconciliation_id: String,
    pub evidence_asset_id: String,
    pub actor_id: String,
    pub resolved_at: Instant,
}

/// 原释放回执保留人工核验文件及同账户对账依据，不能只留下自由文本。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DepositReleaseVerification {
    pub reconciliation_id: String,
    pub evidence_asset_id: String,
    pub reason: DepositReleaseReason,
    pub verified_at: Instant,
}

impl SupplierDepositReconciliation {
    /// 核对原资金风险完整发生范围和最后核验时刻已被平衡依据覆盖。
    /// # 参数
    /// `since`、`through` 为原资金风险起止，`observed_at` 为最后原始来源或记账观察。
    /// # 返回
    /// 所选平衡依据覆盖完整风险且不早于最后观察时成功。
    /// # 错误
    /// 用旧平衡记录或缩小范围的记录解除较新风险时拒绝。
    pub fn ensure_risk_coverage(&self, since: Instant, through: Instant, observed_at: Instant) -> Result<()> {
        if since > through
            || self.covered_from > since
            || self.covered_until < through
            || self.observed_at < observed_at
        {
            return Err(Error::from("平衡核对早于原资金风险或未覆盖完整范围，请取得最新核对依据"));
        }
        Ok(())
    }

    /// 核对用于结案或释放的后续平衡记录覆盖原范围。
    /// # 参数
    /// `basis` 为同一事务加载的平衡核对，`now` 为服务器时刻。
    /// # 返回
    /// 同账号、范围不缩小且余额一致、无未决交易时成功。
    /// # 错误
    /// 跨账号、陈旧/未来证据、缩小范围或仍有差异时拒绝。
    pub fn ensure_resolution_basis(&self, basis: &Self, now: Instant) -> Result<()> {
        if self.account_id != basis.account_id
            || basis.covered_from > self.covered_from
            || basis.covered_until < self.covered_until
            || basis.observed_at < self.observed_at
            || basis.observed_at > now
            || basis.covered_until > basis.observed_at
            || basis.covered_from >= basis.covered_until
            || basis.remote_balance != basis.local_balance_at_cutoff
            || !basis.unresolved_transaction_ids.is_empty()
            || basis.evidence_reference.trim().is_empty()
        {
            return Err(Error::from("结案须引用同账户、覆盖原范围且无未决项的后续平衡核对"));
        }
        Ok(())
    }

    /// 保存不可覆盖的结案回执，同操作只恢复原结果。
    /// # 参数
    /// `resolution` 来自已核验身份、文件和资金状态的服务端命令。
    /// # 返回
    /// 首次结案返回真，同键同载荷重放返回假。
    /// # 错误
    /// 缺失依据或用新内容覆盖原结案时拒绝。
    pub fn resolve(&mut self, resolution: DepositReconciliationResolution) -> Result<bool> {
        for value in [
            &resolution.operation_key,
            &resolution.payload_fingerprint,
            &resolution.balanced_reconciliation_id,
            &resolution.evidence_asset_id,
            &resolution.actor_id,
        ] {
            required(value.clone(), "资金结案依据")?;
        }

        if let Some(previous) = &self.resolution {
            if previous.operation_key != resolution.operation_key
                || previous.payload_fingerprint != resolution.payload_fingerprint
                || previous.actor_id != resolution.actor_id
            {
                return Err(Error::from("原资金结案依据不得覆盖，请查询原办理结果"));
            }
            return Ok(false);
        }
        if self.resolved_by.is_some() || self.resolution_reference.is_some() {
            return Err(Error::from("历史结案缺少完整回执，请先核实原依据"));
        }

        self.resolved_by = Some(resolution.actor_id.clone());
        self.resolution_reference = Some(resolution.evidence_asset_id.clone());
        self.resolution = Some(resolution);
        Ok(true)
    }
}

impl FundsEvidenceInbox {
    /// 重核前核对原交易、原流水和关联占用的完整时间范围。
    /// # 参数
    /// 已保存平衡核对、由原稳定交易取得的真实流水及可选原占用。
    /// # 返回
    /// 原资金风险全部落在平衡依据范围且来源观察不更新时成功。
    /// # 错误
    /// 跨账号、原占用缺失、未解释来源、陈旧平衡或缩小范围时拒绝。
    pub fn ensure_review_coverage(
        &self,
        basis: &SupplierDepositReconciliation,
        ledger: &SupplierDepositLedgerEntry,
        reservation: Option<&SupplierDepositReservation>,
    ) -> Result<()> {
        let FundsEvidenceResult::Transaction { occurred_at, .. } = &self.input.result else {
            return Err(Error::from("未解释资金不能以平衡余额替代原交易核验"));
        };
        self.ensure_ledger_match(ledger, ledger.kind)?;
        if basis.account_id != self.account_id
            || ledger.reservation_id.as_deref() != reservation.map(|value| value.base.id.as_str())
            || reservation.is_some_and(|value| value.account_id != self.account_id)
        {
            return Err(Error::from("风险重核的平衡依据、流水或原占用归属不一致"));
        }

        let registered_at = timestamp(ledger.base.created_at)?;
        let mut since = (*occurred_at).min(ledger.evidence.occurred_at);
        if let Some(reservation) = reservation {
            since = since.min(timestamp(reservation.base.created_at)?);
        }
        let through = (*occurred_at).max(ledger.evidence.occurred_at).max(registered_at);
        let observed_at = self
            .last_observed_at
            .max(self.input.source_observed_at)
            .max(timestamp(self.base.updated_at)?)
            .max(timestamp(ledger.base.updated_at)?);
        basis.ensure_risk_coverage(since, through, observed_at)
    }
}

fn timestamp(value: u64) -> Result<Instant> {
    let seconds = i64::try_from(value).map_err(|_| Error::from("原资金风险时间越界"))?;
    Instant::try_from_unix_secs(seconds)
}

impl SupplierDepositReservation {
    /// 核对原执行全部净扣款已核销，取消/失败场景须没有剩余资金使用。
    /// # 参数
    /// `usages` 为原占用所有使用，`reason` 为已核验订单终态对应的释放原因。
    /// # 返回
    /// 返回服务器计算的未使用尾款，不接受客户端指定释放金额。
    /// # 错误
    /// 跨占用、批次不守恒、核销未完成、退款未完成或无尾款时拒绝。
    pub fn verified_release_amount(
        &self,
        usages: &[SupplierDepositUsage],
        reason: DepositReleaseReason,
    ) -> Result<Amount> {
        let mut consumed = Amount::zero();
        let mut net = Amount::zero();
        for usage in usages {
            if usage.account_id != self.account_id || usage.reservation_id != self.base.id {
                return Err(Error::from("风险核实的资金使用不属于原占用"));
            }
            let debit = subtract(usage.amount, usage.returned_amount)?;
            let settlement = subtract(usage.settled_amount, usage.reversed_settlement_amount)?;
            if debit < Amount::zero() || debit != settlement {
                return Err(Error::from("原执行实际资金尚未完成核销或返款冲回"));
            }
            net = add(net, debit)?;
            consumed = usage.batches.iter().try_fold(consumed, |sum, batch| add(sum, batch.amount))?;
        }

        if consumed != self.consumed_amount
            || (reason != DepositReleaseReason::ChargesSettled && net != Amount::zero())
            || (reason == DepositReleaseReason::FailedWithoutCharge && !usages.is_empty())
        {
            return Err(Error::from("原执行批次、扣款或返款尚不满足风险释放条件"));
        }

        let amount = self.remaining()?;
        if amount <= Amount::zero() {
            return Err(Error::from("原占用没有待释放余额"));
        }
        Ok(amount)
    }
}

#[cfg(test)]
mod tests {
    use entity_core::BaseModel;

    use super::*;

    fn record() -> SupplierDepositReconciliation {
        SupplierDepositReconciliation {
            base: BaseModel::new("review".into()),
            account_id: "account".into(),
            operation_key: "original".into(),
            covered_from: Instant::from_unix_secs(1),
            covered_until: Instant::from_unix_secs(10),
            observed_at: Instant::from_unix_secs(11),
            remote_balance: "100".parse().unwrap(),
            local_balance_at_cutoff: "100".parse().unwrap(),
            unresolved_transaction_ids: vec![],
            evidence_reference: "source".into(),
            resolved_by: None,
            resolution_reference: None,
            resolution: None,
        }
    }

    #[test]
    fn complete_basis_is_required_and_old_difference_is_preserved() {
        let mut original = record();
        original.remote_balance = "90".parse().unwrap();
        let valid = record();
        original.ensure_resolution_basis(&valid, Instant::from_unix_secs(12)).unwrap();
        for dimension in ["account", "range", "time", "difference", "pending"] {
            let mut invalid = valid.clone();
            match dimension {
                "account" => invalid.account_id = "other".into(),
                "range" => invalid.covered_from = Instant::from_unix_secs(2),
                "time" => invalid.observed_at = Instant::from_unix_secs(9),
                "difference" => invalid.remote_balance = "99".parse().unwrap(),
                _ => invalid.unresolved_transaction_ids.push("unresolved".into()),
            }
            assert!(original.ensure_resolution_basis(&invalid, Instant::from_unix_secs(12)).is_err());
        }
        assert_eq!(original.remote_balance, "90".parse().unwrap());
    }

    #[test]
    fn closure_replay_preserves_first_evidence_and_rejects_conflict() {
        let mut record = record();
        let resolution = DepositReconciliationResolution {
            operation_key: "resolve".into(),
            payload_fingerprint: "payload".into(),
            balanced_reconciliation_id: "balanced".into(),
            evidence_asset_id: "document".into(),
            actor_id: "reviewer".into(),
            resolved_at: Instant::from_unix_secs(12),
        };
        assert!(record.resolve(resolution.clone()).unwrap());

        let mut replay = resolution.clone();
        replay.resolved_at = Instant::from_unix_secs(13);
        assert!(!record.resolve(replay.clone()).unwrap());
        replay.payload_fingerprint = "different".into();
        assert!(record.resolve(replay).is_err());
        assert_eq!(record.resolution, Some(resolution));
    }
}
