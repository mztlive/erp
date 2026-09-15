//! 范围配置中的外部身份存在性由所属业务域提供，身份域不读取跨域集合。
use crate::{access_control::ScopeDimension, Result};
use async_trait::async_trait;
use persistence_core::Executor;

/// 结算主体及仓库目标的配置校验；只校验身份，不提供正向授权。
#[async_trait]
pub trait ScopeTargetPort: Send + Sync {
    /// 批量核验显式目标。
    ///
    /// # 参数
    /// * `dimension` - 已注册的目标身份维度
    /// * `ids` - 不超过模型上限的目标 ID
    /// * `executor` - 范围配置原事务
    /// # 返回
    /// 全部目标在所属领域存在时成功。
    /// # 错误
    /// 目标缺失、维度未接线及仓储错误必须拒绝配置。
    async fn validate_targets(
        &self,
        dimension: ScopeDimension,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<()>;
}
