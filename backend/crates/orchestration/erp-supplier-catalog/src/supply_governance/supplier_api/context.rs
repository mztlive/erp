use application_core::AuditActor;
use erp_identity::{AuthorizationRegistry, AuthorizationService, Permission};
use erp_supply::entity::supplier_api::SupplierConnectionAction;
use erp_supply::service::supplier_api::context::action_permission;
use persistence_core::NoTransaction;

use super::SupplierApiGovernanceProcess;
use crate::{Error, Result};
impl SupplierApiGovernanceProcess {
    /// 按连接动作要求对应权限。
    ///
    /// # 参数
    /// * `actor` - 当前认证账号。
    /// * `action` - 连接治理动作。
    ///
    /// # 返回
    /// 无返回值。具备该动作权限即通过。
    ///
    /// # 错误
    /// 未注入 RBAC、权限码无法解析、鉴权读取失败，或当前角色无权时返回 `Forbidden` 或对应错误。
    pub(super) async fn ensure_action_permission(
        &self,
        actor: &AuditActor,
        action: SupplierConnectionAction,
    ) -> Result<()> {
        let permission = action_permission(action);
        self.ensure_permission(actor, permission).await
    }
    /// 要求当前账号持有指定权限；设施错误保持原分类。
    ///
    /// # 参数
    /// * `actor` - 当前认证账号。
    /// * `permission` - 权限码。
    ///
    /// # 返回
    /// 无返回值。
    ///
    /// # 错误
    /// 无权时返回 `Forbidden`；权限码无法解析或鉴权读取失败时返回对应错误。
    pub(super) async fn ensure_permission(&self, actor: &AuditActor, permission: &str) -> Result<()> {
        let permission = Permission::parse(permission)?;
        if !AuthorizationService::new(self.db.clone(), self.rbac.clone(), AuthorizationRegistry::default())
            .has_permission(actor, &permission, &mut NoTransaction)
            .await?
        {
            return Err(Error::Forbidden("当前角色不能执行该连接治理动作".to_string()));
        }
        Ok(())
    }
}
