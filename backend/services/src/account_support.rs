//! 账号生命周期的共享校验与更新解析。

use database::{DatabaseExt, Executor};
use entities::{AccountCore, AccountCoreUpdate, AccountKind, LoginAccount};
use mongodb::Database;

use crate::auth::password;
use crate::errors::Result;

/// 将可选账号收窄为指定后台类型，缺失或类型不符统一按不存在处理。
///
/// # 参数
/// * `account` - 仓储返回的可选账号
/// * `expected_kind` - 调用场景要求的账号类型
/// * `not_found_message` - 统一的不存在提示
///
/// # 返回值
/// 返回类型匹配的账号。
///
/// # 错误
/// 账号不存在或类型不匹配时返回 `NotFound`，避免暴露同 ID 的其他账号类型。
pub(crate) fn account_of_kind(
    account: Option<AccountCore>,
    expected_kind: AccountKind,
    not_found_message: &str,
) -> Result<AccountCore> {
    account
        .filter(|account| account.is_kind(expected_kind))
        .ok_or_else(|| crate::errors::Error::NotFound(not_found_message.to_string()))
}

/// 确保账号可用。
///
/// # 参数
/// * `db` - 数据库实例
/// * `account` - 待检查账号
/// * `exclude_account_id` - 可选排除账号ID（更新场景）
/// * `executor` - 数据访问执行器，由调用方决定是否位于事务中
///
/// # 返回值
/// 校验通过返回 Ok
///
/// # 错误
/// * `ConflictError` - 账号已存在
pub(crate) async fn ensure_account_available(
    db: &Database,
    account: &LoginAccount,
    exclude_account_id: Option<&str>,
    executor: &mut dyn Executor,
) -> Result<()> {
    let Some(existing) = db
        .accounts()
        .find_by_account_including_deleted(account.as_str(), executor)
        .await?
    else {
        return Ok(());
    };
    if exclude_account_id.is_some_and(|id| existing.base.id == id) {
        return Ok(());
    }

    Err(crate::errors::Error::ConflictError("账号已存在".into()))
}

/// 应用账号更新，并在包含密码时通过共享有界阻塞边界完成哈希。
///
/// 不含密码的 patch 直接执行轻量领域校验；包含密码时返回已完整应用更新的
/// 账号，供调用方随后进入 MongoDB 事务或单集合写入。
///
/// # 参数
/// * `account` - 当前账号实体
/// * `update` - 待应用的账号更新
///
/// # 返回值
/// 返回已应用领域更新、可安全持久化的账号。
///
/// # 错误
/// 当领域校验或密码处理失败时返回错误。
pub(crate) async fn apply_account_update(
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
    use super::{account_of_kind, apply_account_update};
    use crate::auth::password::{hash_secret, verify_password, PasswordCheck};
    use entities::{
        AccountCore, AccountCoreData, AccountCoreUpdate, AccountKind, AccountStatus, LoginAccount,
    };

    #[tokio::test]
    async fn password_update_returns_fully_prepared_account() {
        let secret = hash_secret(LoginAccount::new("admin01").unwrap(), "password123".to_string())
            .await
            .unwrap();
        let account = AccountCore::new(
            "account-1".to_string(),
            AccountCoreData {
                secret,
                name: "管理员".to_string(),
                kind: AccountKind::Admin,
                status: AccountStatus::Active,
                email: None,
                phone: None,
                avatar: None,
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
        assert!(matches!(
            verify_password(Some(account.secret), "next-password".to_string())
                .await
                .unwrap(),
            PasswordCheck::Current
        ));
    }

    #[test]
    fn missing_account_has_not_found_semantics() {
        let result = account_of_kind(None, AccountKind::Admin, "管理员不存在");

        assert!(matches!(result, Err(crate::errors::Error::NotFound(_))));
    }
}
