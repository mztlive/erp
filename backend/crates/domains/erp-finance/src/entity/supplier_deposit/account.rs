//! 账户身份和可用余额投影。

use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::common::time::Instant;
use erp_core::money::Amount;
use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};

use super::{add, positive, required, subtract};

/// 首期明确支持人民币，不默认推导其他币种。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum DepositCurrency {
    CNY,
}

/// 账户归属；外部引用指向服务端资金账号身份，不保存凭据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DepositAccountIdentity {
    pub payer_party_id: String,
    pub supplier_id: String,
    pub external_account_ref: String,
    pub currency: DepositCurrency,
}

impl DepositAccountIdentity {
    /// 验证并规范化资金归属。
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回已核验的完整归属。
    /// # 错误
    /// 主体、供应商或账号引用为空或超长时拒绝。
    pub fn normalize(mut self) -> Result<Self> {
        self.payer_party_id = required(self.payer_party_id, "我方付款主体")?;
        self.supplier_id = required(self.supplier_id, "供应商")?;
        self.external_account_ref = required(self.external_account_ref, "外部资金账号")?;
        Ok(self)
    }
}

/// 已核实流水形成的账户投影；远端余额不得覆盖本地余额。
#[derive(Debug, Clone, Serialize, Deserialize, Entity, PartialEq, Eq)]
pub struct SupplierDepositAccount {
    #[serde(flatten)]
    pub base: BaseModel,
    pub identity: DepositAccountIdentity,
    pub book_balance: Amount,
    pub reserved_amount: Amount,
    /// 与占用不重叠的其他不可用额度。
    pub unavailable_amount: Amount,
    /// 无法解释的资金差异限制新增使用，但不限制已发生资金事实入账。
    pub reconciliation_required: bool,
    /// 最后一次新增资金风险的时刻；普通核对或版本写屏障不得覆盖。
    #[serde(default)]
    pub last_reconciliation_risk_at: Option<Instant>,
    pub enabled: bool,
}

impl SupplierDepositAccount {
    /// 创建零余额账户，不导入远端余额。
    /// # 参数
    /// * `id` - 服务端账户身份。
    /// * `identity` - 付款主体、供应商、账号与币种。
    /// # 返回
    /// 返回等待真实到账的账户。
    /// # 错误
    /// 身份不合法时拒绝。
    pub fn new(id: String, identity: DepositAccountIdentity) -> Result<Self> {
        Ok(Self {
            base: BaseModel::new(required(id, "预存款账户")?),
            identity: identity.normalize()?,
            book_balance: Amount::zero(),
            reserved_amount: Amount::zero(),
            unavailable_amount: Amount::zero(),
            reconciliation_required: false,
            last_reconciliation_risk_at: None,
            enabled: true,
        })
    }

    /// 计算不重复扣减占用的可用额度。
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回账面减未消耗占用和其他不可用金额；负值保留供对账。
    /// # 错误
    /// 定点运算溢出时拒绝。
    pub fn available(&self) -> Result<Amount> {
        subtract(subtract(self.book_balance, self.reserved_amount)?, self.unavailable_amount)
    }

    /// 以本账户为唯一范围占用真实到账余额。
    /// # 参数
    /// * `amount` - 已批准全额预算。
    /// # 返回
    /// 成功时增加占用，账面和正式已付不变。
    /// # 错误
    /// 停用、对账异常、非正或可用额度不足时拒绝且不修改。
    pub fn reserve(&mut self, amount: Amount) -> Result<()> {
        positive(amount)?;
        if !self.enabled || self.reconciliation_required || amount > self.available()? {
            return Err(Error::from("预存款账户停用、待对账或可用余额不足"));
        }
        self.reserved_amount = add(self.reserved_amount, amount)?;
        Ok(())
    }

    /// 接收已核实到账，允许停用账户继续记录真实资金。
    /// # 参数
    /// * `amount` - 已核实充值或返款。
    /// # 返回
    /// 账面余额增加；调用方须同步追加唯一流水。
    /// # 错误
    /// 非正或溢出时拒绝。
    pub fn credit(&mut self, amount: Amount) -> Result<()> {
        positive(amount)?;
        self.book_balance = add(self.book_balance, amount)?;
        Ok(())
    }

    /// 记录已发生扣款并消耗同额原占用，超占扣款保留真实资金并限制新增使用。
    /// # 参数
    /// * `amount` - 真实扣款。
    /// * `covered` - 本次能够归属的原占用消耗。
    /// # 返回
    /// 账面减少，已覆盖部分从占用移除；不足部分进入对账。
    /// # 错误
    /// 金额非法、覆盖大于扣款或账户占用、运算溢出时拒绝。
    pub fn debit(&mut self, amount: Amount, covered: Amount) -> Result<()> {
        positive(amount)?;
        if covered < Amount::zero() || covered > amount || covered > self.reserved_amount {
            return Err(Error::from("扣款归属占用金额无效"));
        }

        let balance = subtract(self.book_balance, amount)?;
        let reserved = subtract(self.reserved_amount, covered)?;
        if covered < amount || balance < Amount::zero() {
            self.flag_reconciliation_risk(Instant::now())?;
        }

        self.book_balance = balance;
        self.reserved_amount = reserved;
        Ok(())
    }

    /// 释放已证明无外部扣款风险的未使用占用。
    /// # 参数
    /// * `amount` - 可证明释放金额。
    /// # 返回
    /// 占用减少，账面不变。
    /// # 错误
    /// 非正或超出剩余占用时拒绝。
    pub fn release(&mut self, amount: Amount) -> Result<()> {
        positive(amount)?;
        if amount > self.reserved_amount {
            return Err(Error::from("释放金额超过原预存款占用"));
        }
        self.reserved_amount = subtract(self.reserved_amount, amount)?;
        Ok(())
    }
}
