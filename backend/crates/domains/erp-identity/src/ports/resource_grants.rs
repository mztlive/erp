//! 授权发布所需的可信业务事实。
use async_trait::async_trait;
use persistence_core::Executor;

use crate::Result;
use crate::entity::resource_access::{GrantTarget, SourceUpdate};

/// 由组合根注入的业务事实校验；禁止 identity 直接读取业务集合。
#[async_trait]
pub trait ResourceGrantFacts: Send + Sync {
    /// 同事务校验目标真实存在且允许读取授权。
    /// # 参数
    /// targets 为全部目标；executor 为授权事务。
    /// # 返回
    /// 全部目标有效时成功。
    /// # 错误
    /// 缺失、删除、类型错配或读取失败时拒绝。
    async fn validate_targets(&self, targets: &[GrantTarget], executor: &mut dyn Executor) -> Result<()>;
    /// 发布前重验自动来源的规则、触发事实、正式分配接收人及当前事实版本。
    /// # 参数
    /// update 为完整来源结果；executor 为授权事务。
    /// # 返回
    /// 来源及完整目标与可信事实相符时成功；开票来源必须核验最终审批通过及申请、审批实例、执行任务关联。
    /// # 错误
    /// 停用规则、版本漂移、证据不足或关联冲突时拒绝；不能将失败解释为空集。
    async fn validate_source(&self, update: &SourceUpdate, executor: &mut dyn Executor) -> Result<()>;
}
