//! 审批动作分派的本地错误；保留各业务库的原结构直到工作流边界。
use erp_workflow::Error as WorkflowError;
#[derive(Debug, thiserror::Error)]
pub enum DispatchError {
    #[error("系统内部错误: {0}")]
    Internal(String),
    #[error("业务逻辑错误: {0}")]
    BusinessLogicError(String),
    #[error("数据冲突: {0}")]
    ConflictError(String),
    #[error("权限不足: {0}")]
    Forbidden(String),
    #[error(transparent)]
    Sales(#[from] erp_order_to_cash::Error),
    #[error(transparent)]
    Procurement(#[from] erp_procure_to_pay::Error),
    #[error(transparent)]
    Fulfillment(#[from] erp_fulfillment_execution::Error),
    #[error(transparent)]
    Finance(#[from] erp_finance_posting::Error),
    #[error(transparent)]
    Reverse(#[from] erp_reverse_flow::Error),
}
pub type Result<T> = std::result::Result<T, DispatchError>;

// Preserve the established final workflow classification, while the inter-crate bridge
// above retains the exact typed source instead of formatting it into an intermediate error.
macro_rules! workflow_error {
    ($error:expr, $provider:ident) => {
        match $error {
            $provider::Error::ValidationError(value) => WorkflowError::ValidationError(value),
            $provider::Error::InvalidInput(value) => WorkflowError::InvalidInput(value),
            $provider::Error::NotFound(value) => WorkflowError::NotFound(value),
            $provider::Error::BusinessLogicError(value) => WorkflowError::BusinessLogicError(value),
            $provider::Error::ConflictError(value) => WorkflowError::ConflictError(value),
            $provider::Error::Forbidden(value) => WorkflowError::Forbidden(value),
            $provider::Error::Unauthenticated(value) => WorkflowError::Unauthenticated(value),
            $provider::Error::Internal(value) => WorkflowError::Internal(value),
            $provider::Error::Logic(value) => WorkflowError::Logic(value),
            $provider::Error::OutcomeUnknown(value) => WorkflowError::OutcomeUnknown(value),
            $provider::Error::RepositoryError(value) => WorkflowError::RepositoryError(value),
            other => WorkflowError::Internal(other.to_string()),
        }
    };
}

pub(super) fn map_service_error(error: DispatchError) -> WorkflowError {
    match error {
        DispatchError::Internal(value) => WorkflowError::Internal(value),
        DispatchError::BusinessLogicError(value) => WorkflowError::BusinessLogicError(value),
        DispatchError::ConflictError(value) => WorkflowError::ConflictError(value),
        DispatchError::Forbidden(value) => WorkflowError::Forbidden(value),
        DispatchError::Sales(error) => workflow_error!(error, erp_order_to_cash),
        DispatchError::Procurement(error) => workflow_error!(error, erp_procure_to_pay),
        DispatchError::Fulfillment(error) => workflow_error!(error, erp_fulfillment_execution),
        DispatchError::Finance(error) => workflow_error!(error, erp_finance_posting),
        DispatchError::Reverse(error) => workflow_error!(error, erp_reverse_flow),
    }
}

#[cfg(test)]
mod tests {
    use erp_finance_posting::Error as PostingError;
    use erp_workflow::ErrorCode;
    use persistence_core::Error as PersistenceError;

    use super::*;
    #[test]
    fn dispatch_preserves_workflow_error_class_and_legacy_fallback_message() {
        let converted = map_service_error(PostingError::Forbidden("actor".into()).into());
        assert!(matches!(converted, WorkflowError::Forbidden(value) if value == "actor"));
        let source = PersistenceError::EntityMetadataOutOfRange("receipt");
        let converted = map_service_error(PostingError::ReceiptDuplicate(source).into());
        assert!(
            matches!(converted, WorkflowError::Internal(value) if value == "数据冲突: 数据已存在，请勿重复提交")
        );
        let code = ErrorCode::ApprovalTaskVersionConflict;
        let expected = PostingError::Coded(code).to_string();
        assert!(
            matches!(map_service_error(PostingError::Coded(code).into()), WorkflowError::Internal(value) if value == expected)
        );
    }
}
