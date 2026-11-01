use application_core::AuditActor;
use erp_core::ids::SupplierApiConnectionId;
use erp_identity::{Permission, subject};
use erp_supply::entity::supplier_api::{
    SupplierApiConnection, SupplierConnectionAction, SupplierHealthCheckRun,
};
use erp_supply::repository::SupplierApiExt;
use erp_supply::service::supplier_api::context::action_permission;

use super::SupplierApiReadService;
use crate::Result;
pub(super) struct GovernanceContext {
    pub(super) health_runs: Vec<SupplierHealthCheckRun>,
}
impl SupplierApiReadService {
    /// 读取手动连接诊断记录，不加载人工能力和采购确认。
    ///
    /// # 参数
    /// * `connection` - 已加载的供应连接。
    /// * `executor` - 调用方执行器。
    ///
    /// # 返回
    /// 返回连接诊断记录。
    ///
    /// # 错误
    /// 治理事实或后台任务计数读取失败时返回对应错误。
    pub(super) async fn governance_context(
        &self,
        connection: &SupplierApiConnection,
        executor: &mut dyn persistence_core::Executor,
    ) -> Result<GovernanceContext> {
        let health_runs = self
            .db
            .supplier_api()
            .recent_health_runs(&SupplierApiConnectionId::new(&connection.base.id), 50, executor)
            .await?;
        Ok(GovernanceContext { health_runs })
    }
    /// 判断操作人是否持有该连接动作要求的权限。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人。
    /// * `action` - 供应连接动作。
    ///
    /// # 返回
    /// 持有对应权限时返回 `true`。未注入 RBAC 时返回 `false`。
    ///
    /// # 错误
    /// 权限码无法解析或 RBAC 执行失败时返回对应错误。
    pub(super) async fn has_action_permission(
        &self,
        actor: &AuditActor,
        action: SupplierConnectionAction,
    ) -> Result<bool> {
        self.has_permission(actor, action_permission(action)).await
    }
    /// 判断操作人是否持有给定权限码。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人。
    /// * `permission` - 权限码。
    ///
    /// # 返回
    /// 持有权限时返回 `true`。未注入 RBAC 时返回 `false`，不把它当成授权通过。
    ///
    /// # 错误
    /// 权限码无法解析或 RBAC 执行失败时返回对应错误。
    pub(super) async fn has_permission(&self, actor: &AuditActor, permission: &str) -> Result<bool> {
        let Some(rbac) = self.rbac.as_ref() else {
            return Ok(false);
        };
        let permission = Permission::parse(permission)?;
        Ok(rbac.enforce(&subject(actor.kind(), actor.id()), &permission).await?)
    }
}
