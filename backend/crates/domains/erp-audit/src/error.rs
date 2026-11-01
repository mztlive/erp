//! 审计领域应用错误，保留原有的审计错误映射。

use application_core::{ValidationIssues, impl_application_error_conversions, impl_domain_error_class};
use persistence_core::impl_repository_error;

/// 审计领域结果别名。
pub type Result<T> = std::result::Result<T, Error>;

impl_application_error_conversions!(Error);

/// 审计日志与命令回执错误。
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

impl_repository_error!(Error, duplicate = Error::ReceiptDuplicate);

#[cfg(test)]
mod tests {
    use std::error::Error as StdError;

    use application_core::ErrorClass;
    use mongodb::error::Error as MongoError;

    use super::Error;

    #[test]
    fn audit_duplicate_conversion_keeps_the_receipt_source() {
        let source = persistence_core::Error::DuplicateKey(MongoError::custom("receipt-source"));
        let error = Error::from(source);
        assert!(matches!(error, Error::ReceiptDuplicate(_)));
        assert_eq!(error.class(), ErrorClass::Conflict);
        assert_eq!(error.to_string(), "数据冲突: 数据已存在，请勿重复提交");
        let source = error.source().unwrap().downcast_ref::<persistence_core::Error>().unwrap();
        let persistence_core::Error::DuplicateKey(mongo) = source else {
            panic!("审计回执必须保留唯一键 source 类型");
        };
        assert_eq!(mongo.get_custom::<&str>().copied(), Some("receipt-source"));
    }
}
