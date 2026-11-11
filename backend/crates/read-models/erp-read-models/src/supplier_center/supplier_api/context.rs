use application_core::AuditActor;
use erp_core::Result as CoreResult;
use erp_core::ids::SupplierApiConnectionId;
use erp_identity::{AuthorizationRegistry, AuthorizationService, Permission, PermissionSet};
use erp_supply::entity::supplier_api::{
    SupplierApiConnection, SupplierConnectionAction, SupplierHealthCheckRun,
};
use erp_supply::repository::SupplierApiExt;
use erp_supply::service::supplier_api::context::action_permission;
use persistence_core::NoTransaction;

use super::SupplierApiReadService;
use crate::Result;

pub(super) const REFERENCE_METADATA_PERMISSION: &str = "supplier_api_connection:view_reference_metadata";

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
    /// 在同次身份快照读取连接详情的全部独立功能资格。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人。
    ///
    /// # 返回
    /// 返回已授予的动作与引用元数据权限；失效账号返回空集合。
    ///
    /// # 错误
    /// 权限码无法解析或 RBAC 执行失败时返回对应错误。
    pub(super) async fn connection_permissions(&self, actor: &AuditActor) -> Result<PermissionSet> {
        let mut permissions = SupplierConnectionAction::all()
            .into_iter()
            .map(|action| Permission::parse(action_permission(action)))
            .collect::<CoreResult<Vec<_>>>()?;
        permissions.push(Permission::parse(REFERENCE_METADATA_PERMISSION)?);
        Ok(AuthorizationService::new(self.db.clone(), self.rbac.clone(), AuthorizationRegistry::default())
            .permissions(actor, &permissions, &mut NoTransaction)
            .await?
            .map(|qualified| qualified.permissions())
            .unwrap_or_default())
    }
    /// 判断操作人是否持有给定权限码。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人。
    /// * `permission` - 权限码。
    ///
    /// # 返回
    /// 持有权限时返回 `true`；账号失效或未授予时返回 `false`。
    ///
    /// # 错误
    /// 权限码无法解析或 RBAC 执行失败时返回对应错误。
    pub(super) async fn has_permission(&self, actor: &AuditActor, permission: &str) -> Result<bool> {
        let permission = Permission::parse(permission)?;
        Ok(AuthorizationService::new(self.db.clone(), self.rbac.clone(), AuthorizationRegistry::default())
            .has_permission(actor, &permission, &mut NoTransaction)
            .await?)
    }
}
