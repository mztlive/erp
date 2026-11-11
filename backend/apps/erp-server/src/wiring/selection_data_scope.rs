//! 选品授权接线；客户集合和多负责人资格由客户拥有领域提供。
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_customer::authorization::CustomerMapping;
use erp_customer::{CustomerAccess, Error as CustomerError};
use erp_identity::SharedRbacService;
use erp_sales::ports::sales_selection::SelectionCustomerAccessPort;
use erp_sales::service::sales_selection::SelectionAccess;
use erp_sales::{Error as SalesError, Result as SalesResult};
use mongodb::Database;
use persistence_core::Executor;

/// 构造选品统一授权访问器。
/// # 参数
/// db、rbac 为应用共享数据库与身份设施。
/// # 返回
/// 绑定客户来源 Mapping 和完整客户对象资格的选品访问器。
/// # 错误
/// 固定目录非法时启动失败；不执行 I/O。
pub fn selection_access(db: Database, rbac: SharedRbacService) -> SelectionAccess {
    SelectionAccess::new(
        db.clone(),
        rbac.clone(),
        CustomerMapping::shared(db.clone()),
        Arc::new(SelectionCustomers(CustomerAccess::new(db, rbac))),
    )
}

struct SelectionCustomers(CustomerAccess);

#[async_trait]
impl SelectionCustomerAccessPort for SelectionCustomers {
    async fn require_detail(
        &self,
        actor: &AuditActor,
        customer_id: &str,
        executor: &mut dyn Executor,
    ) -> SalesResult<()> {
        if executor.session().is_some() {
            self.0.require_with(actor.clone(), "detail", customer_id, executor).await
        } else {
            // 纯读入口沿用客户领域公开查询快照；写命令始终走上面的原事务分支。
            self.0.require(actor, "detail", customer_id).await
        }
        .map(|_| ())
        .map_err(customer_error)
    }
}

/// 客户拒绝与设施错误保留类别，不转换为选品空范围。
fn customer_error(error: CustomerError) -> SalesError {
    match error {
        CustomerError::Internal(v) => SalesError::Internal(v),
        CustomerError::NotFound(v) => SalesError::NotFound(v),
        CustomerError::ValidationError(v) => SalesError::ValidationError(v),
        CustomerError::InvalidInput(v) => SalesError::InvalidInput(v),
        CustomerError::BusinessLogicError(v) => SalesError::BusinessLogicError(v),
        CustomerError::ConflictError(v) => SalesError::ConflictError(v),
        CustomerError::ReceiptDuplicate(v) => SalesError::ReceiptDuplicate(v),
        CustomerError::TransientTransaction(v) => SalesError::TransientTransaction(v),
        CustomerError::Forbidden(v) => SalesError::Forbidden(v),
        CustomerError::Unauthenticated(v) => SalesError::Unauthenticated(v),
        CustomerError::Logic(v) => SalesError::Logic(v),
        CustomerError::OutcomeUnknown(v) => SalesError::OutcomeUnknown(v),
        CustomerError::RepositoryError(v) => SalesError::RepositoryError(v),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn customer_authorization_denial_and_invalidated_snapshot_keep_their_classes() {
        assert!(matches!(
            customer_error(CustomerError::Forbidden("customer denied".into())),
            SalesError::Forbidden(message) if message == "customer denied"
        ));
        assert!(matches!(
            customer_error(CustomerError::NotFound("customer missing".into())),
            SalesError::NotFound(message) if message == "customer missing"
        ));
        assert!(matches!(
            customer_error(CustomerError::ConflictError("DATA_SCOPE_CHANGED".into())),
            SalesError::ConflictError(message) if message == "DATA_SCOPE_CHANGED"
        ));
        assert!(matches!(
            customer_error(CustomerError::Internal("customer policy unavailable".into())),
            SalesError::Internal(message) if message == "customer policy unavailable"
        ));
    }
}
