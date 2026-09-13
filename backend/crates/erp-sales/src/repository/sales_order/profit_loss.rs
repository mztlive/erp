//! 实际盈亏读取的有界销售单事实查询；权限范围在数据库过滤中交集执行。
use crate::entity::sales_order::{CommercialStatus, FulfillmentProgress, SalesAttribution};
use crate::repository::owned::SalesOrderRepository;
use erp_core::common::time::Instant;
use mongodb::{
    bson::{doc, Document},
    options::FindOptions,
};
use persistence_core::{mongo_ops, Executor, Result};
use serde::Deserialize;

/// 报表单次最多读取的销售单数；额外读取一条用于拒绝截断统计。
pub const PROFIT_LOSS_ORDER_LIMIT: usize = 10_000;

/// 订单统计所需的稳定事实，不携带草稿和联系方式。
#[derive(Debug, Clone, Deserialize)]
pub struct ProfitLossOrder {
    pub id: String,
    pub order_no: String,
    pub version: u64,
    pub customer_id: String,
    pub current_revision_id: Option<String>,
    pub effective_at: Option<Instant>,
    pub fulfillment_progress: FulfillmentProgress,
    pub attribution: Option<SalesAttribution>,
}

/// 生效日期区间为左闭右开，客户授权集合与显式筛选取交集。
pub struct ProfitLossOrderFilter {
    pub from: i64,
    pub until: i64,
    pub customer_id: Option<String>,
    pub sales_order_id: Option<String>,
    pub authorized_scope: super::scope::SalesReadScope,
}
impl ProfitLossOrderFilter {
    /// 不接受客户端范围标识替代授权集合；空集合必定无结果。
    fn document(&self) -> Document {
        let mut filter = doc! { "deleted_at": 0_i64, "business_type": "GOODS_SERVICE",
        "commercial_status": CommercialStatus::Effective.as_str(),
        "effective_at": { "$gte": self.from, "$lt": self.until } };
        let mut conditions = vec![self.authorized_scope.document()];
        if let Some(id) = &self.customer_id {
            conditions.push(doc! { "customer_id": id });
        }
        filter.insert("$and", conditions);
        if let Some(id) = &self.sales_order_id {
            filter.insert("id", id);
        }
        filter
    }
}
impl SalesOrderRepository<'_> {
    /// 有界读取正式非卡券销售单；调用方必须对超限结果整体拒绝。
    pub async fn profit_loss_orders(
        &self,
        filter: &ProfitLossOrderFilter,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ProfitLossOrder>> {
        let options = FindOptions::builder()
            .limit((PROFIT_LOSS_ORDER_LIMIT + 1) as i64)
            .sort(doc! { "effective_at": 1, "id": 1 })
            .projection(
                doc! { "id": 1, "order_no": 1, "customer_id": 1, "current_revision_id": 1,
                "effective_at": 1, "fulfillment_progress": 1, "attribution": 1, "version": 1 },
            )
            .build();
        mongo_ops::find_many(
            &self.collection().clone_with_type(),
            filter.document(),
            options,
            executor,
        )
        .await
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_customer_never_overrides_empty_authorization() {
        let filter = ProfitLossOrderFilter {
            from: 1,
            until: 2,
            customer_id: Some("other".into()),
            sales_order_id: None,
            authorized_scope: super::super::scope::SalesReadScope::default(),
        }
        .document();
        assert_eq!(filter.get_array("$and").unwrap().len(), 2);
        assert_eq!(
            filter.get_array("$and").unwrap()[0].as_document().unwrap(),
            &doc! { "$expr": false }
        );
        assert_eq!(filter.get_str("business_type").unwrap(), "GOODS_SERVICE");
    }
}
