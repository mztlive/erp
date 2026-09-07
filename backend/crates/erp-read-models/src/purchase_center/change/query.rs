use erp_procurement::repository::PurchaseOrderExt;
use erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding;
use persistence_core::NoTransaction;
use validator::Validate;

use super::super::dto::PurchaseChangeOrderView;
use super::super::PurchaseOrderReadService;
use super::mapping::change_list_view;
use crate::{Error, Result};
use application_core::{normalize_sort, PageView, SortDir};
use erp_procurement::dto::purchase_order::PurchaseChangeOrderListParams;
use erp_workflow::service::document_registry::find_approval_binding;

impl PurchaseOrderReadService {
    /// 分页查询采购变更单列表。
    ///
    /// # 参数
    /// * `params` - 查询参数
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法
    pub async fn change_order_list(
        &self,
        params: &PurchaseChangeOrderListParams,
    ) -> Result<PageView<PurchaseChangeOrderView>> {
        params.validate()?;
        let (_, sort_dir) = normalize_sort(&params.sort_by, &params.sort_dir, &["created_at"])?;
        let page = params.page.unwrap_or(1);
        let page_size = params.page_size.unwrap_or(20).clamp(1, 100);
        let purchase_order_id = normalized_filter(params.purchase_order_id.as_deref());
        let status = normalized_filter(params.status.as_deref());
        let result = self
            .db
            .purchase_order()
            .search_change_orders(
                purchase_order_id.as_deref(),
                status.as_deref(),
                page,
                page_size,
                matches!(sort_dir, SortDir::Asc),
                &mut NoTransaction,
            )
            .await?;
        let total = result.total;
        let views = result
            .items
            .into_iter()
            .map(|change| change_list_view(change, None))
            .collect();
        Ok(PageView {
            items: views,
            total,
            page,
            page_size,
        })
    }

    /// 查询采购变更单详情。
    ///
    /// 返回统一只读审批结构；创建后未提交只返回绑定定义。
    ///
    /// # 参数
    /// * `id` - 变更单 ID
    ///
    /// # 返回
    /// 返回变更单视图。
    ///
    /// # 错误
    /// * `NotFound` - 变更单不存在
    pub async fn change_order_detail(&self, id: &str) -> Result<PurchaseChangeOrderView> {
        let change = self
            .db
            .purchase_change_orders()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("采购变更单不存在".to_string()))?;
        Ok(change_list_view(change, self.load_change_binding(id).await?))
    }

    /// 读取变更单创建时冻结的审批绑定。未注册时返回空绑定。
    ///
    /// # 错误
    /// 仓储失败时返回错误。
    async fn load_change_binding(&self, id: &str) -> Result<Option<ApprovalDefinitionBinding>> {
        match find_approval_binding(&self.db, id, &mut NoTransaction)
            .await
            .map_err(crate::Error::from)
        {
            Ok(binding) => Ok(binding),
            Err(Error::NotFound(_)) => Ok(None),
            Err(error) => Err(error),
        }
    }
}

/// 规范化可选列表筛选文本。
///
/// # 参数
/// * `value` - 原始可选筛选值
///
/// # 返回
/// 空白值返回 `None`，否则返回去除首尾空白后的字符串。
fn normalized_filter(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}
