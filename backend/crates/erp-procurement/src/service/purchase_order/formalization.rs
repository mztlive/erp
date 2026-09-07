//! 采购正式版本的单域序号读取与冻结构造。
use super::PurchaseOrderService;
use crate::entity::purchase_order::*;
use crate::repository::PurchaseOrderExt;
use crate::{Error, Result};
use erp_core::common::time::Instant;
use erp_core::ids::{PurchaseOrderRevisionId, PurchaseOrderRevisionLineId};
use id_generator::next_id;
use persistence_core::NoTransaction;
impl PurchaseOrderService {
    /// 计算下一个版本号（同一采购单内从 1 递增）。
    pub async fn next_revision_no(&self, order: &PurchaseOrder) -> Result<u32> {
        let existing = self
            .db
            .purchase_order()
            .list_revisions_by_order(&order.base.id.clone().into(), &mut NoTransaction)
            .await?;
        PurchaseOrderRevision::next_revision_no(&existing).map_err(Into::into)
    }
    /// 形成生效版本与版本行（§8.1.4 复制已通过提交）。
    ///
    /// 说明：`purchase_line_sales_allocation` 的 Data 类型未从实体层导出
    /// （entities 冻结），分配写入本阶段无法构造实体，已在报告中提出；
    /// 版本行保留销售提交行引用与分配数量，供入库预占沿分配关系回查。
    pub async fn build_effective_revision(
        &self,
        order: &PurchaseOrder,
        submission: &PurchaseOrderSubmission,
        submission_lines: &[PurchaseOrderSubmissionLine],
        revision_no: u32,
    ) -> Result<(PurchaseOrderRevision, Vec<PurchaseOrderRevisionLine>)> {
        if submission.purchase_order_id.as_ref() != order.base.id {
            return Err(crate::Error::BusinessLogicError(
                "采购提交不属于当前采购单".to_string(),
            ));
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
    /// 形成变更生效版本与版本行。
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
/// 在调用方事务中依次创建版本及行、分配，推进订单与审核提交并按原序执行 CAS。
/// 任一步失败立即返回，后续财务和任务不得继续。
pub async fn persist_formalized_order(
    db: &mongodb::Database,
    write: FormalizedOrderWrite<'_>,
    actor_id: &str,
    executor: &mut dyn persistence_core::Executor,
) -> Result<PurchaseOrder> {
    let FormalizedOrderWrite {
        order,
        submission,
        revision,
        revision_lines,
        allocations,
    } = write;
    db.purchase_order()
        .create_effective_revision(revision, revision_lines, executor)
        .await?;
    super::allocation_maintenance::persist_current_sales_allocations(db, allocations, executor).await?;
    let mut order_mut = order;
    order_mut.formalize_with_revision(revision.base.id.clone().into(), actor_id)?;
    let mut submission_mut = submission;
    submission_mut.record_review(
        PurchaseOrderReviewDecision::Approved { comment: None },
        Instant::now(),
        actor_id,
    )?;
    db.purchase_order_submissions()
        .update(&mut submission_mut, executor)
        .await?;
    db.purchase_orders().update(&mut order_mut, executor).await?;

    Ok(order_mut)
}

/// 在原正式化写入前校验冻结提交与逐行金额一致，不增加外域或数据库规则。
pub fn ensure_review_sources(
    submission: &PurchaseOrderSubmission,
    lines: &[PurchaseOrderSubmissionLine],
) -> Result<()> {
    submission
        .ensure_line_totals(lines)
        .map_err(|error| Error::BusinessLogicError(error.to_string()))
}
