//! 审批启动与撤回的有界结果查证；原写事务不得重放。

use std::future::Future;

use erp_workflow::service::approval::execution::command_recovery_delay;
use tokio::time::sleep;

use crate::{Error, Result};

const RECOVERY_ATTEMPTS: usize = 8;

/// 在失败写事务退出后，有界查证同一审批命令的已提交结果。
///
/// # 参数
/// * `original_error` - 首次写事务错误；查证耗尽时原样返回。
/// * `probe` - 一次只读查证；每次调用须建立独立执行上下文并核对原命令身份。
///
/// # 返回
/// 首次查证命中时返回原命令的已提交结果。
///
/// # 错误
/// 回执竞争、瞬态事务和提交结果未知继续查证，其他查证错误立即返回；
/// 八次均未确认时返回首次错误。此策略不适用于所有查证错误均须保留未知提交的入口。
pub(crate) async fn recover_approval_command<T, F, Fut>(original_error: Error, probe: F) -> Result<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<Option<T>>>,
{
    recover_with_wait(original_error, probe, |attempt| sleep(command_recovery_delay(attempt))).await
}

/// 执行同一有界查证状态机；等待注入仅替换计时，不替换结果和错误判定。
async fn recover_with_wait<T, F, Fut, W, Wait>(original_error: Error, mut probe: F, mut wait: W) -> Result<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<Option<T>>>,
    W: FnMut(usize) -> Wait,
    Wait: Future<Output = ()>,
{
    for attempt in 0..RECOVERY_ATTEMPTS {
        match probe().await {
            Ok(Some(result)) => return Ok(result),
            Ok(None) => {},
            Err(error) if error.command_may_have_committed() => {},
            Err(error) => return Err(error),
        }
        if attempt + 1 < RECOVERY_ATTEMPTS {
            wait(attempt).await;
        }
    }
    Err(original_error)
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::collections::VecDeque;
    use std::future::ready;

    use mongodb::error::Error as MongoError;
    use persistence_core::Error as PersistenceError;

    use super::{recover_approval_command, recover_with_wait};
    use crate::Error;

    fn unknown(source: &'static str) -> Error {
        Error::OutcomeUnknown(PersistenceError::CommitOutcomeUnknown(MongoError::custom(source)))
    }

    #[tokio::test]
    async fn committed_result_returns_without_further_probes() {
        let calls = Cell::new(0);
        let result = recover_approval_command(unknown("first write"), || {
            calls.set(calls.get() + 1);
            ready(Ok(Some("original-instance")))
        })
        .await
        .unwrap();
        assert_eq!(result, "original-instance");
        assert_eq!(calls.get(), 1);
    }

    #[tokio::test]
    async fn unresolved_and_uncertain_probes_wait_before_recovering() {
        let mut outcomes = VecDeque::from([
            Err(unknown("probe commit")),
            Err(Error::ReceiptDuplicate(PersistenceError::DuplicateKey(MongoError::custom("receipt")))),
            Err(Error::TransientTransaction(PersistenceError::TransientTransactionConflict(
                MongoError::custom("transaction"),
            ))),
            Ok(None),
            Ok(Some("original-instance")),
        ]);
        let waits = RefCell::new(Vec::new());
        let result = recover_with_wait(
            unknown("first write"),
            || ready(outcomes.pop_front().expect("no probe after success")),
            |attempt| {
                waits.borrow_mut().push(attempt);
                ready(())
            },
        )
        .await
        .unwrap();
        assert_eq!(result, "original-instance");
        assert!(outcomes.is_empty());
        assert_eq!(*waits.borrow(), vec![0, 1, 2, 3]);
    }

    #[tokio::test]
    async fn exhausted_uncertain_probes_preserve_first_unknown_source() {
        let calls = Cell::new(0);
        let waits = RefCell::new(Vec::new());
        let error = recover_with_wait::<(), _, _, _, _>(
            unknown("first write"),
            || {
                calls.set(calls.get() + 1);
                ready(Err(unknown("later probe")))
            },
            |attempt| {
                waits.borrow_mut().push(attempt);
                ready(())
            },
        )
        .await
        .unwrap_err();
        let Error::OutcomeUnknown(PersistenceError::CommitOutcomeUnknown(source)) = error else {
            panic!("the first write error must survive exhaustion");
        };
        assert_eq!(source.get_custom::<&str>(), Some(&"first write"));
        assert_eq!(calls.get(), 8);
        assert_eq!(*waits.borrow(), vec![0, 1, 2, 3, 4, 5, 6]);
    }

    #[tokio::test]
    async fn missing_results_preserve_original_error_after_eight_probes() {
        let calls = Cell::new(0);
        let error = recover_with_wait::<(), _, _, _, _>(
            Error::ConflictError("original conflict".into()),
            || {
                calls.set(calls.get() + 1);
                ready(Ok(None))
            },
            |_| ready(()),
        )
        .await
        .unwrap_err();
        assert!(matches!(error, Error::ConflictError(message) if message == "original conflict"));
        assert_eq!(calls.get(), 8);
    }

    #[tokio::test]
    async fn explicit_conflict_stops_immediately_even_after_unknown_write() {
        let calls = Cell::new(0);
        let waits = Cell::new(0);
        let error = recover_with_wait::<(), _, _, _, _>(
            unknown("first write"),
            || {
                calls.set(calls.get() + 1);
                ready(Err(Error::ConflictError("receipt identity mismatch".into())))
            },
            |_| {
                waits.set(waits.get() + 1);
                ready(())
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(error, Error::ConflictError(message) if message == "receipt identity mismatch"));
        assert_eq!(calls.get(), 1);
        assert_eq!(waits.get(), 0);
    }
}
