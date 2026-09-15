//! CustomerRefund 详情与分页视图装配。
use erp_returns::repository::ReturnsExt;
use persistence_core::NoTransaction;
use validator::Validate;

use super::ReturnsReadService;
use super::dto::{CustomerRefundListParams, CustomerRefundView, PageView, SortDir};
use crate::{Error, Result};
/// 客户退款列表筛选条件类型。
type CustomerRefundFilter = <mongodb::Database as ReturnsExt>::CustomerRefundFilter;
use erp_workflow::DocumentRegistryExt;
use erp_workflow::service::document_registry::find_approval_binding;

use super::customer_refund_list::{
    CustomerRefundListFacts, customer_refund_view_from_facts, map_customer_refund_list_page,
};

impl ReturnsReadService {
    // -----------------------------------------------------------------------

    /// 分页查询客户退款列表。
    ///
    /// # 参数
    /// * `params` - 查询参数
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    pub async fn customer_refund_list(
        &self,
        params: &CustomerRefundListParams,
    ) -> Result<PageView<CustomerRefundView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = CustomerRefundFilter {
            refund_no: query.refund_no,
            customer_id: query.customer_id,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self.db.customer_refunds().search_customer_refunds(&filter, &mut NoTransaction).await?;
        let document_ids = page.items.iter().map(|row| row.id.clone()).collect::<Vec<_>>();
        let documents =
            self.db.business_documents().find_documents_by_ids(&document_ids, &mut NoTransaction).await?;
        let items = map_customer_refund_list_page(
            page.items
                .into_iter()
                .map(|row| CustomerRefundListFacts {
                    id: row.id,
                    refund_no: row.refund_no,
                    status: row.status,
                    sales_return_case_id: row.sales_return_case_id,
                    customer_id: row.customer_id,
                    original_receipt_id: row.original_receipt_id,
                    original_receivable_entry_id: row.original_receivable_entry_id,
                    reason_code: row.reason_code,
                    reason_text: row.reason_text,
                    amount: row.amount,
                    handled_by: row.handled_by,
                    reviewed_by: row.reviewed_by,
                    occurred_at: row.occurred_at,
                    version: row.version,
                    created_at: row.created_at,
                })
                .collect(),
            documents,
        );
        Ok(PageView { items, total: page.total, page: filter.page, page_size: filter.page_size })
    }

    /// 查询客户退款详情。
    ///
    /// # 参数
    /// * `id` - 退款单 ID
    ///
    /// # 返回
    /// 返回退款单视图。
    ///
    /// # 错误
    /// * `NotFound` - 退款单不存在
    pub async fn customer_refund_detail(&self, id: &str) -> Result<CustomerRefundView> {
        self.customer_refund_view(id.to_string()).await
    }

    // -----------------------------------------------------------------------

    // -----------------------------------------------------------------------
    // 私有视图装配
    // -----------------------------------------------------------------------

    /// 装配客户退款单视图。
    ///
    /// # 参数
    /// * `id` - 退款单 ID
    ///
    /// # 返回
    /// 返回退款单视图（含只读审批结构）。
    ///
    /// # 错误
    /// * `NotFound` - 退款单不存在
    async fn customer_refund_view(&self, id: String) -> Result<CustomerRefundView> {
        let refund = self
            .db
            .customer_refunds()
            .find_by_id(&id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("客户退款单不存在".to_string()))?;
        let binding = match find_approval_binding(&self.db, &id, &mut NoTransaction)
            .await
            .map_err(crate::Error::from)
        {
            Ok(binding) => binding,
            Err(Error::NotFound(_)) => None,
            Err(error) => return Err(error),
        };
        let mut view =
            customer_refund_view_from_facts(CustomerRefundListFacts::from_refund(&refund), binding.as_ref());
        view.approval = super::approval::load_runtime(
            &self.db,
            erp_workflow::entity::document_registry::DocumentType::CustomerRefund,
            &id,
            view.approval,
        )
        .await?;
        Ok(view)
    }
}

#[cfg(test)]
mod tests {
    /// 列表必须批量读取注册行，不得对每个分页行再读详情。
    #[test]
    fn list_batches_document_bindings_and_keeps_missing_registry_rows() {
        let query = include_str!("customer_refund.rs").split("#[cfg(test)]").next().expect("查询生产代码");
        let command = include_str!("../../../erp-processes/src/reverse_flow/customer_refund.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("命令生产代码");
        let domain = include_str!("../../../erp-returns/src/service/customer_refund.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("本域生产代码");
        let production = format!("{query}\n{command}\n{domain}");
        assert!(production.contains("find_documents_by_ids"));
        assert!(production.contains("map_customer_refund_list_page"));
        assert!(!production.contains("customer_refund_view(row.id)"));
        assert!(production.contains("refund.matches_version(req.expected_version)"));
        assert!(production.contains("conflict_if_stale_version"));
        assert!(!production.contains("fn ensure_expected_version"));
    }
}
