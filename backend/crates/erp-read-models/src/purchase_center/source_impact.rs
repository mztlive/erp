//! 销售单下采购的当前覆盖与受影响判定（第一期 §6.3“销售变更对采购的影响”）。
//!
//! 覆盖与受影响采购使用同一批事实计算，供销售详情、任务同步与命令复验共用。

use erp_core::ids::{SalesChangeSubmissionId, SalesOrderId, SalesOrderRevisionId};
use erp_procurement::entity::purchase_order::{
    ProcurementCoverageFacts, PurchaseOrder, PurchaseSourceImpact, SalesProcurementCoverage,
    build_procurement_coverage, evaluate_source_impacts, predict_source_impacts,
};
use erp_procurement::repository::purchase_source_update::find_open_for_purchase;
use erp_sales::entity::sales_order::SalesOrder;
use erp_sales::repository::SalesReviewExt;
use erp_sales::repository::prelude::*;
use mongodb::Database;
use persistence_core::Executor;

use super::repository::load_procurement_coverage_facts;
use super::repository::mapping::proposed_line_fact;
use crate::{Error, Result};

/// 同一批事实得到的当前覆盖与受影响采购。
#[derive(Debug, Clone)]
pub struct PurchaseSourceState {
    /// 覆盖与判定所用事实。
    pub facts: ProcurementCoverageFacts,
    /// 当前销售版本逐行覆盖。
    pub coverage: SalesProcurementCoverage,
    /// 受影响采购，顺序与事实中的采购一致。
    pub impacts: Vec<PurchaseSourceImpact>,
}

impl PurchaseSourceState {
    /// 查找指定采购的受影响判定。
    ///
    /// # 参数
    /// * `purchase_order_id` - 采购单 ID
    ///
    /// # 返回
    /// 采购受影响时返回判定；否则返回 `None`。
    ///
    /// # 错误
    /// 无。
    pub fn impact_of(&self, purchase_order_id: &str) -> Option<&PurchaseSourceImpact> {
        self.impacts.iter().find(|impact| impact.purchase_order_id == purchase_order_id)
    }
}

/// 加载销售单当前覆盖并判定受影响采购。
///
/// # 参数
/// * `db` - MongoDB 数据库句柄
/// * `order` - 已加载的销售单
/// * `executor` - 调用方执行器；事务内重验必须复用
///
/// # 返回
/// 返回同一批事实计算出的覆盖与受影响采购。
///
/// # 错误
/// 销售单缺少当前版本、来源事实不完整、履约期限无法换算或仓储读取失败时返回错误。
pub async fn load_purchase_source_state(
    db: &Database,
    order: &SalesOrder,
    executor: &mut dyn Executor,
) -> Result<PurchaseSourceState> {
    let revision_id = order
        .current_revision_id()
        .ok_or_else(|| Error::BusinessLogicError("销售单缺少当前版本，无法计算采购剩余量".into()))?;
    let facts = load_procurement_coverage_facts(
        db,
        &SalesOrderRevisionId::new(revision_id),
        &SalesOrderId::new(&order.base.id),
        executor,
    )
    .await?;
    let coverage = build_procurement_coverage(facts.clone()).map_err(Error::Logic)?;
    let impacts = evaluate_source_impacts(&facts, &coverage).map_err(Error::Logic)?;
    Ok(PurchaseSourceState { facts, coverage, impacts })
}

/// 判断采购是否因销售变更减量或移除而暂停付款。
///
/// # 参数
/// * `db` - MongoDB 数据库句柄
/// * `purchase_order_id` - 采购单 ID
/// * `executor` - 调用方执行器；付款事务内复验必须复用
///
/// # 返回
/// 存在未处理且原因为数量归零或减少的依据更新记录时返回 `true`。
///
/// # 错误
/// 仓储读取失败时返回错误。
///
/// # 关键业务约束
/// 第一期 §6.3“销售变更对采购的影响”第 8 条：其他原因及未受影响的采购不因销售
/// 版本变化阻断付款。
pub async fn payment_paused_by_source_update(
    db: &Database,
    purchase_order_id: &str,
    executor: &mut dyn Executor,
) -> Result<bool> {
    Ok(find_open_for_purchase(db, purchase_order_id, executor)
        .await?
        .is_some_and(|update| update.blocks_payment))
}

/// 按待审批销售变更的提交内容预判受影响采购。
///
/// # 参数
/// * `db` - MongoDB 数据库句柄
/// * `order` - 来源销售单
/// * `submission_id` - 变更当前提交
/// * `executor` - 调用方执行器
///
/// # 返回
/// 返回参与覆盖的采购与预判结果；预判仅供审批参考。
///
/// # 错误
/// 销售单缺少当前版本、覆盖事实不完整或读取失败时返回错误。
pub async fn predict_change_impacts(
    db: &Database,
    order: &SalesOrder,
    submission_id: &str,
    executor: &mut dyn Executor,
) -> Result<(Vec<PurchaseOrder>, Vec<PurchaseSourceImpact>)> {
    let revision_id = order
        .current_revision_id()
        .ok_or_else(|| Error::BusinessLogicError("销售单缺少当前版本，无法预判采购影响".into()))?;
    let facts = load_procurement_coverage_facts(
        db,
        &SalesOrderRevisionId::new(revision_id),
        &SalesOrderId::new(&order.base.id),
        executor,
    )
    .await?;
    let lines = db
        .sales_change_submission_lines()
        .list_lines_by_submission(&SalesChangeSubmissionId::new(submission_id), executor)
        .await?;
    let proposed = lines.into_iter().filter_map(proposed_line_fact).collect();
    let impacts = predict_source_impacts(&facts, &format!("proposed:{submission_id}"), proposed)
        .map_err(Error::Logic)?;
    Ok((facts.purchase_orders, impacts))
}
