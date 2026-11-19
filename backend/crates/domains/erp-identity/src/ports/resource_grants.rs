//! 授权发布所需的可信业务事实。
use async_trait::async_trait;
use persistence_core::Executor;

use crate::Result;
use crate::entity::resource_access::event::GrantEvent;
use crate::entity::resource_access::{GrantTarget, SourceUpdate};

/// 由组合根注入的可信业务事实；identity 不直接读取业务集合。
#[async_trait]
pub trait ResourceGrantFacts: Send + Sync {
    /// 发布记录读取授权前，在原事务内确认每个目标都存在且可授权。缺失、删除、类型不符或读取失败时终止发布。
    async fn validate_targets(&self, targets: &[GrantTarget], executor: &mut dyn Executor) -> Result<()>;
    /// 沿调用方执行器核验事件的正式业务证据、实际接收人及事实版本，返回该自动来源的完整结果。
    /// 开票来源还须确认最终审批通过及申请、审批实例和执行任务的关联；证据不足、版本回退或关联冲突时返回错误。
    async fn compute_source(&self, event: &GrantEvent, executor: &mut dyn Executor) -> Result<SourceUpdate>;
}
