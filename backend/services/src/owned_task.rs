use std::future::Future;

use crate::errors::{Error, Result};

/// 将 Tokio JoinError 映射为内部错误，保留任务名便于诊断。
///
/// # 参数
/// * `name` - 用于 JoinError 诊断的任务名称
/// * `error` - 任务 join 失败原因（panic 或取消）
///
/// # 返回值
/// 统一的 `Error::Internal`。
///
/// # 错误
/// 无；本函数本身只做错误转换。
fn map_owned_join_error(name: &'static str, error: tokio::task::JoinError) -> Error {
    Error::Internal(format!("{name}后台任务异常终止: {error}"))
}

/// 在独立 Tokio 任务中运行关键操作，使调用方取消不会终止收尾流程。
///
/// # 参数
/// * `name` - 用于 JoinError 诊断的任务名称
/// * `operation` - 需要独立持有的异步操作
///
/// # 返回值
/// 返回异步操作的原始结果。
///
/// # 错误
/// 当异步操作失败或任务 panic/被终止时返回错误。
pub(crate) async fn await_owned<T, F>(name: &'static str, operation: F) -> Result<T>
where
    T: Send + 'static,
    F: Future<Output = Result<T>> + Send + 'static,
{
    tokio::spawn(async move {
        let result = operation.await;
        if let Err(error) = &result {
            tracing::error!(
                task = name,
                error = %error,
                "Owned background operation failed"
            );
        }
        result
    })
    .await
    .map_err(|error| map_owned_join_error(name, error))?
}

#[cfg(test)]
mod tests {
    use super::{await_owned, map_owned_join_error};
    use crate::errors::Error;

    #[tokio::test]
    async fn operation_continues_after_waiter_is_cancelled() {
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = tokio::sync::oneshot::channel();
        let (completed_tx, completed_rx) = tokio::sync::oneshot::channel();
        let waiter = tokio::spawn(async move {
            await_owned("测试", async move {
                let _ = started_tx.send(());
                let _ = release_rx.await;
                let _ = completed_tx.send(());
                Ok(())
            })
            .await
        });

        started_rx.await.unwrap();
        waiter.abort();
        assert!(waiter.await.unwrap_err().is_cancelled());
        let _ = release_tx.send(());
        tokio::time::timeout(std::time::Duration::from_secs(1), completed_rx)
            .await
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn join_error_maps_to_internal_error() {
        // 本地 Cranelift codegen 下 catch_unwind 不可靠，panic 会穿过 JoinHandle；
        // 用 abort 产生同等 JoinError，验证映射合同。
        let handle = tokio::spawn(std::future::pending::<()>());
        handle.abort();
        let join_error = handle.await.expect_err("aborted task must yield JoinError");
        let error = map_owned_join_error("测试", join_error);
        assert!(matches!(
            error,
            Error::Internal(message) if message.contains("测试后台任务异常终止")
        ));
    }
}
