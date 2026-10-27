//! 自动下单开关的版本校验与持久化，不读取人工能力或活动业务计数。
use persistence_core::Executor;

use super::SupplierApiService;
use super::context::ensure_version;
use crate::entity::supplier_api::*;
use crate::{Error, Result};
impl SupplierApiService {
    /// 读取并校验连接版本。
    ///
    /// # 参数
    /// * `id` - 供应商连接主键。
    /// * `expected_version` - 调用方持有的连接版本。
    /// * `executor` - 调用方事务的执行器。
    ///
    /// # 返回
    /// 返回版本匹配的连接。
    ///
    /// # 错误
    /// 连接不存在、版本不一致或治理快照读取失败时返回对应错误。
    pub async fn prepare_status_target(
        &self,
        id: &str,
        expected_version: u64,
        executor: &mut dyn Executor,
    ) -> Result<SupplierApiConnection> {
        let connection = self.load_connection(id, executor).await?;
        ensure_version(connection.base.version, expected_version)?;
        Ok(connection)
    }
    /// 校验当前配置检查结果并保存自动下单开关。
    ///
    /// # 参数
    /// * `connection` - 待启停的连接。
    /// * `action` - 启用或停用。
    /// * `actor_id` - 操作人。
    /// * `executor` - 调用方事务的执行器。
    ///
    /// # 返回
    /// 无返回值。连接状态已按 CAS 写回。
    ///
    /// # 错误
    /// 首个治理阻塞返回 `BusinessLogicError`；动作不是启用或停用时返回 `Internal`。状态迁移或连接 CAS 失败时返回对应错误。
    pub async fn apply_status_change(
        &self,
        connection: &mut SupplierApiConnection,
        action: SupplierConnectionAction,
        actor_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let governance = SupplierConnectionGovernance {
            connection,
            capabilities: &[],
            confirmations: &[],
            health_runs: &[],
        };
        let blockers = governance.blockers(action, Default::default(), true);
        if let Some(blocker) = blockers.into_iter().next() {
            return Err(Error::BusinessLogicError(blocker.message));
        }
        match action {
            SupplierConnectionAction::Enable => connection.enable(actor_id),
            SupplierConnectionAction::Disable => connection.disable(actor_id),
            _ => return Err(Error::Internal("状态命令分派错误".to_string())),
        }
        self.persist_connection(connection, executor).await
    }
}
