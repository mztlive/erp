//! 销售单回款/开票进度与关闭状态的派生刷新（§9.3）。

use std::str::FromStr;

use database::{ReceivableExt, SalesOrderExt};
use entities::ids::SalesOrderId;
use entities::money::Amount;
use entities::receivable::ReceivableAccount;
use entities::sales_order::{CloseStatus, CollectionProgress, FulfillmentProgress, InvoiceProgress};
use mongodb::bson::doc;
use mongodb::Database;

use crate::errors::{Error, Result};

/// 按应收子账事实刷新销售单回款/开票进度与关闭状态。
///
/// 回款/开票进度从子账开放余额派生；关闭状态按 §9.3 判定：全部明细履约完成
/// 且客户应收结清后自动结案（开票完成不参与关闭判定）。任一字段变化时更新
/// 销售单并写版本触及；无变化时不写。
///
/// # 参数
/// * `db` - 数据库实例
/// * `session` - 事务会话执行器
/// * `sales_order_id` - 销售单
/// * `actor_id` - 审计操作人
/// * `fulfillment` - 外部已算好的履约进度；`None` 表示不修改履约进度，
///   关闭状态按销售单当前履约进度推导
///
/// # 返回
/// 无返回值。
///
/// # 错误
/// 销售单不存在或仓储写入失败时返回错误。
pub(crate) async fn update_sales_order_money_progress(
    db: &Database,
    session: &mut mongodb::ClientSession,
    sales_order_id: &SalesOrderId,
    actor_id: String,
    fulfillment: Option<FulfillmentProgress>,
) -> Result<()> {
    let mut order = db
        .sales_orders()
        .find_by_id(sales_order_id.as_ref(), session)
        .await?
        .ok_or_else(|| Error::NotFound("销售单不存在".to_string()))?;
    let accounts = db
        .receivable_accounts()
        .find_many(doc! { "sales_order_id": sales_order_id.to_string() }, session)
        .await?;
    let collection = derive_collection_progress(&accounts);
    let invoice = derive_invoice_progress(&accounts);
    let fulfillment_done =
        fulfillment.unwrap_or(order.fulfillment_progress) == FulfillmentProgress::Completed;
    let close = match (fulfillment_done, collection == CollectionProgress::Settled) {
        (true, true) => CloseStatus::Closed,
        (true, false) | (false, true) => CloseStatus::Closeable,
        (false, false) => CloseStatus::NotSatisfied,
    };
    let fulfillment_changed = fulfillment.is_some_and(|progress| order.fulfillment_progress != progress);
    if !fulfillment_changed
        && order.collection_progress == collection
        && order.invoice_progress == invoice
        && order.close_status == close
    {
        return Ok(());
    }
    if close == CloseStatus::Closed && order.close_status != CloseStatus::Closed {
        order.closed_at = Some(entities::common::time::Instant::now());
    }
    if let Some(progress) = fulfillment {
        order.fulfillment_progress = progress;
    }
    order.collection_progress = collection;
    order.invoice_progress = invoice;
    order.close_status = close;
    order.stable.touch(actor_id);
    db.sales_orders().update(&mut order, session).await?;
    Ok(())
}

/// 回款进度：全部子账已结清 → 已结清；任一子账有结清 → 部分回款；否则未收。
fn derive_collection_progress(accounts: &[ReceivableAccount]) -> CollectionProgress {
    if accounts.is_empty() {
        return CollectionProgress::NotCollected;
    }
    let zero = Amount::from_str("0").expect("零金额必须可解析");
    let all_settled = accounts
        .iter()
        .all(|account| account.open_total == zero && account.settled_total > zero);
    let any_settled = accounts.iter().any(|account| account.settled_total > zero);
    if all_settled {
        CollectionProgress::Settled
    } else if any_settled {
        CollectionProgress::PartiallyCollected
    } else {
        CollectionProgress::NotCollected
    }
}

