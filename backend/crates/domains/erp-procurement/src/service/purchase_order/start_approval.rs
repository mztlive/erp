//! 采购启动审批时的冻结提交与旧草稿持久化。

use persistence_core::Executor;

use crate::Result;
use crate::entity::purchase_order::{PurchaseOrder, PurchaseOrderSubmission, PurchaseOrderSubmissionLine};
use crate::repository::PurchaseOrderExt;

/// 组合层完成审批回执、正式编号守卫和采购覆盖校验后，写入冻结提交及明细，并以乐观锁切换采购单当前提交指针。
/// 三处写入复用调用方事务；任一步失败立即返回原错误。
pub async fn persist_started_submission(
    db: &mongodb::Database,
    order: &mut PurchaseOrder,
    submission: &PurchaseOrderSubmission,
    lines: &[PurchaseOrderSubmissionLine],
    executor: &mut dyn Executor,
) -> Result<()> {
    db.purchase_order().create_purchase_submission(order, submission, lines, executor).await?;
    Ok(())
}

/// 冻结提交已写入后，用同一执行器按期望版本保存已标为失效的原草稿，保留其历史明细。
/// 遇到版本冲突或持久化失败即返回，调用方不能继续推进审批运行事实。
pub async fn persist_superseded_draft(
    db: &mongodb::Database,
    draft: &mut PurchaseOrderSubmission,
    executor: &mut dyn Executor,
) -> Result<()> {
    db.purchase_order_submissions().update(draft, executor).await?;
    Ok(())
}
