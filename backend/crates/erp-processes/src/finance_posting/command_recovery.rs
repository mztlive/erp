//! 财务命令查证的纯错误合同；数据库查询和对象授权仍由具体用例负责。

use crate::{Error, Result};

pub(super) struct RecoveredResource {
    pub id: String,
    pub original_unknown: Option<Error>,
}

/// 解释原事务错误后的独立回执查询结果。
/// # 参数
/// * `original` - 首次事务失败或未知提交错误。
/// * `receipt_result` - 针对原命令的独立财务回执查询结果。
/// # 返回
/// 返回原对象 ID 及后续原视图读取必须保留的未知错误。
/// # 错误
/// 未执行时保留原错误；查证失败时保留首次未知错误或正常查询错误。
pub(super) fn recovered_resource(
    original: Error,
    receipt_result: Result<Option<String>>,
) -> Result<RecoveredResource> {
    let unknown = matches!(original, Error::OutcomeUnknown(_));
    match receipt_result {
        Ok(Some(id)) => Ok(RecoveredResource { id, original_unknown: unknown.then_some(original) }),
        Ok(None) => Err(original),
        Err(_) if unknown => Err(original),
        Err(error) => Err(error),
    }
}

/// 完成原视图查证，禁止以新的读取错误覆盖首次未知提交错误。
/// # 参数
/// * `view` - 原结果对象的实际读取结果。
/// * `original_unknown` - 回执恢复时保留的首次未知提交错误。
/// # 返回
/// 成功时返回原视图。
/// # 错误
/// 视图读取失败时返回首次未知错误；普通失败沿用具体读取错误。
pub(super) fn recovered_view<T>(view: Result<T>, original_unknown: Option<Error>) -> Result<T> {
    match (view, original_unknown) {
        (Ok(value), _) => Ok(value),
        (Err(_), Some(original)) => Err(original),
        (Err(error), None) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use application_core::{AuditActor, CommandReceipt};
    use erp_core::AccountKind;
    use erp_finance::FinanceCommandReceipt;
    use mongodb::error::Error as MongoError;
    use persistence_core::Error as PersistenceError;

    use super::*;

    fn unknown() -> Error {
        Error::OutcomeUnknown(PersistenceError::CommitOutcomeUnknown(MongoError::custom("original unknown")))
    }

    fn command(value: u32) -> CommandReceipt {
        let actor = AuditActor::new("actor".into(), "finance".into(), AccountKind::Admin);
        CommandReceipt::from_payload("finance-", actor.id(), "invoice.commit", "invoice", "key", &value)
            .unwrap()
    }

    fn confirm_original_unknown(error: Error) {
        let Error::OutcomeUnknown(PersistenceError::CommitOutcomeUnknown(source)) = error else {
            panic!("必须保留原未知提交错误");
        };
        assert_eq!(source.get_custom::<&'static str>(), Some(&"original unknown"));
    }

    #[test]
    fn unknown_lookup_failure_missing_damaged_or_conflicting_receipt_keeps_original_error() {
        let command = command(10);
        let receipt =
            FinanceCommandReceipt::resource(&command, "invoice-1".into(), "event-1".into()).unwrap();
        let mut damaged = receipt.clone();
        damaged.result_schema_version = 2;
        let results = [
            Ok(None),
            Err(Error::Internal("查询不可用".into())),
            damaged.resource_id(&command).map(Some).map_err(Error::from),
            receipt.resource_id(&self::command(20)).map(Some).map_err(Error::from),
        ];
        for result in results {
            match recovered_resource(unknown(), result) {
                Err(error) => confirm_original_unknown(error),
                Ok(_) => panic!("查证失败不能生成已提交结果"),
            }
        }
    }

    #[test]
    fn recovered_original_view_keeps_unknown_error_on_read_failure() {
        let recovered = recovered_resource(unknown(), Ok(Some("invoice-1".into()))).unwrap();
        assert_eq!(recovered.id, "invoice-1");
        confirm_original_unknown(
            recovered_view::<u32>(Err(Error::NotFound("视图缺失".into())), recovered.original_unknown)
                .unwrap_err(),
        );
        let recovered = recovered_resource(unknown(), Ok(Some("invoice-1".into()))).unwrap();
        assert_eq!(recovered_view(Ok(17_u32), recovered.original_unknown).unwrap(), 17);
    }

    #[test]
    fn known_failure_preserves_first_error_if_missing_and_exposes_read_conflict_if_present() {
        let result = recovered_resource(Error::BusinessLogicError("原业务失败".into()), Ok(None));
        assert!(matches!(result, Err(Error::BusinessLogicError(value)) if value == "原业务失败"));
        let result = recovered_resource(
            Error::ConflictError("原事务失败".into()),
            Err(Error::Internal("回执损坏".into())),
        );
        assert!(matches!(result, Err(Error::Internal(value)) if value == "回执损坏"));
        let error = recovered_view::<u32>(Err(Error::NotFound("原视图不存在".into())), None).unwrap_err();
        assert!(matches!(error, Error::NotFound(value) if value == "原视图不存在"));
    }
}
