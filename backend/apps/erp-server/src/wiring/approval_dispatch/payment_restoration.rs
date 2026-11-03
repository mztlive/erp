//! 应用装配付款冲正的原申请恢复能力；沿用调用方事务。
use application_core::AuditActor;
use async_trait::async_trait;
use erp_finance::entity::payable::{PaymentRequestAllocation, SupplierPayment};
use erp_finance::ports::payment_restoration::PaymentRequestRestorationPort;
use erp_finance_posting::finance_posting::payable::payment_request::restore_reversed_payment;
use erp_finance_posting::{Error as PostingError, FinancePostingPorts};
use erp_identity::SharedRbacService;
use erp_reverse_flow::Error as ReverseError;
use mongodb::Database;
use persistence_core::Executor;

use crate::wiring::finance_posting_ports;

/// 原付款申请恢复的应用接线；不拥有业务状态或事务。
pub(crate) struct PaymentRequestRestoration {
    db: Database,
    ports: FinancePostingPorts,
}
impl PaymentRequestRestoration {
    /// 绑定当前应用数据库，不执行读写。
    ///
    /// # 参数
    /// `db` 为冲正命令使用的数据库，`rbac` 为同一应用的共享权限来源。
    ///
    /// # 返回
    /// 可注入逆向编排的原申请恢复能力。
    ///
    /// # 错误
    /// 不返回错误。
    pub(crate) fn new(db: Database, rbac: SharedRbacService) -> Self {
        Self { ports: finance_posting_ports(db.clone(), rbac), db }
    }
}
#[async_trait]
impl PaymentRequestRestorationPort for PaymentRequestRestoration {
    type Error = ReverseError;
    async fn restore(
        &self,
        payment: &SupplierPayment,
        allocations: &[PaymentRequestAllocation],
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<(), Self::Error> {
        restore_reversed_payment(&self.db, &self.ports, payment, allocations, actor, executor)
            .await
            .map_err(map_error)
    }
}
fn map_error(error: PostingError) -> ReverseError {
    match error {
        PostingError::Internal(value) => ReverseError::Internal(value),
        PostingError::NotFound(value) => ReverseError::NotFound(value),
        PostingError::ValidationError(value) => ReverseError::ValidationError(value),
        PostingError::InvalidInput(value) => ReverseError::InvalidInput(value),
        PostingError::BusinessLogicError(value) => ReverseError::BusinessLogicError(value),
        PostingError::ConflictError(value) => ReverseError::ConflictError(value),
        PostingError::ReceiptDuplicate(value) => ReverseError::ReceiptDuplicate(value),
        PostingError::TransientTransaction(value) => ReverseError::TransientTransaction(value),
        PostingError::Forbidden(value) => ReverseError::Forbidden(value),
        PostingError::Unauthenticated(value) => ReverseError::Unauthenticated(value),
        PostingError::Logic(value) => ReverseError::Logic(value),
        PostingError::Rbac(value) => ReverseError::Rbac(value),
        PostingError::OutcomeUnknown(value) => ReverseError::OutcomeUnknown(value),
        // 恢复步骤仅在调用方执行器内更新申请与占用，不调用提交后开票回读。
        PostingError::CommittedResultUnavailable(_) => {
            ReverseError::Internal("PAYMENT_RESTORATION_PORT_UNEXPECTED_COMMITTED_RESPONSE".into())
        },
        PostingError::RepositoryError(value) => ReverseError::RepositoryError(value),
        PostingError::Coded(value) => ReverseError::Coded(value),
    }
}

#[cfg(test)]
mod tests {
    use erp_workflow::ErrorCode;
    use persistence_core::Error as PersistenceError;

    use super::*;
    #[test]
    fn restoration_bridge_keeps_structured_failure_and_stable_code() {
        let code = ErrorCode::ApprovalTaskVersionConflict;
        assert!(
            matches!(map_error(PostingError::Coded(code)), ReverseError::Coded(actual) if actual == code)
        );
        assert!(
            matches!(map_error(PostingError::Rbac("rbac-payload".into())), ReverseError::Rbac(value) if value == "rbac-payload")
        );
        let source = PersistenceError::EntityMetadataOutOfRange("source-marker");
        assert!(matches!(
            map_error(PostingError::OutcomeUnknown(source)),
            ReverseError::OutcomeUnknown(PersistenceError::EntityMetadataOutOfRange("source-marker"))
        ));
    }
    #[test]
    fn restoration_bridge_keeps_conflict_distinct_from_validation() {
        assert!(
            matches!(map_error(PostingError::ConflictError("version".into())), ReverseError::ConflictError(value) if value == "version")
        );
        assert!(
            matches!(map_error(PostingError::ValidationError("field".into())), ReverseError::ValidationError(value) if value == "field")
        );
    }

    #[test]
    fn restoration_port_rejects_impossible_committed_response() {
        let error = map_error(PostingError::CommittedResultUnavailable("unreadable invoice contents".into()));
        assert!(matches!(
            error,
            ReverseError::Internal(message) if message == "PAYMENT_RESTORATION_PORT_UNEXPECTED_COMMITTED_RESPONSE"
        ));
    }
}
