//! SalesReturnCase 详情与分页视图装配。
use super::dto::SalesReturnCaseView;
use super::dto::{PageView, SalesReturnCaseListParams, SortDir};
use super::ReturnsReadService;
use crate::{Error, Result};
use erp_returns::repository::ReturnsExt;
use persistence_core::NoTransaction;
use validator::Validate;
/// 销售退货处理单列表筛选条件类型（经 `ReturnsExt` 关联类型跨 crate 可达）。
type SalesReturnCaseFilter = <mongodb::Database as ReturnsExt>::SalesReturnCaseFilter;

impl ReturnsReadService {
    // -----------------------------------------------------------------------

    /// 分页查询销售退货/拒收处理单列表。
    ///
    /// # 参数
    /// * `params` - 查询参数（`return_no`/`sales_order_id`/`status` 扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    pub async fn sales_return_case_list(
        &self,
        params: &SalesReturnCaseListParams,
    ) -> Result<PageView<SalesReturnCaseView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = SalesReturnCaseFilter {
            return_no: query.return_no,
            sales_order_id: query.sales_order_id,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .sales_return_cases()
            .search_sales_return_cases(&filter, &mut NoTransaction)
            .await?;
        let mut views = Vec::with_capacity(page.items.len());
        for row in page.items {
            views.push(self.sales_return_case_view(row.id).await?);
        }
        Ok(PageView {
            items: views,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 查询销售退货/拒收处理单详情（处理单 + 明细行）。
    ///
    /// # 参数
    /// * `id` - 处理单 ID
    ///
    /// # 返回
    /// 返回完整处理单视图。
    ///
    /// # 错误
    /// * `NotFound` - 处理单不存在
    pub async fn sales_return_case_detail(&self, id: &str) -> Result<SalesReturnCaseView> {
        self.sales_return_case_view(id.to_string()).await
    }

    // -----------------------------------------------------------------------

    // -----------------------------------------------------------------------
    // 私有视图装配
    // -----------------------------------------------------------------------

    /// 装配销售退货/拒收处理单视图。
    ///
    /// # 参数
    /// * `id` - 处理单 ID
    ///
    /// # 返回
    /// 返回完整处理单视图。
    ///
    /// # 错误
    /// * `NotFound` - 处理单不存在
    async fn sales_return_case_view(&self, id: String) -> Result<SalesReturnCaseView> {
        let case = self
            .db
            .sales_return_cases()
            .find_by_id(&id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("销售退货处理单不存在".to_string()))?;
        let lines = self
            .db
            .sales_return_lines()
            .find_lines_by_cases(&[case.base.id.clone().into()], &mut NoTransaction)
            .await?
            .into_iter()
            .map(|line| super::dto::SalesReturnLineView {
                id: line.base.id.clone(),
                sales_order_line_id: line.sales_order_line_id.to_string(),
                requested_quantity: line.requested_quantity,
                received_quantity: line.received_quantity,
                quality_result: line.quality_result.map(|result| result.as_str().to_string()),
                restockable_quantity: line.restockable_quantity,
            })
            .collect();
        Ok(SalesReturnCaseView {
            id: case.base.id.clone(),
            return_no: case.return_no,
            sales_order_id: case.sales_order_id.to_string(),
            acceptance_id: case.acceptance_id.map(|id| id.to_string()),
            case_type: case.case_type,
            reason: case.reason,
            discovered_at: case.discovered_at,
            return_route: case.return_route,
            status: case.stable.status(),
            version: case.base.version,
            created_at: case.base.created_at,
            lines,
        })
    }
}
