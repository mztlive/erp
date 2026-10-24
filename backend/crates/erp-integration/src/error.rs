//! 集成领域应用错误，保留原唯一索引冲突文案映射。

use application_core::{ValidationIssues, impl_application_error_conversions, impl_domain_error_class};
use persistence_core::impl_repository_error;

/// 集成领域结果别名。
pub type Result<T> = std::result::Result<T, Error>;

impl_application_error_conversions!(Error);

/// 集成消息、错误任务与对账决定的应用错误。
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
    duplicate = |error| { Error::ConflictError(duplicate_key_conflict_message(&error)) }
);

/// 乐观锁版本冲突的统一文案（`From<persistence_core::Error>` 与版本校验共用）。
pub const OPTIMISTIC_LOCK_CONFLICT_MESSAGE: &str = "数据已被其他请求修改，请刷新后重试";

/// 由乐观锁冲突构造版本冲突错误（版本校验与仓储映射的统一入口）。
///
/// # 参数
/// 无。
///
/// # 返回
/// 返回 `ConflictError`。
///
/// # 错误
/// 不返回错误。
pub fn optimistic_lock_conflict() -> Error {
    Error::ConflictError(OPTIMISTIC_LOCK_CONFLICT_MESSAGE.to_string())
}

/// 将唯一键冲突映射为面向用户的冲突提示（唯一调用方为仓储错误映射）。
///
/// 未知索引回落通用文案；错误分类保持 `Conflict` 不变。
fn duplicate_key_conflict_message(error: &persistence_core::Error) -> String {
    duplicate_index_conflict_message(error.duplicate_index_name())
}

/// 将集成域唯一索引名称映射为面向用户的冲突提示。
///
/// 未知索引回落通用文案；错误分类保持 `Conflict` 不变。
fn duplicate_index_conflict_message(index_name: Option<&str>) -> String {
    match index_name {
        Some("uk_inbox_messages_identity") => "入站消息已存在，请勿重复提交".to_string(),
        Some("uk_integration_error_tasks_message_class") => {
            "该消息的错误任务已存在，请勿重复提交".to_string()
        },
        Some("uk_reconciliation_differences_object") => "该对象的对账差异已存在，请勿重复提交".to_string(),
        _ => "数据已存在，请勿重复提交".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use application_core::ErrorClass;
    use mongodb::error::Error as MongoError;

    use super::Error;

    #[test]
    fn integration_unique_conflicts_keep_original_messages() {
        let generic = Error::from(persistence_core::Error::DuplicateKey(MongoError::custom("duplicate key")));
        assert_eq!(generic.class(), ErrorClass::Conflict);
        assert_eq!(generic.to_string(), "数据冲突: 数据已存在，请勿重复提交");
        assert_eq!(
            super::duplicate_index_conflict_message(Some("uk_inbox_messages_identity")),
            "入站消息已存在，请勿重复提交"
        );
        assert_eq!(
            super::duplicate_index_conflict_message(Some("uk_integration_error_tasks_message_class")),
            "该消息的错误任务已存在，请勿重复提交"
        );
        assert_eq!(
            super::duplicate_index_conflict_message(Some("uk_reconciliation_differences_object")),
            "该对象的对账差异已存在，请勿重复提交"
        );
        assert_eq!(
            super::duplicate_index_conflict_message(Some("uk_unknown_index")),
            "数据已存在，请勿重复提交"
        );
        assert_eq!(super::duplicate_index_conflict_message(None), "数据已存在，请勿重复提交");
    }

    #[test]
    fn optimistic_lock_maps_to_conflict_refresh_message() {
        let error = Error::from(persistence_core::Error::OptimisticLockingError);
        assert_eq!(error.class(), ErrorClass::Conflict);
        assert_eq!(error.to_string(), "数据冲突: 数据已被其他请求修改，请刷新后重试");
    }
}
