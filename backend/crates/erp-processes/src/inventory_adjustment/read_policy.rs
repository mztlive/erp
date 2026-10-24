//! 为库存读取模型接入正式提交和撤回命令共用的动作政策。

use application_core::AuditActor;
use async_trait::async_trait;
use bpm::model::{ApprovalNodeExecution, ApprovalProcessInstance};
use erp_identity::SharedRbacService;
use erp_inventory::StockAdjustment;
use erp_read_models::inventory_center::InventoryAdjustmentApprovalPolicyPort;
use erp_read_models::{Error as ReadError, Result as ReadResult};
use erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding;
use erp_workflow::entity::work_item::WorkItem;
use mongodb::Database;
use persistence_core::Executor;

use super::approval_prepare::ensure_submit_authorized;
use super::cancel_runtime::{
    ensure_cancel_authorized_with_executor, ensure_cancel_instance_binding, ensure_cancel_instance_subject,
    ensure_stock_adjustment_open_task_identity,
};
use crate::Error;

pub(super) struct StockAdjustmentReadPolicy {
    pub(super) db: Database,
    pub(super) rbac: SharedRbacService,
}

#[async_trait]
impl InventoryAdjustmentApprovalPolicyPort for StockAdjustmentReadPolicy {
    async fn actor_can_submit(
        &self,
        adjustment: &StockAdjustment,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> ReadResult<bool> {
        eligibility(ensure_submit_authorized(&self.db, &self.rbac, adjustment, actor, executor).await)
    }

    async fn actor_can_cancel(
        &self,
        instance: &ApprovalProcessInstance,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> ReadResult<bool> {
        eligibility(
            ensure_cancel_authorized_with_executor(&self.db, &self.rbac, instance, actor, executor)
                .await
                .map(|_| ()),
        )
    }

    fn ensure_instance_binding(
        &self,
        instance: &ApprovalProcessInstance,
        binding: &ApprovalDefinitionBinding,
    ) -> ReadResult<()> {
        ensure_cancel_instance_binding(instance, binding).map_err(read_error)
    }

    fn ensure_instance_subject(
        &self,
        instance: &ApprovalProcessInstance,
        id: &str,
        version: u32,
    ) -> ReadResult<()> {
        ensure_cancel_instance_subject(instance, id, version).map_err(read_error)
    }

    fn ensure_open_task_identity(
        &self,
        task: &WorkItem,
        instance: &ApprovalProcessInstance,
        execution: &ApprovalNodeExecution,
        organization: &str,
    ) -> ReadResult<()> {
        ensure_stock_adjustment_open_task_identity(task, instance, execution, organization)
            .map_err(read_error)
    }
}

fn eligibility(result: crate::Result<()>) -> ReadResult<bool> {
    match result {
        Ok(()) => Ok(true),
        Err(Error::Forbidden(_)) => Ok(false),
        Err(error) => Err(read_error(error)),
    }
}

fn read_error(error: Error) -> ReadError {
    match error {
        Error::Internal(value) => ReadError::Internal(value),
        Error::NotFound(value) => ReadError::NotFound(value),
        Error::ValidationError(value) => ReadError::ValidationError(value),
        Error::InvalidInput(value) => ReadError::InvalidInput(value),
        Error::BusinessLogicError(value) => ReadError::BusinessLogicError(value),
        Error::ConflictError(value) => ReadError::ConflictError(value),
        Error::ReceiptDuplicate(value) => ReadError::ReceiptDuplicate(value),
        Error::TransientTransaction(value) => ReadError::TransientTransaction(value),
        Error::Forbidden(value) => ReadError::Forbidden(value),
        Error::Unauthenticated(value) => ReadError::Unauthenticated(value),
        Error::Logic(value) => ReadError::Logic(value),
        Error::Rbac(value) => ReadError::Rbac(value),
        Error::OutcomeUnknown(value) => ReadError::OutcomeUnknown(value),
        Error::RepositoryError(value) => ReadError::RepositoryError(value),
        Error::Coded(value) => ReadError::Coded(value),
    }
}

#[cfg(test)]
mod tests {
    use erp_workflow::ErrorCode;

    use super::*;

    #[test]
    fn action_eligibility_only_hides_explicit_forbidden() {
        assert!(eligibility(Ok(())).unwrap());
        assert!(!eligibility(Err(Error::Forbidden("没有动作资格".into()))).unwrap());
        assert!(
            matches!(eligibility(Err(Error::ConflictError("冻结快照不一致".into()))), Err(ReadError::ConflictError(message)) if message == "冻结快照不一致")
        );
        for code in ErrorCode::ALL {
            assert!(
                matches!(eligibility(Err(Error::Coded(code))), Err(ReadError::Coded(actual)) if actual == code)
            );
        }
    }
}
