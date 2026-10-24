//! 财务领域应用错误，并保留唯一索引冲突的既有映射。

use application_core::{ValidationIssues, impl_application_error_conversions, impl_domain_error_class};
use persistence_core::impl_repository_error;

/// 财务领域结果别名。
pub type Result<T> = std::result::Result<T, Error>;

impl_application_error_conversions!(Error);

/// 财务账户、发票、付款与成本错误。
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("系统内部错误: {0}")]
    Internal(String),

    #[error("数据不存在: {0}")]
    NotFound(String),

    #[error("参数验证失败: {0}")]
    ValidationError(String),

    /// 字段校验明细，跨边界不得压成字符串。
    #[error(transparent)]
    InvalidInput(#[from] ValidationIssues),

    #[error("业务逻辑错误: {0}")]
    BusinessLogicError(String),

    #[error("数据冲突: {0}")]
    ConflictError(String),

    #[error("数据冲突: 数据已存在，请勿重复提交")]
    ReceiptDuplicate(#[source] persistence_core::Error),

    #[error("数据冲突: 并发事务冲突，请重试")]
    TransientTransaction(#[source] persistence_core::Error),

    #[error("权限不足: {0}")]
    Forbidden(String),

    #[error("认证失败: {0}")]
    Unauthenticated(String),

    #[error(transparent)]
    Logic(#[from] erp_core::Error),

    #[error("操作结果暂无法确认，请查询当前状态后再决定是否重试")]
    OutcomeUnknown(#[source] persistence_core::Error),

    #[error("数据库错误：{0}")]
    RepositoryError(persistence_core::Error),
}

impl_domain_error_class!(Error);

impl_repository_error!(
    Error,
    duplicate = |_error| { Error::ConflictError("数据已存在，请勿重复提交".to_string()) }
);

#[cfg(test)]
mod tests {
    use std::error::Error as StdError;

    use application_core::{ErrorClass, ValidationIssues};
    use mongodb::bson::{deserialize_from_document, doc};
    use mongodb::error::{Error as MongoError, ErrorKind, WriteError, WriteFailure};

    use super::Error;

    #[test]
    fn finance_unique_conflicts_keep_original_messages() {
        for index in [
            "uk_customer_receipts_no",
            "uk_receivable_accounts_sales_order",
            "uk_invoices_coded",
            "uk_unknown_index",
        ] {
            let write_error: WriteError = deserialize_from_document(doc! {
                "code": 11000,
                "codeName": "DuplicateKey",
                "errmsg": format!("E11000 duplicate key collection: erp.finance index: {index} dup key: {{ id: 1 }}"),
                "errInfo": null,
            })
            .expect("唯一键错误样本应可反序列化");
            let mongo_error: MongoError = ErrorKind::Write(WriteFailure::WriteError(write_error)).into();
            let repository_error = persistence_core::Error::from(mongo_error);
            assert_eq!(repository_error.duplicate_index_name(), Some(index));
            let error = Error::from(repository_error);
            assert_eq!(error.class(), ErrorClass::Conflict);
            assert_eq!(error.to_string(), "数据冲突: 数据已存在，请勿重复提交");
        }
        let error = Error::from(persistence_core::Error::DuplicateKey(MongoError::custom("duplicate key")));
        assert_eq!(error.class(), ErrorClass::Conflict);
        assert_eq!(error.to_string(), "数据冲突: 数据已存在，请勿重复提交");
    }

    #[test]
    fn optimistic_lock_maps_to_conflict_refresh_message() {
        let error = Error::from(persistence_core::Error::OptimisticLockingError);
        assert_eq!(error.class(), ErrorClass::Conflict);
        assert_eq!(error.to_string(), "数据冲突: 数据已被其他请求修改，请刷新后重试");
    }

    #[test]
    fn standard_domain_classification_keeps_every_variant_category() {
        let database = || persistence_core::Error::DatabaseError(MongoError::custom("database"));
        let cases = [
            (Error::Internal("internal".into()), ErrorClass::Internal),
            (Error::Logic(erp_core::Error::from("rule")), ErrorClass::Internal),
            (Error::RepositoryError(database()), ErrorClass::Internal),
            (Error::OutcomeUnknown(database()), ErrorClass::Internal),
            (Error::ConflictError("conflict".into()), ErrorClass::Conflict),
            (Error::ReceiptDuplicate(database()), ErrorClass::Conflict),
            (Error::TransientTransaction(database()), ErrorClass::Conflict),
            (Error::BusinessLogicError("rule".into()), ErrorClass::BusinessRule),
            (Error::ValidationError("required".into()), ErrorClass::BusinessRule),
            (
                Error::InvalidInput(ValidationIssues::field("amount", "金额必须大于零")),
                ErrorClass::BusinessRule,
            ),
            (Error::NotFound("missing".into()), ErrorClass::BusinessRule),
            (Error::Forbidden("scope".into()), ErrorClass::Forbidden),
            (Error::Unauthenticated("account".into()), ErrorClass::Forbidden),
        ];
        for (error, expected) in cases {
            assert_eq!(error.class(), expected, "{error}");
        }
    }

    #[test]
    fn transaction_conversion_preserves_source_and_public_instructions() {
        let transient =
            persistence_core::Error::TransientTransactionConflict(MongoError::custom("transient-source"));
        let error = Error::from(transient);
        assert!(matches!(error, Error::TransientTransaction(_)));
        assert_eq!(error.class(), ErrorClass::Conflict);
        assert_eq!(error.to_string(), "数据冲突: 并发事务冲突，请重试");
        let source = error.source().unwrap().downcast_ref::<persistence_core::Error>().unwrap();
        let persistence_core::Error::TransientTransactionConflict(mongo) = source else {
            panic!("source 必须保留瞬态事务原始类型");
        };
        assert_eq!(mongo.get_custom::<&str>().copied(), Some("transient-source"));

        let unknown = persistence_core::Error::CommitOutcomeUnknown(MongoError::custom("unknown-source"));
        let error = Error::from(unknown);
        assert!(matches!(error, Error::OutcomeUnknown(_)));
        assert_eq!(error.class(), ErrorClass::Internal);
        assert_eq!(error.to_string(), "操作结果暂无法确认，请查询当前状态后再决定是否重试");
        let source = error.source().unwrap().downcast_ref::<persistence_core::Error>().unwrap();
        let persistence_core::Error::CommitOutcomeUnknown(mongo) = source else {
            panic!("source 必须保留提交结果未知原始类型");
        };
        assert_eq!(mongo.get_custom::<&str>().copied(), Some("unknown-source"));
    }

    #[test]
    fn structured_application_validation_survives_domain_conversion() {
        let issues = ValidationIssues::field("allocations[0].amount", "金额必须大于零");
        let error = Error::from(application_core::Error::InvalidInput(issues.clone()));
        assert_eq!(error.class(), ErrorClass::BusinessRule);
        assert_eq!(error.to_string(), "请修改标记的字段后重新提交");
        let Error::InvalidInput(actual) = error else {
            panic!("字段错误不得转为字符串");
        };
        assert_eq!(actual, issues);
    }
}
