//! 来源观察应用的生产步骤；取消完成只推进检查点，不进入发货写段。

use async_trait::async_trait;
use erp_core::common::time::Instant;
use erp_supply::entity::supplier_fulfillment::{CancelStatus, SupplierFulfillmentOrder};
use erp_supply::order_tracking::{OrderObservation, OrderTracking};
use persistence_core::Executor;

use crate::Result;

/// 当前观察应用的领域步骤，不提供任意仓储读写接口。
#[async_trait]
pub(super) trait ObservationApplication: Send + Sync {
    /// 读取同一执行身份的正式供应商订单以核对取消。
    /// # 参数
    /// 原执行身份及调用方事务执行器。
    /// # 返回
    /// 当前正式订单。
    /// # 错误
    /// 缺失或读取失败时拒绝。
    async fn cancelled_order(
        &self,
        order_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<SupplierFulfillmentOrder>;
    /// 应用普通订单的真实包裹及必要 ERP 发货。
    /// # 参数
    /// 已核验的检查点、来源观察和调用方事务执行器。
    /// # 返回
    /// 是否已经结束常规轮询。
    /// # 错误
    /// 数量、来源或当前执行门槛不满足时拒绝。
    async fn deliveries(
        &self,
        tracking: &OrderTracking,
        observation: &OrderObservation,
        executor: &mut dyn Executor,
    ) -> Result<bool>;
    /// 保存业务应用检查点，与前置业务步骤共用事务。
    /// # 参数
    /// 已完成领域变更的检查点和同一事务执行器。
    /// # 返回
    /// 保存成功。
    /// # 错误
    /// 并发或持久化失败时返回错误。
    async fn checkpoint(&self, tracking: &mut OrderTracking, executor: &mut dyn Executor) -> Result<()>;
}

/// 执行同一事务内的真实应用顺序，可信取消不会生成发货或资金写入。
/// # 参数
/// 应用步骤、当前检查点、来源观察、ERP 时刻及调用方事务执行器。
/// # 返回
/// 来源观察和业务步骤成功后才完成检查点。
/// # 错误
/// 身份、取消资格或任何应用步骤失败时停止，不保存成功检查点。
pub(super) async fn apply_current_observation(
    application: &impl ObservationApplication,
    tracking: &mut OrderTracking,
    observation: &OrderObservation,
    now: Instant,
    executor: &mut dyn Executor,
) -> Result<()> {
    tracking.ensure_current_observation(observation)?;
    let complete = if observation.cancellation == Some(CancelStatus::Canceled) {
        let order = application.cancelled_order(&tracking.base.id, executor).await?;
        observation.ensure_safe_cancellation(&order)?;
        true
    } else {
        application.deliveries(tracking, observation, executor).await?
    };
    tracking.record_applied(observation, complete, now)?;
    application.checkpoint(tracking, executor).await
}

#[cfg(test)]
mod tests;
