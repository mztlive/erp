//! 组织停用的外域未结业务检查由实际组合层提供。

use async_trait::async_trait;
use persistence_core::Executor;

/// 只读取阻止组织停用的未结业务事实，不修改业务归属或任务。
#[async_trait]
pub trait OrganizationBusinessPort: Send + Sync {
    /// 停用组织前向所属业务域确认该组织是否仍有待交接事项。存在未结业务时返回 `true`；查询失败直接返回错误，让停用流程终止。
    async fn has_unsettled_business(
        &self,
        org_unit_id: &str,
        executor: &mut dyn Executor,
    ) -> crate::Result<bool>;
}
