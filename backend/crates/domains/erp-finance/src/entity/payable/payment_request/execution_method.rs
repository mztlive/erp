//! 付款申请明确选择银行付款或供应商预存款，批准不等于实际使用。

use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};

use super::types::required;
use super::{PaymentRequestAccountSnapshot, PaymentRequestSubmission, PurchasePaymentRequest};
use crate::entity::supplier_deposit::{DepositAccountIdentity, DepositCurrency};

/// 旧申请缺少字段时只按原银行方式读取。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PaymentRequestExecutionMethod {
    #[default]
    BankTransfer,
    SupplierDeposit {
        account_id: String,
    },
}

/// 审批冻结的真实预存款账户归属；不冻结随流水变化的余额版本。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PaymentRequestDepositAccountSnapshot {
    pub account_id: String,
    pub identity: DepositAccountIdentity,
}

impl PaymentRequestExecutionMethod {
    pub(super) fn is_bank_transfer(&self) -> bool {
        matches!(self, Self::BankTransfer)
    }

    pub(super) fn normalize(&mut self, bank_account_id: &mut String) -> Result<()> {
        match self {
            Self::BankTransfer => *bank_account_id = required(bank_account_id.clone(), "收款账户", 128)?,
            Self::SupplierDeposit { account_id } => {
                *account_id = required(account_id.clone(), "预存款账户", 128)?;
                if !bank_account_id.trim().is_empty() {
                    return Err(Error::from("预存款方式不使用本次订单银行收款账户"));
                }
                bank_account_id.clear();
            },
        }
        Ok(())
    }

    /// 为界面或响应展示提供申请批准的付款方式的中文名称；机器筛选和存储不使用该名称。
    pub fn label(&self) -> &'static str {
        match self {
            Self::BankTransfer => "银行付款",
            Self::SupplierDeposit { .. } => "供应商预存款",
        }
    }
}

impl PurchasePaymentRequest {
    /// 核对原审批选择的预存款账户，不允许银行方式临时改走余额。
    /// 返回冻结账户归属。
    /// 方式、账户或原付款主体供应商不一致时拒绝。
    ///
    /// 输入约定：
    /// - `account_id`：本次实际使用的账户。
    pub fn require_deposit_account(&self, account_id: &str) -> Result<&PaymentRequestDepositAccountSnapshot> {
        let PaymentRequestExecutionMethod::SupplierDeposit { account_id: selected } =
            &self.data.execution_method
        else {
            return Err(Error::from("原付款申请未批准预存款执行方式"));
        };
        let snapshot =
            self.deposit_account.as_ref().ok_or_else(|| Error::from("原付款申请缺少预存款账户审核依据"))?;
        if selected != account_id
            || snapshot.account_id != account_id
            || self.payee_account.is_some()
            || snapshot.identity.payer_party_id != self.payer_party_id
            || snapshot.identity.supplier_id != self.supplier_id
            || snapshot.identity.currency != DepositCurrency::CNY
        {
            return Err(Error::from("预存款执行账户与批准主体或供应商不一致"));
        }
        Ok(snapshot)
    }

    /// 禁止银行付款或留抵命令消费已选择预存款的批准额度。
    /// 结果：原批准为银行方式时成功。
    /// 原批准为预存款方式时拒绝。
    pub fn require_bank_execution(&self) -> Result<()> {
        if self.data.execution_method != PaymentRequestExecutionMethod::BankTransfer
            || self.deposit_account.is_some()
        {
            return Err(Error::from("本付款申请使用供应商预存款，请按原余额方式执行"));
        }
        Ok(())
    }

    pub(super) fn validate_funding_submission(&self, submission: &PaymentRequestSubmission) -> Result<()> {
        match &self.data.execution_method {
            PaymentRequestExecutionMethod::BankTransfer => {
                let bank = submission
                    .payee_account
                    .as_ref()
                    .ok_or_else(|| Error::from("银行付款缺少收款账户审核依据"))?;
                if bank.account_id != self.data.payee_bank_account_id || submission.deposit_account.is_some()
                {
                    return Err(Error::from("付款账户或方式与表单不一致"));
                }
                validate_bank(bank)
            },
            PaymentRequestExecutionMethod::SupplierDeposit { account_id } => {
                let deposit = submission
                    .deposit_account
                    .as_ref()
                    .ok_or_else(|| Error::from("预存款付款缺少账户审核依据"))?;
                if deposit.account_id != *account_id
                    || submission.payee_account.is_some()
                    || deposit.identity.payer_party_id != submission.payer_party_id
                    || deposit.identity.supplier_id != submission.supplier_id
                    || deposit.identity.currency != DepositCurrency::CNY
                {
                    return Err(Error::from("预存款账户与本次批准主体、供应商或币种不一致"));
                }
                deposit.identity.clone().normalize()?;
                Ok(())
            },
        }
    }

    pub(super) fn has_funding_snapshot(&self) -> bool {
        match &self.data.execution_method {
            PaymentRequestExecutionMethod::BankTransfer => {
                self.payee_account.is_some() && self.deposit_account.is_none()
            },
            PaymentRequestExecutionMethod::SupplierDeposit { account_id } => {
                self.require_deposit_account(account_id).is_ok()
            },
        }
    }
}

fn validate_bank(bank: &PaymentRequestAccountSnapshot) -> Result<()> {
    required(bank.supplier_party_id.clone(), "供应商主体", 128)?;
    required(bank.account_name.clone(), "收款账户户名", 256)?;
    required(bank.bank_name.clone(), "开户行", 256)?;
    required(bank.masked_account_number.clone(), "脱敏收款账号", 128)?;
    if bank.version == 0 {
        return Err(Error::from("收款账户审核版本无效"));
    }
    Ok(())
}
