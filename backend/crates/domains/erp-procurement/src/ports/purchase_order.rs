//! 当前采购分配所消费的销售当前版本行事实。
use async_trait::async_trait;
use erp_core::ids::SalesOrderId;
use persistence_core::Executor;

use crate::entity::purchase_order::CurrentSalesAllocationLine;
/// 当前销售版本行读取；提供方须保持销售单不存在和无当前版本的原首错。
#[async_trait]
pub trait SalesAllocationPort: Send + Sync {
    /// 依次读取销售单、当前版本指针和当前版本行，复用调用方执行器。
    ///
    /// # 参数
    /// * `id` - 来源销售单。
    /// * `executor` - 调用方执行器。
    ///
    /// # 返回
    /// 返回当前版本行的稳定行与版本行身份。
    ///
    /// # 错误
    /// 销售单不存在或缺少当前版本时按原首错返回；读取失败时返回对应错误。
    async fn current_lines(
        &self,
        id: &SalesOrderId,
        executor: &mut dyn Executor,
    ) -> crate::Result<Vec<CurrentSalesAllocationLine>>;
}
