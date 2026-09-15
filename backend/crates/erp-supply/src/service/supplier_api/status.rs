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
        if let Some(blocker) = blockers.first() {
            return Err(Error::BusinessLogicError(blocker.message.clone()));
        }
        match action {
            SupplierConnectionAction::Enable => connection.enable(actor_id),
            SupplierConnectionAction::Disable => connection.disable(actor_id),
            _ => return Err(Error::Internal("状态命令分派错误".to_string())),
        }
        self.persist_connection(connection, executor).await
    }
}
