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
    /// 门户身份服务。
    ///
    /// `db` 为身份集合所在数据库。
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}

// 开通、停用或配置门户账号前确认当前管理者属于内部账号，不让外部身份执行维护命令。
fn require_internal_actor(actor: &AuditActor) -> Result<()> {
    if actor.kind() == AccountKind::Admin {
        return Ok(());
    }
    Err(Error::Forbidden("供应商账号管理仅允许内部人员执行".into()))
}

// 供应商门户会话失效时统一返回安全错误，不暴露究竟是账号还是绑定状态变化。
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
