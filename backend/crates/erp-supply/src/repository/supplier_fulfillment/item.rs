//! 供应商履约明细查询：按订单批量读取明细。

use super::*;

impl<'a> SupplierFulfillmentItemRepository<'a> {
    /// 批量按供应商子订单查询履约明细（`$in` 一次取回，避免 N+1）。
    ///
    /// 明细随子订单同事务创建且创建后不可修改（§6.19），本方法供详情页与
    /// 结算/退款编排加载整单明细。
    ///
    /// # 参数
    /// * `order_ids` - 供应商子订单 ID 集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回命中集合对应的全部未删除履约明细。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_items_by_order_ids(
        &self,
        order_ids: &[SupplierFulfillmentOrderId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierFulfillmentItem>> {
        if order_ids.is_empty() {
            return Ok(Vec::new());
        }

        let mut items = self
            .find_many(
                doc! { "supplier_fulfillment_order_id": { "$in": ids_to_strings(order_ids) } },
                executor,
            )
            .await?;
        items.sort_by(|left, right| left.base.id.cmp(&right.base.id));
        Ok(items)
    }
}
