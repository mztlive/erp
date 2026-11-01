//! 当前采购覆盖所需外域事实加载合同。
use async_trait::async_trait;
use erp_core::ids::{SalesOrderId, SalesOrderRevisionId};
use persistence_core::Executor;

use crate::entity::purchase_order::ProcurementCoverageFacts;
/// 提供方组合当前修订、采购本域来源及现有库存事实；不得开启新事务。
#[async_trait]
pub trait ProcurementCoveragePort: Send + Sync {
    /// 加载当前采购覆盖事实，保留缺失语义并复用调用方执行器。
    ///
    /// # 参数
    /// * `revision_id` - 销售订单当前修订。
    /// * `sales_order_id` - 来源销售单。
    /// * `executor` - 调用方执行器；实现不得另开事务。
    ///
    /// # 返回
    /// 返回组合后的 `ProcurementCoverageFacts`。修订或版本文档缺失时由 `revision` 为空表达，不改写成错误。
    ///
    /// # 错误
    /// 提供方读取失败时返回对应错误。
    async fn load_procurement_coverage_facts(
        &self,
        revision_id: &SalesOrderRevisionId,
        sales_order_id: &SalesOrderId,
        executor: &mut dyn Executor,
    ) -> crate::Result<ProcurementCoverageFacts>;
}