/// 开票进度：全部子账可开票余额清零且已有开票 → 已完成；任一子账有开票 → 部分开票；否则未开。
fn derive_invoice_progress(accounts: &[ReceivableAccount]) -> InvoiceProgress {
    if accounts.is_empty() {
        return InvoiceProgress::NotInvoiced;
    }
    let zero = Amount::from_str("0").expect("零金额必须可解析");
    let all_invoiced = accounts
        .iter()
        .all(|account| account.open_invoiceable_total == zero && account.invoiced_total > zero);
    let any_invoiced = accounts.iter().any(|account| account.invoiced_total > zero);
    if all_invoiced {
        InvoiceProgress::Completed
    } else if any_invoiced {
        InvoiceProgress::PartiallyInvoiced
    } else {
        InvoiceProgress::NotInvoiced
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use entities::common::stable::StableBase;
    use entities::money::Amount;
    use entities::receivable::{ReceivableAccount, ReceivableAccountStatus};
    use entities::sales_order::{CloseStatus, CollectionProgress, FulfillmentProgress, InvoiceProgress};

    use super::{derive_collection_progress, derive_invoice_progress};

    fn account(settled: &str, open: &str, invoiced: &str, open_invoiceable: &str) -> ReceivableAccount {
        ReceivableAccount {
            base: entities::BaseModel::new("account-1".to_string()),
            stable: StableBase::new(ReceivableAccountStatus::Open, "system"),
            sales_order_id: entities::ids::SalesOrderId::new("order-1"),
            account_seq: 1,
            customer_id: entities::ids::CustomerAccountId::new("customer-1"),
            counterparty_party_id: entities::ids::PartyId::new("party-1"),
            source_sales_order_revision_id: entities::ids::SalesOrderRevisionId::new("revision-1"),
            review_status: entities::receivable::AccountReviewStatus::NotApplicable,
            reviewed_by: None,
            reviewed_at: None,
            review_evidence_reference: None,
            gross_total: Amount::from_str("100.00").unwrap(),
            settled_total: Amount::from_str(settled).unwrap(),
            open_total: Amount::from_str(open).unwrap(),
            invoiceable_total: Amount::from_str("100.00").unwrap(),
            invoiced_total: Amount::from_str(invoiced).unwrap(),
            open_invoiceable_total: Amount::from_str(open_invoiceable).unwrap(),
        }
    }

    #[test]
    fn collection_progress_derives_from_open_and_settled_totals() {
        assert_eq!(derive_collection_progress(&[]), CollectionProgress::NotCollected);
        assert_eq!(
            derive_collection_progress(&[account("0", "100", "0", "100")]),
            CollectionProgress::NotCollected
        );
        assert_eq!(
            derive_collection_progress(&[account("50", "50", "0", "100")]),
            CollectionProgress::PartiallyCollected
        );
        assert_eq!(
            derive_collection_progress(&[account("100", "0", "0", "100")]),
            CollectionProgress::Settled
        );
    }

    #[test]
    fn invoice_progress_derives_from_invoiceable_and_invoiced_totals() {
        assert_eq!(derive_invoice_progress(&[]), InvoiceProgress::NotInvoiced);
        assert_eq!(
            derive_invoice_progress(&[account("0", "100", "0", "100")]),
            InvoiceProgress::NotInvoiced
        );
        assert_eq!(
            derive_invoice_progress(&[account("0", "100", "50", "50")]),
            InvoiceProgress::PartiallyInvoiced
        );
        assert_eq!(
            derive_invoice_progress(&[account("0", "100", "100", "0")]),
            InvoiceProgress::Completed
        );
    }

    #[test]
    fn close_status_requires_both_fulfillment_and_settlement() {
        let settled = CollectionProgress::Settled;
        let open = CollectionProgress::NotCollected;
        let done = FulfillmentProgress::Completed;
        let pending = FulfillmentProgress::NotStarted;
        assert_eq!(close_for(done, settled), CloseStatus::Closed);
        assert_eq!(close_for(done, open), CloseStatus::Closeable);
        assert_eq!(close_for(pending, settled), CloseStatus::Closeable);
        assert_eq!(close_for(pending, open), CloseStatus::NotSatisfied);
    }

    fn close_for(fulfillment: FulfillmentProgress, collection: CollectionProgress) -> CloseStatus {
        match (
            fulfillment == FulfillmentProgress::Completed,
            collection == CollectionProgress::Settled,
        ) {
            (true, true) => CloseStatus::Closed,
            (true, false) | (false, true) => CloseStatus::Closeable,
            (false, false) => CloseStatus::NotSatisfied,
        }
    }
}
