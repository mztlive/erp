//! 供应商履约动作查询：动作与动作行按订单读取。

#![allow(async_fn_in_trait)]

use super::*;

/// 供应商履约动作仓储的域查询。
#[allow(async_fn_in_trait)]
pub trait SupplierOrderActionRepositoryExt {
    /// 按履约订单读取动作，按创建时间和主键倒序排列。
    ///
    /// # 参数
    /// * `order_id` - 供应商履约订单主键
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回最新动作在前的动作集合。
    ///
    /// # 错误
    /// MongoDB 查询或游标读取失败时返回错误。
    async fn list_by_order_newest(
        &self,
        order_id: &SupplierFulfillmentOrderId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierOrderAction>>;

    /// 按履约订单和动作类型读取动作，按创建时间和主键倒序排列。
    ///
    /// # 参数
    /// * `order_id` - 供应商履约订单主键
    /// * `action_type` - 供应商动作类型
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回最新动作在前的动作集合。
    ///
    /// # 错误
    /// MongoDB 查询或游标读取失败时返回错误。
    async fn list_by_order_and_type_newest(
        &self,
        order_id: &SupplierFulfillmentOrderId,
        action_type: crate::entity::supplier_fulfillment::SupplierOrderActionType,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierOrderAction>>;

    /// 读取履约订单最近一次指定类型动作。
    ///
    /// # 参数
    /// * `order_id` - 供应商履约订单主键
    /// * `action_type` - 供应商动作类型
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回最近动作；不存在时返回 `None`。
    ///
    /// # 错误
    /// MongoDB 查询或游标读取失败时返回错误。
    async fn latest_by_order_and_type(
        &self,
        order_id: &SupplierFulfillmentOrderId,
        action_type: crate::entity::supplier_fulfillment::SupplierOrderActionType,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierOrderAction>>;

    /// 按对供应商动作幂等键查找唯一动作。
    ///
    /// 唯一性由 `uk_supplier_order_actions_idempotency_key` 唯一索引保证；人工重放
    /// 与网络超时恢复继续使用原幂等键（§6.19），本方法返回既有动作避免重复创建。
    ///
    /// # 参数
    /// * `idempotency_key` - 对供应商动作幂等键
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的未删除动作；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    async fn find_by_idempotency_key(
        &self,
        idempotency_key: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierOrderAction>>;
}

impl SupplierOrderActionRepositoryExt for SupplierOrderActionRepository<'_> {
    async fn list_by_order_newest(
        &self,
        order_id: &SupplierFulfillmentOrderId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierOrderAction>> {
        self.find_many_sorted(
            doc! { "supplier_fulfillment_order_id": order_id.to_string() },
            doc! { "created_at": -1, "id": -1 },
            executor,
        )
        .await
    }

    async fn list_by_order_and_type_newest(
        &self,
        order_id: &SupplierFulfillmentOrderId,
        action_type: crate::entity::supplier_fulfillment::SupplierOrderActionType,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierOrderAction>> {
        self.find_many_sorted(
            doc! {
                "supplier_fulfillment_order_id": order_id.to_string(),
                "action_type": action_type.as_str(),
            },
            doc! { "created_at": -1, "id": -1 },
            executor,
        )
        .await
    }

    async fn latest_by_order_and_type(
        &self,
        order_id: &SupplierFulfillmentOrderId,
        action_type: crate::entity::supplier_fulfillment::SupplierOrderActionType,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierOrderAction>> {
        Ok(self.list_by_order_and_type_newest(order_id, action_type, executor).await?.into_iter().next())
    }

    async fn find_by_idempotency_key(
        &self,
        idempotency_key: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierOrderAction>> {
        self.find_one(doc! { "idempotency_key": idempotency_key }, executor).await
    }
}

/// 供应商履约动作行仓储的域查询。
#[allow(async_fn_in_trait)]
pub trait SupplierOrderActionLineRepositoryExt {
    /// 批量按动作头查询动作行（`$in` 一次取回，避免 N+1）。
    ///
    /// 动作行随动作头同事务创建且创建后不可修改（§6.19），本方法供详情页
    /// 加载一次取消/退款实际提交给供应商的范围。
    ///
    /// # 参数
    /// * `action_ids` - 供应商动作 ID 集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回命中集合对应的全部未删除动作行。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    async fn find_lines_by_action_ids(
        &self,
        action_ids: &[SupplierOrderActionId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierOrderActionLine>>;
}

impl SupplierOrderActionLineRepositoryExt for SupplierOrderActionLineRepository<'_> {
    async fn find_lines_by_action_ids(
        &self,
        action_ids: &[SupplierOrderActionId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierOrderActionLine>> {
        if action_ids.is_empty() {
            return Ok(Vec::new());
        }

        let mut lines = self
            .find_many(doc! { "supplier_order_action_id": { "$in": ids_to_strings(action_ids) } }, executor)
            .await?;
        lines.sort_by(|left, right| left.base.id.cmp(&right.base.id));
        Ok(lines)
    }
}
