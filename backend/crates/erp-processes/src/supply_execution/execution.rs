//! 供应商外呼只在原意图提交成功之后进入；不把执行器传给外部步骤。
use std::future::Future;

use crate::Result;

/// 原意图失败时禁止进入供应商步骤，保留最先返回的错误。
pub(super) async fn after_intent<I, N, F, T>(intent: I, next: N) -> Result<T>
where
    I: Future<Output = Result<()>>,
    N: FnOnce() -> F,
    F: Future<Output = Result<T>>,
{
    intent.await?;
    next().await
}

/// 已冻结的供应商结果直接复用；外呼及持久化只在不存在结果时执行。
pub(super) async fn reuse_or_prepare<T, N, F>(prepared: Option<T>, fresh: N) -> Result<T>
where
    N: FnOnce() -> F,
    F: Future<Output = Result<T>>,
{
    match prepared {
        Some(prepared) => Ok(prepared),
        None => fresh().await,
    }
}
/// 仅接收最终事务结果；任意事务错误后只回读一次，回读错误优先透出。
pub(super) async fn recover_final_result<T, N, F>(result: Result<T>, replay: N) -> Result<T>
where
    N: FnOnce() -> F,
    F: Future<Output = Result<Option<T>>>,
{
    match result {
        Ok(result) => Ok(result),
        Err(error) => {
            if let Some(result) = replay().await? {
                return Ok(result);
            }
            Err(error)
        },
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::{after_intent, recover_final_result, reuse_or_prepare};
    use crate::Error;

    #[tokio::test]
    async fn committed_intent_precedes_external_dispatch_and_result() {
        let steps = Arc::new(Mutex::new(Vec::new()));
        let intent_steps = steps.clone();
        let next_steps = steps.clone();
        let value = after_intent(
            async move {
                intent_steps.lock().unwrap().push("intent_committed");
                Ok(())
            },
            move || async move {
                next_steps.lock().unwrap().extend(["gateway", "result"]);
                Ok(7)
            },
        )
        .await
        .unwrap();
        assert_eq!(value, 7);
        assert_eq!(*steps.lock().unwrap(), ["intent_committed", "gateway", "result"]);
    }
    #[tokio::test]
    async fn failed_intent_never_dispatches_and_result_error_is_preserved() {
        let called = Arc::new(Mutex::new(false));
        let external = called.clone();
        let result = after_intent(async { Err(Error::ConflictError("intent".into())) }, move || async move {
            *external.lock().unwrap() = true;
            Ok(())
        })
        .await;
        assert!(matches!(result,Err(Error::ConflictError(message)) if message=="intent"));
        assert!(!*called.lock().unwrap());
        let result =
            after_intent(async { Ok(()) }, || async { Err::<(), _>(Error::NotFound("result".into())) }).await;
        assert!(matches!(result,Err(Error::NotFound(message)) if message=="result"));
    }

    #[tokio::test]
    async fn durable_prepared_reuse_skips_external_step_even_after_final_failure() {
        let calls = Arc::new(Mutex::new(0));
        let frozen = reuse_or_prepare(Some("first-result"), || async {
            *calls.lock().unwrap() += 1;
            Ok("new-result")
        })
        .await
        .unwrap();
        assert_eq!(frozen, "first-result");
        assert_eq!(*calls.lock().unwrap(), 0);
        let failure =
            recover_final_result::<(), _, _>(Err(Error::ConflictError("final CAS".into())), || async {
                Ok(None)
            })
            .await;
        assert!(matches!(failure, Err(Error::ConflictError(_))));
        let frozen = reuse_or_prepare(Some("first-result"), || async {
            *calls.lock().unwrap() += 1;
            Ok("second-result")
        })
        .await
        .unwrap();
        assert_eq!(frozen, "first-result");
        assert_eq!(*calls.lock().unwrap(), 0);
    }
    #[tokio::test]
    async fn fresh_prepare_failure_does_not_enter_final_recovery() {
        let result =
            reuse_or_prepare(None, || async { Err::<(), _>(Error::NotFound("prepare".into())) }).await;
        assert!(matches!(result,Err(Error::NotFound(message)) if message=="prepare"));
    }
    #[tokio::test]
    async fn final_recovery_reads_once_for_any_error_and_preserves_first_read_error() {
        let calls = Arc::new(Mutex::new(0));
        for error in [
            Error::ConflictError("cas".into()),
            Error::BusinessLogicError("rule".into()),
            Error::NotFound("evidence".into()),
        ] {
            let result = recover_final_result::<u8, _, _>(Err(error), || async {
                *calls.lock().unwrap() += 1;
                Ok(Some(7))
            })
            .await
            .unwrap();
            assert_eq!(result, 7);
        }
        assert_eq!(*calls.lock().unwrap(), 3);
        let result =
            recover_final_result::<u8, _, _>(Err(Error::ConflictError("original".into())), || async {
                Err(Error::Internal("receipt read".into()))
            })
            .await;
        assert!(matches!(result,Err(Error::Internal(message)) if message=="receipt read"));
        let result =
            recover_final_result::<u8, _, _>(Err(Error::ConflictError("original".into())), || async {
                Ok(None)
            })
            .await;
        assert!(matches!(result,Err(Error::ConflictError(message)) if message=="original"));
        let result = recover_final_result(Ok(9), || async {
            Err(Error::Internal("successful final transaction must not replay".into()))
        })
        .await
        .unwrap();
        assert_eq!(result, 9);
    }
}
