//! 用财务余额提供方组合销售自有的进度操作。
use erp_core::ids::SalesOrderId;
use erp_sales::entity::sales_order::FulfillmentProgress;
use mongodb::Database;
use persistence_core::Executor;

use super::adapters::finance::FinanceMoneyProgressAdapter;

/// 用财务事实刷新销售进度，沿用调用方未改变的执行器。
///
/// 销售侧先确认单据存在，再读取财务；任一步失败则不再继续写入。
///
/// # 参数
/// * `db` - 业务数据库。
/// * `executor` - 调用方执行器，本函数不另开事务。
/// * `id` - 销售单标识。
/// * `actor_id` - 操作人。
/// * `fulfillment` - 交给销售进度操作的可选履约进度。
///
/// # 返回
/// 进度刷新写入成功时返回。
///
/// # 错误
/// 销售单缺失、财务读取失败或进度写入失败时返回下层错误。
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
