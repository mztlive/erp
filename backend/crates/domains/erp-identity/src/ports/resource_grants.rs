//! 授权发布所需的可信业务事实。
use async_trait::async_trait;
use persistence_core::Executor;

use crate::Result;
use crate::entity::resource_access::{GrantTarget, SourceUpdate};

/// 由组合根注入的业务事实校验；禁止 identity 直接读取业务集合。
#[async_trait]
pub trait ResourceGrantFacts: Send + Sync {
    /// 发布记录读取授权前，在原事务内确认每个目标都存在且可授权。缺失、删除、类型不符或读取失败时终止发布。
    async fn validate_targets(&self, targets: &[GrantTarget], executor: &mut dyn Executor) -> Result<()>;
    /// 应用自动授权前，沿原事务复核规则状态、业务证据、实际接收人、事实版本和完整目标。开票来源还须确认最终审批通过，以及申请、审批实例和执行任务的关联。证据不足、版本漂移或关联冲突时返回错误。
    async fn validate_source(&self, update: &SourceUpdate, executor: &mut dyn Executor) -> Result<()>;
}
