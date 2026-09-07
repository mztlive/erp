use super::SupplierApiGovernanceProcess;
use crate::{Error, Result};
use application_core::AuditActor;
use erp_identity::{subject, Permission};
use erp_supply::entity::supplier_api::SupplierConnectionAction;
use erp_supply::service::supplier_api::context::action_permission;
impl SupplierApiGovernanceProcess {
    pub(super) async fn ensure_action_permission(
        &self,
        actor: &AuditActor,
        action: SupplierConnectionAction,
    ) -> Result<()> {
        let permission = action_permission(action);
        self.ensure_permission(actor, permission).await
    }
    pub(super) async fn ensure_permission(&self, actor: &AuditActor, permission: &str) -> Result<()> {
        if self.has_permission(actor, permission).await? {
            return Ok(());
        }
        Err(Error::Forbidden("当前角色不能执行该连接治理动作".to_string()))
    }
    pub(super) async fn has_permission(&self, actor: &AuditActor, permission: &str) -> Result<bool> {
        let Some(rbac) = self.rbac.as_ref() else {
            return Ok(false);
        };
        let permission = Permission::parse(permission)?;
        Ok(rbac
            .enforce(&subject(actor.kind(), actor.id()), &permission)
            .await?)
    }
}
