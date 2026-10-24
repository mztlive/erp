//! 供应商领域应用错误，沿用原有服务映射。

use application_core::{ValidationIssues, impl_application_error_conversions, impl_domain_error_class};
use persistence_core::impl_repository_error;

/// Supplier-domain result alias.
pub type Result<T> = std::result::Result<T, Error>;

impl_application_error_conversions!(Error);

/// Supplier account, capability, qualification and profile-command errors.
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
///
/// 冲突文案映射内聚在此处（erp-supplier-010）：`From` 只做三类分流
/// （唯一键→ConflictError、乐观锁→ConflictError、瞬态/未知→透传），
/// 索引名→文案的明细在 [`duplicate_index_conflict_message`] 旁维护。
fn duplicate_key_conflict_message(error: &persistence_core::Error) -> String {
    duplicate_index_conflict_message(error.duplicate_index_name())
}

/// 将供应商域唯一索引名称映射为面向用户的冲突提示。
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
        "uk_supplier_accounts_party" => Some("该主体已绑定供应商角色"),
        "uk_supplier_accounts_supplier_no" => Some("供应商编号已存在"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use application_core::ErrorClass;
    use mongodb::error::Error as MongoError;

    use super::Error;

    #[test]
    fn supplier_unique_conflicts_keep_original_messages() {
        let generic = Error::from(persistence_core::Error::DuplicateKey(MongoError::custom("duplicate key")));
        assert_eq!(generic.class(), ErrorClass::Conflict);
        assert_eq!(generic.to_string(), "数据冲突: 数据已存在，请勿重复提交");
        assert_eq!(
            super::duplicate_index_conflict_message(Some("uk_supplier_accounts_party")),
            "该主体已绑定供应商角色"
        );
        assert_eq!(
            super::duplicate_index_conflict_message(Some("uk_supplier_accounts_supplier_no")),
            "供应商编号已存在"
        );
    }

    #[test]
    fn known_duplicate_export_keeps_exact_owned_index_matrix() {
        let cases = [
            ("uk_supplier_accounts_party", "该主体已绑定供应商角色"),
            ("uk_supplier_accounts_supplier_no", "供应商编号已存在"),
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
