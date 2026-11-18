use std::future::Future;

use crate::Error;

// 将关键后台任务的 panic 或取消归为内部错误，并保留任务名供排查。
fn map_owned_join_error(name: &'static str, error: tokio::task::JoinError) -> Error {
    Error::Internal(format!("{name}后台任务异常终止: {error}"))
}

/// 把关键操作交给独立 Tokio 任务。任务启动后，即使等待方取消，操作仍能完成提交或收尾。
/// 业务错误原样返回并在任务内记录；任务 panic 或被终止则转换为内部错误。
pub async fn await_owned<T, E, F>(name: &'static str, operation: F) -> std::result::Result<T, E>
where
    T: Send + 'static,
    E: From<Error> + std::fmt::Display + Send + 'static,
    F: Future<Output = std::result::Result<T, E>> + Send + 'static,
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
    .map_err(|error| E::from(map_owned_join_error(name, error)))?
}

#[cfg(test)]
mod tests {
    use super::map_owned_join_error;
    use crate::Error;

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
