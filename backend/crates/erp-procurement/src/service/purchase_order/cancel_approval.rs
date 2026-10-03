//! 采购撤回审批后的单据状态持久化。

use erp_core::ids::{PurchaseOrderSubmissionId, PurchaseOrderSubmissionLineId};
use id_generator::next_id;
use persistence_core::Executor;

use crate::entity::purchase_order::PurchaseOrder;
use crate::repository::PurchaseOrderExt;
use crate::{Error, Result};

/// 在审批取消运行事实写入后，以同一执行器 CAS 写回已执行撤回动作的采购单。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `order` - 已执行取消领域动作但尚未持久化的采购单
/// * `executor` - 审批取消事务的执行器
///
/// # 返回
/// 创建独立草稿提交与明细，切换当前提交指针并写回采购单。
///
/// # 错误
/// 采购单版本冲突、冻结来源不一致或仓储写入失败时返回原错误。
///
/// # 关键业务约束
/// 调用方持有根事务并先执行采购撤回状态动作；本方法不得另开事务。
/// 原冻结提交与明细保持不变，新草稿和采购单指针随审批取消事实原子提交。
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
