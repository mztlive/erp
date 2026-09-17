//! Sales-domain application errors with the original unique-index mapping.

use application_core::ErrorClass;

/// Sales-domain result alias.
pub type Result<T> = std::result::Result<T, Error>;

impl From<application_core::Error> for Error {
    /// 将应用合同错误映射为销售领域错误。
    fn from(error: application_core::Error) -> Self {
        match error {
            application_core::Error::Internal(message) => Self::Internal(message),
            application_core::Error::ValidationError(message) => Self::ValidationError(message),
        }
    }
}

/// Sales order, revision and selection errors.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("系统内部错误: {0}")]
    Internal(String),

    #[error("数据不存在: {0}")]
    NotFound(String),

    #[error("参数验证失败: {0}")]
    ValidationError(String),

    #[error("业务逻辑错误: {0}")]
    BusinessLogicError(String),

    #[error("数据冲突: {0}")]
    ConflictError(String),

    #[error("选品冲突: {0}")]
    SelectionConflict(String),

    #[error("选品已结束: {0}")]
    SelectionEnded(String),

    #[error("选品超限: {0}")]
    SelectionLimitExceeded(String),

    #[error("选品准备失败: {0}")]
    SelectionPrepareFailed(String),

    #[error("选品提交结果待核对: {0}")]
    SelectionPendingCheck(String),

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

impl Error {
    /// Stable error class used by HTTP mapping; do not parse display text.
    pub fn class(&self) -> ErrorClass {
        match self {
            Self::Internal(_) | Self::Logic(_) => ErrorClass::Internal,
            Self::RepositoryError(error) => persistence_error_class(error),
            Self::ConflictError(_) | Self::SelectionConflict(_) => ErrorClass::Conflict,
            Self::ReceiptDuplicate(error) | Self::TransientTransaction(error) => {
                persistence_error_class(error)
            },
            Self::BusinessLogicError(_)
            | Self::ValidationError(_)
            | Self::NotFound(_)
            | Self::SelectionEnded(_)
            | Self::SelectionLimitExceeded(_)
            | Self::SelectionPrepareFailed(_) => ErrorClass::BusinessRule,
            Self::Forbidden(_) | Self::Unauthenticated(_) => ErrorClass::Forbidden,
            Self::OutcomeUnknown(error) => persistence_error_class(error),
            Self::SelectionPendingCheck(_) => ErrorClass::Internal,
        }
    }

    /// 选品版本冲突。
    ///
    /// # 参数
    /// * `message` - 冲突说明
    ///
    /// # 返回
    /// 返回选品冲突错误。
    pub fn selection_conflict(message: impl Into<String>) -> Self {
        Self::SelectionConflict(message.into())
    }

    /// 选品结束态。
    ///
    /// # 参数
    /// * `message` - 结束说明
    ///
    /// # 返回
    /// 返回选品结束错误。
    pub fn selection_ended(message: impl Into<String>) -> Self {
        Self::SelectionEnded(message.into())
    }

    /// 选品超限。
    ///
    /// # 参数
    /// * `message` - 超限说明
    ///
    /// # 返回
    /// 返回选品超限错误。
    pub fn selection_limit(message: impl Into<String>) -> Self {
        Self::SelectionLimitExceeded(message.into())
    }

    /// 选品准备失败。
    ///
    /// # 参数
    /// * `message` - 失败说明
    ///
    /// # 返回
    /// 返回准备失败错误。
    pub fn selection_prepare_failed(message: impl Into<String>) -> Self {
        Self::SelectionPrepareFailed(message.into())
    }

    /// 选品提交结果待核对。
    ///
    /// # 参数
    /// * `message` - 待核对说明
    ///
    /// # 返回
    /// 返回待核对错误。
    pub fn selection_pending(message: impl Into<String>) -> Self {
        Self::SelectionPendingCheck(message.into())
    }
}

impl From<persistence_core::Error> for Error {
    /// 将仓储错误转换为销售领域错误。
    ///
    /// 唯一键、乐观锁和瞬态事务冲突保留为稳定的业务冲突语义，
    /// 其余错误保持内部仓储错误。
    fn from(error: persistence_core::Error) -> Self {
        match error {
            error @ persistence_core::Error::DuplicateKey(_) => {
                Self::ConflictError(duplicate_key_conflict_message(&error))
            },
            persistence_core::Error::OptimisticLockingError => {
                Self::ConflictError("数据已被其他请求修改，请刷新后重试".to_string())
            },
            error @ persistence_core::Error::TransientTransactionConflict(_) => {
                Self::TransientTransaction(error)
            },
            error @ persistence_core::Error::CommitOutcomeUnknown(_) => Self::OutcomeUnknown(error),
            other => Self::RepositoryError(other),
        }
    }
}

