//! 采购启动审批时的冻结提交与旧草稿持久化。

use persistence_core::Executor;

use crate::Result;
use crate::entity::purchase_order::{PurchaseOrder, PurchaseOrderSubmission, PurchaseOrderSubmissionLine};
use crate::repository::PurchaseOrderExt;

/// 在调用方事务内写入冻结提交、提交行和采购单当前提交指针。
///
/// # 参数
/// * `db` - 采购数据库
/// * `order` - 已带期望版本和新提交指针的采购单
/// * `submission` - 待写入的冻结提交
/// * `lines` - 待写入的提交行
/// * `executor` - 调用方事务执行器
///
/// # 返回
/// 写入成功时无返回值。
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
    db.purchase_order().create_purchase_submission(order, submission, lines, executor).await?;
    Ok(())
}

/// 在已写入冻结提交之后，使用同一执行器将原草稿标记为已失效。
///
/// # 参数
/// * `db` - 采购数据库
/// * `draft` - 已改为失效状态、带期望版本的旧草稿
/// * `executor` - 与冻结提交相同的执行器
///
/// # 返回
/// 更新成功时无返回值。
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
