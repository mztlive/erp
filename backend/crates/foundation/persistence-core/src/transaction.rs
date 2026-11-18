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
use crate::executor::TransactionExecutor;

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

// 提交失败后只对结果未知标签使用等待预算；达到时限保留未知状态，其他失败不走该重试分支。
fn commit_error_action(label: CommitErrorLabel, elapsed: Duration) -> CommitErrorAction {
    if label != CommitErrorLabel::UnknownResult {
        return CommitErrorAction::DefiniteFailure;
    }
    if elapsed < COMMIT_RETRY_TIMEOUT {
        return CommitErrorAction::Retry;
    }
    CommitErrorAction::OutcomeUnknown
}

// 统一用 `snapshot` 读取和 `majority` 提交，使业务与授权事实可在同一 majority-committed 快照中重验。
// 默认 `local` 在分片部署不保证跨集合、跨分片看到同一快照。
fn transaction_options() -> TransactionOptions {
    TransactionOptions::builder()
        .read_concern(ReadConcern::snapshot())
        .write_concern(WriteConcern::majority())
        .build()
}

// 在同一会话上重试带结果未知标签的提交，不重新执行回调。超过重试预算仍未知则保留专用错误，
// 其他驱动错误按原分类返回。
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

// 开始业务回调前创建因果一致的会话，并用统一快照及提交选项开启事务；初始化失败时不运行回调。
async fn start_transaction_session(client: &Client) -> Result<ClientSession> {
    let session_options = SessionOptions::builder().causal_consistency(true).build();
    let mut session = client.start_session().with_options(session_options).await?;

    let txn_options = transaction_options();
    session.start_transaction().with_options(txn_options).await?;
    Ok(session)
}

// 回调已失败时尽力回滚；回滚错误只记日志，以免覆盖调用方需要处理的原失败。
async fn abort_quietly(session: &mut ClientSession) {
    if let Err(abort_error) = session.abort_transaction().await {
        tracing::warn!(error = ?abort_error, "failed to abort MongoDB transaction after callback error");
    }
}

/// 在事务上下文中执行操作。
#[async_trait]
pub trait Transactional {
    /// Service 或 Process 需要原子完成多步读写时，向回调交出同一个受管执行器，
    /// 由设施启动事务并在提交成功后返回结果。回调失败时尝试回滚，回滚错误仅记录；
    /// 事务初始化或提交失败转换为调用方错误，提交未知不重跑回调。
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
    // 事务开始后仅让业务回调接触受管 `Executor`，不把原始会话传到仓储。回调成功再提交；失败时回滚且保留原错误。
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

        let outcome = f(&mut TransactionExecutor::new(&mut session)).await;

        match outcome {
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
