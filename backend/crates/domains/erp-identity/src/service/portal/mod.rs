//! 外部门户身份用例；供应商启用事实由组合层在每次访问及写事务中重验。

mod account;
mod authentication;
mod password;

use application_core::AuditActor;
use erp_core::AccountKind;
use mongodb::Database;

use crate::entity::portal::{PortalActor, PortalRole};
use crate::{AccountCore, Error, Result, Secret};

/// 开通门户账号前完成校验与哈希的资料，供调用方在事务中创建账号和绑定。
pub struct PreparedPortalAccount {
    // 待写入的供应商账号，凭证已哈希。
    account: AccountCore,
    // 内部流程选定的供应商 ID，用来建立唯一归属绑定。
    supplier_id: String,
    // 供应商内授予的门户岗位，不参与后台 RBAC。
    role: PortalRole,
}

/// 自助改密在事务外验证原密码并完成哈希后，交给写事务重验并落地的命令。
pub struct PreparedPortalPassword {
    // 验证原密码时使用的门户身份及账号、绑定版本。
    actor: PortalActor,
    // 已哈希的新登录凭证，事务内重验后替换旧凭证。
    secret: Secret,
}

/// 处理供应商门户账号开通、绑定维护、登录和会话重验，不授予内部 RBAC 权限或组织身份。
#[derive(Clone)]
pub struct PortalIdentityService {
    // 存放账号及供应商绑定的身份数据库。
    db: Database,
}

impl PortalIdentityService {
    /// 装配门户身份服务时绑定身份数据库，构造时不访问外部存储。
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}

// 开通或调整门户账号前拒绝外部操作人；供应商对象授权由调用方完成。
fn require_internal_actor(actor: &AuditActor) -> Result<()> {
    if actor.kind() == AccountKind::Admin {
        return Ok(());
    }
    Err(Error::Forbidden("供应商账号管理仅允许内部人员执行".into()))
}

// 门户会话重验失败时统一报告身份失效，不区分账号与绑定的变化。
fn invalid_session() -> Error {
    Error::Unauthenticated("认证已失效".into())
}

#[cfg(test)]
mod tests {
    use application_core::AuditActor;
    use erp_core::AccountKind;

    use super::require_internal_actor;

    // 验证门户账号维护的身份检查允许内部账号通过，但拒绝供应商账号。
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
