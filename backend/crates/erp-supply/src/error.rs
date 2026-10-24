//! 供应链领域应用错误，并保留原有唯一索引映射。

use application_core::{ValidationIssues, impl_application_error_conversions, impl_domain_error_class};
use persistence_core::impl_repository_error;

/// 供应链领域结果别名。
pub type Result<T> = std::result::Result<T, Error>;

impl_application_error_conversions!(Error);

/// 供应供给、连接能力、履约和结算的应用错误。
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

/// 将唯一键冲突映射为面向用户的冲突提示。
fn duplicate_key_conflict_message(error: &persistence_core::Error) -> String {
    duplicate_index_conflict_message(error.duplicate_index_name())
}

/// 将供应链域唯一索引名称映射为面向用户的冲突提示。
///
/// 供给重复 SKU 保持原专用提示，其余供应链唯一索引保持通用提示。
fn duplicate_index_conflict_message(index_name: Option<&str>) -> String {
    index_name.and_then(known_duplicate_index_message).unwrap_or("数据已存在，请勿重复提交").to_string()
}

/// 返回供应链领域已知唯一索引的固定冲突提示。
///
/// # 参数
/// * `index_name` - 已由 persistence-core 提取的 MongoDB 索引名称
///
/// # 返回
/// 精确命中本域索引时返回原提示；未知名称返回 `None`，由调用边界选择通用提示。
///
/// # 错误
/// 不返回错误。
///
/// # 约束
/// 不匹配子串或前后缀、不规范化索引名称，不拥有其他领域或 HTTP 历史索引的提示。
pub fn known_duplicate_index_message(index_name: &str) -> Option<&'static str> {
    match index_name {
        "uk_supplier_offerings_supplier_sku" => Some("该供应商 SKU 已登记供给"),
        "uk_supplier_delivery_templates_supplier_name" => Some("该供应商已有同名配送模板"),
        "uk_supplier_delivery_templates_default" => Some("该供应商的默认配送模板已变化，请刷新后重试"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use application_core::ErrorClass;
    use mongodb::error::Error as MongoError;

    use super::*;

    #[test]
    fn supply_unique_conflicts_keep_original_messages() {
        assert_eq!(
            duplicate_index_conflict_message(Some("uk_supplier_offerings_supplier_sku")),
            "该供应商 SKU 已登记供给"
        );
        for index in [
            None,
            Some("uk_supplier_offerings_supplier_sku_extra"),
            Some("uk_supplier_connection_command_receipts_command"),
        ] {
            assert_eq!(duplicate_index_conflict_message(index), "数据已存在，请勿重复提交");
        }
        let error = Error::from(persistence_core::Error::DuplicateKey(MongoError::custom("duplicate key")));
        assert_eq!(error.class(), ErrorClass::Conflict);
        assert_eq!(error.to_string(), "数据冲突: 数据已存在，请勿重复提交");
    }

    #[test]
    fn concurrency_errors_keep_recovery_variants_and_messages() {
        let error = Error::from(persistence_core::Error::OptimisticLockingError);
        assert_eq!(error.to_string(), "数据冲突: 数据已被其他请求修改，请刷新后重试");
        assert_eq!(error.class(), ErrorClass::Conflict);
        let error = Error::from(persistence_core::Error::TransientTransactionConflict(MongoError::custom(
            "transient",
        )));
        assert!(matches!(error, Error::TransientTransaction(_)));
        let error = Error::from(persistence_core::Error::CommitOutcomeUnknown(MongoError::custom("unknown")));
        assert!(matches!(error, Error::OutcomeUnknown(_)));
        assert_eq!(error.to_string(), "操作结果暂无法确认，请查询当前状态后再决定是否重试");
    }

    /// 真实公开查询与私有格式化均保留精确命中及未知、缺失名称的原结果。
    #[test]
    fn known_duplicate_export_preserves_exact_match_and_fallback() {
        let cases = [
            (Some("uk_supplier_offerings_supplier_sku"), Some("该供应商 SKU 已登记供给")),
            (Some(""), None),
            (Some("unknown_index"), None),
            (Some("uk_supplier_offerings_supplier_sku_extra"), None),
            (Some("prefix_uk_supplier_offerings_supplier_sku"), None),
            (Some(" uk_supplier_offerings_supplier_sku"), None),
            (Some("UK_SUPPLIER_OFFERINGS_SUPPLIER_SKU"), None),
            (None, None),
        ];
        for (index_name, expected) in cases {
            if let Some(index_name) = index_name {
                assert_eq!(super::known_duplicate_index_message(index_name), expected);
            }
            assert_eq!(
                super::duplicate_index_conflict_message(index_name),
                expected.unwrap_or("数据已存在，请勿重复提交")
            );
        }
    }
}
