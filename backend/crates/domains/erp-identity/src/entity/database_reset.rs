//! 全库重置的保留身份约束。

use application_core::AuditActor;
use erp_core::AccountKind;

use super::{AccountCore, ROOT_ROLE_ID, Role};
use crate::{Error, Result};

/// 全库重置前核对操作人是后台身份，且其 ID 与启用的 `admin` 后台账号一致。
/// 同时要求传入的角色为未删除、未停用的内建 `ROOT_ROLE_ID`；任一条件不符返回 `Error::Forbidden`。
pub(crate) fn ensure_reset_admin(actor: &AuditActor, account: &AccountCore, role: &Role) -> Result<()> {
    if actor.kind() != AccountKind::Admin
        || actor.id() != account.base.id
        || account.secret.account() != "admin"
        || !account.is_active_backoffice()
        || role.base.id != ROOT_ROLE_ID
        || !role.system
        || role.disabled
        || role.base.is_deleted()
    {
        return Err(Error::Forbidden("仅启用的 admin 超级管理员可以清空演示数据库".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RoleId;
    use crate::entity::{AccountCoreData, AccountStatus, LoginAccount, RoleData, Secret};

    // 构造登录名固定为 `admin` 的启用后台账号，作为重置身份核对的基准。
    fn account() -> AccountCore {
        AccountCore::new(
            "admin-id".into(),
            AccountCoreData {
                secret: Secret::new(LoginAccount::new("admin").unwrap(), "unchanged-password").unwrap(),
                name: "管理员".into(),
                kind: AccountKind::Admin,
                status: AccountStatus::Active,
            },
        )
        .unwrap()
    }

    // 验证重置接受启用的 `admin` 和内建超管角色，拒绝停用、非系统及错误角色 ID。
    #[test]
    fn active_admin_and_enabled_builtin_root_are_required() {
        let account = account();
        let actor = AuditActor::new(account.base.id.clone(), "admin".into(), AccountKind::Admin);
        let mut role =
            Role::new(RoleId::parse(ROOT_ROLE_ID).unwrap(), RoleData::new("超管").with_system(true)).unwrap();
        assert!(ensure_reset_admin(&actor, &account, &role).is_ok());
        role.disabled = true;
        assert!(ensure_reset_admin(&actor, &account, &role).is_err());
        role.disabled = false;
        role.system = false;
        assert!(ensure_reset_admin(&actor, &account, &role).is_err());
        role.system = true;
        role.base.id = "custom-root".into();
        assert!(ensure_reset_admin(&actor, &account, &role).is_err());
    }

    // 验证重置拒绝同登录名的其他操作人，以及冻结或改名后的 `admin` 账号。
    #[test]
    fn another_identity_or_inactive_admin_cannot_reset() {
        let mut account = account();
        let actor = AuditActor::new(account.base.id.clone(), "admin".into(), AccountKind::Admin);
        let role =
            Role::new(RoleId::parse(ROOT_ROLE_ID).unwrap(), RoleData::new("超管").with_system(true)).unwrap();
        let other = AuditActor::new("other".into(), "admin".into(), AccountKind::Admin);
        assert!(ensure_reset_admin(&other, &account, &role).is_err());
        account.status = AccountStatus::Suspended;
        assert!(ensure_reset_admin(&actor, &account, &role).is_err());
        account.status = AccountStatus::Active;
        account.secret.change_account(LoginAccount::new("other").unwrap());
        assert!(ensure_reset_admin(&actor, &account, &role).is_err());
    }
}
