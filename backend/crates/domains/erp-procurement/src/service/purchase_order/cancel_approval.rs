//! 采购撤回审批后的单据状态持久化。

use erp_core::ids::{PurchaseOrderSubmissionId, PurchaseOrderSubmissionLineId};
use id_generator::next_id;
use persistence_core::Executor;

use crate::entity::purchase_order::PurchaseOrder;
use crate::repository::PurchaseOrderExt;
use crate::{Error, Result};

/// 审批运行时已写入撤回事实且采购单已执行领域撤回后，从原冻结提交复制独立草稿与明细，再 CAS 更新当前指针。
/// 全部步骤沿用调用方审批事务，原提交及明细不修改；来源不一致、版本冲突或仓储写入失败则返回原错误。
pub async fn persist_cancelled_order(
    db: &mongodb::Database,
    order: &mut PurchaseOrder,
    executor: &mut dyn Executor,
) -> Result<()> {
    let previous = super::shared::load_order_by_id(db, &order.base.id, executor).await?;
    previous
        .ensure_expected_version(order.base.version)
        .map_err(|_| Error::ConflictError("采购单取消版本已变化".to_string()))?;

    let source_id = order.draft_submission_id()?;
    let source = db
        .purchase_order_submissions()
        .find_by_id(source_id.as_ref(), executor)
        .await?
        .ok_or_else(|| Error::NotFound("采购单取消来源提交不存在".to_string()))?;
    let source_lines = db.purchase_order().list_submission_lines(&source_id, executor).await?;

    let line_ids =
        source_lines.iter().map(|_| PurchaseOrderSubmissionLineId::new(next_id())).collect::<Vec<_>>();
    let draft = order
        .reopen_cancelled_draft(
            &previous,
            &source,
            &source_lines,
            PurchaseOrderSubmissionId::new(next_id()),
            &line_ids,
        )
        .map_err(|error| Error::ConflictError(error.to_string()))?;

    db.purchase_order_submissions().create(&draft.submission, executor).await?;
    for line in &draft.lines {
        db.purchase_order_submission_lines().create(line, executor).await?;
    }
    db.purchase_orders().update(order, executor).await?;
    Ok(())
}
