//! 退货与资金纠错的稳定命令编号、来源版本及已过账前置规则。
use erp_core::money::Amount;
use sha2::{Digest, Sha256};

use crate::entity::returns::CumulativeAmountLimit;
use crate::{Error, Result};

/// 一次提交构造器的默认财务复核人（经办/复核分离，复核人固定）。
pub const DEFAULT_FINANCE_REVIEWER: &str = "finance_reviewer";
/// 客户退款一次提交单号前缀。
pub const CUSTOMER_REFUND_COMMAND_PREFIX: &str = "TK";
/// 供应商退款一次提交单号前缀。
pub const SUPPLIER_REFUND_COMMAND_PREFIX: &str = "GTK";
/// 回款冲正一次提交单号前缀。
pub const RECEIPT_REVERSAL_COMMAND_PREFIX: &str = "CZ";
/// 付款冲正一次提交单号前缀。
pub const PAYMENT_REVERSAL_COMMAND_PREFIX: &str = "PCZ";

/// 首次提交资金纠错单时，用操作人 ID 和去除首尾空白的幂等键生成稳定单号；返回业务前缀加 SHA-256 摘要的前 8 位十六进制字符，不回显原键。
pub fn return_command_no(prefix: &str, actor_id: &str, idempotency_key: &str) -> String {
    let digest = hex::encode(Sha256::digest(format!("{actor_id}|{}", idempotency_key.trim()).as_bytes()));
    format!("{prefix}-{}", &digest[..8])
}

/// 纠错入事务后用重读的原资金事实先核对准备阶段的版本，再要求原事实已过账。
/// 版本变化返回 `ConflictError`，未过账返回该纠错类型的业务规则错误。
pub fn ensure_posted_source(
    actual_version: u64,
    expected_version: u64,
    is_posted: bool,
    not_posted_message: &str,
) -> Result<()> {
    if actual_version != expected_version {
        return Err(Error::ConflictError("原资金记录已变化，请刷新后重试".to_string()));
    }
    if !is_posted {
        return Err(Error::BusinessLogicError(not_posted_message.to_string()));
    }
    Ok(())
}

// 将仓储查无记录的 `None` 转成该单据类型的 `NotFound` 文案，已有对象直接交给后续业务检查。
pub(crate) fn or_not_found<T>(option: Option<T>, message: &str) -> Result<T> {
    option.ok_or_else(|| Error::NotFound(message.to_string()))
}

// 资金纠错过账前拒绝已冲正单据，以免重复过账；按调用单据的文案返回 `BusinessLogicError`。
pub(crate) fn reject_if_reversed(is_reversed: bool, message: &str) -> Result<()> {
    if is_reversed {
        return Err(Error::BusinessLogicError(message.to_string()));
    }
    Ok(())
}

// 资金纠错过账前，以同一原收付款的已过账合计（不含本次）加本次金额核对原单限额，超限按单据类型返回 `BusinessLogicError`。
pub(crate) fn ensure_cumulative_within(
    source: Amount,
    posted_before: Amount,
    current: Amount,
    message: &str,
) -> Result<()> {
    CumulativeAmountLimit::ensure_within_limit(source, posted_before, current)
        .map_err(|_| Error::BusinessLogicError(message.to_string()))
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::{ensure_cumulative_within, ensure_posted_source, or_not_found, reject_if_reversed};
    use crate::Error;
    use crate::repository::returns::{
        CustomerRefundFilter, PurchaseReturnOrderFilter, SalesReturnCaseFilter,
    };

    #[test]
    fn correction_source_requires_same_posted_fact() {
        assert!(ensure_posted_source(3, 3, true, "必须已过账").is_ok());
        assert!(matches!(ensure_posted_source(4, 3, true, "必须已过账"), Err(Error::ConflictError(_))));
        assert!(matches!(ensure_posted_source(3, 3, false, "必须已过账"), Err(Error::BusinessLogicError(_))));
    }

    #[test]
    fn shared_plumbing_keeps_not_found_reversed_and_limit_messages() {
        assert_eq!(or_not_found(Some(1), "缺失").unwrap(), 1);
        assert!(matches!(or_not_found::<i32>(None, "缺失"), Err(Error::NotFound(_))));

        assert!(reject_if_reversed(false, "已冲正").is_ok());
        assert!(matches!(reject_if_reversed(true, "已冲正"), Err(Error::BusinessLogicError(_))));

        let amount = |value: &str| super::Amount::from_str(value).unwrap();
        assert!(ensure_cumulative_within(amount("100"), amount("60"), amount("40"), "超限").is_ok());
        assert!(ensure_cumulative_within(amount("100"), amount("60"), amount("41"), "超限").is_err());
    }

    #[test]
    fn tier_c_return_filters_default_to_first_page_size_20() {
        assert_eq!(
            (SalesReturnCaseFilter::default().page, SalesReturnCaseFilter::default().page_size),
            (1, 20)
        );
        assert_eq!(
            (PurchaseReturnOrderFilter::default().page, PurchaseReturnOrderFilter::default().page_size),
            (1, 20)
        );
        assert_eq!(
            (CustomerRefundFilter::default().page, CustomerRefundFilter::default().page_size),
            (1, 20)
        );
    }
}
