//! 采购草稿冻结、不可变提交行构造与本域序号读取。
use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_core::ids::{PurchaseOrderSubmissionId, PurchaseOrderSubmissionLineId};
use id_generator::next_id;
use persistence_core::NoTransaction;

use super::PurchaseOrderService;
use super::line_input::{build_submission_lines, compute_request_totals};
use crate::Result;
use crate::dto::purchase_order::SavePurchaseOrderLine;
use crate::entity::purchase_order::{
    PurchaseOrder, PurchaseOrderSubmission, PurchaseOrderSubmissionData, PurchaseOrderSubmissionLine,
    inherit_submission_sources,
};
use crate::repository::PurchaseOrderExt;
impl PurchaseOrderService {
    /// 提交未改行的采购草稿时，复制为新的待审核提交，并将 `draft_lines` 原地替换为挂在新提交上的冻结行。
    /// 序号用 `NoTransaction` 读取历史提交后计算；不写库、不改草稿头或采购单指针。原状态或行不合法、序号读取失败或溢出时拒绝。
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
    /// 采购提交同时编辑行内容时，使用服务端当前草稿及完整原行，应用允许的补丁并继承原行正式选源。
    /// 校验数量和金额后返回冻结提交及明细；原行缺失、SKU 来源错配、序号或内容不合法时拒绝，不接受客户端提供正式选源事实。
    pub async fn freeze_submission_from_lines(
        &self,
        order: &PurchaseOrder,
        draft: &PurchaseOrderSubmission,
        draft_lines: &[PurchaseOrderSubmissionLine],
        requested_lines: &[SavePurchaseOrderLine],
        actor: &AuditActor,
    ) -> Result<(PurchaseOrderSubmission, Vec<PurchaseOrderSubmissionLine>)> {
        let next_no = self.next_submission_no(order).await?;

        let mut inputs = SavePurchaseOrderLine::to_line_inputs(requested_lines)?;
        inherit_submission_sources(&mut inputs, draft_lines)?;
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
                payment_subject: draft.payment_subject.clone(),
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
    /// 冻结采购草稿前，以 `NoTransaction` 读取同一采购单的历史提交，不加入调用方事务。
    /// 按有效历史序号生成下一个 `SUB-` 加六位序号；仓储失败或序号溢出时返回错误。
    pub async fn next_submission_no(&self, order: &PurchaseOrder) -> Result<String> {
        let existing = self
            .db
            .purchase_order()
            .list_submissions_by_order(&order.base.id.clone().into(), &mut NoTransaction)
            .await?;
        PurchaseOrderSubmission::next_submission_no(&existing).map_err(Into::into)
    }
}
/// 首次提交采购单时，以 `PO-` 和稳定单据 ID 分配正式号；已有号既不改写也不重新校验。
/// 新号未通过实体校验时返回 `Logic`，不得用它推进审批。
pub fn assign_formal_purchase_no(order: &mut PurchaseOrder) -> Result<()> {
    if !order.purchase_no.is_empty() {
        return Ok(());
    }
    Ok(order.assign_purchase_no(format!("PO-{}", order.base.id))?)
}
