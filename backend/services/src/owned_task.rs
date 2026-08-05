use std::future::Future;

use crate::errors::{Error, Result};

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
    .map_err(|error| Error::Internal(format!("{name}后台任务异常终止: {error}")))?
}

#[cfg(test)]
mod tests {
    use super::await_owned;
    use crate::errors::{Error, Result};

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
        let result: Result<()> = await_owned("测试", async move {
            panic!("expected test panic");
        })
        .await;

        assert!(matches!(result, Err(Error::Internal(message)) if message.contains("测试后台任务异常终止")));
    }
}
