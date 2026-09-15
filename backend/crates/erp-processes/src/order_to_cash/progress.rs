//! Compose the finance balance provider with the sales-owned progress operation.
use erp_core::ids::SalesOrderId;
use erp_sales::entity::sales_order::FulfillmentProgress;
use mongodb::Database;
use persistence_core::Executor;

use super::adapters::finance::FinanceMoneyProgressAdapter;

/// Refresh sales progress using financial facts and the caller's unchanged executor.
///
/// Sales existence is checked before finance is read; failures stop subsequent writes.
pub async fn update_sales_order_money_progress(
    db: &Database,
    executor: &mut dyn Executor,
    id: &SalesOrderId,
    actor_id: String,
    fulfillment: Option<FulfillmentProgress>,
) -> crate::Result<()> {
    let port = FinanceMoneyProgressAdapter::new(db.clone());
    Ok(erp_sales::service::sales_order::progress::update_sales_order_money_progress(
        db,
        &port,
        executor,
        id,
        actor_id,
        fulfillment,
    )
    .await?)
}
