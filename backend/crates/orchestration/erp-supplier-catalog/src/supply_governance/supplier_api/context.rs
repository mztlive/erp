use application_core::AuditActor;
use erp_identity::{Permission, subject};
use erp_supply::entity::supplier_api::SupplierConnectionAction;
use erp_supply::service::supplier_api::context::action_permission;

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
    /// 要求当前账号持有指定权限；未注入 RBAC 时视为无权。
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
        if self.has_permission(actor, permission).await? {
            return Ok(());
        }
        Err(Error::Forbidden("当前角色不能执行该连接治理动作".to_string()))
    }
    /// 查询当前账号是否持有指定权限。未注入 RBAC 时返回 `false`，不视为错误。
    ///
    /// # 参数
    /// * `actor` - 当前认证账号。
    /// * `permission` - 权限码。
    ///
    /// # 返回
    /// 持有权限时为 `true`；未注入 RBAC 或不持有时为 `false`。
    ///
    /// # 错误
    /// 权限码无法解析或鉴权读取失败时返回对应错误。
    pub(super) async fn has_permission(&self, actor: &AuditActor, permission: &str) -> Result<bool> {
        let Some(rbac) = self.rbac.as_ref() else {
            return Ok(false);
        };
        let permission = Permission::parse(permission)?;
        Ok(rbac.enforce(&subject(actor.kind(), actor.id()), &permission).await?)
    }
}
