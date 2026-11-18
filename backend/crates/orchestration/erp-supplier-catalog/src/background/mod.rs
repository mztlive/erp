//! 供应目录用例到通用后台执行器的业务接线。
use background_runtime::TaskResult;
use persistence_core::Result as PersistenceResult;

use crate::Error;

mod supplier_catalog_adapter;
pub use supplier_catalog_adapter::SupplierCatalogTaskAdapter;
mod supplier_catalog_adoption_adapter;
pub use supplier_catalog_adoption_adapter::SupplierCatalogAdoptionTaskAdapter;
mod delivery_coverage_adapter;
pub use delivery_coverage_adapter::DeliveryCoverageRefreshAdapter;
mod sellable_projection_adapter;
pub use sellable_projection_adapter::SellableProjectionRefreshAdapter;

// 轮询查询先按本业务错误合同分类，再交给无业务依赖的后台执行器。
fn polling_result<T>(result: PersistenceResult<T>) -> TaskResult<T> {
    Ok(result.map_err(Error::from)?)
}

#[cfg(test)]
mod tests {
    use mongodb::error::Error as MongoError;
    use persistence_core::Error as PersistenceError;

    use super::*;

    #[test]
    fn polling_results_preserve_successful_values() {
        assert_eq!(polling_result(Ok(vec!["first", "second"])).unwrap(), ["first", "second"]);
        assert!(polling_result::<Vec<String>>(Ok(Vec::new())).unwrap().is_empty());
    }

    #[test]
    fn polling_database_errors_keep_application_message_and_typed_payload() {
        let cause = PersistenceError::DatabaseError(MongoError::custom("offline query failure"));
        let message = format!("数据库错误：{cause}");
        let error = polling_result::<usize>(Err(cause)).unwrap_err();

        assert_eq!(error.to_string(), message);
        assert!(matches!(
            error.downcast_ref::<Error>(),
            Some(Error::RepositoryError(PersistenceError::DatabaseError(_)))
        ));
    }

    #[test]
    fn polling_transaction_conflicts_keep_application_class_and_typed_source() {
        let cause = PersistenceError::TransientTransactionConflict(MongoError::custom("offline conflict"));
        let source_message = cause.to_string();
        let error = polling_result::<usize>(Err(cause)).unwrap_err();

        assert_eq!(error.to_string(), "数据冲突: 并发事务冲突，请重试");
        assert!(matches!(error.downcast_ref::<Error>(), Some(Error::TransientTransaction(_))));
        let source = error.source().unwrap();
        assert_eq!(source.to_string(), source_message);
        assert!(matches!(
            source.downcast_ref::<PersistenceError>(),
            Some(PersistenceError::TransientTransactionConflict(_))
        ));
    }
}
