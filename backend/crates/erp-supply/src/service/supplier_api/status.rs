//! 连接启停的本域上下文与规则，任务计数由调用方注入。
use persistence_core::Executor;

use super::SupplierApiService;
use super::context::ensure_version;
use crate::entity::supplier_api::*;
use crate::repository::SupplierApiExt;
use crate::repository::supplier_api::SupplierApiGovernanceData;
use crate::{Error, Result};
impl SupplierApiService {
    /// 先读取连接和原本域治理快照；后台任务计数必须随后读取。
    ///
    /// # 参数
    /// * `id` - 供应商连接主键。
    /// * `expected_version` - 调用方持有的连接版本。
    /// * `executor` - 调用方事务的执行器。
    ///
    /// # 返回
    /// 返回版本匹配的连接，以及能力、确认、健康运行和影响事实组成的治理快照。
    ///
    /// # 错误
    /// 连接不存在、版本不一致或治理快照读取失败时返回对应错误。
    pub async fn prepare_status_target(
        &self,
        id: &str,
        expected_version: u64,
        executor: &mut dyn Executor,
    ) -> Result<(SupplierApiConnection, SupplierApiGovernanceData)> {
        let connection = self.load_connection(id, executor).await?;
        ensure_version(connection.base.version, expected_version)?;
        let context =
            self.db.supplier_api().governance_data(&SupplierApiConnectionId::new(id), 50, executor).await?;
        Ok((connection, context))
    }
    /// 使用完整权威影响按原首个 blocker 校验，再推进连接 CAS。
    ///
    /// # 参数
    /// * `connection` - 待启停的连接。
    /// * `context` - 本域治理快照。
    /// * `active_sync_jobs` - 调用方随后读到的活跃同步任务数。
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
        context: &SupplierApiGovernanceData,
        active_sync_jobs: u64,
        action: SupplierConnectionAction,
        actor_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let governance = SupplierConnectionGovernance {
            connection,
            capabilities: &context.capabilities,
            confirmations: &context.confirmations,
            health_runs: &context.health_runs,
        };
        let blockers =
            governance.blockers(action, context.owned_impact.with_active_sync_jobs(active_sync_jobs), true);
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
