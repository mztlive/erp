//! 应付视图装配与跨用例共享的金额、收款账户辅助。

use std::str::FromStr;

use database::{Executor, PartyExt, SupplierExt};
use entities::common::time::BusinessDate;
use entities::ids::{PartyId, SupplierAccountId};
use entities::money::Amount;
use entities::party::PartyBankAccount;
use mongodb::Database;

use super::dto::PaymentRecipientView;
use crate::errors::{Error, Result};

/// 解析供应商在当前业务日唯一生效的默认收款账户。
///
/// # 错误
/// 供应商不存在、未配置默认账户或出现多个默认账户时失败关闭。
pub(super) async fn resolve_current_payment_recipient(
    db: &Database,
    supplier_id: &SupplierAccountId,
    executor: &mut dyn Executor,
) -> Result<PartyBankAccount> {
    let supplier = db
        .supplier_accounts()
        .find_by_id(supplier_id.as_ref(), executor)
        .await?
        .ok_or_else(|| Error::NotFound("供应商不存在".to_string()))?;
    resolve_optional_party_payment_recipient(db, &supplier.party_id, executor)
        .await?
        .ok_or_else(|| Error::BusinessLogicError("供应商未配置当前默认收款账户，无法付款".to_string()))
}

/// 为只读任务/详情投影解析供应商当前默认收款账户。
///
/// 供应商已软删除或缺失时返回空，保证历史应付仍可读取；付款执行必须调用
/// [`resolve_current_payment_recipient`] 并严格校验活跃供应商。
///
/// # 错误
/// 出现多个默认账户或仓储读取失败时返回错误。
pub(super) async fn resolve_optional_payment_recipient_for_read(
    db: &Database,
    supplier_id: &SupplierAccountId,
    executor: &mut dyn Executor,
) -> Result<Option<PartyBankAccount>> {
    let Some(supplier) = db
        .supplier_accounts()
        .find_by_id(supplier_id.as_ref(), executor)
        .await?
    else {
        return Ok(None);
    };
    resolve_optional_party_payment_recipient(db, &supplier.party_id, executor).await
}

/// 解析主体当前唯一默认收款账户；未配置时返回空。
///
/// 当前有效账户集合由 Repository 按业务日期过滤（有效期窗口与启停状态），
/// 唯一默认值解析与主数据损坏判定由 [`PartyBankAccount::resolve_current_default`]
/// 完成，本函数只负责日期读取、错误到 API 的映射与所有权转换。
///
/// # 错误
/// 出现多个默认账户或仓储读取失败时返回错误。
async fn resolve_optional_party_payment_recipient(
    db: &Database,
    party_id: &PartyId,
    executor: &mut dyn Executor,
) -> Result<Option<PartyBankAccount>> {
    let accounts = db
        .party_bank_accounts()
        .list_current_on(party_id, BusinessDate::today(), executor)
        .await?;
    PartyBankAccount::resolve_current_default(&accounts)
        .map(|account| account.cloned())
        .map_err(|_| Error::BusinessLogicError("供应商存在多个当前默认收款账户，请先修复主数据".to_string()))
}

/// 构造不含敏感明文的收款账户摘要。
pub(super) fn payment_recipient_view(account: &PartyBankAccount) -> PaymentRecipientView {
    PaymentRecipientView {
        bank_account_id: account.base.id.clone(),
        version: account.base.version,
        account_name: account.account_name.clone(),
        bank_name: account.bank_name.clone(),
        bank_branch_name: account.bank_branch_name.clone(),
        account_number_masked: masked_bank_account_number(&account.account_number_last4),
    }
}

/// 使用账号末四位构造不可恢复的工作台掩码。
pub(super) fn masked_bank_account_number(last4: &str) -> String {
    let last4 = last4.trim();
    if last4.is_empty() {
        "********".to_string()
    } else {
        format!("********{last4}")
    }
}

/// 返回固定零金额。
///
/// # 返回
/// 返回金额 `0.00`。
pub(super) fn zero_amount() -> Amount {
    Amount::from_str("0.00").expect("固定零金额必须可解析")
}
