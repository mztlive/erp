//! 采购分配命令的销售事实提供方装配。
use erp_procurement::entity::purchase_order::{PurchaseOrder, PurchaseOrderRevisionLine};
use erp_procurement::service::purchase_order::allocation_maintenance::PreparedSalesAllocations;
use persistence_core::Executor;

use crate::Result;
/// 以来源销售单当前版本事实构造采购分配，事实读取和后续写入复用根执行器。
///
/// # 参数
/// * `db` - 读取销售当前版本行的数据库。
/// * `order` - 待分配的采购单。
/// * `revision_lines` - 采购版本行；成功时替换为已重绑销售行的版本行。
/// * `executor` - 与后续写入共用的执行器。
///
/// # 返回
/// 返回按采购行索引的分配计划。
///
/// # 错误
/// 当前销售版本读取或分配计划构造失败时返回对应错误。
pub(super) async fn prepare_current_sales_allocations(
    db: &mongodb::Database,
    order: &PurchaseOrder,
    revision_lines: &mut [PurchaseOrderRevisionLine],
    executor: &mut dyn Executor,
) -> Result<PreparedSalesAllocations> {
    Ok(erp_procurement::service::purchase_order::allocation_maintenance::prepare_current_sales_allocations(
        &super::adapters::SalesAllocationAdapter(db.clone()),
        order,
        revision_lines,
        executor,
    )
    .await?)
}
