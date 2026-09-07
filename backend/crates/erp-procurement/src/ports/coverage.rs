//! 当前采购覆盖所需外域事实加载合同。
use crate::entity::purchase_order::ProcurementCoverageFacts;
use async_trait::async_trait;
use erp_core::ids::{SalesOrderId, SalesOrderRevisionId};
use persistence_core::Executor;
/// 提供方组合当前修订、采购本域来源及现有库存事实；不得开启新事务。
#[async_trait]
pub trait ProcurementCoveragePort: Send + Sync {
    /// 保留事实缺失语义并复用调用方执行器。
    async fn load_procurement_coverage_facts(
        &self,
        revision_id: &SalesOrderRevisionId,
        sales_order_id: &SalesOrderId,
        executor: &mut dyn Executor,
    ) -> crate::Result<ProcurementCoverageFacts>;
}
