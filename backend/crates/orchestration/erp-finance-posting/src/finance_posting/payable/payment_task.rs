//! 旧采购应付直付任务不能揭示收款账号。
use erp_finance::dto::payable::{PaymentRecipientRevealView, RevealPaymentRecipientRequest};
use erp_workflow::service::work_item::expected_task_version;
use validator::Validate;

use crate::{Error, Result};

/// 保留旧揭示入口的校验顺序，不读取付款任务或账户。
/// # 参数
/// * `req` - 原旧直付揭示请求。
/// # 返回
/// 不返回收款账号。
/// # 错误
/// 先返回请求或任务版本的校验错误，校验通过后固定返回冲突。
pub(super) fn reject_legacy_payment_reveal(
    req: &RevealPaymentRecipientRequest,
) -> Result<PaymentRecipientRevealView> {
    req.validate()?;
    expected_task_version(&req.expected_task_version)?;
    Err(Error::ConflictError("旧采购应付直付入口已停止，请先提交并批准采购付款申请".into()))
}

#[cfg(test)]
mod tests {
    use erp_core::ids::WorkItemId;

    use super::*;

    #[test]
    fn legacy_payment_reveal_validates_request_then_task_version_before_rejecting() {
        let mut req = RevealPaymentRecipientRequest {
            work_item_id: WorkItemId::new("task-1"),
            expected_task_version: "0".into(),
            expected_bank_account_id: "account-1".into(),
            expected_bank_account_version: 0,
        };
        assert!(matches!(
            reject_legacy_payment_reveal(&req),
            Err(Error::InvalidInput(issues))
                if issues.fields()["expected_bank_account_version"] == "收款账户版本必须大于0"
        ));

        req.expected_bank_account_version = 1;
        assert!(matches!(
            reject_legacy_payment_reveal(&req),
            Err(Error::ValidationError(message)) if message == "任务版本必须为正整数字符串"
        ));

        req.expected_task_version = "1".into();
        assert!(matches!(
            reject_legacy_payment_reveal(&req),
            Err(Error::ConflictError(message))
                if message == "旧采购应付直付入口已停止，请先提交并批准采购付款申请"
        ));
    }
}
