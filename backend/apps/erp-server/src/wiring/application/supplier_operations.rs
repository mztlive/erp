//! 供应商订单消费的付款任务与目录消息 Port 的应用装配。

use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_finance_posting::finance_posting::payable::payment_request::sync_task;
use erp_supplier_catalog::supplier_connection_execution::CatalogMessageWriter;
use erp_supplier_orders::supplier_deposits::OrdersPaymentTasks;
use erp_supplier_orders::supplier_messages::SupplierMessageCatalog;
use erp_supplier_orders::{Error, Result};
use erp_supply::entity::supplier_api::SupplierApiConnection;
use erp_supply::entity::supplier_catalog::SupplierCatalogItem;
use mongodb::Database;
use persistence_core::Executor;

use super::Application;
use crate::wiring::catalog_service;

struct PaymentTasks(Database);
struct CatalogMessages(CatalogMessageWriter);

macro_rules! map_error {
    ($error:expr, $provider:ident) => {
        match $error {
            $provider::Error::Internal(value) => Error::Internal(value),
            $provider::Error::NotFound(value) => Error::NotFound(value),
            $provider::Error::ValidationError(value) => Error::ValidationError(value),
            $provider::Error::InvalidInput(value) => Error::InvalidInput(value),
            $provider::Error::BusinessLogicError(value) => Error::BusinessLogicError(value),
            $provider::Error::ConflictError(value) => Error::ConflictError(value),
            $provider::Error::ReceiptDuplicate(value) => Error::ReceiptDuplicate(value),
            $provider::Error::TransientTransaction(value) => Error::TransientTransaction(value),
            $provider::Error::Forbidden(value) => Error::Forbidden(value),
            $provider::Error::Unauthenticated(value) => Error::Unauthenticated(value),
            $provider::Error::Logic(value) => Error::Logic(value),
            $provider::Error::Rbac(value) => Error::Rbac(value),
            $provider::Error::OutcomeUnknown(value) => Error::OutcomeUnknown(value),
            $provider::Error::RepositoryError(value) => Error::RepositoryError(value),
            $provider::Error::Coded(value) => Error::Coded(value),
        }
    };
}

#[async_trait]
impl OrdersPaymentTasks for PaymentTasks {
    async fn sync(&self, request_id: &str, executor: &mut dyn Executor) -> Result<()> {
        sync_task(&self.0, request_id, executor).await.map_err(|error| map_error!(error, erp_finance_posting))
    }
}

#[async_trait]
impl SupplierMessageCatalog for CatalogMessages {
    async fn actor(&self, connection: &str) -> Result<AuditActor> {
        self.0.actor(connection).await.map_err(|error| map_error!(error, erp_supplier_catalog))
    }

    async fn save(
        &self,
        connection: &SupplierApiConnection,
        item: SupplierCatalogItem,
        actor: &AuditActor,
    ) -> Result<()> {
        self.0.save(connection, item, actor).await.map_err(|error| map_error!(error, erp_supplier_catalog))
    }
}

impl Application {
    /// 装配复用原执行器的付款请求任务同步提供者。
    /// # 参数
    /// 无；数据库来自当前应用。
    /// # 返回
    /// 返回订单编排消费的共享付款任务 Port。
    /// # 错误
    /// 构造时不执行 I/O，不返回错误。
    pub(crate) fn supplier_payment_tasks(&self) -> Arc<dyn OrdersPaymentTasks> {
        Arc::new(PaymentTasks(self.db()))
    }

    /// 装配消息补查后的目录写入提供者。
    /// # 参数
    /// 无；沿用当前目录服务接线。
    /// # 返回
    /// 返回消息接收方消费的目录 Port。
    /// # 错误
    /// 构造时不执行 I/O，不返回错误。
    pub(super) fn supplier_message_catalog(&self) -> Arc<dyn SupplierMessageCatalog> {
        Arc::new(CatalogMessages(CatalogMessageWriter::new(self.db(), catalog_service(self.db()))))
    }
}

#[cfg(test)]
mod tests {
    use erp_finance_posting::Error as FinanceError;
    use erp_supplier_catalog::Error as CatalogError;
    use mongodb::error::Error as MongoError;
    use persistence_core::Error as PersistenceError;

    use super::*;

    #[test]
    fn payment_task_bridge_keeps_unknown_commit_source() {
        let error = map_error!(
            FinanceError::OutcomeUnknown(PersistenceError::CommitOutcomeUnknown(MongoError::custom(
                "original commit",
            ))),
            erp_finance_posting
        );
        let Error::OutcomeUnknown(PersistenceError::CommitOutcomeUnknown(source)) = error else {
            panic!("付款任务桥接必须保留未知提交类型");
        };
        assert_eq!(source.get_custom::<&str>(), Some(&"original commit"));
    }

    #[test]
    fn catalog_message_bridge_preserves_rejection_and_receipt_source() {
        let rejection = map_error!(CatalogError::Forbidden("原连接授权已失效".into()), erp_supplier_catalog);
        assert!(matches!(rejection, Error::Forbidden(value) if value == "原连接授权已失效"));
        let error = map_error!(
            CatalogError::ReceiptDuplicate(PersistenceError::DuplicateKey(MongoError::custom("receipt"))),
            erp_supplier_catalog
        );
        assert!(matches!(error, Error::ReceiptDuplicate(PersistenceError::DuplicateKey(_))));
    }
}
