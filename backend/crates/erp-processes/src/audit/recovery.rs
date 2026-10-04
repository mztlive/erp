//! 原命令的只读查证；不重放事务，不改变首次未知提交的错误来源。

use crate::{Error, Result};

/// 解释失败事务后对原命令及原结果视图的一次只读查证。
///
/// # 参数
/// * `original` - 首次失败或未知提交错误。
/// * `recovered` - 独立领域回执和当前授权结果视图的查证结果。
///
/// # 返回
/// 查证成功时返回同一命令的原结果。
///
/// # 错误
/// 原结果缺失时保留首次错误；未知提交查证失败时保留原未知错误及来源；
/// 普通失败查证发生明确冲突或读取失败时返回该查证错误。
pub fn recover_command<T>(original: Error, recovered: Result<Option<T>>) -> Result<T> {
    match recovered {
        Ok(Some(result)) => Ok(result),
        Ok(None) => Err(original),
        Err(_) if matches!(original, Error::OutcomeUnknown(_)) => Err(original),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use mongodb::error::Error as MongoError;
    use persistence_core::Error as PersistenceError;

    use super::*;

    fn unknown() -> Error {
        Error::OutcomeUnknown(PersistenceError::CommitOutcomeUnknown(MongoError::custom("first commit")))
    }

    #[test]
    fn recovery_keeps_first_unknown_source_for_missing_conflict_and_failed_view() {
        for result in [
            Ok(None),
            Err(Error::ConflictError("损坏或异载荷回执".into())),
            Err(Error::Forbidden("当前视图不可见".into())),
            Err(Error::Internal("原结果读取失败".into())),
        ] {
            let error = recover_command::<u32>(unknown(), result).unwrap_err();
            let Error::OutcomeUnknown(PersistenceError::CommitOutcomeUnknown(source)) = error else {
                panic!("不得覆盖首次未知提交错误");
            };
            assert_eq!(source.get_custom::<&str>(), Some(&"first commit"));
        }
    }

    #[test]
    fn original_result_recovers_and_normal_failure_keeps_explicit_conflict() {
        assert_eq!(recover_command(unknown(), Ok(Some(17))).unwrap(), 17);
        let error = recover_command::<u32>(
            Error::BusinessLogicError("首次失败".into()),
            Err(Error::ConflictError("回执冲突".into())),
        )
        .unwrap_err();
        assert!(matches!(error, Error::ConflictError(message) if message == "回执冲突"));
        let error =
            recover_command::<u32>(Error::BusinessLogicError("首次失败".into()), Ok(None)).unwrap_err();
        assert!(matches!(error, Error::BusinessLogicError(message) if message == "首次失败"));
    }
}
