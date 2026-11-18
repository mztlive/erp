//! 采购正式版本的单域序号读取与冻结构造。
use erp_core::common::time::Instant;
use erp_core::ids::{PurchaseOrderRevisionId, PurchaseOrderRevisionLineId};
use id_generator::next_id;
use persistence_core::NoTransaction;

use super::PurchaseOrderService;
use crate::entity::purchase_order::*;
use crate::repository::PurchaseOrderExt;
use crate::{Error, Result};
impl PurchaseOrderService {
    /// 正式化或采购变更落库前读取同一采购单的历史版本，取最大序号加一；无历史时从 1 开始。
    /// 读取使用 `NoTransaction`，不加入调用方事务；仓储失败或 `u32` 序号溢出时返回错误。
    pub async fn next_revision_no(&self, order: &PurchaseOrder) -> Result<u32> {
        let existing = self
            .db
            .purchase_order()
            .list_revisions_by_order(&order.base.id.clone().into(), &mut NoTransaction)
            .await?;
        PurchaseOrderRevision::next_revision_no(&existing).map_err(Into::into)
    }
    /// 采购首次审批通过时，由冻结提交及其明细构造生效版本头和版本行，不在此写库或建立销售分配。
    /// 冻结提交必须属于本采购单；提交状态、版本号或版本行不满足实体规则时拒绝构造。
    pub async fn build_effective_revision(
        &self,
        order: &PurchaseOrder,
        submission: &PurchaseOrderSubmission,
        submission_lines: &[PurchaseOrderSubmissionLine],
        revision_no: u32,
    ) -> Result<(PurchaseOrderRevision, Vec<PurchaseOrderRevisionLine>)> {
        if submission.purchase_order_id.as_ref() != order.base.id {
            return Err(crate::Error::BusinessLogicError("采购提交不属于当前采购单".to_string()));
        }

        let revision = PurchaseOrderRevision::from_submission(
            PurchaseOrderRevisionId::new(next_id()),
            revision_no,
            submission,
            Instant::now(),
        )?;

        let revision_id = PurchaseOrderRevisionId::new(revision.base.id.clone());
        let revision_lines = submission_lines
            .iter()
            .map(|line| {
                PurchaseOrderRevisionLine::from_submission_line(
                    PurchaseOrderRevisionLineId::new(next_id()),
                    revision_id.clone(),
                    line,
                )
            })
            .collect::<erp_core::Result<Vec<_>>>()?;
        Ok((revision, revision_lines))
    }
    /// 由变更提交构造生效版本和版本行，不写库。
    ///
    /// 返回尚未落库的版本头和版本行。变更提交或行不能形成版本时返回 `Logic`。
    ///
    /// - `order`：当前采购单，只取其稳定 ID。
    /// - `revision_no`：调用方算好的版本号。
    pub async fn build_change_revision(
        &self,
        order: &PurchaseOrder,
        submission: &PurchaseChangeSubmission,
        lines: &[PurchaseChangeSubmissionLine],
        revision_no: u32,
    ) -> Result<(PurchaseOrderRevision, Vec<PurchaseOrderRevisionLine>)> {
        let revision = PurchaseOrderRevision::from_change_submission(
            PurchaseOrderRevisionId::new(next_id()),
            order.base.id.clone().into(),
            revision_no,
            submission,
            Instant::now(),
        )?;

        let revision_id = PurchaseOrderRevisionId::new(revision.base.id.clone());
        let revision_lines = lines
            .iter()
            .map(|line| {
                PurchaseOrderRevisionLine::from_change_submission_line(
                    PurchaseOrderRevisionLineId::new(next_id()),
                    revision_id.clone(),
                    line,
                )
            })
            .collect::<erp_core::Result<Vec<_>>>()?;
        Ok((revision, revision_lines))
    }
}

/// 已通过原正式化准备与销售当前版本分配校验的采购写入计划。
pub struct FormalizedOrderWrite<'a> {
    /// 待推进的采购单。
    pub order: PurchaseOrder,
    /// 待记录审核结论的采购提交。
    pub submission: PurchaseOrderSubmission,
    /// 新正式版本。
    pub revision: &'a PurchaseOrderRevision,
    /// 已重绑当前销售行的正式版本行。
    pub revision_lines: &'a [PurchaseOrderRevisionLine],
    /// 与新正式版本同事务写入的分配。
    pub allocations: &'a super::allocation_maintenance::PreparedSalesAllocations,
}
/// 首次采购正式化时，在调用方事务内依次写入版本及明细、销售分配，再推进审核提交和采购单并按原顺序做 CAS。
/// 返回已指向生效版本的采购单；状态校验或任一步写入失败即停止，由调用方处理回滚。
pub async fn persist_formalized_order(
    db: &mongodb::Database,
    write: FormalizedOrderWrite<'_>,
    actor_id: &str,
    executor: &mut dyn persistence_core::Executor,
) -> Result<PurchaseOrder> {
    let FormalizedOrderWrite { order, submission, revision, revision_lines, allocations } = write;
    db.purchase_order().create_effective_revision(revision, revision_lines, executor).await?;
    super::allocation_maintenance::persist_current_sales_allocations(db, allocations, executor).await?;

    let mut order_mut = order;
    order_mut.formalize_with_revision(revision.base.id.clone().into(), actor_id)?;
    let mut submission_mut = submission;
    submission_mut.record_review(
        PurchaseOrderReviewDecision::Approved { comment: None },
        Instant::now(),
        actor_id,
    )?;

    db.purchase_order_submissions().update(&mut submission_mut, executor).await?;
    db.purchase_orders().update(&mut order_mut, executor).await?;
    Ok(order_mut)
}

/// 采购正式化前，将冻结提交表头与同次提交的逐行金额核对。
/// 金额不一致按实体文案返回 `BusinessLogicError`；不读取外域或补算来源事实。
pub fn ensure_review_sources(
    submission: &PurchaseOrderSubmission,
    lines: &[PurchaseOrderSubmissionLine],
) -> Result<()> {
    submission.ensure_line_totals(lines).map_err(|error| Error::BusinessLogicError(error.to_string()))
}
