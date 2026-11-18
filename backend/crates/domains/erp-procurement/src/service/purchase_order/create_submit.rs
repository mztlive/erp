//! 同一调用方事务中的采购创建草稿读取与提交冻结。
use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_core::ids::{PurchaseOrderSubmissionId, PurchaseOrderSubmissionLineId};
use id_generator::next_id;
use mongodb::Database;
use persistence_core::Executor;

use crate::Result;
use crate::entity::purchase_order::{PurchaseOrder, PurchaseOrderSubmission, PurchaseOrderSubmissionLine};
use crate::repository::PurchaseOrderExt;
/// 按当前草稿派生正式提交头和行。
///
/// 序号只识别 `SUB-{n}`，忽略草稿编号。
///
/// 返回冻结提交头与重新挂接的行。提交序号溢出或草稿行不变式失败时返回错误。
///
/// - `draft`：当前草稿提交。
/// - `draft_lines`：当前草稿行。
pub async fn freeze_submission_from_created_draft(
    db: &Database,
    order: &PurchaseOrder,
    draft: &PurchaseOrderSubmission,
    draft_lines: &[PurchaseOrderSubmissionLine],
    actor: &AuditActor,
    session: &mut dyn Executor,
) -> Result<(PurchaseOrderSubmission, Vec<PurchaseOrderSubmissionLine>)> {
    let existing =
        db.purchase_order().list_submissions_by_order(&order.base.id.clone().into(), session).await?;

    let formal = PurchaseOrderSubmission::freeze_from_draft(
        PurchaseOrderSubmissionId::new(next_id()),
        PurchaseOrderSubmission::next_submission_no(&existing)?,
        draft,
        Instant::now(),
        actor.id(),
    )?;

    let formal_id = PurchaseOrderSubmissionId::new(formal.base.id.clone());
    let mut lines = Vec::with_capacity(draft_lines.len());
    for line in draft_lines {
        lines.push(PurchaseOrderSubmissionLine::freeze_from_draft(
            PurchaseOrderSubmissionLineId::new(next_id()),
            formal_id.clone(),
            line,
        )?);
    }
    Ok((formal, lines))
}
