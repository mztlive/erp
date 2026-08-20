use database::{AccessControlExt, NoTransaction};
use entities::{AccountCore, AccountCoreUpdate, AccountKind};

use super::dto::ResetAdminPasswordParams;
use super::AdminService;
use crate::account_support::{account_of_kind, apply_account_update};
use crate::errors::{Error, Result};

/// 管理员密码重置结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResetAdminPasswordResult {
    /// 管理员账号 ID。
    pub admin_id: String,
    /// 管理员登录账号。
    pub account: String,
    /// 重置后账号是否仍处于可登录状态。
    pub active: bool,
}

impl AdminService {
    /// 重置已有管理员密码，不改名称、状态或角色绑定。
    ///
    /// 已绑定 `role-root` 的超级管理员不能走普通账号更新入口，因此本方法只更新
    /// `accounts` 集合中的密码哈希。账号不存在、类型不符或已软删除时失败，不会创建或提升账号。
    ///
    /// # 参数
    /// * `params` - 登录账号与新密码
    ///
    /// # 返回值
    /// 返回被重置账号的 ID、登录名与当前启用状态。
    ///
    /// # 错误
    /// * `ValidationError` - 账号或密码不合法
    /// * `NotFound` - 管理员不存在或账号类型不是管理员
    /// * `BusinessLogicError` - 账号已软删除
    /// * `RepositoryError` - 数据库读写失败
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
            AccountCoreUpdate {
                password: Some(password),
                ..Default::default()
            },
        )
        .await?;
        self.db
            .accounts()
            .update(&mut updated, &mut NoTransaction)
            .await?;

        Ok(ResetAdminPasswordResult {
            admin_id,
            account: account.into_string(),
            active,
        })
    }
}

/// 将仓储结果收窄为可重置密码的未删除管理员。
///
/// # 参数
/// * `account` - 按登录账号查出的可选账号，包含软删除记录
///
/// # 返回值
/// 返回类型匹配且未删除的管理员账号。
///
/// # 错误
/// 账号缺失或类型不符时返回 `NotFound`；已软删除时返回业务错误。
fn existing_admin_for_password_reset(account: Option<AccountCore>) -> Result<AccountCore> {
    let account = account_of_kind(account, AccountKind::Admin, "管理员不存在")?;
    if account.base.is_deleted() {
        return Err(Error::BusinessLogicError(
            "账号已删除，请使用 init-admin 修复超级管理员".to_string(),
        ));
    }

    Ok(account)
}

#[cfg(test)]
mod tests {
    use entities::{AccountCore, AccountKind, AccountStatus, BaseModel, LoginAccount, Secret};

    use super::existing_admin_for_password_reset;

    /// 构造指定删除状态的最小管理员账号。
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

    #[test]
    fn missing_account_is_not_found() {
        assert!(matches!(
            existing_admin_for_password_reset(None),
            Err(crate::errors::Error::NotFound(_))
        ));
    }

    #[test]
    fn deleted_admin_cannot_reset_password() {
        let error = existing_admin_for_password_reset(Some(account(true))).unwrap_err();

        assert!(matches!(error, crate::errors::Error::BusinessLogicError(_)));
    }

    #[test]
    fn active_admin_is_accepted_for_password_reset() {
        let account = existing_admin_for_password_reset(Some(account(false))).unwrap();

        assert_eq!(account.base.id, "admin-1");
    }
}
