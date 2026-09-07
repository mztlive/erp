//! 采购启动审批时的冻结提交与旧草稿持久化。

use crate::entity::purchase_order::{PurchaseOrder, PurchaseOrderSubmission, PurchaseOrderSubmissionLine};
use crate::repository::PurchaseOrderExt;
use crate::Result;
use persistence_core::Executor;

/// 在调用方事务内写入冻结提交、提交行和采购单当前提交指针。
///
/// # 错误
/// 新提交或提交行写入、采购单版本 CAS 失败时返回原错误。
///
/// # 关键业务约束
/// 审批收据、正式编号守卫和采购覆盖校验由组合层先行完成；本方法不得新开事务。
pub async fn persist_started_submission(
    db: &mongodb::Database,
    order: &mut PurchaseOrder,
    submission: &PurchaseOrderSubmission,
    lines: &[PurchaseOrderSubmissionLine],
    executor: &mut dyn Executor,
) -> Result<()> {
    db.purchase_order()
        .create_purchase_submission(order, submission, lines, executor)
        .await?;
    Ok(())
}

/// 在已写入冻结提交之后，使用同一执行器将原草稿标记为已失效。
///
/// # 错误
/// 旧草稿版本 CAS 或仓储写入失败时返回原错误；不得推进后续审批运行事实。
pub async fn persist_superseded_draft(
    db: &mongodb::Database,
    draft: &mut PurchaseOrderSubmission,
    executor: &mut dyn Executor,
) -> Result<()> {
    db.purchase_order_submissions().update(draft, executor).await?;
    Ok(())
}
