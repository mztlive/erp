use mongodb::bson::doc;
use mongodb::options::FindOptions;
use persistence_core::{Executor, PageResult, Pagination, QueryFilter, Result, mongo_ops};

use super::ProcurementResponsibilityRuleFilter;
use crate::entity::procurement_responsibility::{EnableStatus, ProcurementResponsibilityRule};

/// 采购责任规则集合的域查询。
#[allow(async_fn_in_trait)]
pub trait ProcurementResponsibilityRuleRepositoryExt {
    /// 分页查询采购责任规则。
    ///
    /// # 参数
    /// * `filter` - 规则列表筛选与分页条件
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回按优先级与创建时间稳定排序的当前页规则及总数。
    ///
    /// # 错误
    /// MongoDB 查询、计数或反序列化失败时返回错误。
    async fn search_procurement_responsibility_rules(
        &self,
        filter: &ProcurementResponsibilityRuleFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<ProcurementResponsibilityRule>>;

    /// 读取全部启用采购责任规则。
    ///
    /// # 参数
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回全部未删除且启用规则。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    async fn list_active_procurement_responsibility_rules(
        &self,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ProcurementResponsibilityRule>>;

    /// 按稳定 ID 读取采购责任规则。
    ///
    /// # 参数
    /// * `id` - 采购责任规则 ID
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回未删除规则；不存在时返回 `None`。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    async fn find_procurement_responsibility_rule(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<ProcurementResponsibilityRule>>;
}

impl ProcurementResponsibilityRuleRepositoryExt
    for persistence_core::Repository<'_, ProcurementResponsibilityRule>
{
    async fn search_procurement_responsibility_rules(
        &self,
        filter: &ProcurementResponsibilityRuleFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<ProcurementResponsibilityRule>> {
        let options = FindOptions::builder()
            .sort(doc! { "rule_type": 1, "created_at": 1, "id": 1 })
            .skip(filter.skip())
            .limit(filter.limit())
            .build();
        let items = mongo_ops::find_many(&self.collection(), filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;
        Ok(PageResult { items, total: total as i64 })
    }

    async fn list_active_procurement_responsibility_rules(
        &self,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ProcurementResponsibilityRule>> {
        self.find_many_sorted(
            doc! { "status": EnableStatus::Active.as_str() },
            doc! { "rule_type": 1, "created_at": 1, "id": 1 },
            executor,
        )
        .await
    }

    async fn find_procurement_responsibility_rule(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<ProcurementResponsibilityRule>> {
        self.find_by_id(id, executor).await
    }
}
