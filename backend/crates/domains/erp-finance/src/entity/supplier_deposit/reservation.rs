//! 执行组的资金保障、原占用和外部写动作风险。

use std::collections::BTreeMap;

use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::money::Amount;
use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};

use super::{DepositReleaseVerification, add, positive, subtract};

/// 固化的来源批次分配，重放不改变资金来源。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DepositBatchAllocation {
    pub batch_id: String,
    pub amount: Amount,
}

/// 原释放命令的不可变回执，重复请求不再次释放。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DepositReleaseReceipt {
    pub amount: Amount,
    pub payload_fingerprint: String,
    pub clearance_evidence_reference: Option<String>,
    pub actor_id: String,
    #[serde(default)]
    pub verification: Option<DepositReleaseVerification>,
}

/// 发送风险与资金核销各自保存；超时不提供自动释放资格。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DepositExposure {
    NotSent,
    PossiblySent,
    Cleared,
}

/// 付款审批与采购版本、执行组和财务授权的共同绑定。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DepositReservationBasis {
    pub payment_request_id: String,
    pub payment_request_version: u32,
    pub payable_account_id: String,
    pub purchase_order_id: String,
    pub purchase_revision_id: String,
    pub execution_group_id: String,
    pub approved_goods_amount: Amount,
    pub approved_freight: Amount,
    pub authorization_id: String,
    pub authorization_version: u64,
}

impl DepositReservationBasis {
    /// 返回本执行组全部批准预算，不能只占用预付比例。
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回商品和批准运费合计。
    /// # 错误
    /// 非正总额、负运费/商品或溢出时拒绝。
    pub fn total(&self) -> Result<Amount> {
        if self.approved_goods_amount < Amount::zero() || self.approved_freight < Amount::zero() {
            return Err(Error::from("批准商品和运费金额不得为负数"));
        }
        let total = add(self.approved_goods_amount, self.approved_freight)?;
        positive(total)?;
        Ok(total)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Entity, PartialEq, Eq)]
pub struct SupplierDepositReservation {
    #[serde(flatten)]
    pub base: BaseModel,
    pub account_id: String,
    pub operation_key: String,
    pub payload_fingerprint: String,
    pub basis: DepositReservationBasis,
    pub amount: Amount,
    pub consumed_amount: Amount,
    pub released_amount: Amount,
    pub batches: Vec<DepositBatchAllocation>,
    pub exposure: DepositExposure,
    pub clearance_evidence_reference: Option<String>,
    pub actor_id: String,
    #[serde(default)]
    pub release_receipts: BTreeMap<String, DepositReleaseReceipt>,
}

impl SupplierDepositReservation {
    /// 计算尚未消耗或释放的原占用。
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回剩余资金保障。
    /// # 错误
    /// 运算溢出或持久化金额不守恒时拒绝。
    pub fn remaining(&self) -> Result<Amount> {
        let value = subtract(subtract(self.amount, self.consumed_amount)?, self.released_amount)?;
        if value < Amount::zero() {
            return Err(Error::from("预存款占用金额不守恒"));
        }
        Ok(value)
    }

    /// 标记外部写请求可能已经送达，必须先持久化再发送。
    /// # 参数
    /// 无。
    /// # 返回
    /// 标记后超时、授权失效或任务接手均不能释放。
    /// # 错误
    /// 已释放或已消解执行不得重新发送。
    pub fn mark_sent(&mut self) -> Result<()> {
        if self.exposure != DepositExposure::NotSent || self.released_amount > Amount::zero() {
            return Err(Error::from("预存款执行已有发送或释放事实，请查询原结果"));
        }
        self.exposure = DepositExposure::PossiblySent;
        Ok(())
    }

    /// 以明确的无后续扣款或冻结风险证据消解发送状态。
    /// # 参数
    /// * `evidence` - 无外部风险的核验记录，不得仅为一次查不到订单。
    /// # 返回
    /// 保留原证据并允许受控释放未使用尾款。
    /// # 错误
    /// 缺少证据时拒绝。
    pub fn clear_exposure(&mut self, evidence: String) -> Result<()> {
        let evidence = super::required(evidence, "无扣款及冻结风险核验依据")?;
        if self.clearance_evidence_reference.as_ref().is_some_and(|previous| previous != &evidence) {
            return Err(Error::from("原资金风险消解依据不得覆盖"));
        }
        self.clearance_evidence_reference = Some(evidence);
        self.exposure = DepositExposure::Cleared;
        Ok(())
    }

    /// 记录可归属原占用的真实资金扣款。
    /// # 参数
    /// * `amount` - 本次归属扣款。
    /// # 返回
    /// 原占用减少；调用方同步更新账户、批次和应付依据。
    /// # 错误
    /// 超过剩余占用或非正时拒绝。
    pub fn consume(&mut self, amount: Amount) -> Result<()> {
        positive(amount)?;
        if amount > self.remaining()? {
            return Err(Error::from("扣款超过执行组剩余占用"));
        }
        self.consumed_amount = add(self.consumed_amount, amount)?;
        Ok(())
    }

    /// 释放未使用部分；已发送未知不能释放。
    /// # 参数
    /// * `amount` - 本次释放金额。
    /// # 返回
    /// 占用减少且保留历史。
    /// # 错误
    /// 可能已送达、超额或非正时拒绝。
    pub fn release(&mut self, amount: Amount) -> Result<()> {
        positive(amount)?;
        if self.exposure == DepositExposure::PossiblySent || amount > self.remaining()? {
            return Err(Error::from("外部结果待核实或释放金额超过原占用"));
        }
        self.released_amount = add(self.released_amount, amount)?;
        Ok(())
    }
}
