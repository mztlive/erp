//! 采购责任规则仓储查询。

use entities::catalog::EnableStatus;
use entities::procurement_responsibility::{
    ProcurementResponsibilityRule, ProcurementResponsibilityRuleType,
};
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;

use super::{PageResult, Pagination, QueryFilter, Repository};
use crate::executor::Executor;
use crate::{mongo_ops, Result};

/// 采购责任规则列表筛选条件。
#[derive(Debug, Clone)]
pub struct ProcurementResponsibilityRuleFilter {
    /// 规则类型；`None` 表示不筛选。
    pub rule_type: Option<ProcurementResponsibilityRuleType>,
    /// 负责人账号 ID；`None` 表示不筛选。
    pub owner_user_id: Option<String>,
    /// 启停状态；`None` 表示不筛选。
    pub status: Option<EnableStatus>,
    /// 页码，从 1 开始。
    pub page: u64,
    /// 每页条数。
    pub page_size: u32,
}

impl QueryFilter for ProcurementResponsibilityRuleFilter {
    /// 构造包含软删除约束的 MongoDB 查询文档。
    ///
    /// # 返回
    /// 返回规则类型、负责人及状态筛选文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(rule_type) = self.rule_type {
            filter.insert("rule_type", rule_type.as_str());
        }
        if let Some(owner_user_id) = self.owner_user_id.as_deref() {
            filter.insert("owner_user_id", owner_user_id);
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        filter
    }
}

impl Pagination for ProcurementResponsibilityRuleFilter {
    /// 返回页码与每页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)`。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, ProcurementResponsibilityRule> {
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
    pub async fn search_procurement_responsibility_rules(
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
        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

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
    pub async fn list_active_procurement_responsibility_rules(
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
}
