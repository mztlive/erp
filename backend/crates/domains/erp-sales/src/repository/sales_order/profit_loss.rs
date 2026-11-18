//! 实际盈亏读取的有界销售单事实查询；权限范围在数据库过滤中交集执行。
use erp_core::common::time::Instant;
use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use persistence_core::{Executor, Repository, aggregate};
use serde::Deserialize;

use super::scope::SalesReadScope;
use crate::Result;
use crate::entity::sales_order::{CommercialStatus, FulfillmentProgress, SalesAttribution, SalesOrder};

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
#[derive(Debug, Clone, Default)]
pub struct ProfitLossOrderFilter {
    pub from: i64,
    pub until: i64,
    pub customer_id: Option<String>,
    pub sales_order_id: Option<String>,
}
impl ProfitLossOrderFilter {
    /// 期间与业务筛选先于授权关联，不能把用户条件作为授权依据。
    fn business_filter(&self) -> Document {
        let mut filter = doc! { "deleted_at": 0_i64, "business_type": "GOODS_SERVICE",
        "commercial_status": CommercialStatus::Effective.as_str(),
        "effective_at": { "$gte": self.from, "$lt": self.until } };
        let mut conditions = Vec::new();
        if let Some(id) = &self.customer_id {
            conditions.push(doc! { "customer_id": id });
        }
        if !conditions.is_empty() {
            filter.insert("$and", conditions);
        }
        if let Some(id) = &self.sales_order_id {
            filter.insert("id", id);
        }
        filter
    }
}
/// 实际盈亏读取的有界销售单事实查询。
#[allow(async_fn_in_trait)]
pub trait SalesOrderRepositoryProfitLossExt {
    /// 有界读取正式非卡券销售单；调用方必须对超限结果整体拒绝。
    ///
    /// # 参数
    /// * `filter` - 生效区间、客户与销售单筛选。
    /// * `scope` - 当前读取阶段的完整销售授权。
    /// * `executor` - 调用方执行器。
    ///
    /// # 返回
    /// 最多返回 `PROFIT_LOSS_ORDER_LIMIT + 1` 条，按 `effective_at`、`id` 升序；多出的一条表示超限。
    ///
    /// # 错误
    /// 数据库读取或反序列化失败时返回仓储错误。
    async fn profit_loss_orders(
        &self,
        filter: &ProfitLossOrderFilter,
        scope: &SalesReadScope,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ProfitLossOrder>>;
}

impl SalesOrderRepositoryProfitLossExt for Repository<'_, SalesOrder> {
    async fn profit_loss_orders(
        &self,
        filter: &ProfitLossOrderFilter,
        scope: &SalesReadScope,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ProfitLossOrder>> {
        scope.ensure_current(executor)?;

        let options = FindOptions::builder()
            .limit((PROFIT_LOSS_ORDER_LIMIT + 1) as i64)
            .sort(doc! { "effective_at": 1, "id": 1 })
            .projection(doc! { "id": 1, "order_no": 1, "customer_id": 1, "current_revision_id": 1,
            "effective_at": 1, "fulfillment_progress": 1, "attribution": 1, "version": 1 })
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
    fn explicit_customer_restricts_business_filter() {
        let filter = ProfitLossOrderFilter {
            from: 1,
            until: 2,
            customer_id: Some("other".into()),
            sales_order_id: None,
        }
        .business_filter();
        assert_eq!(filter.get_array("$and").unwrap().len(), 1);
        assert_eq!(
            filter.get_array("$and").unwrap()[0].as_document().unwrap(),
            &doc! { "customer_id": "other" }
        );
        assert_eq!(filter.get_str("business_type").unwrap(), "GOODS_SERVICE");
    }
}
