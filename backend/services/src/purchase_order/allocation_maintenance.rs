//! 采购当前版本到销售当前版本的正式分配维护。
//!
//! 正式 allocation 必须按采购版本行携带的稳定 `sales_order_line_id`，绑定来源
//! 销售单 `current_revision_id` 下的当前销售版本行。采购变更新当前版本时重新生成
//! 该版本自己的 allocation，历史采购版本 allocation 保持不可变且不参与当前覆盖。

use std::collections::HashMap;

use database::{Executor, PurchaseOrderExt, SalesOrderExt};
use entities::ids::{PurchaseLineSalesAllocationId, SalesOrderRevisionId, SalesOrderRevisionLineId};
use entities::purchase_order::{
    CurrentSalesAllocationLine, CurrentSalesAllocationPlan, CurrentSalesAllocationPlanError,
    PurchaseLineSalesAllocation, PurchaseLineType, PurchaseOrder, PurchaseOrderRevisionLine,
};
use id_generator::next_id;

use crate::errors::{Error, Result};

/// 待写入的当前采购版本销售分配。
pub(super) struct PreparedSalesAllocations {
    /// 采购版本行 ID 到 allocation ID，供履约草稿引用。
    pub by_purchase_line: HashMap<String, PurchaseLineSalesAllocationId>,
    /// 待与采购版本同事务写入的 allocation 实体。
    allocations: Vec<PurchaseLineSalesAllocation>,
}

/// 按来源销售单当前版本重建采购版本行关联并构造正式 allocation。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `order` - 来源采购单
/// * `revision_lines` - 即将成为当前采购版本的行；函数会刷新销售当前版本行字段
/// * `executor` - 数据访问执行器，正式化与变更生效必须传入事务会话
///
/// # 返回
/// 返回待写入 allocation 及采购版本行到 allocation 的引用映射。
///
/// # 错误
/// 来源销售单、当前销售版本或稳定销售行缺失，分配数量与采购数量不一致，或仓储
/// 查询失败时返回错误。
///
/// # 关键业务约束
/// 只查询来源销售单 `stable.current_revision_id`，不在历史销售版本中猜测匹配；
/// Service 按商品/服务行顺序生成分配主键，纯匹配与构造规则委托实体计划。
pub(super) async fn prepare_current_sales_allocations(
    db: &mongodb::Database,
    order: &PurchaseOrder,
    revision_lines: &mut [PurchaseOrderRevisionLine],
    executor: &mut dyn Executor,
) -> Result<PreparedSalesAllocations> {
    let sales_order = db
        .sales_orders()
        .find_by_id(&order.sales_order_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("来源销售单不存在".to_string()))?;
    let revision_id = sales_order
        .stable
        .current_revision_id
        .as_ref()
        .ok_or_else(|| Error::BusinessLogicError("来源销售单缺少当前版本".to_string()))?;
    let sales_lines = db
        .sales_order_revision_lines()
        .list_lines_by_revision(&SalesOrderRevisionId::new(revision_id.clone()), executor)
        .await?;
    let current_sales_lines = sales_lines
        .into_iter()
        .map(|line| CurrentSalesAllocationLine {
            sales_order_line_id: line.sales_order_line_id,
            sales_order_revision_line_id: SalesOrderRevisionLineId::new(line.base.id),
        })
        .collect::<Vec<_>>();
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

/// 将分配计划领域错误转换为既有 Service 错误类别。
///
/// # 参数
/// * `error` - 实体计划构造失败原因
///
/// # 返回
/// 返回保持既有协议分类与错误文本的 Service 错误。
///
/// # 错误
/// 本转换本身不会失败；返回值供调用方沿原错误路径传播。
///
/// # 关键约束
/// 行匹配与数量规则保持 `BusinessLogicError`，分配实体自身错误保持 `Logic`，
/// Service 生成主键数量不一致视为内部不变量损坏。
fn translate_current_sales_allocation_plan_error(error: CurrentSalesAllocationPlanError) -> Error {
    match error {
        CurrentSalesAllocationPlanError::InvalidAllocation(source) => Error::Logic(source),
        error @ CurrentSalesAllocationPlanError::AllocationIdCountMismatch => {
            Error::Internal(error.to_string())
        }
        rule_error => Error::BusinessLogicError(rule_error.to_string()),
    }
}

/// 将已准备的 allocation 写入当前事务。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `prepared` - 已按销售当前版本构造的 allocation
/// * `executor` - 数据访问执行器，必须与采购版本写入使用同一事务
///
/// # 返回
/// 全部写入成功返回 `Ok(())`。
///
/// # 错误
/// 唯一键冲突或仓储写入失败时返回错误。
///
/// # 关键业务约束
/// 每个采购版本行只形成一条当前版本 allocation。
pub(super) async fn persist_current_sales_allocations(
    db: &mongodb::Database,
    prepared: &PreparedSalesAllocations,
    executor: &mut dyn Executor,
) -> Result<()> {
    for allocation in &prepared.allocations {
        db.purchase_line_sales_allocations()
            .create(allocation, executor)
            .await?;
    }
    Ok(())
}
