//! 采购责任列表查询与显示事实装配。
use erp_procurement::repository::procurement_responsibility::ProcurementResponsibilityRuleFilter;
use persistence_core::NoTransaction;

use super::dto::{ProcurementResponsibilityRuleListParams, ProcurementResponsibilityRulePageView};
use super::{ProcurementResponsibilityReadService, load_procurement_rule_list_page, to_rule_list_views};
use crate::Result;
impl ProcurementResponsibilityReadService {
    /// 分页查询采购责任规则。
    ///
    /// # 参数
    /// * `params` - 类型、负责人、状态及分页筛选
    ///
    /// # 返回
    /// 返回规则管理分页视图。
    ///
    /// # 错误
    /// MongoDB 查询失败时返回错误。
    pub async fn rule_list(
        &self,
        params: ProcurementResponsibilityRuleListParams,
    ) -> Result<ProcurementResponsibilityRulePageView> {
        let filter = ProcurementResponsibilityRuleFilter {
            rule_type: params.rule_type,
            owner_user_id: params.owner_user_id,
            status: params.status,
            page: params.page,
            page_size: params.page_size,
        };
        let page = load_procurement_rule_list_page(&self.db, &filter, &mut NoTransaction).await?;
        let items = to_rule_list_views(page.items, &page.facts);
        Ok(ProcurementResponsibilityRulePageView {
            items,
            total: page.total,
            page: params.page,
            page_size: params.page_size,
        })
    }
}
