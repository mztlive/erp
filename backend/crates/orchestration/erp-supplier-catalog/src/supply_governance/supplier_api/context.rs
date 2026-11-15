use application_core::AuditActor;
use erp_permissions::Permission;
use persistence_core::ReadExecutor;

use super::SupplierApiGovernanceProcess;
use crate::{Error, Result};
impl SupplierApiGovernanceProcess {
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
    pub(super) async fn ensure_permission(&self, actor: &AuditActor, permission: &Permission) -> Result<()> {
        if !self.authorization.has_permission(actor, permission, &mut ReadExecutor::default()).await? {
            return Err(Error::Forbidden("当前角色不能执行该连接治理动作".to_string()));
        }
        Ok(())
    }
}
