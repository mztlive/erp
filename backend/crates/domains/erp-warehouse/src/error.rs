//! 仓库领域应用错误，并保留原有唯一索引冲突文案。

use application_core::{ValidationIssues, impl_application_error_conversions, impl_domain_error_class};
use erp_identity::Error as IdentityError;
use persistence_core::impl_repository_error;

/// Warehouse-domain result alias.
pub type Result<T> = std::result::Result<T, Error>;

impl_application_error_conversions!(Error);

/// Warehouse identity, revision and SKU-policy errors.
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
    duplicate =
        |error| { Error::ConflictError(duplicate_index_conflict_message(error.duplicate_index_name())) }
);

// 用仓库唯一索引名区分用户可处理的冲突原因；未登记索引显示通用提示，仍归类为 `Conflict`。
fn duplicate_index_conflict_message(index_name: Option<&str>) -> String {
    match index_name {
        Some("uk_warehouses_warehouse_code") => "仓库代码重复，请勿重复提交".to_string(),
        Some("uk_warehouse_revisions_revision") => "仓库修订序号重复，请勿重复提交".to_string(),
        Some("uk_warehouse_revisions_effective_from") => "仓库生效开始日重复，请勿重复提交".to_string(),
        Some("uk_warehouse_sku_policies_start") => "仓库SKU预警策略已存在，请勿重复提交".to_string(),
        _ => "数据已存在，请勿重复提交".to_string(),
    }
}

/// 保留身份域错误分类，业务调用直接传播原始拒绝与设施失败。
impl From<IdentityError> for Error {
    fn from(error: IdentityError) -> Self {
        match error {
            IdentityError::Internal(value) | IdentityError::Rbac(value) => Self::Internal(value),
            IdentityError::NotFound(value) => Self::NotFound(value),
            IdentityError::ValidationError(value) => Self::ValidationError(value),
            IdentityError::InvalidInput(value) => Self::InvalidInput(value),
            IdentityError::BusinessLogicError(value) => Self::BusinessLogicError(value),
            IdentityError::ConflictError(value) => Self::ConflictError(value),
            IdentityError::ReceiptDuplicate(value) => Self::ReceiptDuplicate(value),
            IdentityError::TransientTransaction(value) => Self::TransientTransaction(value),
            IdentityError::Forbidden(value) => Self::Forbidden(value),
            IdentityError::Unauthenticated(value) => Self::Unauthenticated(value),
            IdentityError::Logic(value) => Self::Logic(value),
            IdentityError::OutcomeUnknown(value) => Self::OutcomeUnknown(value),
            IdentityError::RepositoryError(value) => Self::RepositoryError(value),
        }
    }
}

#[cfg(test)]
mod tests {
    use application_core::ErrorClass;
    use mongodb::error::Error as MongoError;

    use super::{Error, IdentityError};

    #[test]
    fn warehouse_unique_conflicts_keep_original_messages() {
        let generic = Error::from(persistence_core::Error::DuplicateKey(MongoError::custom("duplicate key")));
        assert_eq!(generic.class(), ErrorClass::Conflict);
        assert_eq!(generic.to_string(), "数据冲突: 数据已存在，请勿重复提交");

        assert_eq!(
            super::duplicate_index_conflict_message(Some("uk_warehouses_warehouse_code")),
            "仓库代码重复，请勿重复提交"
        );
        assert_eq!(
            super::duplicate_index_conflict_message(Some("uk_warehouse_revisions_revision")),
            "仓库修订序号重复，请勿重复提交"
        );
        assert_eq!(
            super::duplicate_index_conflict_message(Some("uk_warehouse_sku_policies_start")),
            "仓库SKU预警策略已存在，请勿重复提交"
        );
        assert_eq!(
            super::duplicate_index_conflict_message(Some("uk_unknown_index")),
            "数据已存在，请勿重复提交"
        );
        assert_eq!(super::duplicate_index_conflict_message(None), "数据已存在，请勿重复提交");
    }

    #[test]
    fn identity_denials_and_facility_failures_keep_their_error_class() {
        let denial = Error::from(IdentityError::Forbidden("对象无权访问".into()));
        assert_eq!(denial.class(), ErrorClass::Forbidden);
        assert!(matches!(denial, Error::Forbidden(message) if message == "对象无权访问"));
        let facility = Error::from(IdentityError::Rbac("策略读取失败".into()));
        assert_eq!(facility.class(), ErrorClass::Internal);
        assert!(matches!(facility, Error::Internal(message) if message == "策略读取失败"));
    }

    #[test]
    fn optimistic_lock_maps_to_conflict_refresh_message() {
        let error = Error::from(persistence_core::Error::OptimisticLockingError);
        assert_eq!(error.class(), ErrorClass::Conflict);
        assert_eq!(error.to_string(), "数据冲突: 数据已被其他请求修改，请刷新后重试");
    }
}
