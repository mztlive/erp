//! 客户领域应用错误，并保留原有唯一索引冲突映射。

use application_core::{ValidationIssues, impl_application_error_conversions, impl_domain_error_class};
use persistence_core::impl_repository_error;

/// Customer-domain result alias.
pub type Result<T> = std::result::Result<T, Error>;

impl_application_error_conversions!(Error);

/// Customer account, assignment and profile-command errors.
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

/// 将客户域唯一索引名称映射为面向用户的冲突提示。
fn duplicate_index_conflict_message(index_name: Option<&str>) -> String {
    index_name.and_then(known_duplicate_index_message).unwrap_or("数据已存在，请勿重复提交").to_string()
}

/// 返回本域已知唯一索引的固定冲突提示。
///
/// # 参数
/// * `index_name` - 已由 persistence-core 提取的 MongoDB 索引名称
///
/// # 返回
/// 精确命中本域注册索引时返回原提示；未知名称返回 `None`，由调用边界选择通用提示。
///
/// # 错误
/// 不返回错误。
///
/// # 约束
/// 不匹配子串或前后缀、不规范化索引名称，不拥有其他领域或 HTTP 历史索引的提示。
pub fn known_duplicate_index_message(index_name: &str) -> Option<&'static str> {
    match index_name {
        "uk_customer_accounts_party" => Some("该主体已绑定客户角色"),
        "uk_customer_accounts_customer_no" => Some("客户编号已存在"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use application_core::ErrorClass;
    use mongodb::bson::{deserialize_from_document, doc};
    use mongodb::error::{Error as MongoError, ErrorKind, WriteError, WriteFailure};

    use super::Error;

    #[test]
    fn duplicate_key_error_maps_to_conflict() {
        let error = Error::from(persistence_core::Error::DuplicateKey(MongoError::custom("duplicate key")));
        assert_eq!(error.class(), ErrorClass::Conflict);
        assert_eq!(error.to_string(), "数据冲突: 数据已存在，请勿重复提交");
    }

    #[test]
    fn known_customer_duplicate_indexes_map_to_field_messages() {
        assert_eq!(
            super::duplicate_index_conflict_message(Some("uk_customer_accounts_party")),
            "该主体已绑定客户角色"
        );
        assert_eq!(
            super::duplicate_index_conflict_message(Some("uk_customer_accounts_customer_no")),
            "客户编号已存在"
        );
        assert_eq!(
            super::duplicate_index_conflict_message(Some("uk_customer_assignments_window")),
            "数据已存在，请勿重复提交"
        );
    }

    #[test]
    fn repository_conversion_keeps_the_customer_duplicate_index_policy() {
        for (index, expected) in [
            ("uk_customer_accounts_party", "数据冲突: 该主体已绑定客户角色"),
            ("uk_customer_accounts_customer_no", "数据冲突: 客户编号已存在"),
            ("uk_unknown_index", "数据冲突: 数据已存在，请勿重复提交"),
        ] {
            let write: WriteError = deserialize_from_document(doc! {
                "code": 11000,
                "codeName": "DuplicateKey",
                "errmsg": format!("E11000 duplicate key collection: erp.customer index: {index} dup key: {{ id: 1 }}"),
                "errInfo": null,
            })
            .expect("唯一键错误样本必须有效");
            let mongo: MongoError = ErrorKind::Write(WriteFailure::WriteError(write)).into();
            let error = Error::from(persistence_core::Error::from(mongo));
            assert_eq!(error.class(), ErrorClass::Conflict);
            assert_eq!(error.to_string(), expected);
        }
    }

    #[test]
    fn known_duplicate_export_keeps_exact_owned_index_matrix() {
        let cases = [
            ("uk_customer_accounts_party", "该主体已绑定客户角色"),
            ("uk_customer_accounts_customer_no", "客户编号已存在"),
        ];
        for (index, expected) in cases {
            assert_eq!(super::known_duplicate_index_message(index), Some(expected));
            assert_eq!(super::known_duplicate_index_message(&format!("{index}_extra")), None);
            assert_eq!(super::known_duplicate_index_message(&format!(" {index}")), None);
            assert_eq!(super::known_duplicate_index_message(&index.to_ascii_uppercase()), None);
        }
        assert_eq!(super::known_duplicate_index_message(""), None);
        assert_eq!(super::known_duplicate_index_message("unknown_index"), None);
    }
}
