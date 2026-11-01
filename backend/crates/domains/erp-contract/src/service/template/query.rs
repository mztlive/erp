//! 模板目录和当前申请人记录的数据库分页。

use application_core::AuditActor;
use persistence_core::NoTransaction;

use super::ContractTemplateService;
use crate::PageView;
use crate::dto::template::{ApplicationView, TemplateListParams, TemplateView};
use crate::error::Result;
use crate::repository::templates::{applications_page, templates_page};

impl ContractTemplateService {
    /// 查询共享模板目录，已停用模板只用于维护回显。
    /// # 参数
    /// * `params` - 分页与停用回显选项。
    /// # 返回
    /// 稳定排序的模板公开视图。
    /// # 错误
    /// 数据读取失败。
    pub async fn templates(&self, params: &TemplateListParams) -> Result<PageView<TemplateView>> {
        let result = templates_page(&self.db, params, &mut NoTransaction).await?;
        Ok(PageView {
            items: result.items.into_iter().map(Into::into).collect(),
            total: result.total,
            page: result.page,
            page_size: result.page_size,
        })
    }

    /// 查询本人合同申请，不能通过查询参数指定他人。
    /// # 参数
    /// * `params` - 分页参数。
    /// * `actor` - 当前销售。
    /// # 返回
    /// 本人的申请记录。
    /// # 错误
    /// 数据读取失败。
    pub async fn applications(
        &self,
        params: &TemplateListParams,
        actor: &AuditActor,
    ) -> Result<PageView<ApplicationView>> {
        let result = applications_page(&self.db, actor.id(), params, &mut NoTransaction).await?;
        Ok(PageView {
            items: result.items.into_iter().map(Into::into).collect(),
            total: result.total,
            page: result.page,
            page_size: result.page_size,
        })
    }
}
