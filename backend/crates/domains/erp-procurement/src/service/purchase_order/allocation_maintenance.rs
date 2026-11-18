//! 采购当前版本到销售当前版本的正式分配维护。
//!
//! 正式 allocation 必须按采购版本行携带的稳定 `sales_order_line_id`，绑定来源
//! 销售单 `current_revision_id` 下的当前销售版本行。采购变更新当前版本时重新生成
//! 该版本自己的 allocation，历史采购版本 allocation 保持不可变且不参与当前覆盖。

use std::collections::HashMap;

use erp_core::ids::PurchaseLineSalesAllocationId;
use id_generator::next_id;
use persistence_core::Executor;

use crate::entity::purchase_order::{
    CurrentSalesAllocationPlan, CurrentSalesAllocationPlanError, PurchaseLineSalesAllocation,
    PurchaseLineType, PurchaseOrder, PurchaseOrderRevisionLine,
};
use crate::ports::purchase_order::SalesAllocationPort;
use crate::repository::PurchaseOrderExt;
use crate::{Error, Result};

/// 待写入的当前采购版本销售分配。
pub struct PreparedSalesAllocations {
    /// 采购版本行 ID 到 allocation ID，供履约草稿引用。
    pub by_purchase_line: HashMap<String, PurchaseLineSalesAllocationId>,
    /// 待与采购版本同事务写入的 allocation 实体。
    allocations: Vec<PurchaseLineSalesAllocation>,
}

/// 采购正式化或变更生效时，在同一事务内用来源销售单的当前修订重绑本次版本行，并构造待写分配及引用映射。
/// 只认同一稳定销售行和精确 SKU，不从历史修订猜测。来源缺失、数量不一致或事实读取失败时停止。
pub async fn prepare_current_sales_allocations(
    port: &dyn SalesAllocationPort,
    order: &PurchaseOrder,
    revision_lines: &mut [PurchaseOrderRevisionLine],
    executor: &mut dyn Executor,
) -> Result<PreparedSalesAllocations> {
    let current_sales_lines = port.current_lines(&order.sales_order_id, executor).await?;

    let allocation_ids = revision_lines
        .iter()
        .filter(|line| line.line_type == PurchaseLineType::ItemService)
        .map(|_| PurchaseLineSalesAllocationId::new(next_id()))
        .collect();
    let plan = CurrentSalesAllocationPlan::build(revision_lines, &current_sales_lines, allocation_ids)
        .map_err(translate_current_sales_allocation_plan_error)?;

    revision_lines.clone_from_slice(&plan.rebound_purchase_lines);
    Ok(PreparedSalesAllocations {
        by_purchase_line: plan.allocation_ids_by_purchase_line,
        allocations: plan.allocations,
    })
}

// 将分配计划领域错误转换为既有 Service 错误类别。
//
// 行匹配与数量规则保持 `BusinessLogicError`，分配实体自身错误保持 `Logic`，Service 生成主键数量不一致视为内部不变量损坏。
//
// 返回保持既有协议分类与错误文本的 Service 错误。本转换本身不会失败；返回值供调用方沿原错误路径传播。
//
// `error` 为实体计划构造失败原因。
fn translate_current_sales_allocation_plan_error(error: CurrentSalesAllocationPlanError) -> Error {
    match error {
        CurrentSalesAllocationPlanError::InvalidAllocation(source) => Error::Logic(source),
        error @ CurrentSalesAllocationPlanError::AllocationIdCountMismatch => {
            Error::Internal(error.to_string())
        },
        rule_error => Error::BusinessLogicError(rule_error.to_string()),
    }
}

/// 将按当前销售修订准备好的正式销售分配写入与采购版本相同的事务。
/// 每条参与分配的采购版本行只写一条当前版本分配；唯一键冲突或仓储失败时停止。
pub async fn persist_current_sales_allocations(
    db: &mongodb::Database,
    prepared: &PreparedSalesAllocations,
    executor: &mut dyn Executor,
) -> Result<()> {
    for allocation in &prepared.allocations {
        db.purchase_line_sales_allocations().create(allocation, executor).await?;
    }
    Ok(())
}