/// 将持久化源错误映射为稳定错误分类（唯一分类入口）。
///
/// 唯一键冲突、乐观锁与瞬态事务冲突为可重试/冲突语义；未知提交结果与其它
/// 仓储失败为内部错误。`Logic(erp_core::Error)` 不经过本函数：领域基元错误
/// 无稳定分类（字符串负载或状态迁移），保持 `Internal`。
fn persistence_error_class(error: &persistence_core::Error) -> ErrorClass {
    match error {
        persistence_core::Error::DuplicateKey(_)
        | persistence_core::Error::OptimisticLockingError
        | persistence_core::Error::TransientTransactionConflict(_) => ErrorClass::Conflict,
        _ => ErrorClass::Internal,
    }
}

/// 将唯一键冲突映射为面向用户的冲突提示。
fn duplicate_key_conflict_message(error: &persistence_core::Error) -> String {
    duplicate_index_conflict_message(error.duplicate_index_name())
}

/// 将销售域唯一索引名称映射为面向用户的冲突提示。
///
/// 销售订单、工作副本与变更唯一索引均保持原通用冲突文案。
/// 选品唯一索引映射为明确的选品冲突语义，不得用通用操作失败覆盖。
fn duplicate_index_conflict_message(index_name: Option<&str>) -> String {
    match index_name {
        Some("uk_sales_selection_proposals_book") => "一本选品册只能关联一份销售方案".to_string(),
        Some("uk_sales_selection_proposals_no") => "销售方案编号已存在".to_string(),
        Some("uk_sales_selection_idempotency_key") => "相同幂等键的请求载荷不一致".to_string(),
        Some("uk_sales_selection_display_book_combo") => "同一组合已存在，不得重复陈列".to_string(),
        _ => "数据已存在，请勿重复提交".to_string(),
    }
}

impl From<validator::ValidationErrors> for Error {
    /// 从校验错误构建销售领域错误。
    fn from(err: validator::ValidationErrors) -> Self {
        Error::ValidationError(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use application_core::ErrorClass;
    use mongodb::error::Error as MongoError;

    use super::Error;

    #[test]
    fn sales_unique_conflicts_keep_original_messages() {
        let generic = Error::from(persistence_core::Error::DuplicateKey(MongoError::custom("duplicate key")));
        assert_eq!(generic.class(), ErrorClass::Conflict);
        assert_eq!(generic.to_string(), "数据冲突: 数据已存在，请勿重复提交");
        assert_eq!(
            super::duplicate_index_conflict_message(Some("uk_sales_orders_order_no")),
            "数据已存在，请勿重复提交"
        );
        assert_eq!(
            super::duplicate_index_conflict_message(Some("uk_sales_order_working_copies_active_per_purpose")),
            "数据已存在，请勿重复提交"
        );
        assert_eq!(
            super::duplicate_index_conflict_message(Some("uk_sales_change_orders_active_per_order_base")),
            "数据已存在，请勿重复提交"
        );
    }

    #[test]
    fn persistence_wrappers_delegate_source_classification() {
        let duplicate = persistence_core::Error::DuplicateKey(MongoError::custom("duplicate key"));
        assert_eq!(Error::ReceiptDuplicate(duplicate).class(), ErrorClass::Conflict);

        let transient = persistence_core::Error::TransientTransactionConflict(MongoError::custom("t"));
        assert_eq!(Error::TransientTransaction(transient).class(), ErrorClass::Conflict);

        let unknown = persistence_core::Error::CommitOutcomeUnknown(MongoError::custom("u"));
        assert_eq!(Error::OutcomeUnknown(unknown).class(), ErrorClass::Internal);

        let database = persistence_core::Error::DatabaseError(MongoError::custom("db"));
        assert_eq!(Error::RepositoryError(database).class(), ErrorClass::Internal);
        let conflict_source = persistence_core::Error::DuplicateKey(MongoError::custom("duplicate key"));
        assert_eq!(Error::RepositoryError(conflict_source).class(), ErrorClass::Conflict);
    }

    #[test]
    fn optimistic_lock_maps_to_conflict_refresh_message() {
        let error = Error::from(persistence_core::Error::OptimisticLockingError);
        assert_eq!(error.class(), ErrorClass::Conflict);
        assert_eq!(error.to_string(), "数据冲突: 数据已被其他请求修改，请刷新后重试");
    }

    #[test]
    fn selection_errors_keep_distinct_classes() {
        assert_eq!(Error::selection_conflict("c").class(), ErrorClass::Conflict);
        assert_eq!(Error::selection_ended("e").class(), ErrorClass::BusinessRule);
        assert_eq!(Error::selection_limit("l").class(), ErrorClass::BusinessRule);
        assert_eq!(Error::selection_prepare_failed("f").class(), ErrorClass::BusinessRule);
        assert_eq!(Error::selection_pending("p").class(), ErrorClass::Internal);
        assert_eq!(
            super::duplicate_index_conflict_message(Some("uk_sales_selection_proposals_book")),
            "一本选品册只能关联一份销售方案"
        );
        assert_eq!(
            super::duplicate_index_conflict_message(Some("uk_sales_selection_proposals_no")),
            "销售方案编号已存在"
        );
        assert_eq!(
            super::duplicate_index_conflict_message(Some("uk_sales_selection_idempotency_key")),
            "相同幂等键的请求载荷不一致"
        );
    }
}
