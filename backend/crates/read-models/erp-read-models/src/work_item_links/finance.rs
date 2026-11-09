use erp_finance::entity::payable::{PayableSourceType, PurchasePaymentRequestLine};
use erp_finance::repository::{PayableExt, PurchasePaymentRequestExt, ReceivableExt};
use erp_workflow::entity::work_item::WorkItemLink;
use mongodb::Database;
use persistence_core::Executor;

use super::{link, required};
use crate::Result;
/// 读取当前写事务确认的直接上级引用。
/// # 参数
/// 数据库、已分派的对象类型与 ID、调用方执行器。
/// # 返回
/// 返回类型与 ID 成对的上级业务对象。
/// # 错误
/// 对象缺失、引用非法或仓储读取失败时返回错误。
pub(super) async fn parents(
    db: &Database,
    kind: &str,
    id: &str,
    executor: &mut dyn Executor,
) -> Result<Vec<WorkItemLink>> {
    match kind {
        "receivable_account" => Ok(vec![link(
            "sales_order",
            required(db.receivable_accounts().find_by_id(id, executor).await?)?.sales_order_id,
        )?]),
        "receivable_entry" => Ok(vec![link(
            "receivable_account",
            required(db.receivable_entries().find_by_id(id, executor).await?)?.receivable_account_id,
        )?]),
        "payable_entry" => Ok(vec![link(
            "payable_account",
            required(db.payable_entries().find_by_id(id, executor).await?)?.payable_account_id,
        )?]),
        "payable_account" => {
            let row = required(db.payable_accounts().find_by_id(id, executor).await?)?;
            let kind = match row.source_type {
                PayableSourceType::PurchaseOrder => "purchase_order",
                PayableSourceType::SupplierSettlement => "supplier_settlement",
            };
            Ok(vec![link(kind, row.source_document_id)?])
        },
        "sales_invoice_request" => {
            let row = required(db.sales_invoice_requests().find_by_id(id, executor).await?)?;
            Ok(vec![
                link("sales_order", row.sales_order_id)?,
                link("receivable_account", row.receivable_account_id)?,
            ])
        },
        _ => payments(db, kind, id, executor).await,
    }
}
async fn payments(
    db: &Database,
    kind: &str,
    id: &str,
    executor: &mut dyn Executor,
) -> Result<Vec<WorkItemLink>> {
    match kind {
        "purchase_payment_request" => {
            let row = required(db.purchase_payment_requests().find_by_id(id, executor).await?)?;
            request_links(&row.lines)
        },
        "customer_receipt" => {
            let row = required(db.customer_receipts().find_by_id(id, executor).await?)?;
            row.pending_allocations.iter().map(|a| link("receivable_entry", &a.receivable_entry_id)).collect()
        },
        "supplier_payment" => {
            let row = required(db.supplier_payments().find_by_id(id, executor).await?)?;
            let mut links = row
                .request_allocations
                .iter()
                .map(|a| link("payable_account", &a.payable_account_id))
                .collect::<Result<Vec<_>>>()?;
            if let Some(id) = row.payment_request_id {
                links.push(link("purchase_payment_request", id)?);
            }
            Ok(links)
        },
        "invoice" => {
            let row = required(db.invoices().find_by_id(id, executor).await?)?;
            let mut links = Vec::new();
            if let Some(id) = row.sales_invoice_request_id {
                links.push(link("sales_invoice_request", id)?);
            }
            if let Some(id) = row.original_invoice_id {
                links.push(link("invoice", id)?);
            }
            Ok(links)
        },
        _ => unreachable!("财务链路分派已限定类型"),
    }
}

/// 一份付款申请可以横跨多张采购单及销售单，必须保留每一行的冻结身份。
fn request_links(lines: &[PurchasePaymentRequestLine]) -> Result<Vec<WorkItemLink>> {
    let mut links = Vec::new();
    for line in lines {
        links.push(link("payable_account", &line.payable_account_id)?);
        links.push(link("purchase_order", &line.purchase_order_id)?);
        for sales in &line.source_snapshot.sales_sources {
            links.push(link("sales_order", &sales.sales_order_id)?);
        }
    }
    Ok(links)
}

#[cfg(test)]
mod tests {
    use erp_core::money::Amount;
    use erp_finance::entity::payable::{PaymentRequestSalesSource, PaymentRequestSourceSnapshot};

    use super::*;
    fn line(id: &str, sales: &[&str]) -> PurchasePaymentRequestLine {
        PurchasePaymentRequestLine {
            payable_account_id: format!("account-{id}"),
            purchase_order_id: id.into(),
            purchase_revision_id: format!("revision-{id}"),
            requested_amount: Amount::zero(),
            paid_amount: Amount::zero(),
            closed_amount: Amount::zero(),
            gross_total: Amount::zero(),
            settled_total: Amount::zero(),
            open_total: Amount::zero(),
            other_reserved_total: Amount::zero(),
            payment_terms: serde_json::json!({}),
            source_snapshot: PaymentRequestSourceSnapshot {
                purchase_preview: serde_json::json!({}),
                allowed_attachment_ids: Vec::new(),
                sales_sources: sales
                    .iter()
                    .map(|id| PaymentRequestSalesSource {
                        sales_order_id: (*id).into(),
                        sales_revision_id: "revision".into(),
                        customer_id: "customer".into(),
                        payer_party_id: "party".into(),
                        payer_party_name: "主体".into(),
                        preview: serde_json::json!({}),
                    })
                    .collect(),
            },
        }
    }
    #[test]
    fn payment_links_preserve_all_frozen_sources_and_reject_blank_ids() {
        let lines = vec![line("same", &["same", "sale-2"]), line("purchase-2", &["sale-3"])];
        let links = request_links(&lines).unwrap();
        assert_eq!(links.len(), 7);
        for (kind, id) in [
            ("purchase_order", "same"),
            ("sales_order", "same"),
            ("sales_order", "sale-2"),
            ("sales_order", "sale-3"),
            ("purchase_order", "purchase-2"),
            ("payable_account", "account-same"),
        ] {
            assert!(links.contains(&link(kind, id).unwrap()));
        }
        assert!(request_links(&[line("", &["sales"])]).is_err());
    }
}
