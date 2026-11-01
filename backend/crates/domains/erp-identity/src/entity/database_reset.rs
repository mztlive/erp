//! 全库重置的保留身份约束。

use application_core::AuditActor;
use erp_core::AccountKind;

use super::{AccountCore, ROOT_ROLE_ID, Role};
use crate::{Error, Result};

/// 仅允许有效 admin 本人以有效内建超级管理员身份执行全库重置。
///
/// # 参数
/// * `actor` - 当前操作人，必须是 `account` 本人且类型为后台管理员。
/// * `account` - 被核对的账号；登录名必须是 `admin`，且当前可承担后台责任。
/// * `role` - 被核对的角色；必须是未删除、未停用的内建 `ROOT_ROLE_ID`。
///
/// # 返回
/// 身份全部符合时无返回值。
///
/// # 错误
/// 任一条件不满足时返回 `Error::Forbidden`。
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
    use crate::entity::{AccountCoreData, AccountStatus, LoginAccount, RoleData, Secret};

    fn account() -> AccountCore {
        AccountCore::new(
            "admin-id".into(),
            AccountCoreData {
                secret: Secret::new(LoginAccount::new("admin").unwrap(), "unchanged-password").unwrap(),
                name: "管理员".into(),
                kind: AccountKind::Admin,
                status: AccountStatus::Active,
                email: None,
                phone: None,
                avatar: None,
            },
        )
        .unwrap()
    }

    #[test]
    fn active_admin_and_enabled_builtin_root_are_required() {
        let account = account();
        let actor = AuditActor::new(account.base.id.clone(), "admin".into(), AccountKind::Admin);
        let mut role = Role::new(ROOT_ROLE_ID.into(), RoleData::new("超管").with_system(true)).unwrap();
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

    #[test]
    fn another_identity_or_inactive_admin_cannot_reset() {
        let mut account = account();
        let actor = AuditActor::new(account.base.id.clone(), "admin".into(), AccountKind::Admin);
        let role = Role::new(ROOT_ROLE_ID.into(), RoleData::new("超管").with_system(true)).unwrap();
        let other = AuditActor::new("other".into(), "admin".into(), AccountKind::Admin);
        assert!(ensure_reset_admin(&other, &account, &role).is_err());
        account.status = AccountStatus::Suspended;
        assert!(ensure_reset_admin(&actor, &account, &role).is_err());
        account.status = AccountStatus::Active;
        account.secret.change_account(LoginAccount::new("other").unwrap());
        assert!(ensure_reset_admin(&actor, &account, &role).is_err());
    }
}
