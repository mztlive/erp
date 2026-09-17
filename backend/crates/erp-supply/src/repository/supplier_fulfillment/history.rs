//! 供应商履约状态历史查询：按订单时间线读取。

use super::*;

impl<'a> SupplierOrderStatusHistoryRepository<'a> {
    /// 按履约订单读取状态历史，按发生时间和主键升序排列。
    ///
    /// # 参数
    /// * `order_id` - 供应商履约订单主键
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回按业务发生顺序排列的状态历史。
    ///
    /// # 错误
    /// MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_by_order_chronological(
        &self,
        order_id: &SupplierFulfillmentOrderId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierOrderStatusHistory>> {
        self.find_many_sorted(
            doc! { "supplier_fulfillment_order_id": order_id.to_string() },
            doc! { "occurred_at": 1, "id": 1 },
            executor,
        )
        .await
    }

    /// 按「连接 + 外部事件 ID」查找状态历史（回调幂等判定）。
    ///
    /// 唯一性由 `uk_supplier_order_status_histories_connection_event` 唯一索引保证
    /// （§6.19：回调幂等唯一，避免同一供应商的不同连接或账号合法复用外部事件号）。
    ///
    /// # 参数
    /// * `connection_id` - 供应商 API 连接
    /// * `external_event_id` - 外部事件 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的未删除状态历史；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_connection_and_event(
        &self,
        connection_id: &SupplierApiConnectionId,
        external_event_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierOrderStatusHistory>> {
        self.find_one(
            doc! {
                "connection_id": connection_id.to_string(),
                "external_event_id": external_event_id,
            },
            executor,
        )
        .await
    }
}
