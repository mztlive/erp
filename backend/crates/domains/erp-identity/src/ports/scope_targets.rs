//! 范围配置中的外部身份存在性由所属业务域提供，身份域不读取跨域集合。
use async_trait::async_trait;
use persistence_core::Executor;

use crate::Result;
use crate::access_control::ScopeDimension;

/// 结算主体及仓库目标的配置校验；只校验身份，不提供正向授权。
#[async_trait]
pub trait ScopeTargetPort: Send + Sync {
    /// 保存人员范围配置前，在原事务内向所属业务域批量确认结算主体或仓库 ID 存在。目标缺失、维度未接入或读取失败时拒绝配置。
    async fn validate_targets(
        &self,
        dimension: ScopeDimension,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<()>;
}
