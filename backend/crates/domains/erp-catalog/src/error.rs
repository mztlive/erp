//! 商品域应用错误，并保留原有唯一索引冲突映射。

use application_core::{ValidationIssues, impl_application_error_conversions, impl_domain_error_class};
use persistence_core::impl_repository_error;

/// 商品域结果别名。
pub type Result<T> = std::result::Result<T, Error>;

impl_application_error_conversions!(Error);

/// 商品字典、商品、SKU 与卡券类目错误。
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

/// 将商品域唯一索引名称映射为面向用户的冲突提示。
///
/// `uk_sku_barcode_claims_barcode` 与 `uk_sku_barcode_claims_id` 返回条码占用文案；
/// 其余索引返回通用「数据已存在」文案，不要再编造字段级提示。
fn duplicate_index_conflict_message(index_name: Option<&str>) -> String {
    match index_name {
        Some("uk_sku_barcode_claims_barcode" | "uk_sku_barcode_claims_id") => {
            "条码已被并发建档占用，请刷新并重新核对匹配 SKU".into()
        },
        _ => "数据已存在，请勿重复提交".into(),
    }
}

#[cfg(test)]
mod tests {
    use application_core::ErrorClass;
    use mongodb::error::Error as MongoError;

    use super::Error;

    #[test]
    fn catalog_unique_conflicts_keep_field_messages() {
        let generic = Error::from(persistence_core::Error::DuplicateKey(MongoError::custom("duplicate key")));
        assert_eq!(generic.class(), ErrorClass::Conflict);
        assert_eq!(generic.to_string(), "数据冲突: 数据已存在，请勿重复提交");
        assert_eq!(
            super::duplicate_index_conflict_message(Some("uk_products_product_no")),
            "数据已存在，请勿重复提交"
        );
        assert_eq!(
            super::duplicate_index_conflict_message(Some("uk_skus_product_spec")),
            "数据已存在，请勿重复提交"
        );
        assert_eq!(
            super::duplicate_index_conflict_message(Some("uk_sku_barcode_claims_barcode")),
            "条码已被并发建档占用，请刷新并重新核对匹配 SKU"
        );
    }
}
