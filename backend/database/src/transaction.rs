//! Transaction support for MongoDB operations
//!
//! This module provides transaction management for MongoDB operations,
//! allowing multiple operations to be executed atomically.

use async_trait::async_trait;
use mongodb::{
    error::UNKNOWN_TRANSACTION_COMMIT_RESULT,
    options::{SessionOptions, TransactionOptions, WriteConcern},
    Client, ClientSession,
};
use std::future::Future;
use std::pin::Pin;
use std::time::{Duration, Instant};

use crate::errors::{Error, Result};

const COMMIT_RETRY_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CommitErrorAction {
    Retry,
    OutcomeUnknown,
    DefiniteFailure,
}

/// 根据 MongoDB 错误标签与已用时间决定提交错误的处理方式。
fn commit_error_action(has_unknown_result_label: bool, elapsed: Duration) -> CommitErrorAction {
    if !has_unknown_result_label {
        return CommitErrorAction::DefiniteFailure;
    }
    if elapsed < COMMIT_RETRY_TIMEOUT {
        return CommitErrorAction::Retry;
    }
    CommitErrorAction::OutcomeUnknown
}

/// 构建保证已提交事务不会因副本集主节点切换回滚的默认选项。
fn transaction_options() -> TransactionOptions {
    TransactionOptions::builder()
        .write_concern(WriteConcern::majority())
        .build()
}

/// 在同一会话上重试结果未知的事务提交。
async fn commit_with_retry(session: &mut ClientSession) -> Result<()> {
    let started_at = Instant::now();
    loop {
        match session.commit_transaction().await {
            Ok(()) => return Ok(()),
            Err(error) => {
                let action = commit_error_action(
                    error.contains_label(UNKNOWN_TRANSACTION_COMMIT_RESULT),
                    started_at.elapsed(),
                );
                match action {
                    CommitErrorAction::Retry => continue,
                    CommitErrorAction::OutcomeUnknown => {
                        return Err(Error::CommitOutcomeUnknown(error));
                    }
                    CommitErrorAction::DefiniteFailure => return Err(Error::from(error)),
                }
            }
        }
    }
}

/// Trait for executing operations within a transaction context
#[async_trait]
pub trait Transactional {
    /// Executes an async function within a transaction with a custom error type.
    ///
    /// # Arguments
    ///
    /// * `f` - The async function to execute within the transaction
    ///
    /// # Returns
    ///
    /// Result of the function execution with caller-defined error type
    async fn with_transaction<F, R, E>(&self, f: F) -> std::result::Result<R, E>
    where
        F: for<'a> FnOnce(
                &'a mut ClientSession,
            )
                -> Pin<Box<dyn Future<Output = std::result::Result<R, E>> + Send + 'a>>
            + Send,
        R: Send,
        E: From<Error> + Send;
}

/// Implements transaction support for MongoDB client
#[async_trait]
impl Transactional for Client {
    /// 在事务中执行函数，并允许调用方指定错误类型。
    ///
    /// # 参数
    /// * `f` - 处理函数
    ///
    /// # 返回
    /// 返回执行结果，错误类型由调用方决定。
    ///
    /// # 错误
    /// 当事务初始化、提交、回滚失败时，使用调用方错误类型返回。
    async fn with_transaction<F, R, E>(&self, f: F) -> std::result::Result<R, E>
    where
        F: for<'a> FnOnce(
                &'a mut ClientSession,
            )
                -> Pin<Box<dyn Future<Output = std::result::Result<R, E>> + Send + 'a>>
            + Send,
        R: Send,
        E: From<Error> + Send,
    {
        let session_options = SessionOptions::builder().causal_consistency(true).build();
        let mut session = self
            .start_session()
            .with_options(session_options)
            .await
            .map_err(Error::from)
            .map_err(E::from)?;
        let txn_options = transaction_options();
        session
            .start_transaction()
            .with_options(txn_options)
            .await
            .map_err(Error::from)
            .map_err(E::from)?;

        match f(&mut session).await {
            Ok(result) => {
                commit_with_retry(&mut session).await.map_err(E::from)?;
                Ok(result)
            }
            Err(error) => {
                if let Err(abort_error) = session.abort_transaction().await {
                    tracing::warn!(
                        error = ?abort_error,
                        "failed to abort MongoDB transaction after callback error"
                    );
                }
                Err(error)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use mongodb::options::WriteConcern;

    use super::{commit_error_action, transaction_options, CommitErrorAction, COMMIT_RETRY_TIMEOUT};
    use std::time::Duration;

    #[test]
    fn unknown_commit_result_retries_before_timeout() {
        let elapsed = COMMIT_RETRY_TIMEOUT - Duration::from_millis(1);

        let action = commit_error_action(true, elapsed);

        assert_eq!(action, CommitErrorAction::Retry);
    }

    #[test]
    fn unknown_commit_result_reports_unknown_at_timeout() {
        let action = commit_error_action(true, COMMIT_RETRY_TIMEOUT);

        assert_eq!(action, CommitErrorAction::OutcomeUnknown);
    }

    #[test]
    fn commit_error_without_unknown_label_is_definite_failure() {
        let action = commit_error_action(false, Duration::ZERO);

        assert_eq!(action, CommitErrorAction::DefiniteFailure);
    }

    #[test]
    fn transactions_require_majority_commit_durability() {
        assert_eq!(
            transaction_options().write_concern,
            Some(WriteConcern::majority())
        );
    }
}
