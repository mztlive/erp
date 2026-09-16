//! 客户经营质量双口径的有界销售单事实查询；授权范围在数据库过滤中交集执行。
use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use persistence_core::{Executor, Result, mongo_ops};

use crate::entity::sales_order::{CommercialStatus, SalesOrder};
use crate::repository::owned::SalesOrderRepository;

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
    /// 不接受客户端范围标识替代授权集合；空集合必定无结果。
    fn document(&self) -> Document {
        let mut filter = doc! { "deleted_at": 0_i64,
        "commercial_status": CommercialStatus::Effective.as_str(),
        "effective_at": { "$gte": self.from, "$lt": self.until } };
        let mut conditions = vec![self.authorized_scope.document()];
        match &self.customer_ids {
            None => {},
            Some(ids) if ids.is_empty() => conditions.push(doc! { "$expr": false }),
            Some(ids) => conditions.push(doc! { "customer_id": { "$in": ids } }),
        }
        filter.insert("$and", conditions);
        filter
    }
}

impl SalesOrderRepository<'_> {
    /// 有界读取期间正式销售单；调用方必须对超限结果整体拒绝。
    pub async fn quality_orders(
        &self,
        filter: &QualityOrderFilter,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SalesOrder>> {
        let options = FindOptions::builder()
            .limit((QUALITY_ORDER_LIMIT + 1) as i64)
            .sort(doc! { "effective_at": 1, "id": 1 })
            .build();
        mongo_ops::find_many(&self.collection().clone_with_type(), filter.document(), options, executor).await
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
