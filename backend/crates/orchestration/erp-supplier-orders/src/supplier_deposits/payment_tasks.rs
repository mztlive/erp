//! 资金事实应用后在同一事务同步原付款任务；具体财务编排由应用注入。

use async_trait::async_trait;
use persistence_core::Executor;

use crate::Result;

/// 原付款申请正式任务同步合同，不接受新的资金执行或外部请求。
#[async_trait]
pub trait OrdersPaymentTasks: Send + Sync {
    /// 使用调用方执行器重算原付款任务。
    ///
    /// `payment_request` 为原资金占用冻结的申请身份；`executor` 为当前根事务。
    ///
    /// 原任务完成或保持现有可执行状态。
    ///
    /// 保留实际校验、冲突、数据库及提交未知分类，调用方沿原规则处理。
    async fn sync(&self, payment_request: &str, executor: &mut dyn Executor) -> Result<()>;
}
