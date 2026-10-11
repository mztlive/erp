//! 仅展示当前应收份额，不返回整张跨单回款或发票金额及访问入口。
use erp_core::money::Amount;
use erp_finance::dto::receivable::{ReceivableInvoiceFactView, ReceivableReceiptFactView};
use erp_finance::entity::receivable::ReceivableAccount;
use mongodb::Database;
use persistence_core::Executor;
use serde::Serialize;

use crate::Result;
use crate::finance::receivable::snapshot::{
    invoice_fact_views, load_receivable_snapshot, receipt_fact_views,
};

/// 当前应收最近的已登记票款；金额为本笔净分配金额。
#[derive(Debug, Serialize)]
pub struct InvoiceReviewRecord {
    pub number: String,
    pub date: String,
    pub amount: Amount,
    pub status: String,
}

/// 记录条数与最近十条展示保持一致，汇总金额不受展示截断影响。
#[derive(Debug, Serialize)]
pub struct InvoiceReviewRecords {
    pub receipts: Vec<InvoiceReviewRecord>,
    pub invoices: Vec<InvoiceReviewRecord>,
    pub receipt_count: usize,
    pub invoice_count: usize,
}

/// 按唯一应收读取本笔份额，展示按日期倒序最多十条。
/// # 参数
/// 数据库、已核对授权来源的应收及同一事务执行器。
/// # 返回
/// 回款和发票的本笔净分配记录及完整记录数。
/// # 错误
/// 引用记录缺失或仓储读取失败时拒绝，不将错误解释为零金额。
pub(super) async fn load(
    db: &Database,
    account: &ReceivableAccount,
    executor: &mut dyn Executor,
) -> Result<InvoiceReviewRecords> {
    let snapshot = load_receivable_snapshot(db, account, executor).await?;
    let receipts = receipt_fact_views(&snapshot);
    let invoices = invoice_fact_views(&snapshot);
    Ok(InvoiceReviewRecords {
        receipt_count: receipts.len(),
        invoice_count: invoices.len(),
        receipts: receipts.into_iter().rev().take(10).map(receipt).collect(),
        invoices: invoices.into_iter().rev().take(10).map(invoice).collect(),
    })
}

fn receipt(row: ReceivableReceiptFactView) -> InvoiceReviewRecord {
    InvoiceReviewRecord {
        number: row.receipt_no,
        date: row.received_at,
        amount: row.allocated_to_account,
        status: if row.reversed { "已冲正" } else { "已登记" }.into(),
    }
}

fn invoice(row: ReceivableInvoiceFactView) -> InvoiceReviewRecord {
    InvoiceReviewRecord {
        number: row.invoice_no,
        date: row.issued_at,
        amount: row.allocated_to_account,
        status: if row.reversed {
            "已红冲"
        } else if row.direction == "RED" {
            "红票"
        } else {
            "蓝票"
        }
        .into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn receipt_exposes_only_the_authorized_account_share_and_reversal_state() {
        let row = receipt(ReceivableReceiptFactView {
            receipt_id: "internal".into(),
            receipt_no: "SK-001".into(),
            received_at: "2026-10-12".into(),
            gross_amount: "1000".parse().unwrap(),
            allocated_to_account: "165".parse().unwrap(),
            other_allocation_summary: Some("另一个客户的资金".into()),
            reversed: true,
        });
        assert_eq!(row.amount, "165".parse().unwrap());
        assert_eq!(row.status, "已冲正");
        let json = serde_json::to_value(row).unwrap();
        assert!(json.get("gross_amount").is_none());
        assert!(json.get("receipt_id").is_none());
    }
    #[test]
    fn red_invoice_keeps_its_signed_account_allocation() {
        let row = invoice(ReceivableInvoiceFactView {
            invoice_id: "internal".into(),
            invoice_no: "FP-001".into(),
            issued_at: "2026-10-12".into(),
            direction: "RED".into(),
            gross_amount: "1000".parse().unwrap(),
            net_amount: "1000".parse().unwrap(),
            tax_amount: Amount::zero(),
            allocated_to_account: "-165".parse().unwrap(),
            reversed: false,
        });
        assert_eq!(row.amount, "-165".parse().unwrap());
        assert_eq!(row.status, "红票");
        assert!(serde_json::to_value(row).unwrap().get("gross_amount").is_none());
    }
}
