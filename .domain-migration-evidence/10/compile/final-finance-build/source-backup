//! Finance fact projections and stable money, version, and ledger error contracts.

use crate::dto::receivable::{ReceivableInvoiceFactView, ReceivableReceiptFactView};
use crate::entity::receivable::{
    AllocationAction, CustomerReceiptStatus, InvoiceKind, InvoiceStatus, ReceivableFundsReview,
    ReceivableFundsReviewChain, ReceivableFundsSnapshot,
};
use crate::ports::receivable::CardFundsSnapshot;
use crate::{Error, Result};
use erp_core::money::Amount;
use std::str::FromStr;

/// 装配当前账户关联的正式回款事实投影。
pub fn receipt_fact_views(snapshot: &CardFundsSnapshot) -> Vec<ReceivableReceiptFactView> {
    let mut receipts = snapshot.receipts.iter().collect::<Vec<_>>();
    receipts.sort_by(|left, right| {
        left.received_at
            .cmp(&right.received_at)
            .then_with(|| left.base.id.cmp(&right.base.id))
    });
    receipts
        .into_iter()
        .map(|receipt| {
            let allocated_to_account = snapshot
                .receipt_allocations
                .iter()
                .filter(|allocation| allocation.customer_receipt_id.as_ref() == receipt.base.id.as_str())
                .fold(zero_amount(), |total, allocation| {
                    match allocation.allocation_action {
                        AllocationAction::Apply => total.checked_add(allocation.allocated_amount),
                        AllocationAction::Reverse => total.checked_sub(allocation.allocated_amount),
                    }
                });
            ReceivableReceiptFactView {
                receipt_id: receipt.base.id.clone(),
                receipt_no: receipt.receipt_no.clone(),
                received_at: receipt.received_at.as_utc().to_rfc3339(),
                gross_amount: receipt.amount,
                allocated_to_account,
                other_allocation_summary: None,
                reversed: receipt.status == CustomerReceiptStatus::Reversed,
            }
        })
        .collect()
}

/// 装配当前账户关联的正式销项发票事实投影。
pub fn invoice_fact_views(snapshot: &CardFundsSnapshot) -> Vec<ReceivableInvoiceFactView> {
    let mut invoices = snapshot.invoices.iter().collect::<Vec<_>>();
    invoices.sort_by(|left, right| {
        left.invoice_date
            .cmp(&right.invoice_date)
            .then_with(|| left.base.id.cmp(&right.base.id))
    });
    invoices
        .into_iter()
        .map(|invoice| {
            let allocated_to_account = snapshot
                .invoice_allocations
                .iter()
                .filter(|allocation| allocation.invoice_id.as_ref() == invoice.base.id.as_str())
                .fold(zero_amount(), |total, allocation| {
                    match allocation.allocation_action {
                        AllocationAction::Apply => total.checked_add(allocation.allocated_gross_amount),
                        AllocationAction::Reverse => total.checked_sub(allocation.allocated_gross_amount),
                    }
                });
            ReceivableInvoiceFactView {
                invoice_id: invoice.base.id.clone(),
                invoice_no: invoice.invoice_no.clone(),
                direction: match invoice.invoice_kind {
                    InvoiceKind::Blue => "BLUE",
                    InvoiceKind::Red => "RED",
                }
                .to_string(),
                issued_at: invoice.invoice_date.to_string(),
                gross_amount: invoice.gross_amount,
                net_amount: invoice.net_amount,
                tax_amount: invoice.tax_amount,
                allocated_to_account,
                reversed: invoice.stable.status() == InvoiceStatus::RedInvoiced,
            }
        })
        .collect()
}

/// 将 HTTP 字符串任务版本严格解析为运行时乐观锁版本。
pub fn parse_task_version(value: &str) -> Result<u64> {
    let normalized = value.trim();
    let parsed = normalized
        .parse::<u64>()
        .map_err(|_| Error::ValidationError("任务版本必须是无符号整数字符串".to_string()))?;
    if parsed == 0 || parsed.to_string() != normalized {
        return Err(Error::ValidationError(
            "任务版本必须是规范的正整数字符串".to_string(),
        ));
    }
    Ok(parsed)
}

/// 从已装载复核事实构造领域复核链。
///
/// # 参数
/// * `reviews` - 快照内复核事实
///
/// # 返回
/// 返回已验证连续性的链。
///
/// # 错误
/// 连续性损坏或溢出映射为 Internal，文案与原 helper 一致。
///
/// # 约束
/// 不扫描全链寻找后继；exact 定位在仓储层。
pub fn card_funds_review_chain(reviews: &[ReceivableFundsReview]) -> Result<ReceivableFundsReviewChain> {
    ReceivableFundsReviewChain::from_reviews(reviews).map_err(map_chain_error)
}

/// 从服务快照构造领域票款事实快照。
///
/// # 参数
/// * `snapshot` - 已装载的 W13 快照
///
/// # 返回
/// 返回无重复主键的领域快照。
///
/// # 错误
/// 重复主键映射为 Internal。
///
/// # 约束
/// 不读取数据库。
pub fn card_funds_snapshot_of(snapshot: &CardFundsSnapshot) -> Result<ReceivableFundsSnapshot> {
    ReceivableFundsSnapshot::from_facts(
        snapshot.entries.clone(),
        snapshot.receipt_allocations.clone(),
        snapshot.invoice_allocations.clone(),
        snapshot.receipts.clone(),
        snapshot.invoices.clone(),
    )
    .map_err(|error| Error::Internal(error.to_string()))
}

/// 将复核链领域错误映射为 Internal。
///
/// # 参数
/// * `error` - 领域错误
///
/// # 返回
/// 返回 Internal，文案保持 `应收复核链连续性损坏` / `应收复核号已达到上限`。
///
/// # 错误
/// 本函数即错误转换。
///
/// # 约束
/// 不改写文案。
pub fn map_chain_error(error: erp_core::Error) -> Error {
    Error::Internal(error.to_string())
}

/// 将回款账本错误映射为既有业务错误。
///
/// # 参数
/// * `error` - 领域错误
///
/// # 返回
/// 返回 BusinessLogicError，文案与原过账路径一致。
///
/// # 错误
/// 本函数即错误转换。
///
/// # 约束
/// 不得落到透明 Logic。
pub fn map_ledger_error(error: erp_core::Error) -> Error {
    Error::BusinessLogicError(error.to_string())
}

/// 返回固定零金额（`Amount::from_str("0.00")` 的确定性快捷方式）。
///
/// # 返回
/// 返回金额 `0.00`。
pub fn zero_amount() -> Amount {
    Amount::from_str("0.00").expect("固定零金额必须可解析")
}

/// 校验乐观锁版本。
///
/// # 错误
/// 不一致时返回冲突。
pub fn ensure_expected_version(actual: u64, expected: u64) -> Result<()> {
    if actual == expected {
        return Ok(());
    }
    Err(Error::ConflictError(
        "数据已被其他请求修改，请刷新后重试".to_string(),
    ))
}

/// 返回复核类型对应的待复核账户缓存状态。
pub fn pending_review_status(
    review_type: crate::dto::receivable::CardFundsReviewType,
) -> crate::entity::receivable::AccountReviewStatus {
    match review_type {
        crate::dto::receivable::CardFundsReviewType::Opening => {
            crate::entity::receivable::AccountReviewStatus::OpeningPending
        }
        crate::dto::receivable::CardFundsReviewType::SyncDelta => {
            crate::entity::receivable::AccountReviewStatus::SyncDeltaPending
        }
    }
}
