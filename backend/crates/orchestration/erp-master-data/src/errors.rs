//! 应用边界拥有的错误类型；领域错误载荷与命令恢复分类保持冻结合同。

use application_core::ValidationIssues;
use erp_audit::AuditAttemptResult;
use erp_audit::execution::AuditFailure;

/// 应用边界结果，不通过旧 services 或另一应用边界定义别名。
pub type Result<T> = std::result::Result<T, Error>;

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

    #[error("RBAC 错误: {0}")]
    Rbac(String),

    #[error("操作结果暂无法确认，请查询当前状态后再决定是否重试")]
    OutcomeUnknown(#[source] persistence_core::Error),

    #[error("数据库错误：{0}")]
    RepositoryError(persistence_core::Error),
}

impl From<application_core::Error> for Error {
    /// 将应用合同错误映射为应用错误。
    fn from(error: application_core::Error) -> Self {
        match error {
            application_core::Error::Internal(message) => Self::Internal(message),
            application_core::Error::ValidationError(message) => Self::ValidationError(message),
            application_core::Error::InvalidInput(message) => Self::InvalidInput(message),
        }
    }
}

// 同构提供方只做穷尽载荷移动；非同构场景 adapter 继续在各自调用点处理。
macro_rules! from_domain {
    ($provider:ident $(, $extra:ident)*) => {
        impl From<$provider::Error> for Error {
            fn from(error: $provider::Error) -> Self {
                match error {
                    $provider::Error::Internal(payload) => Self::Internal(payload),
                    $provider::Error::NotFound(payload) => Self::NotFound(payload),
                    $provider::Error::ValidationError(payload) => Self::ValidationError(payload),
                    $provider::Error::InvalidInput(payload) => Self::InvalidInput(payload),
                    $provider::Error::BusinessLogicError(payload) => Self::BusinessLogicError(payload),
                    $provider::Error::ConflictError(payload) => Self::ConflictError(payload),
                    $provider::Error::ReceiptDuplicate(payload) => Self::ReceiptDuplicate(payload),
                    $provider::Error::TransientTransaction(payload) => Self::TransientTransaction(payload),
                    $provider::Error::Forbidden(payload) => Self::Forbidden(payload),
                    $provider::Error::Unauthenticated(payload) => Self::Unauthenticated(payload),
                    $provider::Error::Logic(payload) => Self::Logic(payload),
                    $provider::Error::OutcomeUnknown(payload) => Self::OutcomeUnknown(payload),
                    $provider::Error::RepositoryError(payload) => Self::RepositoryError(payload),
                    $( $provider::Error::$extra(payload) => Self::$extra(payload), )*
                }
            }
        }
    };
}

from_domain!(erp_identity, Rbac);
from_domain!(erp_file_assets);
from_domain!(erp_party);
from_domain!(erp_customer);
from_domain!(erp_supplier);
from_domain!(erp_support);
from_domain!(erp_audit);
from_domain!(erp_catalog);
from_domain!(erp_warehouse);
from_domain!(erp_contract);
from_domain!(erp_supply);
impl From<persistence_core::Error> for Error {
    /// 保留持久化分类和原错误；仅归档索引交 HTTP 的兼容分支最终形成文案。
    fn from(error: persistence_core::Error) -> Self {
        match error {
            error @ persistence_core::Error::DuplicateKey(_) => {
                if historical_http_index(error.duplicate_index_name()) {
                    // 这三个索引无活动生产者。保留 typed DuplicateKey，不能伪装成回执重复。
                    Self::RepositoryError(error)
                } else {
                    Self::ConflictError(duplicate_key_conflict_message(&error))
                }
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

/// 仅识别 HTTP 持有的三个历史索引；不拥有其提示文案，也不增加命令恢复范围。
fn historical_http_index(index_name: Option<&str>) -> bool {
    matches!(
        index_name,
        Some("uk_procurement_confirmation_lines_confirmation_line")
            | Some("uk_procurement_confirmation_lines_active_confirmation_line")
            | Some("uk_product_publication_revisions_publication_revision")
    )
}

fn duplicate_key_conflict_message(error: &persistence_core::Error) -> String {
    duplicate_index_conflict_message(error.duplicate_index_name())
}

/// 已知提示只由实际拥有域提供。无名/未知索引保持原通用冲突提示。
fn duplicate_index_conflict_message(index_name: Option<&str>) -> String {
    index_name
        .and_then(|name| {
            erp_party::known_duplicate_index_message(name)
                .or_else(|| erp_supplier::known_duplicate_index_message(name))
                .or_else(|| erp_supply::known_duplicate_index_message(name))
                .or_else(|| erp_contract::known_duplicate_index_message(name))
                .or_else(|| erp_customer::known_duplicate_index_message(name))
        })
        .unwrap_or("数据已存在，请勿重复提交")
        .to_string()
}

impl From<validator::ValidationErrors> for Error {
    /// 从给定值构建实例。
    ///
    /// # 参数
    /// * `err` - 错误对象
    ///
    /// # 返回
    /// 返回创建的实例。
    fn from(err: validator::ValidationErrors) -> Self {
        Error::InvalidInput(err.into())
    }
}

impl Error {
    /// 判断失败后服务端是否仍可能已经提交。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// `OutcomeUnknown`、`ReceiptDuplicate` 或 `TransientTransaction` 时返回 `true`，其余返回 `false`。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn command_may_have_committed(&self) -> bool {
        matches!(self, Self::OutcomeUnknown(_) | Self::ReceiptDuplicate(_) | Self::TransientTransaction(_))
    }
}

impl AuditFailure for Error {
    fn audit_attempt_result(&self) -> AuditAttemptResult {
        match self {
            Self::OutcomeUnknown(_) => AuditAttemptResult::Unknown,
            Self::ValidationError(_)
            | Self::BusinessLogicError(_)
            | Self::NotFound(_)
            | Self::Forbidden(_)
            | Self::Unauthenticated(_) => AuditAttemptResult::Rejected,
            _ => AuditAttemptResult::Failed,
        }
    }
    fn outcome_unknown(&self) -> bool {
        matches!(self, Self::OutcomeUnknown(_))
    }
}

#[cfg(test)]
mod tests {
    use application_core::ValidationIssues;

    use super::*;

    #[test]
    fn domain_conversion_preserves_field_issues_and_unknown_commit_source() {
        let issues = ValidationIssues::field("bank_accounts[0].account_no", "账号格式无效");
        assert!(
            matches!(Error::from(erp_supplier::Error::InvalidInput(issues.clone())), Error::InvalidInput(actual) if actual == issues)
        );
        let source = persistence_core::Error::CommitOutcomeUnknown(mongodb::error::Error::custom(
            "master-data-commit",
        ));
        let error = Error::from(source);
        assert!(error.command_may_have_committed());
        assert_eq!(error.audit_attempt_result(), AuditAttemptResult::Unknown);
        let Error::OutcomeUnknown(persistence_core::Error::CommitOutcomeUnknown(source)) = error else {
            panic!("unknown commit type must be retained");
        };
        assert_eq!(source.get_custom::<&str>(), Some(&"master-data-commit"));
    }

    #[test]
    fn duplicate_prompts_come_from_actual_owning_domains() {
        assert_eq!(duplicate_index_conflict_message(Some("uk_parties_party_no")), "主体编号已存在");
        assert_eq!(duplicate_index_conflict_message(Some("uk_contracts_contract_no")), "合同编号已存在");
        assert_eq!(duplicate_index_conflict_message(Some("unknown")), "数据已存在，请勿重复提交");
    }
}
