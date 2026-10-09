//! 仅重试公开选品写入中已经进入回滚路径的瞬态事务冲突。

use std::future::Future;
use std::time::Duration;

use persistence_core::Error as PersistenceError;

use crate::{Error, Result};

const RETRY_DELAYS: [Duration; 3] =
    [Duration::from_millis(20), Duration::from_millis(40), Duration::from_millis(80)];

/// 区分事务体失败与初始化、提交阶段失败。
///
/// 事务体错误仅在 `with_transaction` 已执行回滚路径并返回后交给重试入口。
#[derive(Debug)]
pub(super) enum SelectionAttemptError {
    AbortedBody(Error),
    Transaction(PersistenceError),
}

impl From<PersistenceError> for SelectionAttemptError {
    fn from(error: PersistenceError) -> Self {
        Self::Transaction(error)
    }
}

impl SelectionAttemptError {
    fn retryable(&self) -> bool {
        matches!(
            self,
            Self::AbortedBody(Error::TransientTransaction(PersistenceError::TransientTransactionConflict(_)))
        )
    }

    fn into_error(self) -> Error {
        match self {
            Self::AbortedBody(error) => error,
            Self::Transaction(error) => Error::from(error),
        }
    }
}

/// 以原始个人请求最多重试三次已回滚的瞬态事务冲突。
///
/// # 参数
/// * `operation` - 每次创建新事务并克隆相同请求的操作；限流与解码必须在外部执行
///
/// # 返回
/// 首次成功结果；初始化、提交、业务和授权失败直接返回。
///
/// # 错误
/// 只有事务体中的 `TransientTransactionConflict` 可重试；结果未知和业务版本冲突绝不重放。
pub(super) async fn retry_public_write<T, F, Fut>(mut operation: F) -> Result<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = std::result::Result<T, SelectionAttemptError>>,
{
    let mut retries = 0;
    loop {
        match operation().await {
            Ok(result) => return Ok(result),
            Err(error) if retries < RETRY_DELAYS.len() && error.retryable() => {
                tokio::time::sleep(RETRY_DELAYS[retries]).await;
                retries += 1;
            },
            Err(error) => return Err(error.into_error()),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::future::ready;

    use mongodb::error::Error as MongoError;

    use super::*;

    fn aborted_conflict() -> SelectionAttemptError {
        SelectionAttemptError::AbortedBody(Error::TransientTransaction(
            PersistenceError::TransientTransactionConflict(MongoError::custom("transaction aborted")),
        ))
    }

    #[tokio::test]
    async fn aborted_conflicts_retry_with_same_command_until_success() {
        let attempts = Cell::new(0);
        let original_key = "person-command-key".to_string();
        let result = retry_public_write(|| {
            let count = attempts.get();
            attempts.set(count + 1);
            let key = original_key.clone();
            ready(if count < 2 { Err(aborted_conflict()) } else { Ok((key, 7_u64)) })
        })
        .await
        .unwrap();
        assert_eq!(attempts.get(), 3);
        assert_eq!(result, (original_key, 7));
    }

    #[tokio::test]
    async fn persistent_aborted_conflict_stops_after_initial_and_three_retries() {
        let attempts = Cell::new(0);
        let result = retry_public_write::<(), _, _>(|| {
            attempts.set(attempts.get() + 1);
            ready(Err(aborted_conflict()))
        })
        .await;
        assert_eq!(attempts.get(), 4);
        assert!(matches!(
            result,
            Err(Error::TransientTransaction(PersistenceError::TransientTransactionConflict(_)))
        ));
    }

    #[tokio::test]
    async fn unknown_commit_business_and_authorization_failures_execute_once() {
        for error in [
            SelectionAttemptError::AbortedBody(Error::OutcomeUnknown(
                PersistenceError::CommitOutcomeUnknown(MongoError::custom("unknown")),
            )),
            SelectionAttemptError::Transaction(PersistenceError::CommitOutcomeUnknown(MongoError::custom(
                "unknown commit",
            ))),
            SelectionAttemptError::Transaction(PersistenceError::TransientTransactionConflict(
                MongoError::custom("not a rolled back body"),
            )),
            SelectionAttemptError::AbortedBody(Error::ConflictError("personal version changed".into())),
            SelectionAttemptError::AbortedBody(Error::Forbidden("password changed".into())),
            SelectionAttemptError::AbortedBody(Error::ValidationError("address invalid".into())),
            SelectionAttemptError::AbortedBody(Error::ReceiptDuplicate(PersistenceError::DuplicateKey(
                MongoError::custom("duplicate"),
            ))),
        ] {
            let attempts = Cell::new(0);
            let mut failure = Some(error);
            let result = retry_public_write::<(), _, _>(|| {
                attempts.set(attempts.get() + 1);
                ready(Err(failure.take().expect("nonretry error is consumed once")))
            })
            .await;
            assert!(result.is_err());
            assert_eq!(attempts.get(), 1);
        }
    }
}
