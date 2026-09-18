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

/// 由操作者与幂等键生成不泄露原键的稳定纠错单号。
pub fn return_command_no(prefix: &str, actor_id: &str, idempotency_key: &str) -> String {
    let digest = hex::encode(Sha256::digest(format!("{actor_id}|{}", idempotency_key.trim()).as_bytes()));
    format!("{prefix}-{}", &digest[..8])
}

/// 校验纠错命令的原资金事实仍为同一版本且已经过账。
///
/// # 参数
/// * `actual_version` - 事务内重读所得版本
/// * `expected_version` - 命令准备阶段读取的版本
/// * `is_posted` - 原事实是否处于已过账状态
/// * `not_posted_message` - 当前纠错类型对应的业务说明
///
/// # 返回
/// 版本和状态均满足时返回成功。
///
/// # 错误
/// 版本变化返回冲突；原事实未过账返回业务规则错误。
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

/// 缺失单据统一映射为 `NotFound`（文案由调用方保持原语义）。
pub(crate) fn or_not_found<T>(option: Option<T>, message: &str) -> Result<T> {
    option.ok_or_else(|| Error::NotFound(message.to_string()))
}

/// 已冲正单据禁止再次过账（文案由调用方保持原语义）。
pub(crate) fn reject_if_reversed(is_reversed: bool, message: &str) -> Result<()> {
    if is_reversed {
        return Err(Error::BusinessLogicError(message.to_string()));
    }
    Ok(())
}

/// 累计限额判断并按单据类型映射为面向用户的业务文案。
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
