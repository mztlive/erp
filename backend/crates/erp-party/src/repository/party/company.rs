//! 公司专用查询始终限定我方公司角色。
use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use persistence_core::{Executor, PageResult, Repository, Result, mongo_ops};

use crate::Party;
use crate::dto::company::CompanyListParams;
use crate::entity::party::company::identity_name;

/// 构建分页前公司筛选条件，列表和总数共用。
fn company_filter(params: &CompanyListParams) -> Document {
    let mut filter = doc! { "deleted_at": 0_i64, "company_profile": { "$type": "object" } };
    if let Some(status) = params.status {
        filter.insert("status", status.as_str());
    }
    if let Some(keyword) = params.keyword.as_deref().filter(|s| !s.trim().is_empty()) {
        let keyword = regex::escape(&identity_name(keyword));
        filter.insert("company_profile.names", doc! { "$regex": keyword, "$options": "i" });
    }
    filter
}

/// 公司专用查询扩展，始终限定我方公司角色。
#[allow(async_fn_in_trait)]
pub trait PartyRepositoryCompanyExt {
    /// 查询公司列表，稳定排序且有界分页。
    ///
    /// # Errors
    /// 查询或计数失败时返回仓储错误。
    async fn companies(
        &self,
        params: &CompanyListParams,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<Party>>;

    /// 精确解析公司全称、简称或别名；不使用模糊结果猜测身份。
    ///
    /// # Errors
    /// 查询失败返回仓储错误；已停用或无匹配返回空值。
    async fn company_by_name(&self, name: &str, executor: &mut dyn Executor) -> Result<Option<Party>>;
}

impl PartyRepositoryCompanyExt for Repository<'_, Party> {
    async fn companies(
        &self,
        params: &CompanyListParams,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<Party>> {
        let page = params.page.unwrap_or(1).clamp(1, 1_000_000);
        let size = params.page_size.unwrap_or(30).clamp(1, 100);
        let options = FindOptions::builder()
            .sort(doc! { "party_no": 1, "id": 1 })
            .skip((page - 1) * u64::from(size))
            .limit(i64::from(size))
            .build();
        let items =
            mongo_ops::find_many(&self.collection(), company_filter(params), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), company_filter(params), executor).await?;
        Ok(PageResult { items, total: total as i64 })
    }

    async fn company_by_name(&self, name: &str, executor: &mut dyn Executor) -> Result<Option<Party>> {
        self.find_one(doc! { "company_profile.names": identity_name(name), "status": "active" }, executor)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn company_filter_never_includes_other_party_roles() {
        let filter =
            company_filter(&CompanyListParams { keyword: Some("公司(甲)".into()), ..Default::default() });
        assert!(filter.contains_key("company_profile"));
        assert_eq!(
            filter.get_document("company_profile.names").unwrap().get_str("$regex").unwrap(),
            "公司\\(甲\\)"
        );
    }
}
