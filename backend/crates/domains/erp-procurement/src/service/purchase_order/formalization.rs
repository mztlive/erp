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
    /// 计算下一个版本号（同一采购单内从 1 递增）。
    ///
    /// 用 `NoTransaction` 读取已有版本，不加入调用方事务。
    ///
    /// # 参数
    /// * `order` - 当前采购单
    ///
    /// # 返回
    /// 没有历史版本时返回 `1`，否则返回最大版本号加一。
    ///
    /// # 错误
    /// 版本列表读取失败时返回仓储错误；版本号达到 `u32::MAX` 时返回 `Logic`。
    pub async fn next_revision_no(&self, order: &PurchaseOrder) -> Result<u32> {
        let existing = self
            .db
            .purchase_order()
            .list_revisions_by_order(&order.base.id.clone().into(), &mut NoTransaction)
            .await?;
        PurchaseOrderRevision::next_revision_no(&existing).map_err(Into::into)
    }
    /// 由已冻结采购提交构造生效版本和版本行，不写库、不构造销售分配。
    ///
    /// # 参数
    /// * `order` - 当前采购单，只核对其稳定 ID
    /// * `submission` - 已冻结提交
    /// * `submission_lines` - 该提交的行
    /// * `revision_no` - 调用方算好的版本号
    ///
    /// # 返回
    /// 返回尚未落库的版本头和版本行。
    ///
    /// # 错误
    /// 提交不属于当前采购单时返回 `BusinessLogicError`；提交状态、版本号或行不变式
    /// 不满足时返回 `Logic`。
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
    /// # 参数
    /// * `order` - 当前采购单，只取其稳定 ID
    /// * `submission` - 变更提交
    /// * `lines` - 变更提交行
    /// * `revision_no` - 调用方算好的版本号
    ///
    /// # 返回
    /// 返回尚未落库的版本头和版本行。
    ///
    /// # 错误
    /// 变更提交或行不能形成版本时返回 `Logic`。
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
///
/// 任一步失败立即返回，本函数不继续后续写入。不开启事务。
///
/// # 参数
/// * `db` - 采购数据库
/// * `write` - 已准备的版本、分配、订单和提交
/// * `actor_id` - 最终通过执行人
/// * `executor` - 调用方事务执行器
///
/// # 返回
/// 返回已推进为正式版本的采购单。
///
/// # 错误
/// 版本或分配写入失败时返回仓储错误；订单或提交状态不允许时返回 `Logic`；
/// 提交或订单 CAS 失败时返回对应仓储错误。
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

/// 在原正式化写入前校验冻结提交与逐行金额一致，不增加外域或数据库规则。
///
/// # 参数
/// * `submission` - 待审核提交
/// * `lines` - 该提交的行
///
/// # 返回
/// 金额一致时返回 `Ok(())`。
///
/// # 错误
/// 行金额与提交头不一致时返回 `BusinessLogicError`，文案取实体错误文本。
pub fn ensure_review_sources(
    submission: &PurchaseOrderSubmission,
    lines: &[PurchaseOrderSubmissionLine],
) -> Result<()> {
    submission.ensure_line_totals(lines).map_err(|error| Error::BusinessLogicError(error.to_string()))
}
