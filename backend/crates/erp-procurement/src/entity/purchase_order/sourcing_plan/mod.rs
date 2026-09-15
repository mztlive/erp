//! 选源计划领域值对象与纯规则。
//!
//! 选源命令把客户确认的逐行供给分配一次落地为现有库存预占与采购缺口单：选源
//! 行必须先规范化（字符串类型化、同销售行同依据去重、稳定排序），再按精确采购
//! 依据与库存余额分组形成计划，并保证库存与采购合计不超过最新销售剩余量。
//! 本模块承载规范化、分组、仓库履约契约与跨方案总量不变式等无 I/O 规则；
//! DTO 负责调用 [`SourcingAssignment::parse`] 完成字符串类型化，Repository
//! 返回销售单、精确依据与库存余额事实，Service 负责事务内最新事实重验、
//! 预占写入与采购单创建编排。

mod assignment;
mod plan;
mod stock;

pub use assignment::{SourcingAssignment, SourcingAssignmentSet, SupplySourceType};
pub use plan::{SourcingDraftPlan, SourcingPlan, SourcingPlanError};
pub use stock::{
    RequestedStockLine, StockAllocationPlan, StockBasisGroup, StockBasisLine, stock_basis_id_for,
};

#[cfg(test)]
mod tests;
