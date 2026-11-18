//! 应用边界拥有的错误类型；领域错误载荷与命令恢复分类保持冻结合同。

use application_core::ValidationIssues;
use erp_workflow::ErrorCode as WorkflowErrorCode;

/// 应用边界结果，不通过旧 services 或另一应用边界定义别名。
pub type Result<T> = std::result::Result<T, ReadError>;

#[derive(Debug, thiserror::Error)]
pub enum ReadError {
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

    #[error("{0}")]
    Coded(WorkflowErrorCode),
}

impl From<application_core::Error> for ReadError {
    // 为导入导出读取接入共享应用合同错误，沿用原业务类别供上层区分权限、校验及系统失败。
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
        impl From<$provider::Error> for ReadError {
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
from_domain!(erp_workflow, Rbac, Coded);
from_domain!(erp_party);
from_domain!(erp_customer);
from_domain!(erp_supplier);
from_domain!(erp_support);
from_domain!(erp_audit);
from_domain!(erp_catalog);
from_domain!(erp_warehouse);
from_domain!(erp_contract);
from_domain!(erp_inventory);
from_domain!(erp_finance);
from_domain!(erp_integration);
from_domain!(erp_supply);
from_domain!(erp_fulfillment_execution, Rbac, Coded);
from_domain!(erp_returns);
impl From<erp_fulfillment::Error> for ReadError {
    // 将履约读取错误转入导入导出边界。履约唯一键冲突已按索引归为 `ConflictError`，保留此冲突分类，不再寻找 `ReceiptDuplicate`。
    fn from(error: erp_fulfillment::Error) -> Self {
        match error {
            erp_fulfillment::Error::Internal(payload) => Self::Internal(payload),
            erp_fulfillment::Error::NotFound(payload) => Self::NotFound(payload),
            erp_fulfillment::Error::ValidationError(payload) => Self::ValidationError(payload),
            erp_fulfillment::Error::InvalidInput(payload) => Self::InvalidInput(payload),
            erp_fulfillment::Error::BusinessLogicError(payload) => Self::BusinessLogicError(payload),
            erp_fulfillment::Error::ConflictError(payload) => Self::ConflictError(payload),
            erp_fulfillment::Error::TransientTransaction(payload) => Self::TransientTransaction(payload),
            erp_fulfillment::Error::Forbidden(payload) => Self::Forbidden(payload),
            erp_fulfillment::Error::Unauthenticated(payload) => Self::Unauthenticated(payload),
            erp_fulfillment::Error::Logic(payload) => Self::Logic(payload),
            erp_fulfillment::Error::OutcomeUnknown(payload) => Self::OutcomeUnknown(payload),
            erp_fulfillment::Error::RepositoryError(payload) => Self::RepositoryError(payload),
        }
    }
}
from_domain!(erp_procurement);
from_domain!(erp_import);

impl From<erp_sales::Error> for ReadError {
    // 传播销售读取失败时，把选品专用变体归入既有冲突、业务或内部错误分类。
    fn from(error: erp_sales::Error) -> Self {
        match error {
            erp_sales::Error::Internal(payload) => Self::Internal(payload),
            erp_sales::Error::NotFound(payload) => Self::NotFound(payload),
            erp_sales::Error::ValidationError(payload) => Self::ValidationError(payload),
            erp_sales::Error::InvalidInput(payload) => Self::InvalidInput(payload),
            erp_sales::Error::BusinessLogicError(payload) => Self::BusinessLogicError(payload),
            erp_sales::Error::ConflictError(payload) => Self::ConflictError(payload),
            erp_sales::Error::SelectionConflict(payload) => Self::ConflictError(payload),
            erp_sales::Error::SelectionEnded(payload) => Self::BusinessLogicError(payload),
            erp_sales::Error::SelectionLimitExceeded(payload) => Self::BusinessLogicError(payload),
            erp_sales::Error::SelectionPrepareFailed(payload) => Self::BusinessLogicError(payload),
            erp_sales::Error::SelectionPendingCheck(payload) => Self::Internal(payload),
            erp_sales::Error::ReceiptDuplicate(payload) => Self::ReceiptDuplicate(payload),
            erp_sales::Error::TransientTransaction(payload) => Self::TransientTransaction(payload),
            erp_sales::Error::Forbidden(payload) => Self::Forbidden(payload),
            erp_sales::Error::Unauthenticated(payload) => Self::Unauthenticated(payload),
            erp_sales::Error::Logic(payload) => Self::Logic(payload),
            erp_sales::Error::OutcomeUnknown(payload) => Self::OutcomeUnknown(payload),
            erp_sales::Error::RepositoryError(payload) => Self::RepositoryError(payload),
        }
    }
}

impl From<persistence_core::Error> for ReadError {
    // 导入导出读取失败时保留原持久化错误分类和载荷，仅将已归档索引交 HTTP 兼容层决定最终文案。
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

// 筛出必须交给 HTTP 兼容层的三个归档索引；筛选不生成提示，也不扩大命令恢复范围。
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

// 导入导出遇到唯一冲突时向索引拥有域取提示；未命名或无法识别的索引使用通用提示。
fn duplicate_index_conflict_message(index_name: Option<&str>) -> String {
    index_name
        .and_then(|name| {
            erp_party::known_duplicate_index_message(name)
                .or_else(|| erp_supplier::known_duplicate_index_message(name))
                .or_else(|| erp_supply::known_duplicate_index_message(name))
                .or_else(|| erp_contract::known_duplicate_index_message(name))
                .or_else(|| erp_procurement::known_duplicate_index_message(name))
                .or_else(|| erp_customer::known_duplicate_index_message(name))
                .or_else(|| erp_workflow::known_duplicate_index_message(name))
        })
        .unwrap_or("数据已存在，请勿重复提交")
        .to_string()
}

impl From<validator::ValidationErrors> for ReadError {
    // 把导入导出读取参数的验证问题转为含字段细节的 `InvalidInput`，不混入仓储或系统错误。
    fn from(err: validator::ValidationErrors) -> Self {
        ReadError::InvalidInput(err.into())
    }
}

from_domain!(erp_read_models, Rbac, Coded);

impl From<ReadError> for erp_data_import::Error {
    fn from(error: ReadError) -> Self {
        match error {
            ReadError::Internal(payload) => Self::Internal(payload),
            ReadError::NotFound(payload) => Self::NotFound(payload),
            ReadError::ValidationError(payload) => Self::ValidationError(payload),
            ReadError::InvalidInput(payload) => Self::InvalidInput(payload),
            ReadError::BusinessLogicError(payload) => Self::BusinessLogicError(payload),
            ReadError::ConflictError(payload) => Self::ConflictError(payload),
            ReadError::ReceiptDuplicate(payload) => Self::ReceiptDuplicate(payload),
            ReadError::TransientTransaction(payload) => Self::TransientTransaction(payload),
            ReadError::Forbidden(payload) => Self::Forbidden(payload),
            ReadError::Unauthenticated(payload) => Self::Unauthenticated(payload),
            ReadError::Logic(payload) => Self::Logic(payload),
            ReadError::Rbac(payload) => Self::Rbac(payload),
            ReadError::OutcomeUnknown(payload) => Self::OutcomeUnknown(payload),
            ReadError::RepositoryError(payload) => Self::RepositoryError(payload),
            ReadError::Coded(payload) => Self::Coded(payload),
        }
    }
}
