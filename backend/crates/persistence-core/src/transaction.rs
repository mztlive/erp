//! MongoDB 操作的事务支持。
//!
//! 提供事务管理，使多次操作得以原子执行。

use std::future::Future;
use std::pin::Pin;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use mongodb::error::UNKNOWN_TRANSACTION_COMMIT_RESULT;
use mongodb::options::{ReadConcern, SessionOptions, TransactionOptions, WriteConcern};
use mongodb::{Client, ClientSession};

use crate::Executor;
use crate::errors::{Error, Result};

const COMMIT_RETRY_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CommitErrorAction {
    Retry,
    OutcomeUnknown,
    DefiniteFailure,
}

/// 提交错误的结果标签（具名枚举，避免调用点 `true`/`false` 难读）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CommitErrorLabel {
    UnknownResult,
    Definite,
}

/// 根据 MongoDB 错误标签与已用时间决定提交错误的处理方式。
fn commit_error_action(label: CommitErrorLabel, elapsed: Duration) -> CommitErrorAction {
    if label != CommitErrorLabel::UnknownResult {
        return CommitErrorAction::DefiniteFailure;
    }
    if elapsed < COMMIT_RETRY_TIMEOUT {
        return CommitErrorAction::Retry;
    }
    CommitErrorAction::OutcomeUnknown
}

/// 构建统一业务事务选项：读取同一个 majority-committed 快照，并以 majority 提交。
///
/// 不得依赖 MongoDB 默认的 `local` read concern：它不能为分片部署保证跨集合、
/// 跨分片的一致快照，会破坏 Service 在同一事务内重验业务与授权事实的前提。
fn transaction_options() -> TransactionOptions {
    TransactionOptions::builder()
        .read_concern(ReadConcern::snapshot())
        .write_concern(WriteConcern::majority())
        .build()
}

/// 在同一会话上重试结果未知的事务提交。
async fn commit_with_retry(session: &mut ClientSession) -> Result<()> {
    let started_at = Instant::now();
    loop {
        let Err(error) = session.commit_transaction().await else {
            return Ok(());
        };
        let label = if error.contains_label(UNKNOWN_TRANSACTION_COMMIT_RESULT) {
            CommitErrorLabel::UnknownResult
        } else {
            CommitErrorLabel::Definite
        };
        match commit_error_action(label, started_at.elapsed()) {
            CommitErrorAction::Retry => continue,
            CommitErrorAction::OutcomeUnknown => return Err(Error::CommitOutcomeUnknown(error)),
            CommitErrorAction::DefiniteFailure => return Err(Error::from(error)),
        }
    }
}

/// 启动带因果一致性的会话并开启统一业务事务选项。
async fn start_transaction_session(client: &Client) -> Result<ClientSession> {
    let session_options = SessionOptions::builder().causal_consistency(true).build();
    let mut session = client.start_session().with_options(session_options).await?;
    let txn_options = transaction_options();
    session.start_transaction().with_options(txn_options).await?;
    Ok(session)
}

/// 静默回滚事务；回滚失败仅记录日志，不覆盖用例原始错误。
async fn abort_quietly(session: &mut ClientSession) {
    if let Err(abort_error) = session.abort_transaction().await {
        tracing::warn!(error = ?abort_error, "failed to abort MongoDB transaction after callback error");
    }
}

/// 在事务上下文中执行操作。
#[async_trait]
pub trait Transactional {
    /// 在同一事务内执行回调，并将持久化错误转换为调用方错误类型。
    ///
    /// # 参数
    ///
    /// * `f` - 使用同一数据访问执行器的异步回调
    ///
    /// # 返回
    ///
    /// 返回回调成功提交后的结果。
    ///
    /// # 错误
    /// 返回回调错误，或经 `From` 转换的事务初始化、提交错误。
    /// 回调失败时尝试回滚，回滚失败只记录日志，不覆盖原错误。
    async fn with_transaction<F, R, E>(&self, f: F) -> std::result::Result<R, E>
    where
        F: for<'a> FnOnce(
                &'a mut dyn Executor,
            )
                -> Pin<Box<dyn Future<Output = std::result::Result<R, E>> + Send + 'a>>
            + Send,
        R: Send,
        E: From<Error> + Send;
}

/// 为 MongoDB 客户端实现事务支持。
#[async_trait]
impl Transactional for Client {
    /// 在事务中执行函数，并允许调用方指定错误类型。
    ///
    /// 回调收到 `&mut dyn Executor`，不得再向下传递 `&mut ClientSession`。
    /// 提交与回滚由本方法完成。
    ///
    /// # 参数
    /// * `f` - 处理函数
    ///
    /// # 返回
    /// 返回执行结果，错误类型由调用方决定。
    ///
    /// # 错误
    /// 返回回调错误，或经 `From` 转换的事务初始化、提交错误。
    /// 回调失败时尝试回滚，回滚失败只记录日志，不覆盖原错误。
    async fn with_transaction<F, R, E>(&self, f: F) -> std::result::Result<R, E>
    where
        F: for<'a> FnOnce(
                &'a mut dyn Executor,
            )
                -> Pin<Box<dyn Future<Output = std::result::Result<R, E>> + Send + 'a>>
            + Send,
        R: Send,
        E: From<Error> + Send,
    {
        let mut session = start_transaction_session(self).await?;

        match f(&mut session).await {
            Ok(result) => {
                commit_with_retry(&mut session).await?;
                Ok(result)
            },
            Err(error) => {
                abort_quietly(&mut session).await;
                Err(error)
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use mongodb::options::{ReadConcern, WriteConcern};

    use super::{
        COMMIT_RETRY_TIMEOUT, CommitErrorAction, CommitErrorLabel, commit_error_action, transaction_options,
    };

    #[test]
    fn unknown_commit_result_retries_before_timeout() {
        let elapsed = COMMIT_RETRY_TIMEOUT - Duration::from_millis(1);

        let action = commit_error_action(CommitErrorLabel::UnknownResult, elapsed);

        assert_eq!(action, CommitErrorAction::Retry);
    }

    #[test]
    fn unknown_commit_result_reports_unknown_at_timeout() {
        let action = commit_error_action(CommitErrorLabel::UnknownResult, COMMIT_RETRY_TIMEOUT);

        assert_eq!(action, CommitErrorAction::OutcomeUnknown);
    }

    #[test]
    fn commit_error_without_unknown_label_is_definite_failure() {
        let action = commit_error_action(CommitErrorLabel::Definite, Duration::ZERO);

        assert_eq!(action, CommitErrorAction::DefiniteFailure);
    }

    #[test]
    fn transactions_require_snapshot_reads_and_majority_commit_durability() {
        assert_eq!(transaction_options().read_concern, Some(ReadConcern::snapshot()));
        assert_eq!(transaction_options().write_concern, Some(WriteConcern::majority()));
    }
}
