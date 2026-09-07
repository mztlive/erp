//! Audit-backed replay of finance card-funds command receipts.

use erp_audit::AuditExt;
use erp_finance::entity::receivable::{
    CardFundsCommandReceiptError, CardFundsRegistrationKind, CardFundsRegistrationReceipt,
    CardFundsRegistrationReceiptError,
};
use mongodb::Database;
use persistence_core::Executor;
use services::{Error, Result};

pub(super) use erp_finance::service::receivable::card_funds_receipt::complete_review_result;

/// Map the finance command receipt error at the composition boundary.
///
/// Conflict and internal classifications preserve the original HTTP contract.
pub fn map_command_receipt_error(error: CardFundsCommandReceiptError) -> Error {
    erp_finance::service::receivable::card_funds_receipt::map_command_receipt_error(error).into()
}

/// Map the finance registration receipt error at the composition boundary.
///
/// Conflict and internal classifications preserve the original HTTP contract.
pub fn map_registration_receipt_error(error: CardFundsRegistrationReceiptError) -> Error {
    erp_finance::service::receivable::card_funds_receipt::map_registration_receipt_error(error).into()
}

/// 在事务内读取并严格验证 W13 登记幂等收据。
///
/// # 参数
/// * `db` - 数据库
/// * `audit_id` - 稳定登记审计主键
/// * `expected_action` - 回款或发票登记动作
/// * `expected_fingerprint` - 当前请求指纹
/// * `executor` - 调用方事务执行器
///
/// # 返回
/// 无审计时 `None`；命中时返回 `(account_id, fact_id)`。
///
/// # 错误
/// 审计身份非法、缺账户、或领域解码失败。
///
/// # 约束
/// 只读审计；编解码由 [`CardFundsRegistrationReceipt`] 独占。
pub async fn replay_card_funds_registration(
    db: &Database,
    audit_id: &str,
    expected_action: &str,
    expected_fingerprint: &str,
    executor: &mut dyn Executor,
) -> Result<Option<(String, String)>> {
    let Some(audit) = db.audit_logs().find_by_id(audit_id, executor).await? else {
        return Ok(None);
    };
    if audit.action != expected_action || audit.resource_type != "receivable_account" || !audit.success {
        return Err(Error::Internal("卡券票款登记幂等收据身份非法".to_string()));
    }
    let account_id = audit
        .resource_id
        .ok_or_else(|| Error::Internal("卡券票款登记幂等收据缺少应收账户".to_string()))?;
    let receipt = CardFundsRegistrationReceipt::parse(
        audit.message.as_deref().unwrap_or(""),
        expected_fingerprint,
        CardFundsRegistrationKind::from_expected_action(expected_action),
    )
    .map_err(map_registration_receipt_error)?;
    Ok(Some((account_id, receipt.fact_id().to_string())))
}
