//! 外部门户身份用例；供应商启用事实由组合层在每次访问及写事务中重验。

mod account;
mod authentication;
mod password;

use application_core::AuditActor;
use erp_core::AccountKind;
use mongodb::Database;

use crate::entity::portal::{PortalActor, PortalRole};
use crate::{AccountCore, Error, Result, Secret};

/// 已完成密码哈希的外部账号开通资料，可交给调用方事务落地。
pub struct PreparedPortalAccount {
    account: AccountCore,
    supplier_id: String,
    role: PortalRole,
}

impl PreparedPortalAccount {
    /// 返回预生成的稳定账号身份，用于命令与审计关联。
    ///
    /// # 参数
    /// 无。
    /// # 返回
    /// 尚未写入数据库的账号 ID。
    /// # 错误
    /// 无。
    pub fn account_id(&self) -> &str {
        &self.account.base.id
    }
}

/// 已核对原密码并完成新凭证哈希的自助密码命令。
pub struct PreparedPortalPassword {
    actor: PortalActor,
    secret: Secret,
}

/// 管理外部账号、绑定及会话；不授予内部 RBAC 权限或组织身份。
#[derive(Clone)]
pub struct PortalIdentityService {
    db: Database,
}

impl PortalIdentityService {
    /// 绑定身份领域数据库，不执行外部 I/O。
    ///
    /// # 参数
    /// `db` 为身份集合所在数据库。
    /// # 返回
    /// 门户身份服务。
    /// # 错误
    /// 无。
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}

/// 内部账号才允许开通、停用和配置外部身份。
fn require_internal_actor(actor: &AuditActor) -> Result<()> {
    if actor.kind() == AccountKind::Admin {
        return Ok(());
    }
    Err(Error::Forbidden("供应商账号管理仅允许内部人员执行".into()))
}

/// 统一失效错误避免暴露账号或绑定具体状态。
fn invalid_session() -> Error {
    Error::Unauthenticated("认证已失效".into())
}

#[cfg(test)]
mod tests {
    use application_core::AuditActor;
    use erp_core::AccountKind;

    use super::require_internal_actor;

    #[test]
    fn supplier_actor_cannot_manage_portal_accounts() {
        assert!(
            require_internal_actor(&AuditActor::new("buyer".into(), "buyer01".into(), AccountKind::Admin))
                .is_ok()
        );
        assert!(
            require_internal_actor(&AuditActor::new(
                "external".into(),
                "supplier01".into(),
                AccountKind::Supplier
            ))
            .is_err()
        );
    }
}
