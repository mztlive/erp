use erp_core::AccountKind;
use persistence_core::NoTransaction;

use super::AdminService;
use super::dto::ResetAdminPasswordParams;
use crate::AccessControlExt;
use crate::entity::{AccountCore, AccountCoreUpdate};
use crate::error::{Error, Result};
use crate::repository::prelude::*;
use crate::service::account_support::{account_of_kind, apply_account_update};

/// 运维重置密码后的账号标识及启用状态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResetAdminPasswordResult {
    /// 本次完成密码重置的管理员账号 ID。
    pub admin_id: String,
    /// 规范化后的管理员登录账号。
    pub account: String,
    /// 账号是否启用；重置沿用原状态。
    pub active: bool,
}

impl AdminService {
    /// 供运维 `reset-password` 为已有且未删除的管理员更换密码，也适用于受普通更新入口保护的 `role-root` 账号。
    ///
    /// 仅修改 `accounts` 中的密码哈希，沿用名称、状态和角色；返回账号 ID、登录名及启用状态。恢复超级管理员须使用 `init-admin`。
    ///
    /// 账号或密码非法返回 `ValidationError`，账号缺失或类型不符返回 `NotFound`，软删除返回 `BusinessLogicError`。
    /// 一般数据库读写失败返回 `RepositoryError`，已分类的冲突沿用仓储错误映射。
    pub async fn reset_admin_password(
        &self,
        params: ResetAdminPasswordParams,
    ) -> Result<ResetAdminPasswordResult> {
        let (account, password) = params.into_validated_parts()?;
        let current = self
            .db
            .accounts()
            .find_by_account_including_deleted(account.as_str(), &mut NoTransaction)
            .await?;
        let current = existing_admin_for_password_reset(current)?;
        let active = current.status.is_active();
        let admin_id = current.base.id.clone();

        let mut updated = apply_account_update(
            current,
            AccountCoreUpdate { password: Some(password), ..Default::default() },
        )
        .await?;
        self.db.accounts().update(&mut updated, &mut NoTransaction).await?;

        Ok(ResetAdminPasswordResult { admin_id, account: account.into_string(), active })
    }
}

// 重置前检查按登录名查到的记录（含软删除）；缺失或类型不符返回 `NotFound`，软删除返回业务错误，其余管理员可重置。
fn existing_admin_for_password_reset(account: Option<AccountCore>) -> Result<AccountCore> {
    let account = account_of_kind(account, AccountKind::Admin, "管理员不存在")?;
    if account.base.is_deleted() {
        return Err(Error::BusinessLogicError("账号已删除，请使用 init-admin 修复超级管理员".to_string()));
    }
    Ok(account)
}

#[cfg(test)]
mod tests {
    use entity_core::BaseModel;
    use erp_core::AccountKind;

    use super::existing_admin_for_password_reset;
    use crate::entity::{AccountCore, AccountStatus, LoginAccount, Secret};

    // 为重置资格测试构造启用的管理员，并按需标记为软删除。
    fn account(deleted: bool) -> AccountCore {
        let secret = Secret::new(LoginAccount::new("admin01").unwrap(), "password123").unwrap();
        AccountCore {
            base: BaseModel {
                id: "admin-1".to_string(),
                deleted_at: if deleted { 1 } else { 0 },
                ..BaseModel::fake()
            },
            secret,
            name: "管理员".to_string(),
            kind: AccountKind::Admin,
            status: AccountStatus::Active,
            email: None,
            phone: None,
            avatar: None,
        }
    }

    // 验证登录账号缺失时返回 `NotFound`。
    #[test]
    fn missing_account_is_not_found() {
        assert!(matches!(existing_admin_for_password_reset(None), Err(crate::error::Error::NotFound(_))));
    }

    // 验证软删除优先于启用状态，已删除管理员重置密码时返回业务错误。
    #[test]
    fn deleted_admin_cannot_reset_password() {
        let error = existing_admin_for_password_reset(Some(account(true))).unwrap_err();
        assert!(matches!(error, crate::error::Error::BusinessLogicError(_)));
    }

    // 验证启用且未软删除的管理员通过重置检查并保留其账号 ID。
    #[test]
    fn active_admin_is_accepted_for_password_reset() {
        let account = existing_admin_for_password_reset(Some(account(false))).unwrap();
        assert_eq!(account.base.id, "admin-1");
    }
}
