//! 采购草稿冻结、不可变提交行构造与本域序号读取。
use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_core::ids::{PurchaseOrderSubmissionId, PurchaseOrderSubmissionLineId};
use id_generator::next_id;
use persistence_core::NoTransaction;

use super::PurchaseOrderService;
use super::line_input::{build_submission_lines, compute_request_totals, to_line_inputs};
use crate::Result;
use crate::dto::purchase_order::SavePurchaseOrderLine;
use crate::entity::purchase_order::{
    PurchaseOrder, PurchaseOrderSubmission, PurchaseOrderSubmissionData, PurchaseOrderSubmissionLine,
};
use crate::repository::PurchaseOrderExt;
impl PurchaseOrderService {
    /// 冻结草稿为正式提交（复制明细并重指向正式提交、推进主表指针）。
    pub async fn freeze_submission(
        &self,
        order: &mut PurchaseOrder,
        draft: &mut PurchaseOrderSubmission,
        draft_lines: &mut [PurchaseOrderSubmissionLine],
        actor: &AuditActor,
    ) -> Result<PurchaseOrderSubmission> {
        let next_no = self.next_submission_no(order).await?;
        let formal = PurchaseOrderSubmission::freeze_from_draft(
            PurchaseOrderSubmissionId::new(next_id()),
            next_no,
            draft,
            Instant::now(),
            actor.id(),
        )?;
        let formal_id = PurchaseOrderSubmissionId::new(formal.base.id.clone());
        for line in draft_lines.iter_mut() {
            *line = PurchaseOrderSubmissionLine::freeze_from_draft(
                PurchaseOrderSubmissionLineId::new(next_id()),
                formal_id.clone(),
                line,
            )?;
        }
        let _ = order;
        Ok(formal)
    }
    /// 按提交命令携带的草稿补丁直接构造正式采购提交。
    pub async fn freeze_submission_from_lines(
        &self,
        order: &PurchaseOrder,
        draft: &PurchaseOrderSubmission,
        requested_lines: &[SavePurchaseOrderLine],
        actor: &AuditActor,
    ) -> Result<(PurchaseOrderSubmission, Vec<PurchaseOrderSubmissionLine>)> {
        let next_no = self.next_submission_no(order).await?;
        let inputs = to_line_inputs(requested_lines)?;
        let (gross, net, tax) = compute_request_totals(&inputs)?;
        let mut formal = PurchaseOrderSubmission::new(
            PurchaseOrderSubmissionId::new(next_id()),
            PurchaseOrderSubmissionData {
                purchase_order_id: order.base.id.clone().into(),
                submission_no: next_no,
                supplier_id: draft.supplier_id.clone(),
                purchase_type: draft.purchase_type,
                fulfillment_responsibility: draft.fulfillment_responsibility,
                supplier_revision_id: draft.supplier_revision_id.clone(),
                supplier_snapshot: draft.supplier_snapshot.clone(),
                payment_term_snapshot: draft.payment_term_snapshot.clone(),
                gross_amount: gross,
                net_amount: net,
                tax_amount: tax,
            },
        )?;
        formal.submit(Instant::now(), actor.id())?;
        let lines = build_submission_lines(&formal.base.id.clone().into(), &inputs)?;
        Ok((formal, lines))
    }
    /// 计算下一个提交序号（`SUB-{n}`，聚合内唯一）。
    pub async fn next_submission_no(&self, order: &PurchaseOrder) -> Result<String> {
        let existing = self
            .db
            .purchase_order()
            .list_submissions_by_order(&order.base.id.clone().into(), &mut NoTransaction)
            .await?;
        PurchaseOrderSubmission::next_submission_no(&existing).map_err(Into::into)
    }
}
/// 首次提交分配不可复用正式号。已有正式号时保持不变。
///
/// # 错误
/// 编号非法时返回校验错误。
pub fn assign_formal_purchase_no(order: &mut PurchaseOrder) -> Result<()> {
    if !order.purchase_no.is_empty() {
        return Ok(());
    }
    Ok(order.assign_purchase_no(format!("PO-{}", order.base.id))?)
}
