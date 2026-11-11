//! 客户经营质量双口径的有界销售单事实查询；授权范围在数据库过滤中交集执行。
use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use persistence_core::{Executor, Repository, aggregate};

use crate::Result;
use crate::entity::sales_order::{CommercialStatus, SalesOrder};

/// 双口径单次最多读取的销售单数；额外读取一条用于拒绝截断统计。
pub const QUALITY_ORDER_LIMIT: usize = 10_000;

/// 生效日期区间为左闭右开，客户集合与显式筛选取交集。
pub struct QualityOrderFilter {
    pub from: i64,
    pub until: i64,
    pub customer_ids: Option<Vec<String>>,
    pub authorized_scope: super::scope::SalesReadScope,
}

impl QualityOrderFilter {
    /// 返回期间、客户筛选与完整销售授权的查询前缀。
    /// # 参数
    /// 无。
    /// # 返回
    /// 不含排序及数量上限的阶段，供跨域读取继续求交来源授权。
    /// # 错误
    /// 无；执行方须在原执行器上复验授权凭证。
    pub fn stages(&self) -> Vec<Document> {
        self.authorized_scope.lookup_after(self.business_filter())
    }

    /// 不接受客户端范围标识替代授权集合；空集合必定无结果。
    fn document(&self) -> Document {
        let mut filter = self.business_filter();
        let mut conditions = filter.get_array("$and").cloned().unwrap_or_default();
        conditions.insert(0, self.authorized_scope.document().into());
        filter.insert("$and", conditions);
        filter
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
    /// * `filter` - 生效区间、客户集合与授权范围。
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
        executor: &mut dyn Executor,
    ) -> Result<Vec<SalesOrder>>;
}

impl SalesOrderRepositoryQualityExt for Repository<'_, SalesOrder> {
    async fn quality_orders(
        &self,
        filter: &QualityOrderFilter,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SalesOrder>> {
        filter.authorized_scope.validate(executor).await?;
        let options = FindOptions::builder()
            .limit((QUALITY_ORDER_LIMIT + 1) as i64)
            .sort(doc! { "effective_at": 1, "id": 1 })
            .build();
        let rows = aggregate::find(
            &self.collection().clone_with_type(),
            filter.document(),
            options,
            filter.stages(),
            executor,
        )
        .await?;
        filter.authorized_scope.validate(executor).await?;
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_authorization_or_customer_set_matches_nothing() {
        let filter = QualityOrderFilter {
            from: 1,
            until: 2,
            customer_ids: Some(Vec::new()),
            authorized_scope: super::super::scope::SalesReadScope::default(),
        }
        .document();
        let and = filter.get_array("$and").unwrap();
        assert_eq!(and.len(), 2);
        assert_eq!(and[0].as_document().unwrap(), &doc! { "$expr": false });
        assert_eq!(and[1].as_document().unwrap(), &doc! { "$expr": false });
    }

    #[test]
    fn explicit_customers_intersect_authorization() {
        let filter = QualityOrderFilter {
            from: 1,
            until: 2,
            customer_ids: Some(vec!["c-1".into()]),
            authorized_scope: super::super::scope::SalesReadScope::default(),
        }
        .document();
        let and = filter.get_array("$and").unwrap();
        assert_eq!(and.len(), 2);
        assert_eq!(and[1].as_document().unwrap(), &doc! { "customer_id": { "$in": ["c-1"] } });
        assert_eq!(filter.get_str("commercial_status").unwrap(), CommercialStatus::Effective.as_str());
    }
}
