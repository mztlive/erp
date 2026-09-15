//! 事务失败后的命令结果恢复；不重试写入、不改变错误优先级。
use async_trait::async_trait;

use super::*;
#[async_trait]
pub(super) trait CommandResultPort: Sync {
    async fn command(
        &self,
        key: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierOfferingCommand>>;
}
#[async_trait]
impl CommandResultPort for SupplierOfferingService {
    async fn command(
        &self,
        key: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierOfferingCommand>> {
        self.command_record(key, executor).await
    }
}
/// 任意事务错误触发一次重读；重读错、指纹错、结果解码错优先于原错。
pub(super) async fn resolve<T: DeserializeOwned, E: From<Error>, P: CommandResultPort>(
    port: &P,
    transaction_result: std::result::Result<T, E>,
    idempotency_key: &str,
    operation: &str,
    fingerprint: &str,
    executor: &mut dyn Executor,
) -> std::result::Result<T, E> {
    match transaction_result {
        Ok(result) => Ok(result),
        Err(error) => match port.command(idempotency_key, executor).await? {
            Some(command) => {
                command
                    .ensure_replayable(operation, fingerprint)
                    .map_err(|e| Error::ConflictError(e.to_string()))?;
                command.replay_result().map_err(|e| Error::Internal(e.to_string()).into())
            },
            None => Err(error),
        },
    }
}
#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    struct Marker(u64);
    impl Executor for Marker {
        fn session(&mut self) -> Option<&mut mongodb::ClientSession> {
            None
        }
    }
    struct Recorder {
        pointer: usize,
        calls: Mutex<usize>,
        command: Option<SupplierOfferingCommand>,
        read_error: bool,
    }
    #[async_trait]
    impl CommandResultPort for Recorder {
        async fn command(
            &self,
            key: &str,
            executor: &mut dyn Executor,
        ) -> Result<Option<SupplierOfferingCommand>> {
            assert_eq!(executor as *mut dyn Executor as *mut () as usize, self.pointer);
            assert_eq!(key, " key ");
            *self.calls.lock().unwrap() += 1;
            if self.read_error {
                return Err(Error::NotFound("read failure".into()));
            }
            Ok(self.command.clone())
        }
    }
    const FP: &str = "0000000000000000000000000000000000000000000000000000000000000000";
    fn command() -> SupplierOfferingCommand {
        SupplierOfferingCommand::with_result("command", "key", CREATE_OFFERING_COMMAND, FP, &73u64).unwrap()
    }
    async fn invoke(
        transaction: Result<u64>,
        command: Option<SupplierOfferingCommand>,
        read_error: bool,
        operation: &str,
    ) -> (Result<u64>, usize) {
        let mut executor = Marker(166);
        let port = Recorder {
            pointer: &mut executor as *mut Marker as usize,
            calls: Mutex::new(0),
            command,
            read_error,
        };
        let result = resolve(&port, transaction, " key ", operation, FP, &mut executor).await;
        assert_eq!(executor.0, 166);
        (result, port.calls.into_inner().unwrap())
    }
    #[tokio::test]
    async fn committed_success_does_not_read_and_any_error_recovers_original_result() {
        let (result, calls) = invoke(Ok(9), None, true, CREATE_OFFERING_COMMAND).await;
        assert_eq!(result.unwrap(), 9);
        assert_eq!(calls, 0);
        for error in [
            Error::ConflictError("write".into()),
            Error::Internal("write".into()),
            Error::ValidationError("write".into()),
        ] {
            let (result, calls) = invoke(Err(error), Some(command()), false, CREATE_OFFERING_COMMAND).await;
            assert_eq!(result.unwrap(), 73);
            assert_eq!(calls, 1);
        }
    }
    #[tokio::test]
    async fn missing_receipt_retains_transaction_error_but_read_error_wins() {
        let (result, calls) =
            invoke(Err(Error::Forbidden("original".into())), None, false, CREATE_OFFERING_COMMAND).await;
        assert!(matches!(result,Err(Error::Forbidden(ref e)) if e=="original"));
        assert_eq!(calls, 1);
        let (result, _) =
            invoke(Err(Error::Forbidden("original".into())), None, true, CREATE_OFFERING_COMMAND).await;
        assert!(matches!(result,Err(Error::NotFound(ref e)) if e=="read failure"));
    }
    #[tokio::test]
    async fn mismatched_operation_and_bad_stored_result_preserve_original_errors() {
        let (result, _) =
            invoke(Err(Error::Internal("write".into())), Some(command()), false, REVISE_OFFERING_COMMAND)
                .await;
        assert!(matches!(result,Err(Error::ConflictError(ref e)) if e=="幂等键已用于不同的供给命令"));
        let mut bad = command();
        bad.result_json = "{".into();
        let (result, _) =
            invoke(Err(Error::Internal("write".into())), Some(bad), false, CREATE_OFFERING_COMMAND).await;
        assert!(matches!(result,Err(Error::Internal(ref e)) if e=="供给命令结果反序列化失败"));
    }
}
