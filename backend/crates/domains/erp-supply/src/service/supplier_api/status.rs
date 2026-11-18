//! 自动下单开关的版本校验与持久化，不读取人工能力或活动业务计数。
use persistence_core::Executor;

use super::SupplierApiService;
use super::context::ensure_version;
use crate::entity::supplier_api::*;
use crate::{Error, Result};
impl SupplierApiService {
    /// 自动下单开关变更前读取目标连接并核对客户端期望版本；连接缺失、读取失败或版本变化时停止。
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
    /// 在调用方执行器内检查启停治理条件并 CAS 保存自动下单开关，不采用旧人工能力声明。治理阻塞返回业务错
    /// 误，非启停动作返回内部错误；状态迁移或 CAS 失败向上传播。
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
