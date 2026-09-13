//! 组织停用的外域未结业务检查由实际组合层提供。

use async_trait::async_trait;
use persistence_core::Executor;

/// 只读取阻止组织停用的未结业务事实，不修改业务归属或任务。
#[async_trait]
pub trait OrganizationBusinessPort: Send + Sync {
    /// 判断组织是否仍存在需要交接的未结业务。
    ///
    /// # 错误
    /// 外域查询失败必须拒绝停用，禁止将失败解释为空业务。
    async fn has_unsettled_business(
        &self,
        org_unit_id: &str,
        executor: &mut dyn Executor,
    ) -> crate::Result<bool>;
}
