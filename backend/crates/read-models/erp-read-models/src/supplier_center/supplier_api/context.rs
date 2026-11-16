use application_core::AuditActor;
use erp_core::ids::SupplierApiConnectionId;
use erp_identity::PermissionSet;
use erp_supply::entity::supplier_api::{
    SupplierApiConnection, SupplierConnectionAction, SupplierHealthCheckRun,
};
use erp_supply::repository::SupplierApiExt;
use erp_supply::service::supplier_api::context::action_permission;
use persistence_core::Executor;

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
        executor: &mut dyn Executor,
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
    /// * `executor` - 最终详情查询前的原快照执行器。
    ///
    /// # 返回
    /// 返回已授予的动作与引用元数据权限；失效账号返回空集合。
    ///
    /// # 错误
    /// 权限码无法解析或 RBAC 执行失败时返回对应错误。
    pub(super) async fn connection_permissions(
        &self,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<PermissionSet> {
        let mut permissions =
            SupplierConnectionAction::all().into_iter().map(action_permission).collect::<Vec<_>>();
        permissions.push(erp_permissions::supplier_api::REFERENCES_OPTIONS);
        Ok(self
            .authorization
            .permissions(actor, &permissions, executor)
            .await?
            .map(|qualified| qualified.permissions())
            .unwrap_or_default())
    }
}
