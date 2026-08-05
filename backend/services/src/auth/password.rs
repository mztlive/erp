use entities::{LoginAccount, PasswordVerification, Secret};
use tokio::sync::Semaphore;

use crate::errors::{Error, Result};

/// 同时执行的密码哈希任务上限，避免 Argon2 内存成本被无界放大。
const PASSWORD_WORK_LIMIT: usize = 4;
static PASSWORD_WORK_SLOTS: Semaphore = Semaphore::const_new(PASSWORD_WORK_LIMIT);

/// 有界密码校验的结果。
pub(crate) enum PasswordCheck {
    Mismatch,
    Current,
    Upgraded(Secret),
}

impl PasswordCheck {
    /// 判断密码是否匹配当前或已迁移凭证。
    ///
    /// # 返回值
    /// 当前密码有效时返回 `true`。
    pub(crate) fn is_match(&self) -> bool {
        !matches!(self, Self::Mismatch)
    }

    /// 消费结果并取出由 legacy MD5 迁移得到的新凭证。
    ///
    /// # 返回值
    /// legacy 凭证匹配时返回升级后的凭证，否则返回 `None`。
    pub(crate) fn into_upgraded_secret(self) -> Option<Secret> {
        match self {
            Self::Upgraded(secret) => Some(secret),
            Self::Mismatch | Self::Current => None,
        }
    }
}

/// 在有界阻塞任务中校验密码，并按需生成升级后的 Argon2 凭证。
///
/// `secret` 与 `password` 均由调用方转移所有权，保证阻塞任务不会借用
/// async 栈上的敏感数据。信号量许可随阻塞任务生命周期持有，即使请求取消，
/// 仍不会突破并发上限。
///
/// # 参数
/// * `secret` - 已加载凭证；不可认证或不存在时传入 `None`
/// * `password` - 待校验的明文密码
///
/// # 返回值
/// 返回密码匹配状态及可选升级凭证。
///
/// # 错误
/// 当信号量、阻塞任务或密码处理失败时返回错误。
pub(crate) async fn verify_password(secret: Option<Secret>, password: String) -> Result<PasswordCheck> {
    run_password_work(
        move || verify_password_sync(secret, password),
        |_| Error::Internal("密码处理失败".to_string()),
    )
    .await
}

/// 在有界阻塞任务中为账号生成 Argon2 凭证。
///
/// # 参数
/// * `account` - 已规范化登录账号
/// * `password` - 明文密码
///
/// # 返回值
/// 返回已生成 Argon2 哈希的凭证。
///
/// # 错误
/// 当密码、信号量或阻塞任务处理失败时返回错误。
pub(crate) async fn hash_secret(account: LoginAccount, password: String) -> Result<Secret> {
    run_hashing(move || Secret::new(account, password)).await
}

/// 在共享有界阻塞边界执行一项领域密码哈希工作。
///
/// 调用方负责在闭包中保留所属领域的不变式；该函数只负责 Argon2 的资源
/// 隔离、并发上限与领域错误传播。
///
/// # 参数
/// * `work` - 拥有全部输入且可能执行密码哈希的同步领域工作
///
/// # 返回值
/// 返回同步领域工作生成的值。
///
/// # 错误
/// 当领域校验、信号量或阻塞任务处理失败时返回错误。
pub(crate) async fn run_hashing<T, Work>(work: Work) -> Result<T>
where
    T: Send + 'static,
    Work: FnOnce() -> entities::Result<T> + Send + 'static,
{
    run_password_work(work, Error::from).await
}

/// 统一持有密码工作许可并等待阻塞任务完成。
///
/// 信号量许可被移动到阻塞任务中，请求取消后仍会保留到实际计算结束。
async fn run_password_work<T, Work, MapError>(work: Work, map_error: MapError) -> Result<T>
where
    T: Send + 'static,
    Work: FnOnce() -> entities::Result<T> + Send + 'static,
    MapError: FnOnce(entities::Error) -> Error,
{
    let permit = PASSWORD_WORK_SLOTS
        .acquire()
        .await
        .map_err(|_| Error::Internal("密码处理资源不可用".to_string()))?;
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        work()
    })
    .await
    .map_err(|_| Error::Internal("密码处理任务失败".to_string()))?;
    result.map_err(map_error)
}

/// 完成单次同步密码工作；仅在 legacy 匹配时生成新 Argon2 哈希。
fn verify_password_sync(secret: Option<Secret>, password: String) -> entities::Result<PasswordCheck> {
    let verification = Secret::verify_password_or_dummy(secret.as_ref(), password.as_str());
    match verification {
        PasswordVerification::Mismatch => Ok(PasswordCheck::Mismatch),
        PasswordVerification::Current => Ok(PasswordCheck::Current),
        PasswordVerification::Legacy => {
            let Some(mut secret) = secret else {
                return Err(entities::Error::LogicError("密码处理失败".to_string()));
            };
            secret.change_password(password)?;
            Ok(PasswordCheck::Upgraded(secret))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{hash_secret, run_hashing, verify_password, PasswordCheck};
    use crate::errors::Error;
    use entities::{LoginAccount, Secret};

    #[tokio::test]
    async fn password_work_should_run_off_the_async_worker() {
        let async_thread = std::thread::current().id();

        let blocking_thread = run_hashing(|| Ok(std::thread::current().id())).await.unwrap();

        assert_ne!(async_thread, blocking_thread);
    }

    #[tokio::test]
    async fn password_validation_should_keep_domain_error_semantics() {
        let error = hash_secret(LoginAccount::new("admin01").unwrap(), String::new())
            .await
            .unwrap_err();

        assert!(matches!(error, Error::Logic(_)));
    }

    #[tokio::test]
    async fn modern_password_should_accept_match_and_reject_mismatch() {
        let secret = hash_secret(LoginAccount::new("admin01").unwrap(), "password123".to_string())
            .await
            .unwrap();

        let matched = verify_password(Some(secret.clone()), "password123".to_string())
            .await
            .unwrap();
        let mismatched = verify_password(Some(secret), "wrong-password".to_string())
            .await
            .unwrap();

        assert!(matches!(matched, PasswordCheck::Current));
        assert!(matches!(mismatched, PasswordCheck::Mismatch));
    }

    #[tokio::test]
    async fn missing_secret_should_run_dummy_work_and_fail_closed() {
        let result = verify_password(None, "password123".to_string()).await.unwrap();

        assert!(matches!(result, PasswordCheck::Mismatch));
    }

    #[tokio::test]
    async fn matching_legacy_password_should_return_upgraded_secret() {
        let secret: Secret = serde_json::from_value(serde_json::json!({
            "account": "legacy01",
            "password": "482c811da5d5b4bc6d497ffa98491e38",
        }))
        .unwrap();

        let result = verify_password(Some(secret), "password123".to_string())
            .await
            .unwrap();
        let PasswordCheck::Upgraded(secret) = result else {
            panic!("matching legacy credential should be upgraded");
        };

        assert!(!secret.is_legacy_password_hash());
        assert!(matches!(
            verify_password(Some(secret), "password123".to_string())
                .await
                .unwrap(),
            PasswordCheck::Current
        ));
    }
}
