use tokio::sync::Semaphore;

use crate::entity::auth::{LoginAccount, Secret};
use crate::error::{Error, Result};

/// 同时执行的密码哈希任务上限，避免 Argon2 内存成本被无界放大。
const PASSWORD_WORK_LIMIT: usize = 4;
static PASSWORD_WORK_SLOTS: Semaphore = Semaphore::const_new(PASSWORD_WORK_LIMIT);

/// 登录或修改密码时在有界阻塞任务中验密；账号不存在或不可认证时传入 `None` 做占位验证。
/// 任务拥有凭证、密码及信号量许可，不借用异步栈；请求取消也不会提前释放许可。
pub async fn verify_password(secret: Option<Secret>, password: String) -> Result<bool> {
    run_password_work(
        move || Ok(Secret::verify_password_or_dummy(secret.as_ref(), password.as_str())),
        |_| Error::Internal("密码处理失败".to_string()),
    )
    .await
}

/// 开通或改密时为规范化登录账号生成 Argon2 凭证，避免在异步线程执行哈希。
pub async fn hash_secret(account: LoginAccount, password: String) -> Result<Secret> {
    run_hashing(move || Secret::new(account, password)).await
}

/// 领域操作需要计算密码哈希时，将拥有全部输入的同步闭包交给共享限流的阻塞线程池。
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

#[cfg(test)]
mod tests {
    use super::{hash_secret, run_hashing, verify_password};
    use crate::entity::LoginAccount;
    use crate::error::Error;

    // 验证密码计算在不同于异步调用方的线程执行，且领域校验错误保持不变。
    #[tokio::test]
    async fn password_work_runs_off_async_worker_and_verifies_credentials() {
        let async_thread = std::thread::current().id();
        let blocking_thread = run_hashing(|| Ok(std::thread::current().id())).await.unwrap();
        assert_ne!(async_thread, blocking_thread);

        let error = hash_secret(LoginAccount::new("admin01").unwrap(), String::new()).await.unwrap_err();
        assert!(matches!(error, Error::Logic(_)));

        let secret = hash_secret(LoginAccount::new("admin01").unwrap(), "password123".into()).await.unwrap();
        assert!(verify_password(Some(secret.clone()), "password123".into()).await.unwrap());
        assert!(!verify_password(Some(secret), "wrong-password".into()).await.unwrap());
        assert!(!verify_password(None, "password123".into()).await.unwrap());
    }
}
