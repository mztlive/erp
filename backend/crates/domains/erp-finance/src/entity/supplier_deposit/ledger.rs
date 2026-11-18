//! 不可变外部资金证据与使用、返款的追溯记录。

use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::common::time::Instant;
use erp_core::money::Amount;
use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};

use super::{DepositBatchAllocation, DepositReconciliationResolution, add, positive};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DepositLedgerKind {
    Recharge,
    Debit,
    Return,
    RechargeReversal,
}

/// 原始资金交易必须带稳定身份、方向、金额和来源依据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DepositTransactionEvidence {
    pub external_transaction_id: String,
    pub external_order_id: String,
    pub evidence_reference: String,
    pub occurred_at: Instant,
}

#[derive(Debug, Clone, Serialize, Deserialize, Entity, PartialEq, Eq)]
pub struct SupplierDepositLedgerEntry {
    #[serde(flatten)]
    pub base: BaseModel,
    pub account_id: String,
    pub operation_key: String,
    pub payload_fingerprint: String,
    pub kind: DepositLedgerKind,
    pub amount: Amount,
    pub reservation_id: Option<String>,
    pub original_usage_id: Option<String>,
    pub batch_allocations: Vec<DepositBatchAllocation>,
    pub evidence: DepositTransactionEvidence,
    pub actor_id: String,
}

/// 实际扣款和正式核销分别保存，超批扣款保留未核销部分。
#[derive(Debug, Clone, Serialize, Deserialize, Entity, PartialEq, Eq)]
pub struct SupplierDepositUsage {
    #[serde(flatten)]
    pub base: BaseModel,
    pub account_id: String,
    pub reservation_id: String,
    pub ledger_entry_id: String,
    pub amount: Amount,
    pub settled_amount: Amount,
    pub returned_amount: Amount,
    pub reversed_settlement_amount: Amount,
    pub batches: Vec<DepositBatchAllocation>,
    pub payment_allocation_ids: Vec<String>,
}

impl SupplierDepositUsage {
    /// 记录正式核销，不能超过真实使用金额。
    /// # 参数
    /// * `amount` - 本次已批准可核销金额。
    /// * `allocation_ids` - 同一事务产生的正式核销记录。
    /// # 返回
    /// 增加已核销金额并保留来源。
    /// # 错误
    /// 非正、缺少核销记录或超使用金额时拒绝。
    pub fn record_settlement(&mut self, amount: Amount, allocation_ids: Vec<String>) -> Result<()> {
        positive(amount)?;
        let settled = add(self.settled_amount, amount)?;
        if settled > self.amount
            || allocation_ids.is_empty()
            || allocation_ids.iter().any(|id| id.trim().is_empty())
        {
            return Err(Error::from("预存款正式核销超过真实扣款或缺少分配记录"));
        }

        self.settled_amount = settled;
        self.payment_allocation_ids.extend(allocation_ids);
        Ok(())
    }

    /// 核实实际账户返款并引用原核销反向记录。
    /// # 参数
    /// * `amount` - 实际到账返款。
    /// * `reversed_settlement` - 已同步冲回的正式核销部分。
    /// # 返回
    /// 累计真实返款与核销冲回，二者分别受原事实限制。
    /// # 错误
    /// 重复超返、超核销或金额非法时拒绝。
    pub fn record_return(&mut self, amount: Amount, reversed_settlement: Amount) -> Result<()> {
        positive(amount)?;
        let returned = add(self.returned_amount, amount)?;
        let reversed = add(self.reversed_settlement_amount, reversed_settlement)?;
        if returned > self.amount
            || reversed_settlement < Amount::zero()
            || reversed_settlement > amount
            || reversed > self.settled_amount
        {
            return Err(Error::from("预存款返款或冲回超过原使用及核销"));
        }

        self.returned_amount = returned;
        self.reversed_settlement_amount = reversed;
        Ok(())
    }
}

/// 同一时间范围的完整对账证据；只记录差异，不覆盖账户余额。
#[derive(Debug, Clone, Serialize, Deserialize, Entity, PartialEq, Eq)]
pub struct SupplierDepositReconciliation {
    #[serde(flatten)]
    pub base: BaseModel,
    pub account_id: String,
    pub operation_key: String,
    pub covered_from: Instant,
    pub covered_until: Instant,
    pub observed_at: Instant,
    pub remote_balance: Amount,
    pub local_balance_at_cutoff: Amount,
    pub unresolved_transaction_ids: Vec<String>,
    pub evidence_reference: String,
    pub resolved_by: Option<String>,
    pub resolution_reference: Option<String>,
    #[serde(default)]
    pub resolution: Option<DepositReconciliationResolution>,
}
