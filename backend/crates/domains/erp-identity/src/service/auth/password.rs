use tokio::sync::Semaphore;

use crate::entity::auth::{LoginAccount, PasswordVerification, Secret};
use crate::error::{Error, Result};

/// 同时执行的密码哈希任务上限，避免 Argon2 内存成本被无界放大。
const PASSWORD_WORK_LIMIT: usize = 4;
static PASSWORD_WORK_SLOTS: Semaphore = Semaphore::const_new(PASSWORD_WORK_LIMIT);

/// 有界密码校验的结果。
pub(crate) enum PasswordCheck {
    Mismatch,
    Current,
    Upgraded(
        /// 旧密码通过验证后生成的 Argon2 凭证，由调用方按用例决定是否写入。
        Secret
    ),
}

impl PasswordCheck {
    /// 验密后判断能否继续：当前凭证匹配和旧凭证匹配均为成功。
    pub fn is_match(&self) -> bool {
        !matches!(self, Self::Mismatch)
    }

    /// 登录写回前取出匹配旧 MD5 密码后生成的新凭证，其他结果不需迁移。
    pub fn into_upgraded_secret(self) -> Option<Secret> {
        match self {
            Self::Upgraded(secret) => Some(secret),
            Self::Mismatch | Self::Current => None,
        }
    }
}

/// 登录或修改密码时在有界阻塞任务中验密，旧 MD5 匹配时生成 Argon2 凭证。
/// 账号不存在或不可认证时传入 `None` 做占位验证；任务或密码处理失败返回错误。
/// 任务拥有凭证、密码及信号量许可，不借用异步栈；请求取消也不会提前释放许可。
pub async fn verify_password(secret: Option<Secret>, password: String) -> Result<PasswordCheck> {
    run_password_work(
        move || verify_password_sync(secret, password),
        |_| Error::Internal("密码处理失败".to_string()),
    )
    .await
}

/// 开通或改密时为规范化登录账号生成 Argon2 凭证，避免在异步线程执行哈希。
/// 密码校验或阻塞任务失败时不返回凭证。
pub async fn hash_secret(account: LoginAccount, password: String) -> Result<Secret> {
    run_hashing(move || Secret::new(account, password)).await
}

/// 领域操作需要计算密码哈希时，将拥有全部输入的同步闭包交给共享限流的阻塞线程池。
/// 领域不变式仍由闭包保证；返回其结果，保留领域错误并报告阻塞任务失败。
pub async fn run_hashing<T, Work>(work: Work) -> Result<T>
where
    T: Send + 'static,
    Work: FnOnce() -> erp_core::Result<T> + Send + 'static,
{
    run_password_work(work, Error::from).await
}

// 调度密码工作前取得许可，由阻塞任务持有至计算结束，不随调用请求取消而释放。
async fn run_password_work<T, Work, MapError>(work: Work, map_error: MapError) -> Result<T>
where
    T: Send + 'static,
    Work: FnOnce() -> erp_core::Result<T> + Send + 'static,
    MapError: FnOnce(erp_core::Error) -> Error,
{
    let permit = PASSWORD_WORK_SLOTS.acquire().await.expect("密码工作信号量为私有静态资源，永不关闭");
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        work()
    })
    .await
    .map_err(|_| Error::Internal("密码处理任务失败".to_string()))?;
    result.map_err(map_error)
}

// 在阻塞线程上完成验密；无凭证时做占位计算，只有旧格式密码匹配才生成 Argon2 哈希。
fn verify_password_sync(secret: Option<Secret>, password: String) -> erp_core::Result<PasswordCheck> {
    let Some(mut secret) = secret else {
        Secret::verify_password_or_dummy(None, password.as_str());
        return Ok(PasswordCheck::Mismatch);
    };

    let verification = Secret::verify_password_or_dummy(Some(&secret), password.as_str());
    match verification {
        PasswordVerification::Mismatch => Ok(PasswordCheck::Mismatch),
        PasswordVerification::Current => Ok(PasswordCheck::Current),
        PasswordVerification::Legacy => {
            secret.change_password(password)?;
            Ok(PasswordCheck::Upgraded(secret))
        },
    }
}

#[cfg(test)]
mod tests {
    use super::{PasswordCheck, hash_secret, run_hashing, verify_password};
    use crate::entity::{LoginAccount, Secret};
    use crate::error::Error;

    // 验证密码计算在不同于异步调用方的线程执行。
    #[tokio::test]
    async fn password_work_should_run_off_the_async_worker() {
        let async_thread = std::thread::current().id();
        let blocking_thread = run_hashing(|| Ok(std::thread::current().id())).await.unwrap();

        assert_ne!(async_thread, blocking_thread);
    }

    // 验证不合规的密码在阻塞哈希后仍以领域校验错误返回。
    #[tokio::test]
    async fn password_validation_should_keep_domain_error_semantics() {
        let error = hash_secret(LoginAccount::new("admin01").unwrap(), String::new()).await.unwrap_err();

        assert!(matches!(error, Error::Logic(_)));
    }

    // 验证当前哈希格式接受正确密码并拒绝错误密码。
    #[tokio::test]
    async fn modern_password_should_accept_match_and_reject_mismatch() {
        let secret =
            hash_secret(LoginAccount::new("admin01").unwrap(), "password123".to_string()).await.unwrap();

        let matched = verify_password(Some(secret.clone()), "password123".to_string()).await.unwrap();
        let mismatched = verify_password(Some(secret), "wrong-password".to_string()).await.unwrap();

        assert!(matches!(matched, PasswordCheck::Current));
        assert!(matches!(mismatched, PasswordCheck::Mismatch));
    }

    // 验证无凭证的请求经占位校验边界后仍返回不匹配。
    #[tokio::test]
    async fn missing_secret_should_run_dummy_work_and_fail_closed() {
        let result = verify_password(None, "password123".to_string()).await.unwrap();

        assert!(matches!(result, PasswordCheck::Mismatch));
    }

    // 验证旧 MD5 仅在密码匹配时迁移，且新凭证可按当前格式再次验证。
    #[tokio::test]
    async fn matching_legacy_password_should_return_upgraded_secret() {
        let secret: Secret = serde_json::from_value(serde_json::json!({
            "account": "legacy01",
            "password": "482c811da5d5b4bc6d497ffa98491e38",
        }))
        .unwrap();

        let mismatch = verify_password(Some(secret.clone()), "wrong-password".into()).await.unwrap();
        assert!(matches!(mismatch, PasswordCheck::Mismatch));

        let result = verify_password(Some(secret), "password123".to_string()).await.unwrap();
        let PasswordCheck::Upgraded(secret) = result else {
            panic!("matching legacy credential should be upgraded");
        };

        assert!(!secret.is_legacy_password_hash());
        assert!(matches!(
            verify_password(Some(secret), "password123".to_string()).await.unwrap(),
            PasswordCheck::Current
        ));
    }
}
