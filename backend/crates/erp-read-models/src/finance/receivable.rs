//! Receivable account and customer receipt read models.

mod account;
pub mod approval_view;
mod customer_receipt;
pub mod snapshot;

/// Read-only receivable projections spanning finance, sales and workflow facts.
///
/// All commands and transaction ownership stay with financial posting processes.
pub struct ReceivableReadService {
    db: mongodb::Database,
}

impl ReceivableReadService {
    /// Construct the read model for the supplied database.
    ///
    /// Construction performs no reads or writes and cannot fail.
    pub fn new(db: mongodb::Database) -> Self {
        Self { db }
    }
}

impl ReceivableReadService {
    /// 发票列表关键词同时支持主体和实际关联的来源、收付款单据。
    ///
    /// 分页与金额视图继续复用财务服务，外域或财务读取失败整次返回错误。
    pub async fn invoice_list(
        &self,
        params: &erp_finance::dto::receivable::InvoiceListParams,
    ) -> crate::Result<erp_finance::dto::receivable::PageView<erp_finance::dto::receivable::InvoiceView>>
    {
        use erp_finance::entity::receivable::InvoiceDirection;
        use erp_finance::repository::keyword::FinanceSearchTarget;
        use validator::Validate;
        params.validate()?;
        let ids = match params.invoice_direction {
            Some(InvoiceDirection::Sales) => {
                super::search::keyword_ids(&self.db, params.q.as_deref(), FinanceSearchTarget::SalesInvoice)
                    .await?
            }
            Some(InvoiceDirection::Purchase) => {
                super::search::keyword_ids(
                    &self.db,
                    params.q.as_deref(),
                    FinanceSearchTarget::PurchaseInvoice,
                )
                .await?
            }
            None => {
                let sales = super::search::keyword_ids(
                    &self.db,
                    params.q.as_deref(),
                    FinanceSearchTarget::SalesInvoice,
                )
                .await?;
                let purchase = super::search::keyword_ids(
                    &self.db,
                    params.q.as_deref(),
                    FinanceSearchTarget::PurchaseInvoice,
                )
                .await?;
                sales.map(|mut ids| {
                    ids.extend(purchase.unwrap_or_default());
                    ids.sort();
                    ids.dedup();
                    ids
                })
            }
        };
        Ok(
            erp_finance::service::receivable::ReceivableService::new(self.db.clone())
                .invoice_list(params, ids)
                .await?,
        )
    }
}
