//! 采购分配命令的销售事实提供方装配。
use crate::Result;
use erp_procurement::entity::purchase_order::{PurchaseOrder, PurchaseOrderRevisionLine};
use erp_procurement::service::purchase_order::allocation_maintenance::PreparedSalesAllocations;
use persistence_core::Executor;
/// 以来源销售单当前版本事实构造采购分配，事实读取和后续写入复用根执行器。
pub(super) async fn prepare_current_sales_allocations(
    db: &mongodb::Database,
    order: &PurchaseOrder,
    revision_lines: &mut [PurchaseOrderRevisionLine],
    executor: &mut dyn Executor,
) -> Result<PreparedSalesAllocations> {
    Ok(
        erp_procurement::service::purchase_order::allocation_maintenance::prepare_current_sales_allocations(
            &super::adapters::SalesAllocationAdapter(db.clone()),
            order,
            revision_lines,
            executor,
        )
        .await?,
    )
}
