//! 当前采购分配所消费的销售当前版本行事实。
use async_trait::async_trait;
use erp_core::ids::SalesOrderId;
use persistence_core::Executor;

use crate::entity::purchase_order::CurrentSalesAllocationLine;
/// 当前销售版本行读取；提供方须保持销售单不存在和无当前版本的原首错。
#[async_trait]
pub trait SalesAllocationPort: Send + Sync {
    /// 依次读取销售单、当前版本指针和当前版本行，复用调用方执行器。
    async fn current_lines(
        &self,
        id: &SalesOrderId,
        executor: &mut dyn Executor,
    ) -> crate::Result<Vec<CurrentSalesAllocationLine>>;
}
