//! 账号生命周期的共享校验与更新解析。

use erp_core::AccountKind;
use mongodb::Database;
use persistence_core::Executor;

use crate::AccessControlExt;
use crate::entity::account_core::{AccountCore, AccountCoreUpdate};
use crate::entity::auth::LoginAccount;
use crate::error::Result;
use crate::repository::prelude::*;
use crate::service::auth::password;

/// 按业务入口要求的后台账号类型取出实体。账号缺失或类型不匹配时统一返回 `NotFound`，避免暴露同一 ID 的其他身份。
pub fn account_of_kind(
    account: Option<AccountCore>,
    expected_kind: AccountKind,
    not_found_message: &str,
) -> Result<AccountCore> {
    account
        .filter(|account| account.is_kind(expected_kind))
        .ok_or_else(|| crate::error::Error::NotFound(not_found_message.to_string()))
}

/// 创建或改名账号前检查登录名是否已被占用。已软删除的账号仍占用登录名；更新时可通过 `exclude_account_id` 排除自身。
pub async fn ensure_account_available(
    db: &Database,
    account: &LoginAccount,
    exclude_account_id: Option<&str>,
    executor: &mut dyn Executor,
) -> Result<()> {
    let Some(existing) = db.accounts().find_by_account_including_deleted(account.as_str(), executor).await?
    else {
        return Ok(());
    };
    if exclude_account_id.is_some_and(|id| existing.base.id == id) {
        return Ok(());
    }
    Err(crate::error::Error::ConflictError("账号已存在".into()))
}

/// 在落库前应用账号变更并完成领域校验。涉及新密码时通过有界阻塞任务生成哈希，避免数据库事务等待密码计算。
pub async fn apply_account_update(
    mut account: AccountCore,
    update: AccountCoreUpdate,
) -> Result<AccountCore> {
    if update.password.is_none() {
        account.update(update)?;
        return Ok(account);
    }

    password::run_hashing(move || {
        account.update(update)?;
        Ok(account)
    })
    .await
}

#[cfg(test)]
mod tests {
    use erp_core::AccountKind;

    use super::{account_of_kind, apply_account_update};
    use crate::entity::{AccountCore, AccountCoreData, AccountCoreUpdate, AccountStatus, LoginAccount};
    use crate::service::auth::password::{hash_secret, verify_password};

    // 验证更新同时保存修改后的名称，并生成可用新密码验证的凭证，调用方可直接持久化返回的账号。
    #[tokio::test]
    async fn password_update_returns_fully_prepared_account() {
        let secret =
            hash_secret(LoginAccount::new("admin01").unwrap(), "password123".to_string()).await.unwrap();
        let account = AccountCore::new(
            "account-1".to_string(),
            AccountCoreData {
                secret,
                name: "管理员".to_string(),
                kind: AccountKind::Admin,
                status: AccountStatus::Active,
            },
        )
        .unwrap();

        let account = apply_account_update(
            account,
            AccountCoreUpdate {
                name: Some("新管理员".to_string()),
                password: Some("next-password".to_string()),
                ..Default::default()
            },
        )
        .await
        .unwrap();

        assert_eq!(account.name, "新管理员");
        assert!(verify_password(Some(account.secret), "next-password".to_string()).await.unwrap());
    }

    // 验证缺失的账号按不存在返回，使业务入口可统一处理缺失或类型不符的身份。
    #[test]
    fn missing_account_has_not_found_semantics() {
        let result = account_of_kind(None, AccountKind::Admin, "管理员不存在");
        assert!(matches!(result, Err(crate::error::Error::NotFound(_))));
    }
}
