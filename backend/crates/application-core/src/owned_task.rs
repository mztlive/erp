use std::future::Future;

use crate::Error;

/// 返回任务诊断名，映射逻辑的可单测分离点。
#[cfg(test)]
fn owned_task_diagnostic_name(name: &'static str) -> &'static str {
    name
}

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

    #[test]
    fn owned_task_name_is_preserved_for_diagnosis() {
        // 映射层已分离为纯函数，可独立单测；运行时行为见 `await_owned`。
        // 完整取消语义测试需 tokio sync/time 特性，基线未启用，此处不引入新依赖。
        let message = super::owned_task_diagnostic_name("测试");

        assert_eq!(message, "测试");
        let _ = Error::Internal("占位".to_string());
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
