//! 客户经营质量双口径的有界销售单事实查询；授权范围在数据库过滤中交集执行。
use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use persistence_core::{Executor, Repository, aggregate};

use super::scope::SalesReadScope;
use crate::Result;
use crate::entity::sales_order::{CommercialStatus, SalesOrder};

/// 双口径单次最多读取的销售单数；额外读取一条用于拒绝截断统计。
pub const QUALITY_ORDER_LIMIT: usize = 10_000;

/// 生效日期区间为左闭右开，客户集合与显式筛选取交集。
pub struct QualityOrderFilter {
    pub from: i64,
    pub until: i64,
    pub customer_ids: Option<Vec<String>>,
}

impl QualityOrderFilter {
    /// 将期间与业务筛选前置于完整销售授权。
    /// # 参数
    /// scope 为本阶段不可缺失的完整授权凭证。
    /// # 返回
    /// 未分页的业务筛选与授权阶段。
    /// # 错误
    /// 不执行 I/O；仓储须在执行前后复验。
    pub fn stages(&self, scope: &SalesReadScope) -> Vec<Document> {
        scope.lookup_after(self.business_filter())
    }

    /// 期间与业务筛选先于授权关联，不能把用户条件作为授权依据。
    fn business_filter(&self) -> Document {
        let mut filter = doc! { "deleted_at": 0_i64,
        "commercial_status": CommercialStatus::Effective.as_str(),
        "effective_at": { "$gte": self.from, "$lt": self.until } };
        let mut conditions = Vec::new();
        match &self.customer_ids {
            None => {},
            Some(ids) if ids.is_empty() => conditions.push(doc! { "$expr": false }),
            Some(ids) => conditions.push(doc! { "customer_id": { "$in": ids } }),
        }
        if !conditions.is_empty() {
            filter.insert("$and", conditions);
        }
        filter
    }
}

/// 客户经营质量双口径的销售单事实查询。
#[allow(async_fn_in_trait)]
pub trait SalesOrderRepositoryQualityExt {
    /// 有界读取期间正式销售单；调用方必须对超限结果整体拒绝。
    ///
    /// # 参数
    /// * `filter` - 生效区间与客户集合。
    /// * `scope` - 当前读取阶段的完整销售授权。
    /// * `executor` - 调用方执行器。
    ///
    /// # 返回
    /// 最多返回 `QUALITY_ORDER_LIMIT + 1` 条，按 `effective_at`、`id` 升序；多出的一条表示超限。
    ///
    /// # 错误
    /// 数据库读取或反序列化失败时返回仓储错误。
    async fn quality_orders(
        &self,
        filter: &QualityOrderFilter,
        scope: &SalesReadScope,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SalesOrder>>;
}

impl SalesOrderRepositoryQualityExt for Repository<'_, SalesOrder> {
    async fn quality_orders(
        &self,
        filter: &QualityOrderFilter,
        scope: &SalesReadScope,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SalesOrder>> {
        scope.ensure_current(executor)?;

        let options = FindOptions::builder()
            .limit((QUALITY_ORDER_LIMIT + 1) as i64)
            .sort(doc! { "effective_at": 1, "id": 1 })
            .build();
        let rows = aggregate::find(
            &self.collection().clone_with_type(),
            filter.business_filter(),
            options,
            scope.lookup_after(filter.business_filter()),
            executor,
        )
        .await?;

        scope.ensure_current(executor)?;
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_customer_set_matches_nothing() {
        let filter =
            QualityOrderFilter { from: 1, until: 2, customer_ids: Some(Vec::new()) }.business_filter();
        let and = filter.get_array("$and").unwrap();
        assert_eq!(and.len(), 1);
        assert_eq!(and[0].as_document().unwrap(), &doc! { "$expr": false });
    }

    #[test]
    fn explicit_customers_intersect_authorization() {
        let filter = QualityOrderFilter { from: 1, until: 2, customer_ids: Some(vec!["c-1".into()]) }
            .business_filter();
        let and = filter.get_array("$and").unwrap();
        assert_eq!(and.len(), 1);
        assert_eq!(and[0].as_document().unwrap(), &doc! { "customer_id": { "$in": ["c-1"] } });
        assert_eq!(filter.get_str("commercial_status").unwrap(), CommercialStatus::Effective.as_str());
    }
}
