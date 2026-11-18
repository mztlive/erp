//! 真实充值批次；分配以到账时间和稳定身份排序。

use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::common::time::Instant;
use erp_core::money::Amount;
use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};

use super::{DepositBatchAllocation, add, positive, required, subtract};
use crate::{Error as FinanceError, Result as FinanceResult};

/// 充值申请、真实银行付款及供应商到账的共同依据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DepositRechargeEvidence {
    pub approved_request_id: String,
    pub approval_reference: String,
    pub supplier_payment_id: String,
    pub bank_reference: String,
    pub bank_receipt_asset_id: String,
    pub external_transaction_id: String,
    pub external_evidence_reference: String,
    pub received_at: Instant,
}

impl DepositRechargeEvidence {
    /// 核验充值证据标识完整；实际审批和付款事实由服务在同一事务重验。
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回规范化完整证据。
    /// # 错误
    /// 任一必要证据为空或超长时拒绝。
    pub fn normalize(mut self) -> Result<Self> {
        self.approved_request_id = required(self.approved_request_id, "充值申请")?;
        self.approval_reference = required(self.approval_reference, "充值批准依据")?;
        self.supplier_payment_id = required(self.supplier_payment_id, "真实充值付款")?;
        self.bank_reference = required(self.bank_reference, "充值银行流水")?;
        self.bank_receipt_asset_id = required(self.bank_receipt_asset_id, "充值银行回单")?;
        self.external_transaction_id = required(self.external_transaction_id, "供应商到账交易")?;
        self.external_evidence_reference = required(self.external_evidence_reference, "供应商到账证据")?;
        Ok(self)
    }
}

/// 可追溯到真实银行付款和供应商到账的资金批次。
#[derive(Debug, Clone, Serialize, Deserialize, Entity, PartialEq, Eq)]
pub struct SupplierDepositBatch {
    #[serde(flatten)]
    pub base: BaseModel,
    pub account_id: String,
    pub amount: Amount,
    pub reserved_amount: Amount,
    pub consumed_amount: Amount,
    pub returned_amount: Amount,
    pub reversed_amount: Amount,
    pub evidence: DepositRechargeEvidence,
}

impl SupplierDepositBatch {
    /// 创建已到账批次；未到账充值不得调用。
    /// # 参数
    /// * `id` / `account_id` - 批次及资金账户身份。
    /// * `amount` - 已核实到账金额。
    /// * `evidence` - 三方完整证据。
    /// # 返回
    /// 返回尚未占用和使用的批次。
    /// # 错误
    /// 金额或证据非法时拒绝。
    pub fn new(
        id: String,
        account_id: String,
        amount: Amount,
        evidence: DepositRechargeEvidence,
    ) -> Result<Self> {
        positive(amount)?;
        Ok(Self {
            base: BaseModel::new(required(id, "充值批次")?),
            account_id: required(account_id, "预存款账户")?,
            amount,
            reserved_amount: Amount::zero(),
            consumed_amount: Amount::zero(),
            returned_amount: Amount::zero(),
            reversed_amount: Amount::zero(),
            evidence: evidence.normalize()?,
        })
    }

    /// 返回本来源批次尚可分配额度。
    /// # 参数
    /// 无。
    /// # 返回
    /// 充值减冲正、净使用和占用。
    /// # 错误
    /// 运算溢出或投影异常时拒绝。
    pub fn available(&self) -> Result<Amount> {
        let result = subtract(
            add(self.amount, self.returned_amount)?,
            add(add(self.reserved_amount, self.consumed_amount)?, self.reversed_amount)?,
        )?;
        if result < Amount::zero() {
            return Err(Error::from("充值批次余额异常"));
        }
        Ok(result)
    }

    /// 固化本批次专属占用。
    /// # 参数
    /// * `amount` - 本批次分配金额。
    /// # 返回
    /// 本批次占用增加。
    /// # 错误
    /// 非正或批次不足时拒绝。
    pub fn reserve(&mut self, amount: Amount) -> Result<()> {
        positive(amount)?;
        if amount > self.available()? {
            return Err(Error::from("充值批次可用额度不足"));
        }
        self.reserved_amount = add(self.reserved_amount, amount)?;
        Ok(())
    }

    fn reserve_up_to(&mut self, remaining: Amount) -> Result<Amount> {
        let allocated = self.available()?.min(remaining);
        if allocated == Amount::zero() {
            return Ok(allocated);
        }
        positive(allocated)?;
        // 金额来自本次可用额度与剩余预算的较小值，无需再次计算同一批次余额。
        self.reserved_amount = add(self.reserved_amount, allocated)?;
        Ok(allocated)
    }

    /// 消耗原占用并记录实际资金使用。
    /// # 参数
    /// * `amount` - 可归属本批次的扣款。
    /// # 返回
    /// 使用增加且占用同额减少。
    /// # 错误
    /// 非正或超过原占用时拒绝。
    pub fn consume(&mut self, amount: Amount) -> Result<()> {
        positive(amount)?;
        if amount > self.reserved_amount {
            return Err(Error::from("使用金额超过批次原占用"));
        }
        let consumed = add(self.consumed_amount, amount)?;
        self.reserved_amount = subtract(self.reserved_amount, amount)?;
        self.consumed_amount = consumed;
        Ok(())
    }

    /// 释放无风险占用。
    /// # 参数
    /// * `amount` - 原占用的未使用金额。
    /// # 返回
    /// 本批次可用额度恢复。
    /// # 错误
    /// 非正或超占用时拒绝。
    pub fn release(&mut self, amount: Amount) -> Result<()> {
        positive(amount)?;
        if amount > self.reserved_amount {
            return Err(Error::from("释放金额超过批次占用"));
        }
        self.reserved_amount = subtract(self.reserved_amount, amount)?;
        Ok(())
    }

    /// 将实际到账返款归还原资金批次。
    /// # 参数
    /// * `amount` - 已核实且引用原使用的返款。
    /// # 返回
    /// 可用来源恢复，累计使用保留。
    /// # 错误
    /// 非正或超过原净使用时拒绝。
    pub fn record_return(&mut self, amount: Amount) -> Result<()> {
        positive(amount)?;
        let returned = add(self.returned_amount, amount)?;
        if returned > self.consumed_amount {
            return Err(Error::from("返款超过原批次使用"));
        }
        self.returned_amount = returned;
        Ok(())
    }
}

impl DepositBatchAllocation {
    /// 按已确定的到账批次顺序分配原执行组预算。
    /// # 参数
    /// * `batches` - 仓储按到账时间与稳定身份排序的真实批次。
    /// * `amount` - 原执行组待占用预算。
    /// # 返回
    /// 返回保持来源顺序的非零分配，并增加对应批次占用。
    /// # 错误
    /// 损坏历史余额、运算失败或合计不足时保留原错误；调用方须回滚整个事务。
    pub(crate) fn reserve_from(
        batches: &mut [SupplierDepositBatch],
        amount: Amount,
    ) -> FinanceResult<Vec<Self>> {
        let mut remaining = amount;
        let mut allocations = Vec::new();
        for batch in batches {
            if remaining == Amount::zero() {
                break;
            }
            let allocated = batch.reserve_up_to(remaining)?;
            if allocated == Amount::zero() {
                continue;
            }
            remaining = remaining.checked_sub(allocated);
            allocations.push(Self { batch_id: batch.base.id.clone(), amount: allocated });
        }

        if remaining > Amount::zero() {
            return Err(FinanceError::BusinessLogicError("到账批次可分配额度不足".into()));
        }
        Ok(allocations)
    }
}
