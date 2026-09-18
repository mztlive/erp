//! 当前采购覆盖来源；只读取采购领域拥有的集合。

use erp_core::ids::{
    PurchaseOrderRevisionId, PurchaseOrderRevisionLineId, PurchaseOrderSubmissionId, SalesOrderId,
};
use persistence_core::{Executor, Result};

use super::PurchaseOrderDomainRepository;
use crate::entity::purchase_order::{
    PurchaseLineSalesAllocation, PurchaseLineType, PurchaseOrder, PurchaseOrderRevisionLine,
    PurchaseOrderStatus, PurchaseOrderSubmissionLine,
};
use crate::repository::PurchaseOrderExt;
use crate::repository::prelude::*;

/// 采购单当前指针下的覆盖来源。
#[derive(Debug, Clone, Default)]
pub struct PurchaseCoverageSources {
    /// 未删除且参与覆盖的采购单。
    pub purchase_orders: Vec<PurchaseOrder>,
    /// 草稿类当前提交行。
    pub submission_lines: Vec<PurchaseOrderSubmissionLine>,
    /// 正式类当前版本行。
    pub purchase_revision_lines: Vec<PurchaseOrderRevisionLine>,
    /// 正式版本商品行的销售分配。
    pub allocations: Vec<PurchaseLineSalesAllocation>,
}

impl PurchaseOrderDomainRepository<'_> {
    /// 批量读取销售单对应的当前采购覆盖来源。
    ///
    /// # Errors
    /// MongoDB 查询或反序列化失败时返回错误。指针缺失由采购领域规则校验。
    ///
    /// 所有读取复用调用方 Executor，按原采购单、提交行、版本行、分配顺序执行。
    pub async fn coverage_sources(
        &self,
        sales_order_id: &SalesOrderId,
        executor: &mut dyn Executor,
    ) -> Result<PurchaseCoverageSources> {
        let purchase_orders =
            self.db.purchase_orders().find_covering_by_sales_order(sales_order_id, executor).await?;
        let submission_ids = current_pointer_ids(&purchase_orders);
        let submission_lines = self
            .db
            .purchase_order_submission_lines()
            .find_lines_by_submission_ids(&submission_ids, executor)
            .await?;
        let revision_ids = current_revision_pointer_ids(&purchase_orders);
        let purchase_revision_lines = self
            .db
            .purchase_order_revision_lines()
            .find_lines_by_revision_ids(&revision_ids, executor)
            .await?;
        let line_ids = purchase_revision_lines
            .iter()
            .filter(|line| line.line_type == PurchaseLineType::ItemService)
            .map(|line| PurchaseOrderRevisionLineId::new(line.base.id.clone()))
            .collect::<Vec<_>>();
        let allocations = self
            .db
            .purchase_line_sales_allocations()
            .find_by_purchase_revision_line_ids(&line_ids, executor)
            .await?;
        Ok(PurchaseCoverageSources {
            purchase_orders,
            submission_lines,
            purchase_revision_lines,
            allocations,
        })
    }
}

/// 提取草稿类当前提交指针，保留原顺序；缺失由领域完整性校验处理。
fn current_pointer_ids(orders: &[PurchaseOrder]) -> Vec<PurchaseOrderSubmissionId> {
    orders
        .iter()
        .filter(|order| {
            matches!(
                order.stable.status,
                PurchaseOrderStatus::Draft
                    | PurchaseOrderStatus::PendingFinanceReview
                    | PurchaseOrderStatus::InApproval
            )
        })
        .filter_map(|order| {
            order.current_submission_id.as_ref().map(|id| PurchaseOrderSubmissionId::new(id.clone()))
        })
        .collect()
}

/// 提取正式类当前版本指针，保留原顺序；历史和作废单不进入结果。
fn current_revision_pointer_ids(orders: &[PurchaseOrder]) -> Vec<PurchaseOrderRevisionId> {
    orders
        .iter()
        .filter(|order| {
            matches!(
                order.stable.status,
                PurchaseOrderStatus::Effective
                    | PurchaseOrderStatus::PartiallyExecuted
                    | PurchaseOrderStatus::Completed
            )
        })
        .filter_map(|order| {
            order.stable.current_revision_id.as_ref().map(|id| PurchaseOrderRevisionId::new(id.clone()))
        })
        .collect()
}
