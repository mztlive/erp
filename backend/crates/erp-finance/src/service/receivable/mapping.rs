//! Finance fact projections and stable money, version, and ledger error contracts.

use crate::dto::receivable::{ReceivableInvoiceFactView, ReceivableReceiptFactView};
/// 固定零金额与账本共用同一实体层实现（`0.00` 确定性拼写）。
pub(crate) use crate::entity::receivable::allocation_amount::zero_amount;
use crate::entity::receivable::{CustomerReceiptStatus, InvoiceKind, InvoiceStatus};
use crate::ports::receivable::ReceivableSnapshot;
use crate::{Error, Result};

/// 装配当前账户关联的正式回款事实投影。
pub fn receipt_fact_views(snapshot: &ReceivableSnapshot) -> Vec<ReceivableReceiptFactView> {
    let mut receipts = snapshot.receipts.iter().collect::<Vec<_>>();
    receipts.sort_by(|left, right| {
        left.received_at.cmp(&right.received_at).then_with(|| left.base.id.cmp(&right.base.id))
    });
    receipts
        .into_iter()
        .map(|receipt| {
            let allocated_to_account = snapshot
                .receipt_allocations
                .iter()
                .filter(|allocation| allocation.customer_receipt_id.as_ref() == receipt.base.id.as_str())
                .fold(zero_amount(), |total, allocation| {
                    allocation.allocation_action.apply_to_net(total, allocation.allocated_amount)
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
pub fn invoice_fact_views(snapshot: &ReceivableSnapshot) -> Vec<ReceivableInvoiceFactView> {
    let mut invoices = snapshot.invoices.iter().collect::<Vec<_>>();
    invoices.sort_by(|left, right| {
        left.invoice_date.cmp(&right.invoice_date).then_with(|| left.base.id.cmp(&right.base.id))
    });
    invoices
        .into_iter()
        .map(|invoice| {
            let allocated_to_account = snapshot
                .invoice_allocations
                .iter()
                .filter(|allocation| allocation.invoice_id.as_ref() == invoice.base.id.as_str())
                .fold(zero_amount(), |total, allocation| {
                    allocation.allocation_action.apply_to_net(total, allocation.allocated_gross_amount)
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
        return Err(Error::ValidationError("任务版本必须是规范的正整数字符串".to_string()));
    }
    Ok(parsed)
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

/// 校验乐观锁版本。
///
/// # 错误
/// 不一致时返回冲突。
pub fn ensure_expected_version(actual: u64, expected: u64) -> Result<()> {
    if actual == expected {
        return Ok(());
    }
    Err(Error::ConflictError("数据已被其他请求修改，请刷新后重试".to_string()))
}
