//! 装配采购覆盖 Port；领域保持指针缺失、覆盖累计和超量规则的唯一实现。
use async_trait::async_trait;
use erp_core::ids::{SalesOrderId, SalesOrderRevisionId};
use erp_procurement::entity::purchase_order::{ProcurementCoverageFacts, SalesProcurementCoverage};
use erp_procurement::ports::coverage::ProcurementCoveragePort;
use erp_sales::entity::sales_order::SalesOrder;
use mongodb::Database;
use persistence_core::Executor;

use crate::Result;
struct DatabaseCoverageSource<'a> {
    db: &'a Database,
}
#[async_trait]
impl ProcurementCoveragePort for DatabaseCoverageSource<'_> {
    async fn load_procurement_coverage_facts(
        &self,
        revision_id: &SalesOrderRevisionId,
        sales_order_id: &SalesOrderId,
        executor: &mut dyn Executor,
    ) -> erp_procurement::Result<ProcurementCoverageFacts> {
        super::load_procurement_coverage_facts(self.db, revision_id, sales_order_id, executor)
            .await
            .map_err(Into::into)
    }
}
/// 加载销售当前修订采购覆盖，保留原当前指针错误与同一执行器。
///
/// # Errors
/// 当前修订缺失、来源不完整、覆盖超额或提供方读取失败时传播原采购错误。
pub async fn load_sales_procurement_coverage(
    db: &Database,
    order: &SalesOrder,
    executor: &mut dyn Executor,
) -> Result<SalesProcurementCoverage> {
    erp_procurement::service::purchase_order::coverage::load_sales_procurement_coverage(
        &DatabaseCoverageSource { db },
        order.current_revision_id(),
        &SalesOrderId::new(order.base.id.clone()),
        executor,
    )
    .await
    .map_err(Into::into)
}
