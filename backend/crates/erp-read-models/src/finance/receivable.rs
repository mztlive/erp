//! 应收子账与客户回款读模型。

mod account;
mod approval_query;
pub mod approval_view;
mod customer_receipt;
pub mod invoice_request;
pub mod invoice_request_source;
pub mod snapshot;

/// 只读应收投影，跨越财务、销售与审批事实。
///
/// 命令与事务所有权仍留在财务过账流程。
pub struct ReceivableReadService {
    db: mongodb::Database,
}

impl ReceivableReadService {
    /// 为给定数据库构造应收读模型。
    ///
    /// 构造不读不写。
    ///
    /// # 参数
    /// * `db` - 目标数据库。
    ///
    /// # 返回
    /// 返回应收读模型。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn new(db: mongodb::Database) -> Self {
        Self { db }
    }
}

impl ReceivableReadService {
    /// 发票列表关键词同时支持主体和实际关联的来源、收付款单据。
    ///
    /// 分页与金额视图继续复用财务服务，外域或财务读取失败整次返回错误。
    ///
    /// # 参数
    /// * `params` - 发票列表查询参数。
    ///
    /// # 返回
    /// 返回财务服务装配的发票分页视图。
    ///
    /// # 错误
    /// 参数校验失败，或关键词、外域与财务发票读取失败时整次返回错误。
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
            },
            Some(InvoiceDirection::Purchase) => {
                super::search::keyword_ids(
                    &self.db,
                    params.q.as_deref(),
                    FinanceSearchTarget::PurchaseInvoice,
                )
                .await?
            },
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
                    crate::support::dedup_sorted(ids)
                })
            },
        };
        Ok(erp_finance::service::receivable::ReceivableService::new(self.db.clone())
            .invoice_list(params, ids)
            .await?)
    }
}
